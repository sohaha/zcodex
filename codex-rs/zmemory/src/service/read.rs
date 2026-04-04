use crate::config::ZmemoryConfig;
use crate::service::common;
use crate::tool_api::ZmemoryToolCallParam;
use crate::tool_api::ZmemoryUri;
use anyhow::Result;
use rusqlite::Connection;
use serde_json::Value;
use serde_json::json;

pub(crate) fn read_action(
    config: &ZmemoryConfig,
    conn: &Connection,
    args: &ZmemoryToolCallParam,
) -> Result<Value> {
    let uri = parse_required_uri(args.uri.as_deref())?;
    if uri.domain == "system" {
        return crate::system_views::read_system_view(
            conn,
            config,
            &uri.path,
            args.limit.unwrap_or(20),
        )
        .map(|view| json!({ "uri": uri.to_string(), "view": view }));
    }
    common::ensure_readable_domain(config, conn, &uri.domain)?;

    let row = common::find_path_row(conn, &uri)?
        .ok_or_else(|| anyhow::anyhow!("memory not found: {uri}"))?;
    let memory = common::read_active_memory(conn, &row.node_uuid)?
        .ok_or_else(|| anyhow::anyhow!("active memory not found for {uri}"))?;
    let keywords = common::load_keywords(conn, &row.node_uuid)?;
    let children = common::list_children(conn, &uri, &row.node_uuid)?;
    let alias_count = common::count_aliases(conn, &row.node_uuid)?;

    Ok(json!({
        "uri": uri.to_string(),
        "nodeUuid": row.node_uuid,
        "memoryId": memory.id,
        "content": memory.content,
        "priority": row.priority,
        "disclosure": row.disclosure,
        "keywords": keywords,
        "children": children,
        "aliasCount": alias_count,
    }))
}

fn parse_required_uri(raw: Option<&str>) -> Result<ZmemoryUri> {
    let raw = raw.ok_or_else(|| anyhow::anyhow!("`uri` is required"))?;
    ZmemoryUri::parse(raw)
}
