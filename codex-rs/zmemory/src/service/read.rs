use crate::config::ZmemoryConfig;
use crate::service::common;
use crate::service::contracts::ReadNodeContract;
use crate::service::snapshot;
use crate::tool_api::ReadActionParams;
use anyhow::Result;
use rusqlite::Connection;
use serde_json::Value;
use serde_json::json;

pub(crate) fn read_action(
    config: &ZmemoryConfig,
    conn: &Connection,
    args: &ReadActionParams,
) -> Result<Value> {
    let uri = &args.uri;
    if uri.domain == "system" {
        return crate::system_views::read_system_view(conn, config, &uri.path, args.limit)
            .map(|view| json!({ "uri": uri.to_string(), "view": view }));
    }
    common::ensure_readable_domain(config, conn, &uri.domain)?;

    let snapshot = snapshot::load_node_snapshot_for_uri(config, conn, uri)?;

    serde_json::to_value(ReadNodeContract {
        uri: snapshot.primary_uri,
        node_uuid: snapshot.node_uuid,
        memory_id: snapshot.memory_id,
        content: snapshot.content,
        priority: snapshot.priority,
        disclosure: snapshot.disclosure,
        keywords: snapshot.keywords,
        children: snapshot.children,
        alias_count: snapshot.alias_count,
    })
    .map_err(Into::into)
}
