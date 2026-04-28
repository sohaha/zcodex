use anyhow::Result;
use chrono::Utc;
use codex_hooks::PostToolUseRequest;
use codex_utils_output_truncation::TruncationPolicy;
use codex_utils_output_truncation::formatted_truncate_text;
use codex_zmemory::tool_api::ZmemoryToolAction;
use codex_zmemory::tool_api::ZmemoryToolCallParam;
use codex_zmemory::tool_api::run_zmemory_tool_with_context;
use serde_json::Value;

use crate::ZmemoryContext;

const MAX_STORED_FIELD_TOKENS: usize = 800;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventCategory {
    Error,
    FileEdit,
    Git,
    Command,
    Tool,
}

impl EventCategory {
    pub fn priority(self) -> i64 {
        match self {
            Self::Error => 0,
            Self::FileEdit => 1,
            Self::Git => 2,
            Self::Command => 3,
            Self::Tool => 4,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::FileEdit => "file_edit",
            Self::Git => "git",
            Self::Command => "command",
            Self::Tool => "tool",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextHookRecord {
    pub uri: String,
    pub content: String,
    pub category: EventCategory,
}

pub fn record_post_tool_use_event(
    context: &ZmemoryContext,
    request: &PostToolUseRequest,
) -> Result<()> {
    let record = build_post_tool_use_record(request)?;
    let args = ZmemoryToolCallParam {
        action: ZmemoryToolAction::Create,
        codex_home: None,
        uri: Some(record.uri),
        parent_uri: None,
        new_uri: None,
        target_uri: None,
        query: None,
        domain: None,
        content: Some(record.content),
        title: None,
        old_string: None,
        new_string: None,
        append: None,
        priority: Some(record.category.priority()),
        disclosure: Some(format!("context hook {}", record.category.as_str())),
        add: None,
        remove: None,
        items: None,
        limit: None,
        audit_action: None,
    };
    run_zmemory_tool_with_context(
        context.codex_home(),
        context.cwd(),
        context.zmemory_path.as_deref(),
        Some(context.settings.clone()),
        args,
    )?;
    Ok(())
}

pub fn build_post_tool_use_record(request: &PostToolUseRequest) -> Result<ContextHookRecord> {
    let category = classify_event(request);
    let uri = format!(
        "session://{}/events/{}/{}",
        sanitize_uri_segment(&request.session_id.to_string()),
        sanitize_uri_segment(&request.turn_id),
        sanitize_uri_segment(&request.tool_use_id)
    );
    let input = pretty_json(&request.tool_input)?;
    let response = pretty_json(&request.tool_response)?;
    let content = format!(
        "# Tool Execution: {tool}\n\n\
         **Session ID**: {session_id}\n\
         **Turn ID**: {turn_id}\n\
         **Call ID**: {call_id}\n\
         **Timestamp**: {timestamp}\n\
         **Category**: {category}\n\
         **Working directory**: {cwd}\n\n\
         ## Input\n{input}\n\n\
         ## Output\n{response}\n",
        tool = request.tool_name,
        session_id = request.session_id,
        turn_id = request.turn_id,
        call_id = request.tool_use_id,
        timestamp = Utc::now().to_rfc3339(),
        category = category.as_str(),
        cwd = request.cwd.display(),
        input = truncate_field(&input),
        response = truncate_field(&response),
    );

    Ok(ContextHookRecord {
        uri,
        content,
        category,
    })
}

fn classify_event(request: &PostToolUseRequest) -> EventCategory {
    if tool_response_failed(&request.tool_response) {
        return EventCategory::Error;
    }

    let tool_name = request.tool_name.to_lowercase();
    let input_text = request.tool_input.to_string().to_lowercase();
    if matches!(
        tool_name.as_str(),
        "apply_patch" | "edit" | "write" | "multi_edit"
    ) || input_text.contains("apply_patch")
    {
        return EventCategory::FileEdit;
    }

    if input_text.contains("git ") || input_text.contains("\"git\"") {
        return EventCategory::Git;
    }

    if matches!(tool_name.as_str(), "bash" | "shell" | "local_shell") {
        EventCategory::Command
    } else {
        EventCategory::Tool
    }
}

fn tool_response_failed(response: &Value) -> bool {
    response
        .get("success")
        .and_then(Value::as_bool)
        .is_some_and(|success| !success)
        || response.get("error").is_some()
        || response
            .get("exit_code")
            .and_then(Value::as_i64)
            .is_some_and(|code| code != 0)
}

fn pretty_json(value: &Value) -> Result<String> {
    serde_json::to_string_pretty(value).map_err(Into::into)
}

fn truncate_field(value: &str) -> String {
    formatted_truncate_text(value, TruncationPolicy::Tokens(MAX_STORED_FIELD_TOKENS))
}

fn sanitize_uri_segment(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

#[cfg(test)]
mod tests {
    use codex_hooks::PostToolUseRequest;
    use codex_protocol::ThreadId;
    use codex_utils_absolute_path::test_support::PathBufExt;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use super::EventCategory;
    use super::build_post_tool_use_record;

    #[test]
    fn post_tool_use_record_classifies_git_error() {
        let request = PostToolUseRequest {
            session_id: ThreadId::from_string("session-1".to_string()),
            turn_id: "turn-1".to_string(),
            cwd: "/workspace".to_abs_path_buf(),
            transcript_path: None,
            model: "gpt".to_string(),
            permission_mode: "default".to_string(),
            tool_name: "Bash".to_string(),
            matcher_aliases: Vec::new(),
            tool_use_id: "call-1".to_string(),
            tool_input: json!({"command": "git status"}),
            tool_response: json!({"success": false, "exit_code": 1}),
        };

        let record = build_post_tool_use_record(&request).expect("record");

        assert_eq!(record.category, EventCategory::Error);
        assert_eq!(record.uri, "session://session-1/events/turn-1/call-1");
        assert!(record.content.contains("**Category**: error"));
        assert!(record.content.contains("git status"));
    }
}
