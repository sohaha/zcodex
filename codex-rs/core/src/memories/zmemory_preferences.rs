use crate::config::ConfigBuilder;
use crate::config::ZmemoryConfig;
use crate::memories::zmemory_contract::StablePreferenceMemory;
use crate::session::session::Session;
use crate::session::turn_context::TurnContext;
use codex_app_server_protocol::ConfigLayerSource;
use codex_protocol::protocol::EventMsg;
use codex_protocol::protocol::WarningEvent;
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
static USER_IDENTITY_PATTERNS: &[&str] = &["我的身份是", "我的角色是", "我是"];
static AGENT_IDENTITY_PATTERNS: &[&str] = &[
    "你的身份是",
    "你的角色是",
    "你现在是",
    "你是",
    "your role is",
    "you are",
    "act as",
];
static DURABLE_PREFERENCE_PATTERNS: &[&str] = &[
    "以后",
    "之后",
    "默认",
    "长期",
    "from now on",
    "going forward",
    "by default",
    "always",
];
static CHINESE_RESPONSE_PATTERNS: &[&str] = &["中文", "chinese"];
static CONCISE_RESPONSE_PATTERNS: &[&str] = &["简洁", "简短", "精简", "concise", "brief", "short"];
static COLLABORATION_CONTINUATION_PATTERNS: &[&str] = &[
    "按上次方式",
    "按照上次",
    "以后都这样",
    "继续这样",
    "as before",
    "same way",
    "going forward",
    "keep doing this",
    "stick with this",
];
static IDENTITY_CONTINUATION_PATTERNS: &[&str] = &[
    "按这个身份",
    "按这个角色",
    "保持这个身份",
    "保持这个角色",
    "continue in this role",
    "keep this role",
    "stay in this role",
];
static AGENT_IDENTITY_REFERENCE_PATTERNS: &[&str] = &[
    "这个身份",
    "这个角色",
    "这种身份",
    "这种角色",
    "this role",
    "this identity",
];
const COLLABORATION_AGENT_ANCHOR_CONTENT: &str =
    "Canonical assistant identity anchor for collaboration preferences.";
const COLLABORATION_CONTRACT_HEADER: &str = "Shared collaboration contract:";
const USER_IDENTITY_HEADER: &str = "User identity anchor:";
const AGENT_IDENTITY_HEADER: &str = "Assistant identity anchor:";
static QUOTED_VALUE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"["“”'‘’「」『』]([^"“”'‘’「」『』]+)["“”'‘’「」『』]"#)
        .unwrap_or_else(|err| panic!("valid regex: {err}"))
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

    let existing_agent_memory = if capture.agent_name.is_some()
        || capture.agent_identity.is_some()
        || !capture.collaboration_style_clauses.is_empty()
    {
        read_canonical_memory(
            &zmemory_context,
            turn_context,
            StablePreferenceMemory::AgentSelfReference,
        )
        .ok()
        .flatten()
    } else {
        None
    };
    let existing_user_memory = if capture.user_address.is_some() || capture.user_identity.is_some()
    {
        read_canonical_memory(
            &zmemory_context,
            turn_context,
            StablePreferenceMemory::UserAddressPreference,
        )
        .ok()
        .flatten()
    } else {
        None
    };
    if capture.agent_name.is_none()
        && let Some(content) = existing_agent_memory
            .as_ref()
            .and_then(CanonicalMemoryRead::usable_content)
    {
        capture.agent_name = parse_name_from_memory_content(content);
    }
    if capture.user_address.is_none()
        && let Some(content) = existing_user_memory
            .as_ref()
            .and_then(CanonicalMemoryRead::usable_content)
    {
        capture.user_address = parse_name_from_memory_content(content);
    }
    let existing_contract = read_canonical_memory(
        &zmemory_context,
        turn_context,
        StablePreferenceMemory::CollaborationAddressContract,
    )
    .ok()
    .flatten();

    let writes = capture.into_writes(
        existing_user_memory
            .as_ref()
            .and_then(CanonicalMemoryRead::usable_content),
        existing_contract
            .as_ref()
            .and_then(CanonicalMemoryRead::usable_content),
        existing_agent_memory
            .as_ref()
            .and_then(CanonicalMemoryRead::usable_content),
    );
    for (memory, content) in writes {
        if let Err(err) =
            write_and_verify_canonical_memory(&zmemory_context, turn_context, memory, &content)
        {
            emit_capture_warning(session, turn_context, err.to_string()).await;
        }
    }
}

pub(crate) async fn build_stable_preference_recall_note(
    session: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
    items: &[UserInput],
) -> Option<String> {
    let texts = items
        .iter()
        .filter_map(|item| match item {
            UserInput::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    build_stable_preference_recall_note_from_texts(session, turn_context, texts.as_slice()).await
}

pub(crate) async fn build_stable_preference_recall_note_from_texts(
    session: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
    texts: &[&str],
) -> Option<String> {
    let recall_targets = recall_targets_for_texts(texts);
    if recall_targets.is_empty() {
        return None;
    }

    let zmemory_context = resolve_zmemory_context_for_turn(session, turn_context).await;
    if inspect_workspace_runtime(&zmemory_context, turn_context).is_err() {
        return None;
    }

    let mut recalled = Vec::new();
    for memory in recall_targets {
        let Ok(Some(content)) = read_canonical_content(&zmemory_context, turn_context, memory)
        else {
            continue;
        };
        recalled.push(format_recalled_memory(memory.uri(), &content));
    }

    (!recalled.is_empty()).then(|| {
        format!(
            "## Zmemory Recall\nUse these canonical long-term memories silently for this turn:\n{}",
            recalled.join("\n")
        )
    })
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
    let should_reload = session_config.cwd.as_path() != turn_context.cwd.as_path() && {
        let mut origins = session_config.config_layer_stack.origins();
        [
            "zmemory.path",
            "zmemory.valid_domains",
            "zmemory.core_memory_uris",
            "zmemory.namespace",
        ]
        .into_iter()
        .any(|key| {
            matches!(
                origins.remove(key).map(|metadata| metadata.name),
                None | Some(ConfigLayerSource::Project { .. })
            )
        })
    };
    let codex_home = turn_context.config.codex_home.clone().to_path_buf();

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
    let existing_content = read_canonical_memory(zmemory_context, turn_context, memory)?;
    if existing_content
        .as_ref()
        .map(|memory| memory.raw_content.as_str())
        == Some(content)
    {
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

    let verified_content = read_canonical_memory(zmemory_context, turn_context, memory)?;
    anyhow::ensure!(
        verified_content
            .as_ref()
            .and_then(CanonicalMemoryRead::usable_content)
            == Some(content),
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
    Ok(
        read_canonical_memory(zmemory_context, turn_context, memory)?
            .and_then(|memory| memory.usable_content().map(str::to_owned)),
    )
}

fn read_canonical_memory(
    zmemory_context: &ResolvedZmemoryContext,
    turn_context: &TurnContext,
    memory: StablePreferenceMemory,
) -> anyhow::Result<Option<CanonicalMemoryRead>> {
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

#[derive(Debug, Clone)]
struct CanonicalMemoryRead {
    raw_content: String,
    usable_content: Option<String>,
}

impl CanonicalMemoryRead {
    fn usable_content(&self) -> Option<&str> {
        self.usable_content.as_deref()
    }
}

fn read_content_from_tool_result(payload: &Value) -> Option<CanonicalMemoryRead> {
    let result = payload.get("result")?;
    let raw_content = result.get("content")?.as_str()?.to_owned();
    let usable_content = match result
        .get("governance")
        .and_then(|governance| governance.get("status"))
        .and_then(Value::as_str)
    {
        Some("conflict") => None,
        Some(_) => result
            .get("governance")
            .and_then(|governance| governance.get("governedContent"))
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| Some(raw_content.clone())),
        None => Some(raw_content.clone()),
    };
    Some(CanonicalMemoryRead {
        raw_content,
        usable_content,
    })
}

fn format_recalled_memory(uri: &str, content: &str) -> String {
    let mut lines = content.lines();
    let Some(first_line) = lines.next() else {
        return format!("- `{uri}`:");
    };
    let remaining = lines.collect::<Vec<_>>();
    if remaining.is_empty() {
        format!("- `{uri}`: {first_line}")
    } else {
        let indented = std::iter::once(first_line)
            .chain(remaining)
            .map(|line| format!("  {line}"))
            .collect::<Vec<_>>()
            .join("\n");
        format!("- `{uri}`:\n{indented}")
    }
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
    user_identity: Option<String>,
    agent_name: Option<String>,
    agent_identity: Option<String>,
    collaboration_style_clauses: Vec<String>,
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
            if let Some(user_identity) = extract_explicit_clause_value(text, USER_IDENTITY_PATTERNS)
            {
                capture.user_identity = Some(user_identity);
            }
            if let Some(agent_name) = extract_explicit_value(text, AGENT_NAME_PATTERNS) {
                capture.agent_name = Some(agent_name);
            }
            if let Some(agent_identity) =
                extract_explicit_clause_value(text, AGENT_IDENTITY_PATTERNS)
            {
                capture.agent_identity = Some(agent_identity);
            }
            for clause in extract_collaboration_style_clauses(text) {
                if !capture.collaboration_style_clauses.contains(&clause) {
                    capture.collaboration_style_clauses.push(clause);
                }
            }
        }

        if capture.user_address.is_some()
            || capture.user_identity.is_some()
            || capture.agent_name.is_some()
            || capture.agent_identity.is_some()
            || !capture.collaboration_style_clauses.is_empty()
        {
            Some(capture)
        } else {
            None
        }
    }

    fn into_writes(
        self,
        existing_user_anchor: Option<&str>,
        existing_contract: Option<&str>,
        existing_agent_anchor: Option<&str>,
    ) -> Vec<(StablePreferenceMemory, String)> {
        let mut writes = Vec::new();
        let mut has_agent_anchor = existing_agent_anchor.is_some();
        let mut user_clauses = Vec::new();
        if let Some(user_address) = self.user_address.as_ref() {
            user_clauses.push(format!(
                "The user prefers to be addressed as \"{user_address}\"."
            ));
        }
        if let Some(user_identity) = self.user_identity.as_ref() {
            user_clauses.push(format!(
                "The user's stable identity is {}.",
                normalize_identity_value(user_identity)
            ));
        }
        if let Some(user_content) =
            merge_identity_anchor_content(existing_user_anchor, USER_IDENTITY_HEADER, &user_clauses)
        {
            writes.push((StablePreferenceMemory::UserAddressPreference, user_content));
        }
        let mut agent_clauses = Vec::new();
        if let Some(agent_name) = self.agent_name.as_ref() {
            agent_clauses.push(format!(
                "The assistant should refer to itself as \"{agent_name}\"."
            ));
        }
        if let Some(agent_identity) = self.agent_identity.as_ref() {
            agent_clauses.push(format!(
                "The assistant's stable identity is {}.",
                normalize_identity_value(agent_identity)
            ));
        }
        if let Some(agent_content) = merge_identity_anchor_content(
            existing_agent_anchor,
            AGENT_IDENTITY_HEADER,
            &agent_clauses,
        ) {
            writes.push((StablePreferenceMemory::AgentSelfReference, agent_content));
            has_agent_anchor = true;
        }
        let mut contract_clauses = self.collaboration_style_clauses;
        if !has_agent_anchor && !contract_clauses.is_empty() {
            writes.push((
                StablePreferenceMemory::AgentSelfReference,
                COLLABORATION_AGENT_ANCHOR_CONTENT.to_string(),
            ));
            has_agent_anchor = true;
        }
        if let (Some(user_address), Some(agent_name)) = (self.user_address, self.agent_name) {
            contract_clauses.insert(
                0,
                format!(
                    "Use \"{agent_name}\" for the assistant and \"{user_address}\" for the user in future interactions."
                ),
            );
        }
        if has_agent_anchor
            && let Some(contract_content) =
                merge_contract_content(existing_contract, contract_clauses.as_slice())
        {
            writes.push((
                StablePreferenceMemory::CollaborationAddressContract,
                contract_content,
            ));
        }
        writes
    }
}

fn merge_identity_anchor_content(
    existing_anchor: Option<&str>,
    header: &str,
    new_clauses: &[String],
) -> Option<String> {
    let mut clauses = extract_identity_anchor_clauses(existing_anchor.unwrap_or(""), header);
    for clause in new_clauses {
        if !clauses.contains(clause) {
            clauses.push(clause.clone());
        }
    }

    format_identity_anchor_content(header, clauses)
}

fn extract_identity_anchor_clauses(content: &str, header: &str) -> Vec<String> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    if let Some(rest) = trimmed.strip_prefix(header) {
        return rest
            .lines()
            .map(str::trim)
            .filter_map(|line| line.strip_prefix("- "))
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect();
    }

    trimmed
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

fn format_identity_anchor_content(header: &str, clauses: Vec<String>) -> Option<String> {
    match clauses.len() {
        0 => None,
        1 => clauses.into_iter().next(),
        _ => Some(format!(
            "{header}\n{}",
            clauses
                .into_iter()
                .map(|clause| format!("- {clause}"))
                .collect::<Vec<_>>()
                .join("\n")
        )),
    }
}

fn recall_targets_for_texts(texts: &[&str]) -> Vec<StablePreferenceMemory> {
    let mut targets = Vec::new();
    for text in texts {
        let lowercase = text.to_lowercase();
        if USER_ADDRESS_PATTERNS
            .iter()
            .any(|pattern| lowercase.contains(&pattern.to_lowercase()))
            && !targets.contains(&StablePreferenceMemory::UserAddressPreference)
        {
            targets.push(StablePreferenceMemory::UserAddressPreference);
        }
        if AGENT_NAME_PATTERNS
            .iter()
            .any(|pattern| lowercase.contains(&pattern.to_lowercase()))
            && !targets.contains(&StablePreferenceMemory::AgentSelfReference)
        {
            targets.push(StablePreferenceMemory::AgentSelfReference);
        }
        if (AGENT_IDENTITY_PATTERNS
            .iter()
            .any(|pattern| lowercase.contains(&pattern.to_lowercase()))
            || AGENT_IDENTITY_REFERENCE_PATTERNS
                .iter()
                .any(|pattern| lowercase.contains(&pattern.to_lowercase()))
            || USER_IDENTITY_PATTERNS
                .iter()
                .any(|pattern| lowercase.contains(&pattern.to_lowercase())))
            && !targets.contains(&StablePreferenceMemory::AgentSelfReference)
        {
            targets.push(StablePreferenceMemory::AgentSelfReference);
        }
        if USER_IDENTITY_PATTERNS
            .iter()
            .any(|pattern| lowercase.contains(&pattern.to_lowercase()))
            && !targets.contains(&StablePreferenceMemory::UserAddressPreference)
        {
            targets.push(StablePreferenceMemory::UserAddressPreference);
        }
        let has_collaboration_clauses = !extract_collaboration_style_clauses(text).is_empty();
        let continues_previous_style =
            contains_any_pattern(text, COLLABORATION_CONTINUATION_PATTERNS);
        let continues_identity = contains_any_pattern(text, IDENTITY_CONTINUATION_PATTERNS);
        if has_collaboration_clauses || continues_previous_style || continues_identity {
            if continues_previous_style
                && !targets.contains(&StablePreferenceMemory::UserAddressPreference)
            {
                targets.push(StablePreferenceMemory::UserAddressPreference);
            }
            if continues_identity && !targets.contains(&StablePreferenceMemory::AgentSelfReference)
            {
                targets.push(StablePreferenceMemory::AgentSelfReference);
            }
            if !targets.contains(&StablePreferenceMemory::CollaborationAddressContract) {
                targets.push(StablePreferenceMemory::CollaborationAddressContract);
            }
            if !targets.contains(&StablePreferenceMemory::AgentSelfReference) {
                targets.push(StablePreferenceMemory::AgentSelfReference);
            }
        }
    }
    targets
}

fn extract_collaboration_style_clauses(text: &str) -> Vec<String> {
    if !contains_any_pattern(text, DURABLE_PREFERENCE_PATTERNS) {
        return Vec::new();
    }

    let mut clauses = Vec::new();
    if contains_any_pattern(text, CHINESE_RESPONSE_PATTERNS) {
        clauses.push("Respond in Chinese by default.".to_string());
    }
    if contains_any_pattern(text, CONCISE_RESPONSE_PATTERNS) {
        clauses.push("Keep responses concise by default.".to_string());
    }
    clauses
}

fn contains_any_pattern(text: &str, patterns: &[&str]) -> bool {
    let lowercase = text.to_lowercase();
    patterns
        .iter()
        .any(|pattern| lowercase.contains(&pattern.to_lowercase()))
}

fn merge_contract_content(existing_contract: Option<&str>, clauses: &[String]) -> Option<String> {
    let mut existing_clauses = extract_contract_clauses(existing_contract.unwrap_or(""));
    for clause in clauses {
        if existing_clauses.contains(clause) {
            continue;
        }
        existing_clauses.push(clause.clone());
    }

    format_contract_clauses(existing_clauses)
}

fn extract_contract_clauses(content: &str) -> Vec<String> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    if let Some(rest) = trimmed.strip_prefix(COLLABORATION_CONTRACT_HEADER) {
        return rest
            .lines()
            .map(str::trim)
            .filter_map(|line| line.strip_prefix("- "))
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect();
    }

    trimmed
        .split('.')
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| format!("{line}."))
        .collect()
}

fn format_contract_clauses(clauses: Vec<String>) -> Option<String> {
    if clauses.is_empty() {
        return None;
    }

    Some(format!(
        "{COLLABORATION_CONTRACT_HEADER}\n{}",
        clauses
            .into_iter()
            .map(|clause| format!("- {clause}"))
            .collect::<Vec<_>>()
            .join("\n")
    ))
}

fn extract_explicit_value(text: &str, patterns: &[&str]) -> Option<String> {
    patterns.iter().find_map(|pattern| {
        let suffix = find_case_insensitive_suffix(text, pattern)?;
        parse_explicit_name_value(suffix)
    })
}

fn extract_explicit_clause_value(text: &str, patterns: &[&str]) -> Option<String> {
    patterns.iter().find_map(|pattern| {
        let suffix = find_case_insensitive_suffix(text, pattern)?;
        parse_explicit_clause_value(suffix)
    })
}

fn find_case_insensitive_suffix<'a>(text: &'a str, pattern: &str) -> Option<&'a str> {
    if pattern.is_empty() {
        return Some(text);
    }

    text.char_indices()
        .map(|(idx, _)| idx)
        .chain(std::iter::once(text.len()))
        .find_map(|start| {
            match_case_insensitive_prefix(&text[start..], pattern)
                .map(|matched_bytes| &text[start + matched_bytes..])
        })
}

fn match_case_insensitive_prefix(source: &str, pattern: &str) -> Option<usize> {
    let mut matched_bytes = 0;
    let mut source_chars = source.chars();

    for pattern_char in pattern.chars() {
        let source_char = source_chars.next()?;
        if !chars_equal_case_insensitive(source_char, pattern_char) {
            return None;
        }
        matched_bytes += source_char.len_utf8();
    }

    Some(matched_bytes)
}

fn chars_equal_case_insensitive(left: char, right: char) -> bool {
    left.to_lowercase().collect::<String>() == right.to_lowercase().collect::<String>()
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

fn parse_explicit_clause_value(raw: &str) -> Option<String> {
    let raw = raw.trim_start_matches(|c: char| {
        c.is_whitespace() || matches!(c, '：' | ':' | '，' | ',' | '=')
    });
    if raw.is_empty() {
        return None;
    }

    if let Some(captures) = QUOTED_VALUE_RE.captures(raw) {
        let value = captures.get(1)?.as_str().trim();
        return is_valid_identity_clause(value).then(|| value.to_string());
    }

    let clause = raw
        .chars()
        .take_while(|c| {
            !matches!(
                *c,
                '，' | ',' | '。' | '.' | '！' | '!' | '？' | '?' | '；' | ';' | '\n'
            )
        })
        .collect::<String>()
        .trim()
        .to_string();
    is_valid_identity_clause(&clause).then_some(clause)
}

fn is_valid_identity_clause(value: &str) -> bool {
    if value.is_empty() {
        return false;
    }

    let lowercase = value.to_lowercase();
    if value.contains('?')
        || value.contains('？')
        || [
            "什么",
            "怎么",
            "为何",
            "为什么",
            "谁",
            "哪",
            "几",
            "how ",
            "what ",
            "why ",
            "who ",
            "when ",
            "where ",
            "which ",
        ]
        .iter()
        .any(|pattern| lowercase.starts_with(pattern))
        || ["对的", "错的", "right", "wrong"].contains(&lowercase.as_str())
    {
        return false;
    }

    true
}

fn normalize_identity_value(value: &str) -> String {
    value
        .trim()
        .trim_end_matches(['。', '.', '！', '!', '？', '?', '；', ';'])
        .trim()
        .to_string()
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
    use super::AGENT_IDENTITY_HEADER;
    use super::AGENT_IDENTITY_PATTERNS;
    use super::StablePreferenceCapture;
    use super::StablePreferenceMemory;
    use super::USER_IDENTITY_HEADER;
    use super::extract_collaboration_style_clauses;
    use super::extract_contract_clauses;
    use super::extract_explicit_clause_value;
    use super::extract_explicit_value;
    use super::extract_identity_anchor_clauses;
    use super::find_case_insensitive_suffix;
    use super::format_contract_clauses;
    use super::format_identity_anchor_content;
    use super::format_recalled_memory;
    use super::merge_contract_content;
    use super::merge_identity_anchor_content;
    use super::normalize_identity_value;
    use super::parse_name_from_memory_content;
    use super::read_content_from_tool_result;
    use super::recall_targets_for_texts;
    use codex_protocol::user_input::UserInput;
    use pretty_assertions::assert_eq;
    use serde_json::json;

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
                user_identity: None,
                agent_name: Some("小白".to_string()),
                agent_identity: None,
                collaboration_style_clauses: Vec::new(),
            })
        );
    }

    #[test]
    fn detects_durable_collaboration_style_preferences() {
        let capture = StablePreferenceCapture::from_items(&[UserInput::Text {
            text: "以后默认用中文回答，尽量简洁一点。".to_string(),
            text_elements: Vec::new(),
        }]);

        assert_eq!(
            capture,
            Some(StablePreferenceCapture {
                user_address: None,
                user_identity: None,
                agent_name: None,
                agent_identity: None,
                collaboration_style_clauses: vec![
                    "Respond in Chinese by default.".to_string(),
                    "Keep responses concise by default.".to_string(),
                ],
            })
        );
    }

    #[test]
    fn detects_preferences_from_mixed_image_and_text_inputs() {
        let capture = StablePreferenceCapture::from_items(&[
            UserInput::Image {
                image_url: "https://example.com/pref.png".to_string(),
            },
            UserInput::Text {
                text: "以后叫我指挥官。".to_string(),
                text_elements: Vec::new(),
            },
        ]);

        assert_eq!(
            capture,
            Some(StablePreferenceCapture {
                user_address: Some("指挥官".to_string()),
                user_identity: None,
                agent_name: None,
                agent_identity: None,
                collaboration_style_clauses: Vec::new(),
            })
        );
    }

    #[test]
    fn detects_rich_agent_identity_from_single_text_item() {
        let capture = StablePreferenceCapture::from_items(&[UserInput::Text {
            text: "你是专业的架构师，后续按这个身份来。".to_string(),
            text_elements: Vec::new(),
        }]);

        assert_eq!(
            capture,
            Some(StablePreferenceCapture {
                user_address: None,
                user_identity: None,
                agent_name: None,
                agent_identity: Some("专业的架构师".to_string()),
                collaboration_style_clauses: Vec::new(),
            })
        );
    }

    #[test]
    fn detects_english_call_me_pattern() {
        let value = extract_explicit_value("From now on, call me Commander.", &["call me"]);
        assert_eq!(value.as_deref(), Some("Commander"));
    }

    #[test]
    fn detects_english_call_me_pattern_from_single_quotes() {
        let value = extract_explicit_value("From now on, call me 'Commander'.", &["call me"]);
        assert_eq!(value.as_deref(), Some("Commander"));
    }

    #[test]
    fn finds_case_insensitive_suffix_without_unicode_offset_mismatch() {
        let suffix = find_case_insensitive_suffix("İ以后叫我“小王”", "叫我");
        assert_eq!(suffix, Some("“小王”"));
    }

    #[test]
    fn parses_existing_memory_content_back_to_name() {
        let value =
            parse_name_from_memory_content("The assistant should refer to itself as \"小白\".");
        assert_eq!(value.as_deref(), Some("小白"));
    }

    #[test]
    fn parses_existing_memory_content_back_to_name_from_curly_single_quotes() {
        let value =
            parse_name_from_memory_content("The assistant should refer to itself as ‘小白’.");
        assert_eq!(value.as_deref(), Some("小白"));
    }

    #[test]
    fn extracts_agent_identity_clause_without_swallowing_followup_instruction() {
        let value = extract_explicit_clause_value(
            "你现在是专业的架构师，之后继续按这个身份来。",
            AGENT_IDENTITY_PATTERNS,
        );
        assert_eq!(value.as_deref(), Some("专业的架构师"));
    }

    #[test]
    fn rejects_question_like_agent_identity_clause() {
        let value = extract_explicit_clause_value("你是怎么做到的？", AGENT_IDENTITY_PATTERNS);
        assert_eq!(value, None);
    }

    #[test]
    fn ignores_one_off_style_instructions_without_durable_marker() {
        let clauses = extract_collaboration_style_clauses("这次请用中文回答，简洁一点。");
        assert!(clauses.is_empty());
    }

    #[test]
    fn merges_new_style_clause_into_existing_contract() {
        let merged = merge_contract_content(
            Some(
                "Shared collaboration contract:\n- Use \"小白\" for the assistant and \"指挥官\" for the user in future interactions.",
            ),
            &["Keep responses concise by default.".to_string()],
        );
        assert_eq!(
            merged.as_deref(),
            Some(
                "Shared collaboration contract:\n- Use \"小白\" for the assistant and \"指挥官\" for the user in future interactions.\n- Keep responses concise by default."
            )
        );
    }

    #[test]
    fn merges_rich_agent_identity_into_existing_name_anchor() {
        let merged = merge_identity_anchor_content(
            Some("The assistant should refer to itself as \"小白\"."),
            AGENT_IDENTITY_HEADER,
            &[String::from(
                "The assistant's stable identity is 专业的架构师.",
            )],
        );
        assert_eq!(
            merged.as_deref(),
            Some(
                "Assistant identity anchor:\n- The assistant should refer to itself as \"小白\".\n- The assistant's stable identity is 专业的架构师."
            )
        );
    }

    #[test]
    fn extracts_identity_anchor_clauses_from_structured_block() {
        let clauses = extract_identity_anchor_clauses(
            "Assistant identity anchor:\n- The assistant should refer to itself as \"小白\".\n- The assistant's stable identity is 专业的架构师.",
            AGENT_IDENTITY_HEADER,
        );
        assert_eq!(
            clauses,
            vec![
                "The assistant should refer to itself as \"小白\".".to_string(),
                "The assistant's stable identity is 专业的架构师.".to_string(),
            ]
        );
    }

    #[test]
    fn formats_multi_clause_identity_anchor_as_structured_block() {
        let content = format_identity_anchor_content(
            USER_IDENTITY_HEADER,
            vec![
                "The user prefers to be addressed as \"指挥官\".".to_string(),
                "The user's stable identity is 后端负责人.".to_string(),
            ],
        );
        assert_eq!(
            content.as_deref(),
            Some(
                "User identity anchor:\n- The user prefers to be addressed as \"指挥官\".\n- The user's stable identity is 后端负责人."
            )
        );
    }

    #[test]
    fn normalizes_identity_value_before_persisting() {
        assert_eq!(normalize_identity_value("专业的架构师。"), "专业的架构师");
    }

    #[test]
    fn extracts_contract_clauses_from_legacy_sentence_format() {
        let clauses = extract_contract_clauses(
            "Use \"小白\" for the assistant and \"指挥官\" for the user in future interactions. Keep responses concise by default.",
        );
        assert_eq!(
            clauses,
            vec![
                "Use \"小白\" for the assistant and \"指挥官\" for the user in future interactions."
                    .to_string(),
                "Keep responses concise by default.".to_string(),
            ]
        );
    }

    #[test]
    fn extracts_contract_clauses_from_structured_block() {
        let clauses = extract_contract_clauses(
            "Shared collaboration contract:\n- Use \"小白\" for the assistant and \"指挥官\" for the user in future interactions.\n- Keep responses concise by default.",
        );
        assert_eq!(
            clauses,
            vec![
                "Use \"小白\" for the assistant and \"指挥官\" for the user in future interactions."
                    .to_string(),
                "Keep responses concise by default.".to_string(),
            ]
        );
    }
    #[test]
    fn formats_contract_clauses_as_structured_block() {
        let content = format_contract_clauses(vec![
            "Respond in Chinese by default.".to_string(),
            "Keep responses concise by default.".to_string(),
        ]);
        assert_eq!(
            content.as_deref(),
            Some(
                "Shared collaboration contract:\n- Respond in Chinese by default.\n- Keep responses concise by default."
            )
        );
    }

    #[test]
    fn recall_targets_include_contract_for_durable_collaboration_request() {
        let targets = recall_targets_for_texts(&["以后默认用中文回答，尽量简洁一点。"]);

        assert_eq!(
            targets,
            vec![
                StablePreferenceMemory::CollaborationAddressContract,
                StablePreferenceMemory::AgentSelfReference,
            ]
        );
    }

    #[test]
    fn canonical_read_prefers_governed_content_for_non_conflicts() {
        let memory = read_content_from_tool_result(&json!({
            "result": {
                "content": "你的名字是“星尘”。以后都用这个名字。",
                "governance": {
                    "status": "normalized",
                    "governedContent": "The assistant should refer to itself as \"星尘\"."
                }
            }
        }))
        .expect("memory should parse");

        assert_eq!(memory.raw_content, "你的名字是“星尘”。以后都用这个名字。");
        assert_eq!(
            memory.usable_content(),
            Some("The assistant should refer to itself as \"星尘\".")
        );
    }

    #[test]
    fn canonical_read_excludes_conflicting_content_from_recall() {
        let memory = read_content_from_tool_result(&json!({
            "result": {
                "content": "Shared collaboration contract:\n- Respond in Chinese by default.\n- Respond in English by default.",
                "governance": {
                    "status": "conflict",
                    "governedContent": "Shared collaboration contract:\n- Respond in Chinese by default.\n- Respond in English by default."
                }
            }
        }))
        .expect("memory should parse");

        assert_eq!(
            memory.raw_content,
            "Shared collaboration contract:\n- Respond in Chinese by default.\n- Respond in English by default."
        );
        assert_eq!(memory.usable_content(), None);
    }

    #[test]
    fn canonical_read_falls_back_to_raw_content_when_governance_is_missing() {
        let memory = read_content_from_tool_result(&json!({
            "result": {
                "content": "The user prefers to be addressed as \"指挥官\"."
            }
        }))
        .expect("memory should parse");

        assert_eq!(
            memory.usable_content(),
            Some("The user prefers to be addressed as \"指挥官\".")
        );
    }

    #[test]
    fn formats_multiline_recalled_memory_with_indented_block() {
        let recalled = format_recalled_memory(
            "core://agent/my_user",
            "Shared collaboration contract:
- Respond in Chinese by default.
- Keep responses concise by default.",
        );

        assert_eq!(
            recalled,
            "- `core://agent/my_user`:
  Shared collaboration contract:
  - Respond in Chinese by default.
  - Keep responses concise by default."
        );
    }

    #[test]
    fn recall_targets_include_identity_layer_for_continue_previous_style_request() {
        let targets = recall_targets_for_texts(&["之后继续按上次方式来。"]);

        assert_eq!(
            targets,
            vec![
                StablePreferenceMemory::UserAddressPreference,
                StablePreferenceMemory::CollaborationAddressContract,
                StablePreferenceMemory::AgentSelfReference,
            ]
        );
    }

    #[test]
    fn recall_targets_include_agent_identity_for_explicit_identity_reference() {
        let targets = recall_targets_for_texts(&["按你专业架构师这个身份继续分析。"]);

        assert_eq!(targets, vec![StablePreferenceMemory::AgentSelfReference]);
    }

    #[test]
    fn recall_targets_include_identity_layer_for_continue_current_role_request() {
        let targets = recall_targets_for_texts(&["后面继续按这个身份来。"]);

        assert_eq!(
            targets,
            vec![
                StablePreferenceMemory::AgentSelfReference,
                StablePreferenceMemory::CollaborationAddressContract,
            ]
        );
    }
}
