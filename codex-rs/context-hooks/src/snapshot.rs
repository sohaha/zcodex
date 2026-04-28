use anyhow::Result;
use codex_utils_output_truncation::TruncationPolicy;
use codex_utils_output_truncation::formatted_truncate_text;
use codex_zmemory::tool_api::ZmemoryToolAction;
use codex_zmemory::tool_api::ZmemoryToolCallParam;
use codex_zmemory::tool_api::run_zmemory_tool_with_context;
use serde_json::Value;

use crate::ContextHooksSettings;
use crate::ZmemoryContext;

pub fn build_session_snapshot(
    context: &ZmemoryContext,
    session_id: &str,
    settings: &ContextHooksSettings,
) -> Result<Option<String>> {
    let args = ZmemoryToolCallParam {
        action: ZmemoryToolAction::Export,
        codex_home: None,
        uri: None,
        parent_uri: None,
        new_uri: None,
        target_uri: None,
        query: None,
        domain: Some("session".to_string()),
        content: None,
        title: None,
        old_string: None,
        new_string: None,
        append: None,
        priority: None,
        disclosure: None,
        add: None,
        remove: None,
        items: None,
        limit: None,
        audit_action: None,
    };
    let result = match run_zmemory_tool_with_context(
        context.codex_home(),
        context.cwd(),
        context.zmemory_path.as_deref(),
        Some(context.settings.clone()),
        args,
    ) {
        Ok(result) => result,
        Err(err) if err.to_string().contains("memory not found") => return Ok(None),
        Err(err) if err.to_string().contains("domain is not readable") => return Ok(None),
        Err(err) => return Err(err),
    };
    let events = extract_session_events(&result.structured_content, session_id);
    if events.is_empty() {
        return Ok(None);
    }

    let total = events.len();
    let selected = select_events(events, settings.max_events_per_snapshot);
    let mut snapshot = format!(
        "# Session Snapshot\n\n\
         **Session**: {session_id}\n\
         **Events included**: {included}/{total}\n\
         **Source**: built-in context hooks\n\n",
        included = selected.len(),
    );
    append_group(&mut snapshot, "Recent Errors", &selected, "error");
    append_group(&mut snapshot, "File Edits", &selected, "file_edit");
    append_group(&mut snapshot, "Git Operations", &selected, "git");
    append_group(&mut snapshot, "Other Tool Events", &selected, "other");
    snapshot.push_str(
        "\nUse `zmemory` search scoped to `session://<session-id>` for complete recorded history.\n",
    );

    Ok(Some(formatted_truncate_text(
        &snapshot,
        TruncationPolicy::Tokens(settings.snapshot_token_budget),
    )))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SnapshotEvent {
    uri: String,
    category: String,
    title: String,
    summary: String,
    priority: i64,
}

fn extract_session_events(payload: &Value, session_id: &str) -> Vec<SnapshotEvent> {
    let prefix = format!("session://{session_id}/events/");
    payload
        .pointer("/result/items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let uri = item.get("uri")?.as_str()?.to_string();
            if !uri.starts_with(&prefix) {
                return None;
            }
            let content = item.get("content")?.as_str()?.to_string();
            let category = metadata_value(&content, "Category").unwrap_or_else(|| "other".into());
            let title = content
                .lines()
                .find_map(|line| line.strip_prefix("# "))
                .unwrap_or("Tool Execution")
                .to_string();
            let priority = item.get("priority").and_then(Value::as_i64).unwrap_or(4);
            Some(SnapshotEvent {
                uri,
                category,
                title,
                summary: summarize_content(&content),
                priority,
            })
        })
        .collect()
}

fn select_events(mut events: Vec<SnapshotEvent>, limit: usize) -> Vec<SnapshotEvent> {
    events.sort_by(|left, right| {
        left.priority
            .cmp(&right.priority)
            .then_with(|| right.uri.cmp(&left.uri))
    });
    events.truncate(limit);
    events
}

fn append_group(snapshot: &mut String, title: &str, events: &[SnapshotEvent], category: &str) {
    let group = events
        .iter()
        .filter(|event| {
            if category == "other" {
                !matches!(event.category.as_str(), "error" | "file_edit" | "git")
            } else {
                event.category == category
            }
        })
        .collect::<Vec<_>>();
    if group.is_empty() {
        return;
    }
    snapshot.push_str("## ");
    snapshot.push_str(title);
    snapshot.push('\n');
    for event in group {
        snapshot.push_str("- ");
        snapshot.push_str(&event.title);
        snapshot.push_str(": ");
        snapshot.push_str(&event.summary);
        snapshot.push('\n');
    }
    snapshot.push('\n');
}

fn metadata_value(content: &str, key: &str) -> Option<String> {
    let needle = format!("**{key}**:");
    content.lines().find_map(|line| {
        line.strip_prefix(&needle)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

fn summarize_content(content: &str) -> String {
    content
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty() && !trimmed.starts_with('#') && !trimmed.starts_with("**")
        })
        .take(3)
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use super::extract_session_events;
    use super::select_events;

    #[test]
    fn extracts_only_matching_session_events() {
        let payload = json!({
            "result": {
                "items": [
                    {
                        "uri": "session://s1/events/t1/c1",
                        "priority": 1,
                        "content": "# Tool Execution: apply_patch\n\n**Category**: file_edit\n\n## Output\nchanged file"
                    },
                    {
                        "uri": "session://s2/events/t1/c1",
                        "priority": 0,
                        "content": "# Tool Execution: Bash\n\n**Category**: error"
                    }
                ]
            }
        });

        let events = extract_session_events(&payload, "s1");

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].category, "file_edit");
        assert_eq!(events[0].summary, "changed file");
    }

    #[test]
    fn selection_prioritizes_lower_priority() {
        let payload = json!({
            "result": {
                "items": [
                    {
                        "uri": "session://s1/events/t1/low",
                        "priority": 4,
                        "content": "# Tool Execution: Bash\n\n**Category**: tool\n\nlow"
                    },
                    {
                        "uri": "session://s1/events/t1/high",
                        "priority": 0,
                        "content": "# Tool Execution: Bash\n\n**Category**: error\n\nhigh"
                    }
                ]
            }
        });
        let events = select_events(extract_session_events(&payload, "s1"), 1);

        assert_eq!(events[0].category, "error");
    }
}
