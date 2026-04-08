use crate::config::ZmemoryConfig;
use crate::service::common;
use crate::service::contracts::NodeAliasContract;
use crate::service::contracts::NodeChildContract;
use crate::tool_api::ZmemoryUri;
use anyhow::Result;
use rusqlite::Connection;
use rusqlite::params;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NodeSnapshot {
    pub(crate) node_uuid: String,
    pub(crate) memory_id: i64,
    pub(crate) primary_uri: String,
    pub(crate) content: String,
    pub(crate) priority: i64,
    pub(crate) disclosure: Option<String>,
    pub(crate) keywords: Vec<String>,
    pub(crate) aliases: Vec<NodeAliasContract>,
    pub(crate) children: Vec<NodeChildContract>,
    pub(crate) alias_count: i64,
}

#[derive(Debug, Clone)]
struct SnapshotPath {
    uri: String,
    domain: String,
    priority: i64,
    disclosure: Option<String>,
}

pub(crate) fn load_node_snapshot_for_uri(
    config: &ZmemoryConfig,
    conn: &Connection,
    uri: &ZmemoryUri,
) -> Result<NodeSnapshot> {
    let row = common::find_path_row(conn, config, uri)?
        .ok_or_else(|| anyhow::anyhow!("memory not found: {uri}"))?;
    load_node_snapshot_for_node(config, conn, &row.node_uuid, Some(uri), None)
}

pub(crate) fn load_node_snapshot_for_node(
    config: &ZmemoryConfig,
    conn: &Connection,
    node_uuid: &str,
    requested_uri: Option<&ZmemoryUri>,
    preferred_domain: Option<&str>,
) -> Result<NodeSnapshot> {
    let memory = common::read_active_memory(conn, config.namespace(), node_uuid)?
        .ok_or_else(|| anyhow::anyhow!("active memory not found for node {node_uuid}"))?;
    let keywords = common::load_keywords(conn, config, node_uuid)?;
    let mut paths = load_paths(conn, config, node_uuid)?;
    anyhow::ensure!(
        !paths.is_empty(),
        "no live paths found for node {node_uuid}"
    );

    let primary_index = select_primary_index(&paths, requested_uri, preferred_domain)?;
    let primary = paths.remove(primary_index);
    let alias_count = (paths.len() + 1) as i64;
    let children = common::list_children(conn, config, &primary.domain, node_uuid)?;
    let aliases = paths
        .into_iter()
        .map(|path| NodeAliasContract {
            uri: path.uri,
            priority: path.priority,
            disclosure: path.disclosure,
        })
        .collect::<Vec<_>>();

    Ok(NodeSnapshot {
        node_uuid: node_uuid.to_string(),
        memory_id: memory.id,
        primary_uri: primary.uri,
        content: memory.content,
        priority: primary.priority,
        disclosure: primary.disclosure,
        keywords,
        aliases,
        children,
        alias_count,
    })
}

fn load_paths(
    conn: &Connection,
    config: &ZmemoryConfig,
    node_uuid: &str,
) -> Result<Vec<SnapshotPath>> {
    let mut stmt = conn.prepare(
        "SELECT p.domain, p.path, e.priority, e.disclosure
         FROM edges e
         JOIN paths p ON p.edge_id = e.id AND p.namespace = e.namespace
         WHERE e.namespace = ?1 AND e.child_uuid = ?2
         ORDER BY e.priority DESC, LENGTH(p.path) ASC, p.domain ASC, p.path ASC",
    )?;
    stmt.query_map(params![config.namespace(), node_uuid], |row| {
        let domain: String = row.get(0)?;
        let path: String = row.get(1)?;
        Ok(SnapshotPath {
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
    paths: &[SnapshotPath],
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
