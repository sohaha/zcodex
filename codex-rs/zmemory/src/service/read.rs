//! Read action for the zmemory service layer.

use crate::config::ZmemoryConfig;
use crate::system_views::read_system_view;
use crate::tool_api::ZmemoryToolCallParam;
use anyhow::Result;
use rusqlite::Connection;
use serde_json::Value;
use serde_json::json;

use super::common::count_aliases;
use super::common::ensure_readable_domain;
use super::common::find_path_row;
use super::common::list_children;
use super::common::load_keywords;
use super::common::parse_required_uri;
use super::common::read_active_memory;

pub(crate) fn read_action(
    config: &ZmemoryConfig,
    conn: &Connection,
    args: &ZmemoryToolCallParam,
) -> Result<Value> {
    let uri = parse_required_uri(args.uri.as_deref())?;
    if uri.domain == "system" {
        return read_system_view(conn, config, &uri.path, args.limit.unwrap_or(20))
            .map(|view| json!({ "uri": uri.to_string(), "view": view }));
    }
    ensure_readable_domain(config, conn, &uri.domain)?;

    let row =
        find_path_row(conn, &uri)?.ok_or_else(|| anyhow::anyhow!("memory not found: {uri}"))?;
    let memory = read_active_memory(conn, &row.node_uuid)?
        .ok_or_else(|| anyhow::anyhow!("active memory not found for {uri}"))?;
    let keywords = load_keywords(conn, &row.node_uuid)?;
    let children = list_children(conn, &uri, &row.node_uuid)?;
    let alias_count = count_aliases(conn, &row.node_uuid)?;

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
