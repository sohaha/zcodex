use crate::config::ZmemoryConfig;
use crate::service::common;
use crate::service::contracts::ChangeSetRecord;
use crate::service::contracts::HistoryVersionContract;
use crate::tool_api::ZmemoryUri;
use anyhow::Result;
use rusqlite::Connection;
use serde_json::Value;

pub(crate) fn history_action(
    config: &ZmemoryConfig,
    conn: &Connection,
    uri: &ZmemoryUri,
) -> Result<Value> {
    common::ensure_readable_domain(config, conn, &uri.domain)?;
    let row = common::find_path_row(conn, config, uri)?
        .ok_or_else(|| anyhow::anyhow!("memory not found: {uri}"))?;
    serde_json::to_value(changeset_for_node(
        conn,
        config.namespace(),
        uri.to_string(),
        row.node_uuid,
    )?)
    .map_err(Into::into)
}

pub(crate) fn changeset_for_node(
    conn: &Connection,
    namespace: &str,
    uri: String,
    node_uuid: String,
) -> Result<ChangeSetRecord> {
    Ok(ChangeSetRecord {
        uri,
        versions: history_versions_for_node(conn, namespace, &node_uuid)?,
        node_uuid,
    })
}

pub(crate) fn history_versions_for_node(
    conn: &Connection,
    namespace: &str,
    node_uuid: &str,
) -> Result<Vec<HistoryVersionContract>> {
    let mut stmt = conn.prepare(
        "SELECT id, content, deprecated, migrated_to, created_at
         FROM memories
         WHERE namespace = ?1 AND node_uuid = ?2
         ORDER BY id DESC",
    )?;
    stmt.query_map([namespace, node_uuid], |entry| {
        Ok(HistoryVersionContract {
            id: entry.get(0)?,
            content: entry.get(1)?,
            deprecated: entry.get(2)?,
            migrated_to: entry.get(3)?,
            created_at: entry.get(4)?,
        })
    })?
    .collect::<rusqlite::Result<Vec<_>>>()
    .map_err(Into::into)
}
