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
    let row = common::find_path_row(conn, &uri)?
        .ok_or_else(|| anyhow::anyhow!("memory not found: {uri}"))?;

    let tx = conn.transaction()?;
    let deleted_paths = tx.execute(
        "DELETE FROM paths WHERE domain = ?1 AND path = ?2",
        params![uri.domain, uri.path],
    )?;
    let deleted_edges = tx.execute("DELETE FROM edges WHERE id = ?1", [row.edge_id])?;
    let remaining_refs: i64 = tx.query_row(
        "SELECT COUNT(*) FROM edges e JOIN paths p ON p.edge_id = e.id WHERE e.child_uuid = ?1",
        [row.node_uuid.as_str()],
        |stmt| stmt.get(0),
    )?;
    let deprecated_nodes = if remaining_refs == 0 {
        tx.execute(
            "UPDATE memories SET deprecated = TRUE WHERE node_uuid = ?1 AND deprecated = FALSE",
            [row.node_uuid.as_str()],
        )?
    } else {
        0
    };
    index::reindex_node(&tx, &row.node_uuid)?;
    tx.commit()?;

    let document_count = conn.query_row("SELECT COUNT(*) FROM search_documents", [], |row| {
        row.get::<_, i64>(0)
    })?;
    Ok(json!({
        "uri": uri.to_string(),
        "nodeUuid": row.node_uuid,
        "deletedPaths": deleted_paths,
        "deletedEdges": deleted_edges,
        "deprecatedNodes": deprecated_nodes,
        "documentCount": document_count,
    }))
}
