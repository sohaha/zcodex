use anyhow::Result;
use codex_utils_output_truncation::TruncationPolicy;
use codex_utils_output_truncation::formatted_truncate_text;
use codex_zmemory::tool_api::ZmemoryToolAction;
use codex_zmemory::tool_api::ZmemoryToolCallParam;
use codex_zmemory::tool_api::run_zmemory_tool_with_context;
use serde_json::Value;

use crate::ContextHooksSettings;
use crate::ZmemoryContext;

/// Default search query used to find session events for snapshot building.
const SNAPSHOT_SEARCH_QUERY: &str = "events";

pub fn build_session_snapshot(
    context: &ZmemoryContext,
    session_id: &str,
    settings: &ContextHooksSettings,
) -> Result<Option<String>> {
    let scope_uri = format!("session://{session_id}");
    let search_result =
        search_session_events(context, &scope_uri, settings.max_events_per_snapshot)?;
    let events = extract_session_events_from_search(&search_result.structured_content, session_id);
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

fn extract_session_events_from_search(payload: &Value, session_id: &str) -> Vec<SnapshotEvent> {
    let prefix = format!("session://{session_id}/events/");
    payload
        .pointer("/matches")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let uri = item.get("uri")?.as_str()?.to_string();
            if !uri.starts_with(&prefix) {
                return None;
            }
            let snippet = item
                .get("snippet")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let category = extract_category_from_snippet(&snippet)
                .unwrap_or("other")
                .to_string();
            let priority = item.get("priority").and_then(Value::as_i64).unwrap_or(4);
            let title = uri.rsplit('/').next().unwrap_or("event").to_string();
            Some(SnapshotEvent {
                uri,
                category,
                title,
                summary: snippet,
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

fn search_session_events(
    context: &ZmemoryContext,
    scope_uri: &str,
    limit: usize,
) -> Result<codex_zmemory::tool_api::ZmemoryToolResult> {
    let args = ZmemoryToolCallParam {
        action: ZmemoryToolAction::Search,
        codex_home: None,
        uri: Some(scope_uri.to_string()),
        parent_uri: None,
        new_uri: None,
        target_uri: None,
        query: Some(SNAPSHOT_SEARCH_QUERY.to_string()),
        domain: None,
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
        limit: Some(limit),
        audit_action: None,
    };
    run_zmemory_tool_with_context(
        context.codex_home(),
        context.cwd(),
        context.zmemory_path.as_deref(),
        Some(context.settings.clone()),
        args,
    )
}

fn extract_category_from_snippet(snippet: &str) -> Option<&str> {
    for line in snippet.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("**Category**:") {
            return Some(rest.trim());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use super::extract_session_events_from_search;
    use super::select_events;

    #[test]
    fn extracts_only_matching_session_events() {
        let payload = json!({
            "matches": [
                {
                    "uri": "session://s1/events/t1/c1",
                    "priority": 1,
                    "snippet": "**Category**: file_edit\n\napply_patch output"
                },
                {
                    "uri": "session://s2/events/t1/c1",
                    "priority": 0,
                    "snippet": "**Category**: error\n\nerror output"
                }
            ]
        });

        let events = extract_session_events_from_search(&payload, "s1");

        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].summary,
            "**Category**: file_edit\n\napply_patch output"
        );
        assert_eq!(events[0].category, "file_edit");
    }

    #[test]
    fn selection_prioritizes_lower_priority() {
        let payload = json!({
            "matches": [
                {
                    "uri": "session://s1/events/t1/low",
                    "priority": 4,
                    "snippet": "tool output"
                },
                {
                    "uri": "session://s1/events/t1/high",
                    "priority": 0,
                    "snippet": "**Category**: error\n\nerror output"
                }
            ]
        });
        let events = select_events(extract_session_events_from_search(&payload, "s1"), 1);

        assert_eq!(events[0].priority, 0);
    }
}
