use crate::config::ZmemoryConfig;
use crate::service::common;
use crate::tool_api::ExportActionParams;
use crate::tool_api::ZmemoryUri;
use anyhow::Result;
use rusqlite::Connection;
use rusqlite::params;
use serde_json::Value;
use serde_json::json;

#[derive(Debug, Clone)]
struct ExportPath {
    uri: String,
    domain: String,
    priority: i64,
    disclosure: Option<String>,
}

pub(crate) fn export_action(
    config: &ZmemoryConfig,
    conn: &Connection,
    args: &ExportActionParams,
) -> Result<Value> {
    match (&args.uri, &args.domain) {
        (Some(uri), None) => export_uri_scope(config, conn, uri),
        (None, Some(domain)) => export_domain_scope(config, conn, domain),
        _ => anyhow::bail!("exactly one of `uri` or `domain` is required for action=export"),
    }
}

fn export_uri_scope(config: &ZmemoryConfig, conn: &Connection, uri: &ZmemoryUri) -> Result<Value> {
    common::ensure_readable_domain(config, conn, &uri.domain)?;
    let row = common::find_path_row(conn, uri)?
        .ok_or_else(|| anyhow::anyhow!("memory not found: {uri}"))?;
    let item = export_item(conn, &row.node_uuid, Some(uri), None)?;
    Ok(json!({
        "scope": {
            "type": "uri",
            "value": uri.to_string(),
        },
        "count": 1,
        "items": [item],
    }))
}

fn export_domain_scope(config: &ZmemoryConfig, conn: &Connection, domain: &str) -> Result<Value> {
    common::ensure_readable_domain(config, conn, domain)?;
    let mut stmt = conn.prepare(
        "SELECT DISTINCT e.child_uuid
         FROM paths p
         JOIN edges e ON e.id = p.edge_id
         WHERE p.domain = ?1
         ORDER BY e.child_uuid ASC",
    )?;
    let node_uuids = stmt
        .query_map([domain], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let items = node_uuids
        .iter()
        .map(|node_uuid| export_item(conn, node_uuid, None, Some(domain)))
        .collect::<Result<Vec<_>>>()?;
    Ok(json!({
        "scope": {
            "type": "domain",
            "value": domain,
        },
        "count": items.len(),
        "items": items,
    }))
}

fn export_item(
    conn: &Connection,
    node_uuid: &str,
    requested_uri: Option<&ZmemoryUri>,
    preferred_domain: Option<&str>,
) -> Result<Value> {
    let memory = common::read_active_memory(conn, node_uuid)?
        .ok_or_else(|| anyhow::anyhow!("active memory not found for node {node_uuid}"))?;
    let keywords = common::load_keywords(conn, node_uuid)?;
    let mut paths = load_paths(conn, node_uuid)?;
    anyhow::ensure!(
        !paths.is_empty(),
        "no live paths found for node {node_uuid}"
    );

    let primary_index = select_primary_index(&paths, requested_uri, preferred_domain)?;
    let primary = paths.remove(primary_index);
    let aliases = paths
        .into_iter()
        .map(|path| {
            json!({
                "uri": path.uri,
                "priority": path.priority,
                "disclosure": path.disclosure,
            })
        })
        .collect::<Vec<_>>();

    Ok(json!({
        "uri": primary.uri,
        "content": memory.content,
        "priority": primary.priority,
        "disclosure": primary.disclosure,
        "keywords": keywords,
        "aliases": aliases,
    }))
}

fn load_paths(conn: &Connection, node_uuid: &str) -> Result<Vec<ExportPath>> {
    let mut stmt = conn.prepare(
        "SELECT p.domain, p.path, e.priority, e.disclosure
         FROM edges e
         JOIN paths p ON p.edge_id = e.id
         WHERE e.child_uuid = ?1
         ORDER BY e.priority DESC, LENGTH(p.path) ASC, p.domain ASC, p.path ASC",
    )?;
    stmt.query_map(params![node_uuid], |row| {
        let domain: String = row.get(0)?;
        let path: String = row.get(1)?;
        Ok(ExportPath {
            uri: format!("{domain}://{path}"),
            domain,
            priority: row.get(2)?,
            disclosure: row.get(3)?,
        })
    })?
    .collect::<rusqlite::Result<Vec<_>>>()
    .map_err(Into::into)
}

fn select_primary_index(
    paths: &[ExportPath],
    requested_uri: Option<&ZmemoryUri>,
    preferred_domain: Option<&str>,
) -> Result<usize> {
    if let Some(uri) = requested_uri {
        let target = uri.to_string();
        return paths
            .iter()
            .position(|path| path.uri == target)
            .ok_or_else(|| anyhow::anyhow!("requested export path not found: {target}"));
    }
    if let Some(domain) = preferred_domain {
        return paths
            .iter()
            .position(|path| path.domain == domain)
            .ok_or_else(|| anyhow::anyhow!("no export path found in domain {domain}"));
    }
    Ok(0)
}
