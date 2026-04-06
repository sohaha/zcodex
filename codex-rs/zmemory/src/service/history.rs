use crate::config::ZmemoryConfig;
use crate::service::common;
use crate::tool_api::ZmemoryUri;
use anyhow::Result;
use rusqlite::Connection;
use serde_json::Value;
use serde_json::json;

pub(crate) fn history_action(
    config: &ZmemoryConfig,
    conn: &Connection,
    uri: &ZmemoryUri,
) -> Result<Value> {
    common::ensure_readable_domain(config, conn, &uri.domain)?;
    let row = common::find_path_row(conn, uri)?
        .ok_or_else(|| anyhow::anyhow!("memory not found: {uri}"))?;
    let mut stmt = conn.prepare(
        "SELECT id, content, deprecated, migrated_to, created_at
         FROM memories
         WHERE node_uuid = ?1
         ORDER BY id DESC",
    )?;
    let versions = stmt
        .query_map([row.node_uuid.as_str()], |entry| {
            Ok(json!({
                "id": entry.get::<_, i64>(0)?,
                "content": entry.get::<_, String>(1)?,
                "deprecated": entry.get::<_, bool>(2)?,
                "migratedTo": entry.get::<_, Option<i64>>(3)?,
                "createdAt": entry.get::<_, String>(4)?,
            }))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(json!({
        "uri": uri.to_string(),
        "nodeUuid": row.node_uuid,
        "versions": versions,
    }))
}
