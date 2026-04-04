use crate::codex::Session;
use crate::codex::TurnContext;
use crate::config::ConfigBuilder;
use crate::config::types::ZmemoryConfig;
use crate::memories::zmemory_contract::StablePreferenceMemory;
use crate::protocol::EventMsg;
use crate::protocol::WarningEvent;
use codex_app_server_protocol::ConfigLayerSource;
use codex_protocol::user_input::UserInput;
use codex_zmemory::tool_api::ZmemoryToolAction;
use codex_zmemory::tool_api::ZmemoryToolCallParam;
use codex_zmemory::tool_api::run_zmemory_tool_with_context;
use regex_lite::Regex;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::LazyLock;
use tracing::warn;

static USER_ADDRESS_PATTERNS: &[&str] = &["称呼我", "叫我", "call me", "refer to me as"];
static AGENT_NAME_PATTERNS: &[&str] = &["你的名字是", "your name is", "call yourself"];
static QUOTED_VALUE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"["“”'‘’「」『』]([^"“”'‘’「」『』]+)["“”'‘’「」『』]"#).expect("valid regex")
});

pub(crate) async fn capture_stable_preference_memories(
    session: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
    items: &[UserInput],
) {
    let Some(mut capture) = StablePreferenceCapture::from_items(items) else {
        return;
    };

    let zmemory_context = resolve_zmemory_context_for_turn(session, turn_context).await;

    if let Err(err) = inspect_workspace_runtime(&zmemory_context, turn_context) {
        emit_capture_warning(session, turn_context, err.to_string()).await;
        return;
    }

    if capture.agent_name.is_none()
        && let Ok(Some(content)) = read_canonical_content(
            &zmemory_context,
            turn_context,
            StablePreferenceMemory::AgentSelfReference,
        )
    {
        capture.agent_name = parse_name_from_memory_content(&content);
    }
    if capture.user_address.is_none()
        && let Ok(Some(content)) = read_canonical_content(
            &zmemory_context,
            turn_context,
            StablePreferenceMemory::UserAddressPreference,
        )
    {
        capture.user_address = parse_name_from_memory_content(&content);
    }

    let writes = capture.into_writes();
    for (memory, content) in writes {
        if let Err(err) =
            write_and_verify_canonical_memory(&zmemory_context, turn_context, memory, &content)
        {
            emit_capture_warning(session, turn_context, err.to_string()).await;
        }
    }
}

#[derive(Clone)]
struct ResolvedZmemoryContext {
    codex_home: PathBuf,
    zmemory_config: ZmemoryConfig,
}

async fn resolve_zmemory_context_for_turn(
    session: &Session,
    turn_context: &TurnContext,
) -> ResolvedZmemoryContext {
    let session_config = session.get_config().await;
    let current_zmemory_config = session_config.zmemory.clone();
    let zmemory_origin = session_config
        .config_layer_stack
        .origins()
        .remove("zmemory.path")
        .map(|metadata| metadata.name);
    let should_reload = session_config.cwd.as_path() != turn_context.cwd.as_path()
        && matches!(
            zmemory_origin,
            None | Some(ConfigLayerSource::Project { .. })
        );
    let codex_home = turn_context.config.codex_home.clone();

    if !should_reload {
        return ResolvedZmemoryContext {
            codex_home,
            zmemory_config: current_zmemory_config,
        };
    }

    let zmemory_config = match ConfigBuilder::default()
        .codex_home(codex_home.clone())
        .fallback_cwd(Some(turn_context.cwd.to_path_buf()))
        .build()
        .await
    {
        Ok(config) => config.zmemory,
        Err(err) => {
            warn!(
                error = %err,
                cwd = %turn_context.cwd.display(),
                "failed to reload proactive zmemory config for current turn cwd; using session config"
            );
            current_zmemory_config
        }
    };

    ResolvedZmemoryContext {
        codex_home,
        zmemory_config,
    }
}

fn inspect_workspace_runtime(
    zmemory_context: &ResolvedZmemoryContext,
    turn_context: &TurnContext,
) -> anyhow::Result<()> {
    run_zmemory_tool_with_context(
        zmemory_context.codex_home.as_path(),
        turn_context.cwd.as_path(),
        zmemory_context.zmemory_config.path.as_deref(),
        Some(zmemory_context.zmemory_config.to_runtime_settings()),
        ZmemoryToolCallParam {
            action: ZmemoryToolAction::Read,
            uri: Some("system://workspace".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )?;
    Ok(())
}

fn write_and_verify_canonical_memory(
    zmemory_context: &ResolvedZmemoryContext,
    turn_context: &TurnContext,
    memory: StablePreferenceMemory,
    content: &str,
) -> anyhow::Result<()> {
    let existing_content = read_canonical_content(zmemory_context, turn_context, memory)?;
    if existing_content.as_deref() == Some(content) {
        return Ok(());
    }

    let action = if existing_content.is_some() {
        ZmemoryToolAction::Update
    } else {
        ZmemoryToolAction::Create
    };

    run_zmemory_tool_with_context(
        zmemory_context.codex_home.as_path(),
        turn_context.cwd.as_path(),
        zmemory_context.zmemory_config.path.as_deref(),
        Some(zmemory_context.zmemory_config.to_runtime_settings()),
        ZmemoryToolCallParam {
            action,
            uri: Some(memory.uri().to_string()),
            content: Some(content.to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )?;

    let verified_content = read_canonical_content(zmemory_context, turn_context, memory)?;
    anyhow::ensure!(
        verified_content.as_deref() == Some(content),
        "zmemory proactive capture verification failed for {}",
        memory.uri()
    );
    Ok(())
}

fn read_canonical_content(
    zmemory_context: &ResolvedZmemoryContext,
    turn_context: &TurnContext,
    memory: StablePreferenceMemory,
) -> anyhow::Result<Option<String>> {
    let uri = memory.uri();
    match run_zmemory_tool_with_context(
        zmemory_context.codex_home.as_path(),
        turn_context.cwd.as_path(),
        zmemory_context.zmemory_config.path.as_deref(),
        Some(zmemory_context.zmemory_config.to_runtime_settings()),
        ZmemoryToolCallParam {
            action: ZmemoryToolAction::Read,
            uri: Some(uri.to_string()),
            ..ZmemoryToolCallParam::default()
        },
    ) {
        Ok(result) => Ok(read_content_from_tool_result(&result.structured_content)),
        Err(err) if err.to_string() == format!("memory not found: {uri}") => Ok(None),
        Err(err) => Err(err),
    }
}

fn read_content_from_tool_result(payload: &Value) -> Option<String> {
    payload
        .get("result")
        .and_then(|result| result.get("content"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

async fn emit_capture_warning(
    session: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
    detail: String,
) {
    let message = format!("主动捕获稳定 zmemory 偏好失败：{detail}");
    warn!("{message}");
    session
        .send_event(turn_context, EventMsg::Warning(WarningEvent { message }))
        .await;
}

#[derive(Debug, Default, PartialEq, Eq)]
struct StablePreferenceCapture {
    user_address: Option<String>,
    agent_name: Option<String>,
}

impl StablePreferenceCapture {
    fn from_items(items: &[UserInput]) -> Option<Self> {
        let mut capture = Self::default();
        for item in items {
            let UserInput::Text { text, .. } = item else {
                continue;
            };
            if let Some(user_address) = extract_explicit_value(text, USER_ADDRESS_PATTERNS) {
                capture.user_address = Some(user_address);
            }
            if let Some(agent_name) = extract_explicit_value(text, AGENT_NAME_PATTERNS) {
                capture.agent_name = Some(agent_name);
            }
        }

        if capture.user_address.is_some() || capture.agent_name.is_some() {
            Some(capture)
        } else {
            None
        }
    }

    fn into_writes(self) -> Vec<(StablePreferenceMemory, String)> {
        let mut writes = Vec::new();
        if let Some(user_address) = self.user_address.as_ref() {
            writes.push((
                StablePreferenceMemory::UserAddressPreference,
                format!("The user prefers to be addressed as \"{user_address}\"."),
            ));
        }
        if let Some(agent_name) = self.agent_name.as_ref() {
            writes.push((
                StablePreferenceMemory::AgentSelfReference,
                format!("The assistant should refer to itself as \"{agent_name}\"."),
            ));
        }
        if let (Some(user_address), Some(agent_name)) =
            (self.user_address.as_ref(), self.agent_name.as_ref())
        {
            writes.push((
                StablePreferenceMemory::CollaborationAddressContract,
                format!(
                    "Use \"{agent_name}\" for the assistant and \"{user_address}\" for the user in future interactions."
                ),
            ));
        }
        writes
    }
}

fn extract_explicit_value(text: &str, patterns: &[&str]) -> Option<String> {
    patterns.iter().find_map(|pattern| {
        let lowercase = text.to_lowercase();
        let pattern_lowercase = pattern.to_lowercase();
        let start = lowercase.find(&pattern_lowercase)?;
        let suffix = &text[start + pattern.len()..];
        parse_explicit_name_value(suffix)
    })
}

fn parse_explicit_name_value(raw: &str) -> Option<String> {
    let raw = raw.trim_start_matches(|c: char| {
        c.is_whitespace() || matches!(c, '：' | ':' | '，' | ',' | '=')
    });
    if raw.is_empty() {
        return None;
    }

    if let Some(captures) = QUOTED_VALUE_RE.captures(raw) {
        let value = captures.get(1)?.as_str().trim();
        return (!value.is_empty()).then(|| value.to_string());
    }

    let bare = raw
        .chars()
        .take_while(|c| {
            !c.is_whitespace()
                && !matches!(
                    *c,
                    '，' | ',' | '。' | '.' | '！' | '!' | '？' | '?' | '；' | ';'
                )
        })
        .collect::<String>()
        .trim()
        .to_string();
    (!bare.is_empty()).then_some(bare)
}

fn parse_name_from_memory_content(content: &str) -> Option<String> {
    QUOTED_VALUE_RE
        .captures(content)
        .and_then(|captures| captures.get(1))
        .map(|m| m.as_str().trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::StablePreferenceCapture;
    use super::extract_explicit_value;
    use super::parse_name_from_memory_content;
    use codex_protocol::user_input::UserInput;
    use pretty_assertions::assert_eq;

    #[test]
    fn detects_both_preferences_from_single_text_item() {
        let capture = StablePreferenceCapture::from_items(&[UserInput::Text {
            text: "你现在开始称呼我\"指挥官\",你的名字是\"小白\"".to_string(),
            text_elements: Vec::new(),
        }]);

        assert_eq!(
            capture,
            Some(StablePreferenceCapture {
                user_address: Some("指挥官".to_string()),
                agent_name: Some("小白".to_string()),
            })
        );
    }

    #[test]
    fn detects_english_call_me_pattern() {
        let value = extract_explicit_value("From now on, call me Commander.", &["call me"]);
        assert_eq!(value.as_deref(), Some("Commander"));
    }

    #[test]
    fn parses_existing_memory_content_back_to_name() {
        let value =
            parse_name_from_memory_content("The assistant should refer to itself as \"小白\".");
        assert_eq!(value.as_deref(), Some("小白"));
    }
}
