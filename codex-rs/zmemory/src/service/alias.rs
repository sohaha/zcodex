use crate::config::ZmemoryConfig;
use crate::service::common;
use crate::service::index;
use crate::tool_api::AddAliasActionParams;
use crate::tool_api::ManageTriggersActionParams;
use anyhow::Result;
use rusqlite::Connection;
use rusqlite::params;
use serde_json::Value;
use serde_json::json;

pub(crate) fn add_alias_action(
    config: &ZmemoryConfig,
    conn: &mut Connection,
    args: &AddAliasActionParams,
) -> Result<Value> {
    let new_uri = &args.new_uri;
    let target_uri = &args.target_uri;
    anyhow::ensure!(!new_uri.is_root(), "cannot alias root path");
    anyhow::ensure!(!target_uri.is_root(), "cannot alias the root node");
    common::ensure_writable_domain(config, conn, &new_uri.domain)?;
    common::ensure_readable_domain(config, conn, &target_uri.domain)?;
    anyhow::ensure!(
        common::find_path_row(conn, &new_uri)?.is_none(),
        "alias path already exists: {new_uri}"
    );

    let target = common::find_path_row(conn, &target_uri)?
        .ok_or_else(|| anyhow::anyhow!("target path does not exist: {target_uri}"))?;
    let parent_uri = new_uri.parent();
    let parent = if parent_uri.is_root() {
        common::PathRow::root()
    } else {
        common::find_path_row(conn, &parent_uri)?
            .ok_or_else(|| anyhow::anyhow!("parent path does not exist: {parent_uri}"))?
    };
    let priority = args.priority.unwrap_or(target.priority);
    let disclosure = args.disclosure.clone();
    let edge_name = new_uri.leaf_name()?;
    let existing_edge_id =
        common::find_edge_id(conn, &parent.node_uuid, &target.node_uuid, edge_name)?;

    let tx = conn.transaction()?;
    let edge_id = if let Some(edge_id) = existing_edge_id {
        edge_id
    } else {
        tx.execute(
            "INSERT INTO edges(parent_uuid, child_uuid, name, priority, disclosure) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![parent.node_uuid, target.node_uuid, edge_name, priority, disclosure],
        )?;
        tx.last_insert_rowid()
    };
    tx.execute(
        "INSERT INTO paths(domain, path, edge_id) VALUES (?1, ?2, ?3)",
        params![new_uri.domain, new_uri.path, edge_id],
    )?;
    index::reindex_node(&tx, &target.node_uuid)?;
    tx.commit()?;

    let document_count = conn.query_row("SELECT COUNT(*) FROM search_documents", [], |row| {
        row.get::<_, i64>(0)
    })?;
    Ok(json!({
        "uri": new_uri.to_string(),
        "targetUri": target_uri.to_string(),
        "nodeUuid": target.node_uuid,
        "edgeId": edge_id,
        "priority": priority,
        "disclosure": disclosure,
        "documentCount": document_count,
    }))
}

pub(crate) fn manage_triggers_action(
    config: &ZmemoryConfig,
    conn: &mut Connection,
    args: &ManageTriggersActionParams,
) -> Result<Value> {
    let uri = &args.uri;
    anyhow::ensure!(!uri.is_root(), "cannot manage triggers for root path");
    common::ensure_writable_domain(config, conn, &uri.domain)?;
    let row = common::find_path_row(conn, &uri)?
        .ok_or_else(|| anyhow::anyhow!("memory not found: {uri}"))?;
    let add = common::normalize_keywords(args.add.clone());
    let remove = common::normalize_keywords(args.remove.clone());
    anyhow::ensure!(
        !(add.is_empty() && remove.is_empty()),
        "no changes requested"
    );

    let tx = conn.transaction()?;
    for keyword in &add {
        tx.execute(
            "INSERT OR IGNORE INTO glossary_keywords(keyword, node_uuid) VALUES (?1, ?2)",
            params![keyword, row.node_uuid],
        )?;
    }
    for keyword in &remove {
        tx.execute(
            "DELETE FROM glossary_keywords WHERE keyword = ?1 AND node_uuid = ?2",
            params![keyword, row.node_uuid],
        )?;
    }
    index::reindex_node(&tx, &row.node_uuid)?;
    tx.commit()?;
    let current = common::load_keywords(conn, &row.node_uuid)?;
    let document_count = conn.query_row("SELECT COUNT(*) FROM search_documents", [], |row| {
        row.get::<_, i64>(0)
    })?;

    Ok(json!({
        "uri": uri.to_string(),
        "nodeUuid": row.node_uuid,
        "added": add,
        "removed": remove,
        "current": current,
        "documentCount": document_count,
    }))
}
