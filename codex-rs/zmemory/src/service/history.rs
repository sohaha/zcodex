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
    let mut stmt = conn.prepare(
        "SELECT id, content, deprecated, migrated_to, created_at
         FROM memories
         WHERE namespace = ?1 AND node_uuid = ?2
         ORDER BY id DESC",
    )?;
    let versions = stmt
        .query_map([config.namespace(), row.node_uuid.as_str()], |entry| {
            Ok(HistoryVersionContract {
                id: entry.get(0)?,
                content: entry.get(1)?,
                deprecated: entry.get(2)?,
                migrated_to: entry.get(3)?,
                created_at: entry.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    serde_json::to_value(ChangeSetRecord {
        uri: uri.to_string(),
        node_uuid: row.node_uuid,
        versions,
    })
    .map_err(Into::into)
}
