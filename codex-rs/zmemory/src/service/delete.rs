use crate::config::ZmemoryConfig;
use crate::service::common;
use crate::service::index;
use crate::tool_api::UriActionParams;
use anyhow::Result;
use rusqlite::Connection;
use rusqlite::params;
use serde_json::Value;
use serde_json::json;

pub(crate) fn delete_path_action(
    config: &ZmemoryConfig,
    conn: &mut Connection,
    args: &UriActionParams,
) -> Result<Value> {
    let uri = &args.uri;
    anyhow::ensure!(!uri.is_root(), "cannot delete root path");
    common::ensure_writable_domain(config, conn, &uri.domain)?;
    let row = common::find_path_row(conn, config, uri)?
        .ok_or_else(|| anyhow::anyhow!("memory not found: {uri}"))?;

    let tx = conn.transaction()?;
    let deleted_paths = tx.execute(
        "DELETE FROM paths WHERE namespace = ?1 AND domain = ?2 AND path = ?3",
        params![config.namespace(), uri.domain, uri.path],
    )?;
    let deleted_edges = tx.execute(
        "DELETE FROM edges WHERE id = ?1 AND namespace = ?2",
        params![row.edge_id, config.namespace()],
    )?;
    let remaining_refs: i64 = tx.query_row(
        "SELECT COUNT(*)
         FROM edges e
         JOIN paths p ON p.edge_id = e.id AND p.namespace = e.namespace
         WHERE e.child_uuid = ?1 AND e.namespace = ?2",
        params![row.node_uuid.as_str(), config.namespace()],
        |stmt| stmt.get(0),
    )?;
    let deprecated_nodes = if remaining_refs == 0 {
        tx.execute(
            "UPDATE memories
             SET deprecated = TRUE
             WHERE namespace = ?1 AND node_uuid = ?2 AND deprecated = FALSE",
            params![config.namespace(), row.node_uuid.as_str()],
        )?
    } else {
        0
    };
    common::insert_audit_log(
        &tx,
        config.namespace(),
        "delete-path",
        Some(&uri.to_string()),
        Some(&row.node_uuid),
        json!({
            "deletedPaths": deleted_paths,
            "deletedEdges": deleted_edges,
            "deprecatedNodes": deprecated_nodes,
            "remainingRefs": remaining_refs,
        }),
    )?;
    index::reindex_node(&tx, config.namespace(), &row.node_uuid)?;
    tx.commit()?;

    let document_count = common::search_document_count(conn, config)?;
    Ok(json!({
        "uri": uri.to_string(),
        "nodeUuid": row.node_uuid,
        "deletedPaths": deleted_paths,
        "deletedEdges": deleted_edges,
        "deprecatedNodes": deprecated_nodes,
        "documentCount": document_count,
    }))
}
