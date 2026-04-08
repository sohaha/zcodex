use crate::config::ZmemoryConfig;
use crate::repository::ZmemoryRepository;
use crate::schema::ROOT_NODE_UUID;
use crate::schema::active_memory_id_for_node;
use crate::schema::ensure_domain_root;
use anyhow::Result;
use rusqlite::Connection;
use rusqlite::OptionalExtension;
use rusqlite::Transaction;
use rusqlite::params;
use serde_json::Value;
use serde_json::json;

pub(crate) fn connect(config: &ZmemoryConfig) -> Result<Connection> {
    let repository = ZmemoryRepository::new(config.clone());
    repository.connect()
}

#[derive(Debug, Clone)]
pub(crate) struct PathRow {
    pub(crate) edge_id: i64,
    pub(crate) node_uuid: String,
    pub(crate) priority: i64,
    pub(crate) disclosure: Option<String>,
}

impl PathRow {
    pub(crate) fn root() -> Self {
        Self {
            edge_id: 0,
            node_uuid: ROOT_NODE_UUID.to_string(),
            priority: 0,
            disclosure: None,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct MemoryRow {
    pub(crate) id: i64,
    pub(crate) content: String,
}

pub(crate) fn find_path_row(
    conn: &Connection,
    config: &ZmemoryConfig,
    uri: &crate::tool_api::ZmemoryUri,
) -> Result<Option<PathRow>> {
    if uri.is_root() {
        return Ok(Some(PathRow::root()));
    }
    conn.query_row(
        "SELECT p.edge_id, e.child_uuid, e.priority, e.disclosure
         FROM paths p
         JOIN edges e ON e.id = p.edge_id AND e.namespace = p.namespace
         WHERE p.namespace = ?1 AND p.domain = ?2 AND p.path = ?3",
        params![config.namespace(), uri.domain, uri.path],
        |row| {
            Ok(PathRow {
                edge_id: row.get(0)?,
                node_uuid: row.get(1)?,
                priority: row.get(2)?,
                disclosure: row.get(3)?,
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

pub(crate) fn find_edge_id(
    conn: &Connection,
    namespace: &str,
    parent_uuid: &str,
    child_uuid: &str,
    name: &str,
) -> Result<Option<i64>> {
    conn.query_row(
        "SELECT id
         FROM edges
         WHERE namespace = ?1 AND parent_uuid = ?2 AND child_uuid = ?3 AND name = ?4",
        params![namespace, parent_uuid, child_uuid, name],
        |row| row.get(0),
    )
    .optional()
    .map_err(Into::into)
}

pub(crate) fn ensure_readable_domain(
    config: &ZmemoryConfig,
    conn: &Connection,
    domain: &str,
) -> Result<()> {
    anyhow::ensure!(
        config.is_valid_domain(domain),
        "unknown domain '{domain}'. valid domains: {}",
        config.valid_domains_for_display().join(", ")
    );
    if domain != "system" {
        ensure_domain_root(conn, config.namespace(), domain)?;
    }
    Ok(())
}

pub(crate) fn ensure_writable_domain(
    config: &ZmemoryConfig,
    conn: &Connection,
    domain: &str,
) -> Result<()> {
    anyhow::ensure!(domain != "system", "system domain is read-only");
    ensure_readable_domain(config, conn, domain)
}

pub(crate) fn read_active_memory(
    conn: &Connection,
    namespace: &str,
    node_uuid: &str,
) -> Result<Option<MemoryRow>> {
    let active_memory_id = active_memory_id_for_node(conn, namespace, node_uuid)?;
    let Some(active_memory_id) = active_memory_id else {
        return Ok(None);
    };
    conn.query_row(
        "SELECT id, content FROM memories WHERE id = ?1",
        [active_memory_id],
        |row| {
            Ok(MemoryRow {
                id: row.get(0)?,
                content: row.get(1)?,
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

pub(crate) fn list_children(
    conn: &Connection,
    config: &ZmemoryConfig,
    uri: &crate::tool_api::ZmemoryUri,
    node_uuid: &str,
) -> Result<Vec<Value>> {
    let mut stmt = conn.prepare(
        "SELECT p.path, e.name, e.priority, e.disclosure
         FROM edges e
         JOIN paths p ON p.edge_id = e.id AND p.namespace = e.namespace
         WHERE e.parent_uuid = ?1 AND e.namespace = ?2 AND p.domain = ?3
         ORDER BY e.priority DESC, e.name ASC",
    )?;
    stmt.query_map(
        params![node_uuid, config.namespace(), uri.domain.as_str()],
        |row| {
            let path: String = row.get(0)?;
            Ok(json!({
                "name": row.get::<_, String>(1)?,
                "priority": row.get::<_, i64>(2)?,
                "disclosure": row.get::<_, Option<String>>(3)?,
                "uri": format!("{}://{}", uri.domain, path),
            }))
        },
    )?
    .collect::<rusqlite::Result<Vec<_>>>()
    .map_err(Into::into)
}

pub(crate) fn load_keywords(
    conn: &Connection,
    config: &ZmemoryConfig,
    node_uuid: &str,
) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT keyword
         FROM glossary_keywords
         WHERE namespace = ?1 AND node_uuid = ?2
         ORDER BY keyword ASC",
    )?;
    stmt.query_map(params![config.namespace(), node_uuid], |row| {
        row.get::<_, String>(0)
    })?
    .collect::<rusqlite::Result<Vec<_>>>()
    .map_err(Into::into)
}

pub(crate) fn count_aliases(
    conn: &Connection,
    config: &ZmemoryConfig,
    node_uuid: &str,
) -> Result<i64> {
    conn.query_row(
        "SELECT COUNT(*)
         FROM edges e
         JOIN paths p ON p.edge_id = e.id AND p.namespace = e.namespace
         WHERE e.child_uuid = ?1 AND e.namespace = ?2",
        params![node_uuid, config.namespace()],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub(crate) fn normalize_optional_text(raw: Option<&str>) -> Option<String> {
    raw.map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

pub(crate) fn normalize_keywords(keywords: Vec<String>) -> Vec<String> {
    let mut normalized = keywords
        .into_iter()
        .map(|keyword| keyword.trim().to_lowercase())
        .filter(|keyword| !keyword.is_empty())
        .collect::<Vec<_>>();
    normalized.sort();
    normalized.dedup();
    normalized
}

pub(crate) fn path_resolution_payload(config: &ZmemoryConfig) -> Value {
    let resolution = config.path_resolution();
    json!({
        "dbPath": resolution.db_path.display().to_string(),
        "workspaceKey": resolution.workspace_key.clone(),
        "source": resolution.source,
        "reason": resolution.reason.clone(),
        "namespace": config.namespace(),
        "namespaceSource": config.namespace_source(),
        "supportsNamespaceSelection": config.supports_namespace_selection(),
    })
}

pub(crate) fn search_document_count(conn: &Connection, config: &ZmemoryConfig) -> Result<i64> {
    conn.query_row(
        "SELECT COUNT(*) FROM search_documents WHERE namespace = ?1",
        [config.namespace()],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub(crate) fn insert_audit_log(
    tx: &Transaction<'_>,
    namespace: &str,
    action: &str,
    uri: Option<&str>,
    node_uuid: Option<&str>,
    details: Value,
) -> Result<()> {
    tx.execute(
        "INSERT INTO audit_log(namespace, action, uri, node_uuid, details)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            namespace,
            action,
            uri,
            node_uuid,
            serde_json::to_string(&details)?,
        ],
    )?;
    Ok(())
}
