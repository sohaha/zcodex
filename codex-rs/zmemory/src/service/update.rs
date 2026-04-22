use crate::config::ZmemoryConfig;
use crate::schema::mark_other_memories_deprecated;
use crate::service::common;
use crate::service::governance;
use crate::service::index;
use crate::tool_api::UpdateActionParams;
use anyhow::Result;
use anyhow::bail;
use rusqlite::Connection;
use rusqlite::params;
use serde_json::Value;
use serde_json::json;

pub(crate) fn update_action(
    config: &ZmemoryConfig,
    conn: &mut Connection,
    args: &UpdateActionParams,
) -> Result<Value> {
    let tx = conn.transaction()?;
    let result = update_action_in_tx(config, &tx, args)?;
    tx.commit()?;
    let document_count = common::search_document_count(conn, config)?;
    Ok(augment_update_result(result, document_count))
}

pub(crate) fn update_action_in_tx(
    config: &ZmemoryConfig,
    conn: &rusqlite::Transaction<'_>,
    args: &UpdateActionParams,
) -> Result<Value> {
    let uri = &args.uri;
    anyhow::ensure!(!uri.is_root(), "cannot update root path");
    common::ensure_writable_domain(config, conn, &uri.domain)?;
    let row = common::find_path_row(conn, config, uri)?
        .ok_or_else(|| anyhow::anyhow!("memory not found: {uri}"))?;
    let current_memory = common::read_active_memory(conn, config.namespace(), &row.node_uuid)?
        .ok_or_else(|| anyhow::anyhow!("active memory not found: {uri}"))?;

    let mut content_changed = false;
    let mut new_memory_id = None;
    let mut disclosure = row.disclosure.clone();
    let mut priority = row.priority;
    let content_governance = resolve_updated_content(args, &current_memory.content)?
        .map(|content| governance::evaluate_write_content(uri, &content))
        .transpose()?;
    let public_governance = content_governance
        .clone()
        .filter(|governance| governance.scope.is_some());
    let updated_content = content_governance
        .as_ref()
        .map(|governance| governance.governed_content.as_str());

    if let Some(content) = updated_content
        && content != current_memory.content
    {
        conn.execute(
            "INSERT INTO memories(namespace, node_uuid, content) VALUES (?1, ?2, ?3)",
            params![config.namespace(), row.node_uuid, content],
        )?;
        let replacement_id = conn.last_insert_rowid();
        mark_other_memories_deprecated(conn, config.namespace(), &row.node_uuid, replacement_id)?;
        new_memory_id = Some(replacement_id);
        content_changed = true;
    }

    if let Some(updated_priority) = args.priority {
        priority = updated_priority;
        conn.execute(
            "UPDATE edges SET priority = ?2 WHERE id = ?1 AND namespace = ?3",
            params![row.edge_id, updated_priority, config.namespace()],
        )?;
    }

    if let Some(updated_disclosure) = args.disclosure.clone() {
        disclosure = Some(updated_disclosure.clone());
        conn.execute(
            "UPDATE edges SET disclosure = ?2 WHERE id = ?1 AND namespace = ?3",
            params![row.edge_id, updated_disclosure, config.namespace()],
        )?;
    }

    if !content_changed && args.priority.is_none() && args.disclosure.is_none() {
        bail!("no changes requested");
    }

    common::insert_audit_log(
        conn,
        config.namespace(),
        "update",
        Some(&uri.to_string()),
        Some(&row.node_uuid),
        json!({
            "oldMemoryId": current_memory.id,
            "newMemoryId": new_memory_id,
            "contentChanged": content_changed,
            "priority": priority,
            "disclosure": disclosure,
            "contentGovernance": public_governance,
        }),
    )?;
    index::reindex_node(conn, config.namespace(), &row.node_uuid)?;
    Ok(json!({
        "uri": uri.to_string(),
        "nodeUuid": row.node_uuid,
        "oldMemoryId": current_memory.id,
        "newMemoryId": new_memory_id,
        "priority": priority,
        "disclosure": disclosure,
        "contentChanged": content_changed,
        "governance": public_governance,
    }))
}

fn augment_update_result(mut result: Value, document_count: i64) -> Value {
    result["documentCount"] = json!(document_count);
    result
}

fn resolve_updated_content(
    args: &UpdateActionParams,
    current_content: &str,
) -> Result<Option<String>> {
    let has_patch_fields = args.old_string.is_some() || args.new_string.is_some();
    let has_append = args.append.is_some();
    let has_content = args.content.is_some();

    anyhow::ensure!(
        !(has_content && (has_patch_fields || has_append)),
        "`content` cannot be combined with `oldString`/`newString`/`append`",
    );
    anyhow::ensure!(
        !(has_patch_fields && has_append),
        "`oldString`/`newString` cannot be combined with `append`",
    );

    match (
        args.content.as_deref(),
        args.old_string.as_deref(),
        args.new_string.as_deref(),
        args.append.as_deref(),
    ) {
        (Some(content), None, None, None) => Ok(Some(required_content(Some(content))?)),
        (None, Some(_), None, None) => {
            bail!("`newString` is required when `oldString` is provided")
        }
        (None, None, Some(_), None) => {
            bail!("`oldString` is required when `newString` is provided")
        }
        (None, Some(old_string), Some(new_string), None) => {
            patch_content(current_content, old_string, new_string).map(Some)
        }
        (None, None, None, Some(append)) => append_content(current_content, append).map(Some),
        (None, None, None, None) => Ok(None),
        _ => unreachable!("conflicting update modes should already be rejected"),
    }
}

fn required_content(content: Option<&str>) -> Result<String> {
    let content = content
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("`content` is required"))?;
    Ok(content.to_string())
}

fn patch_content(current_content: &str, old_string: &str, new_string: &str) -> Result<String> {
    anyhow::ensure!(!old_string.is_empty(), "`oldString` cannot be empty");
    anyhow::ensure!(
        old_string != new_string,
        "`oldString` and `newString` must differ"
    );
    let match_count = current_content.matches(old_string).count();
    anyhow::ensure!(
        match_count > 0,
        "`oldString` was not found in the current content"
    );
    anyhow::ensure!(
        match_count == 1,
        "`oldString` matched multiple locations; provide a more specific value",
    );
    Ok(current_content.replacen(old_string, new_string, 1))
}

fn append_content(current_content: &str, append: &str) -> Result<String> {
    anyhow::ensure!(!append.is_empty(), "`append` cannot be empty");
    Ok(format!("{current_content}{append}"))
}
