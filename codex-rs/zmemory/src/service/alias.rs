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
    let tx = conn.transaction()?;
    let result = add_alias_action_in_tx(config, &tx, args)?;
    tx.commit()?;

    let document_count = common::search_document_count(conn, config)?;
    Ok(augment_document_count(result, document_count))
}

pub(crate) fn add_alias_action_in_tx(
    config: &ZmemoryConfig,
    conn: &rusqlite::Transaction<'_>,
    args: &AddAliasActionParams,
) -> Result<Value> {
    let new_uri = &args.new_uri;
    let target_uri = &args.target_uri;
    anyhow::ensure!(!new_uri.is_root(), "cannot alias root path");
    anyhow::ensure!(!target_uri.is_root(), "cannot alias the root node");
    common::ensure_writable_domain(config, conn, &new_uri.domain)?;
    common::ensure_readable_domain(config, conn, &target_uri.domain)?;
    anyhow::ensure!(
        common::find_path_row(conn, config, new_uri)?.is_none(),
        "alias path already exists: {new_uri}"
    );

    let target = common::find_path_row(conn, config, target_uri)?
        .ok_or_else(|| anyhow::anyhow!("target path does not exist: {target_uri}"))?;
    let parent_uri = new_uri.parent();
    let parent = if parent_uri.is_root() {
        common::PathRow::root()
    } else {
        common::find_path_row(conn, config, &parent_uri)?
            .ok_or_else(|| anyhow::anyhow!("parent path does not exist: {parent_uri}"))?
    };
    let priority = args.priority.unwrap_or(target.priority);
    let disclosure = args.disclosure.clone();
    let edge_name = new_uri.leaf_name()?;
    let existing_edge_id = common::find_edge_id(
        conn,
        config.namespace(),
        &parent.node_uuid,
        &target.node_uuid,
        edge_name,
    )?;

    let edge_id = if let Some(edge_id) = existing_edge_id {
        let (existing_priority, existing_disclosure): (i64, Option<String>) = conn.query_row(
            "SELECT priority, disclosure FROM edges WHERE id = ?1 AND namespace = ?2",
            params![edge_id, config.namespace()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        if let Some(requested_priority) = args.priority {
            anyhow::ensure!(
                requested_priority == existing_priority,
                "alias edge metadata conflicts for {new_uri}: requested priority {requested_priority} but existing priority is {existing_priority}",
            );
        }
        if let Some(requested_disclosure) = args.disclosure.as_deref() {
            anyhow::ensure!(
                Some(requested_disclosure) == existing_disclosure.as_deref(),
                "alias edge metadata conflicts for {new_uri}: requested disclosure {requested_disclosure:?} but existing disclosure is {existing_disclosure:?}",
            );
        }
        edge_id
    } else {
        conn.execute(
            "INSERT INTO edges(namespace, parent_uuid, child_uuid, name, priority, disclosure)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                config.namespace(),
                parent.node_uuid,
                target.node_uuid,
                edge_name,
                priority,
                disclosure
            ],
        )?;
        conn.last_insert_rowid()
    };
    conn.execute(
        "INSERT INTO paths(namespace, domain, path, edge_id) VALUES (?1, ?2, ?3, ?4)",
        params![config.namespace(), new_uri.domain, new_uri.path, edge_id],
    )?;
    common::insert_audit_log(
        conn,
        config.namespace(),
        "add-alias",
        Some(&new_uri.to_string()),
        Some(&target.node_uuid),
        json!({
            "targetUri": target_uri.to_string(),
            "edgeId": edge_id,
            "priority": priority,
            "disclosure": disclosure,
        }),
    )?;
    index::reindex_node(conn, config.namespace(), &target.node_uuid)?;
    Ok(json!({
        "uri": new_uri.to_string(),
        "targetUri": target_uri.to_string(),
        "nodeUuid": target.node_uuid,
        "edgeId": edge_id,
        "priority": priority,
        "disclosure": disclosure,
    }))
}

pub(crate) fn manage_triggers_action(
    config: &ZmemoryConfig,
    conn: &mut Connection,
    args: &ManageTriggersActionParams,
) -> Result<Value> {
    let tx = conn.transaction()?;
    let result = manage_triggers_action_in_tx(config, &tx, args)?;
    tx.commit()?;

    let document_count = common::search_document_count(conn, config)?;
    Ok(augment_document_count(result, document_count))
}

pub(crate) fn manage_triggers_action_in_tx(
    config: &ZmemoryConfig,
    conn: &rusqlite::Transaction<'_>,
    args: &ManageTriggersActionParams,
) -> Result<Value> {
    let uri = &args.uri;
    anyhow::ensure!(!uri.is_root(), "cannot manage triggers for root path");
    common::ensure_writable_domain(config, conn, &uri.domain)?;
    let row = common::find_path_row(conn, config, uri)?
        .ok_or_else(|| anyhow::anyhow!("memory not found: {uri}"))?;
    let add = common::normalize_keywords(args.add.clone());
    let remove = common::normalize_keywords(args.remove.clone());
    anyhow::ensure!(
        !(add.is_empty() && remove.is_empty()),
        "no changes requested"
    );

    for keyword in &add {
        conn.execute(
            "INSERT OR IGNORE INTO glossary_keywords(keyword, node_uuid, namespace) VALUES (?1, ?2, ?3)",
            params![keyword, row.node_uuid, config.namespace()],
        )?;
    }
    for keyword in &remove {
        conn.execute(
            "DELETE FROM glossary_keywords WHERE keyword = ?1 AND node_uuid = ?2 AND namespace = ?3",
            params![keyword, row.node_uuid, config.namespace()],
        )?;
    }
    common::insert_audit_log(
        conn,
        config.namespace(),
        "manage-triggers",
        Some(&uri.to_string()),
        Some(&row.node_uuid),
        json!({
            "added": add,
            "removed": remove,
        }),
    )?;
    index::reindex_node(conn, config.namespace(), &row.node_uuid)?;
    let current = common::load_keywords(conn, config, &row.node_uuid)?;

    Ok(json!({
        "uri": uri.to_string(),
        "nodeUuid": row.node_uuid,
        "added": add,
        "removed": remove,
        "current": current,
    }))
}

fn augment_document_count(mut result: Value, document_count: i64) -> Value {
    result["documentCount"] = json!(document_count);
    result
}
