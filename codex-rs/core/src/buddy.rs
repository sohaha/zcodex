use crate::Prompt;
use crate::ResponseEvent;
use crate::codex::Session;
use crate::codex::TurnContext;
use crate::compact::content_items_to_text;
use crate::config::Config;
use crate::config::edit::ConfigEdit;
use crate::config::edit::ConfigEditsBuilder;
use codex_config::types::BuddySoul;
use codex_protocol::models::BaseInstructions;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ResponseItem;
use futures::StreamExt;
use serde::Deserialize;
use serde_json::Value;
use serde_json::json;
use std::path::Path;
use toml_edit::Item as TomlItem;
use toml_edit::Table as TomlTable;
use toml_edit::value;
use tracing::warn;

const COMPANION_INTRO_MARKER: &str = "# Companion";
const BUDDY_SOUL_PROMPT: &str = r#"# Buddy soul generator

You are generating a companion soul for a coding assistant UI.

Return a JSON object with:
- name: short cute name, ASCII letters only, 3-12 chars, no spaces.
- personality: short Chinese phrase (<= 12 characters) describing the vibe.

Return only JSON. Do not include extra text."#;

const BUDDY_REACTION_PROMPT: &str = r#"# Buddy reaction generator

You write a short reaction for the buddy's speech bubble.

Return a JSON object with:
- text: single-line reaction, <= 30 Chinese characters.

Constraints:
- Friendly, concise, no narration.
- Do not claim to be the assistant or mention tool use.

Return only JSON. Do not include extra text."#;

const MAX_CONTEXT_CHARS: usize = 400;
const MAX_REACTION_CHARS: usize = 40;

#[derive(Deserialize)]
struct BuddySoulOutput {
    name: String,
    personality: String,
}

#[derive(Deserialize)]
struct BuddyReactionOutput {
    text: String,
}

pub(crate) fn maybe_inject_companion_intro(config: &Config, base: &mut BaseInstructions) {
    if !config.tui_show_buddy && !config.tui_buddy_reactions_enabled {
        return;
    }
    if base.text.contains(COMPANION_INTRO_MARKER) {
        return;
    }
    let intro = companion_intro_text(config.tui_buddy_soul.as_ref());
    base.text = format!("{}\n\n{}", base.text.trim_end(), intro);
}

pub(crate) async fn generate_buddy_soul(
    session: &Session,
    turn_context: &TurnContext,
) -> Option<BuddySoul> {
    let prompt = Prompt {
        input: vec![ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: "Generate a new buddy soul.".to_string(),
            }],
            end_turn: None,
            phase: None,
        }],
        tools: Vec::new(),
        parallel_tool_calls: false,
        base_instructions: BaseInstructions {
            text: BUDDY_SOUL_PROMPT.to_string(),
        },
        personality: None,
        output_schema: Some(buddy_soul_output_schema()),
    };
    let raw = match stream_prompt_text(session, turn_context, prompt).await {
        Ok(text) => text,
        Err(err) => {
            warn!(turn_id = %turn_context.sub_id, "buddy soul generation failed: {err}");
            return None;
        }
    };
    let output: BuddySoulOutput = match parse_json_payload(&raw) {
        Some(payload) => payload,
        None => {
            warn!(
                turn_id = %turn_context.sub_id,
                "buddy soul generation returned invalid JSON"
            );
            return None;
        }
    };
    let name = sanitize_name(&output.name)?;
    let personality = sanitize_personality(&output.personality)?;
    Some(BuddySoul { name, personality })
}

pub(crate) async fn generate_buddy_reaction(
    session: &Session,
    turn_context: &TurnContext,
    soul: Option<&BuddySoul>,
    last_user_message: Option<&str>,
    last_agent_message: Option<&str>,
) -> Option<String> {
    if last_user_message.is_none() && last_agent_message.is_none() {
        return None;
    }

    let mut lines = Vec::new();
    if let Some(soul) = soul {
        lines.push(format!("Buddy name: {}", soul.name));
        lines.push(format!("Buddy personality: {}", soul.personality));
    }
    if let Some(message) = last_user_message {
        lines.push("User message:".to_string());
        lines.push(truncate_context(message, MAX_CONTEXT_CHARS));
    }
    if let Some(message) = last_agent_message {
        lines.push("Assistant reply:".to_string());
        lines.push(truncate_context(message, MAX_CONTEXT_CHARS));
    }
    lines.push("Write the buddy reaction now.".to_string());
    let user_text = lines.join("\n");

    let prompt = Prompt {
        input: vec![ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText { text: user_text }],
            end_turn: None,
            phase: None,
        }],
        tools: Vec::new(),
        parallel_tool_calls: false,
        base_instructions: BaseInstructions {
            text: BUDDY_REACTION_PROMPT.to_string(),
        },
        personality: None,
        output_schema: Some(buddy_reaction_output_schema()),
    };
    let raw = match stream_prompt_text(session, turn_context, prompt).await {
        Ok(text) => text,
        Err(err) => {
            warn!(turn_id = %turn_context.sub_id, "buddy reaction failed: {err}");
            return None;
        }
    };
    let output: BuddyReactionOutput = parse_json_payload(&raw)?;
    sanitize_reaction_text(&output.text)
}

pub(crate) fn fallback_buddy_reaction(seed: &str) -> String {
    const FALLBACKS: &[&str] = &[
        "我在旁边看着呢。",
        "稳住，继续敲。",
        "这波有点意思。",
        "进度不错。",
    ];
    let idx = (stable_hash(seed) as usize) % FALLBACKS.len();
    FALLBACKS[idx].to_string()
}

pub(crate) async fn persist_buddy_soul(codex_home: &Path, soul: &BuddySoul) -> anyhow::Result<()> {
    let mut table = TomlTable::new();
    table["name"] = value(soul.name.clone());
    table["personality"] = value(soul.personality.clone());
    let edit = ConfigEdit::SetPath {
        segments: vec!["tui".to_string(), "buddy".to_string(), "soul".to_string()],
        value: TomlItem::Table(table),
    };
    ConfigEditsBuilder::new(codex_home)
        .with_edits(std::iter::once(edit))
        .apply()
        .await
}

fn companion_intro_text(soul: Option<&BuddySoul>) -> String {
    let name = soul.map(|soul| soul.name.as_str()).unwrap_or("your buddy");
    format!(
        r#"{COMPANION_INTRO_MARKER}

A small terminal buddy named {name} sits beside the user's input box and occasionally comments in a speech bubble. You are not {name}; it is a separate watcher.

When the user addresses {name} directly (by name), its bubble will answer. Your job is to stay out of the way: respond in one line or less, or only answer the part meant for you. Do not narrate what {name} might say; the bubble handles that."#
    )
}

fn buddy_soul_output_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "name": { "type": "string" },
            "personality": { "type": "string" }
        },
        "required": ["name", "personality"],
        "additionalProperties": false
    })
}

fn buddy_reaction_output_schema() -> Value {
    json!({
        "type": "object",
        "properties": { "text": { "type": "string" } },
        "required": ["text"],
        "additionalProperties": false
    })
}

async fn stream_prompt_text(
    session: &Session,
    turn_context: &TurnContext,
    prompt: Prompt,
) -> crate::error::Result<String> {
    let mut client_session = session.services.model_client.new_session();
    let turn_metadata_header = turn_context.turn_metadata_state.current_header_value();
    let mut stream = client_session
        .stream(
            &prompt,
            &turn_context.model_info,
            &turn_context.session_telemetry,
            turn_context.reasoning_effort,
            turn_context.reasoning_summary,
            turn_context.config.service_tier.clone(),
            turn_metadata_header.as_deref(),
        )
        .await?;

    let mut result = String::new();
    while let Some(message) = stream.next().await.transpose()? {
        match message {
            ResponseEvent::OutputTextDelta(delta) => result.push_str(&delta),
            ResponseEvent::OutputItemDone(item) => {
                if result.is_empty()
                    && let ResponseItem::Message { content, .. } = item
                    && let Some(text) = content_items_to_text(&content)
                {
                    result.push_str(&text);
                }
            }
            ResponseEvent::Completed { .. } => break,
            _ => {}
        }
    }
    Ok(result)
}

fn parse_json_payload<T: for<'de> Deserialize<'de>>(raw: &str) -> Option<T> {
    if let Ok(value) = serde_json::from_str(raw) {
        return Some(value);
    }
    let start = raw.find('{')?;
    let end = raw.rfind('}')?;
    if start >= end {
        return None;
    }
    serde_json::from_str(&raw[start..=end]).ok()
}

fn sanitize_name(raw: &str) -> Option<String> {
    let filtered: String = raw
        .trim()
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect();
    if (3..=12).contains(&filtered.len()) {
        Some(filtered)
    } else {
        None
    }
}

fn sanitize_personality(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let shortened = trimmed.chars().take(12).collect::<String>();
    Some(shortened)
}

fn sanitize_reaction_text(raw: &str) -> Option<String> {
    let joined = raw
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let trimmed = joined.trim();
    if trimmed.is_empty() {
        return None;
    }
    let shortened = trimmed.chars().take(MAX_REACTION_CHARS).collect::<String>();
    Some(shortened)
}

fn truncate_context(raw: &str, max_chars: usize) -> String {
    let trimmed = raw.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    trimmed.chars().take(max_chars).collect::<String>()
}

fn stable_hash(value: &str) -> u64 {
    let mut hash = 1469598103934665603_u64;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(1099511628211);
    }
    hash
}
