use super::CompatService;
use super::contracts::BreadcrumbItem;
use super::contracts::BrowseChildResponse;
use super::contracts::BrowseNodePayload;
use super::contracts::BrowseNodeResponse;
use super::contracts::DomainSummary;
use super::contracts::GlossaryEntryResponse;
use super::contracts::GlossaryListResponse;
use super::contracts::GlossaryMatchNodeResponse;
use super::contracts::GlossaryMatchResponse;
use super::contracts::GlossaryNodeResponse;
use super::contracts::UpdateNodeResponse;
use crate::schema::ROOT_NODE_UUID;
use crate::service::common;
use crate::service::snapshot;
use crate::tool_api::ZmemoryToolAction;
use crate::tool_api::ZmemoryToolCallParam;
use crate::tool_api::ZmemoryUri;
use anyhow::Result;
use rusqlite::Connection;
use rusqlite::OptionalExtension;
use rusqlite::params;
use std::collections::BTreeMap;

impl CompatService {
    pub fn list_domains_for_namespace(
        &self,
        namespace: Option<&str>,
    ) -> Result<Vec<DomainSummary>> {
        let (conn, config) = self.connect(namespace)?;
        let mut stmt = conn.prepare(
            "SELECT domain, COUNT(*)
             FROM paths
             WHERE namespace = ?1 AND INSTR(path, '/') = 0
             GROUP BY domain
             ORDER BY domain ASC",
        )?;
        stmt.query_map([config.namespace()], |row| {
            Ok(DomainSummary {
                domain: row.get(0)?,
                root_count: row.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
    }

    pub fn list_namespaces(&self) -> Result<Vec<String>> {
        let (conn, _) = self.connect(None)?;
        let mut stmt = conn.prepare(
            "SELECT DISTINCT namespace
             FROM paths
             ORDER BY namespace ASC",
        )?;
        stmt.query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn browse_node(
        &self,
        namespace: Option<&str>,
        domain: &str,
        path: &str,
        nav_only: bool,
    ) -> Result<BrowseNodePayload> {
        let (conn, config) = self.connect(namespace)?;
        if path.is_empty() {
            let node = BrowseNodeResponse {
                path: String::new(),
                domain: domain.to_string(),
                uri: format!("{domain}://"),
                name: "root".to_string(),
                content: String::new(),
                priority: 0,
                disclosure: None,
                created_at: None,
                is_virtual: true,
                aliases: Vec::new(),
                node_uuid: ROOT_NODE_UUID.to_string(),
                glossary_keywords: Vec::new(),
                glossary_matches: Vec::new(),
            };
            return Ok(BrowseNodePayload {
                node,
                children: list_children(&conn, &config, domain, ROOT_NODE_UUID)?,
                breadcrumbs: build_breadcrumbs(path),
            });
        }

        let uri = ZmemoryUri::parse(&format!("{domain}://{path}"))?;
        let row = common::find_path_row(&conn, &config, &uri)?
            .ok_or_else(|| anyhow::anyhow!("Path not found: {domain}://{path}"))?;
        let node_snapshot = snapshot::load_node_snapshot_for_uri(&config, &conn, &uri)?;
        let created_at = active_memory_created_at(&conn, config.namespace(), &row.node_uuid)?;
        let aliases = node_snapshot
            .aliases
            .iter()
            .map(|alias| alias.uri.clone())
            .collect::<Vec<_>>();
        let glossary_matches = if nav_only {
            Vec::new()
        } else {
            glossary_matches(&conn, &config, &node_snapshot.content)?
        };

        Ok(BrowseNodePayload {
            node: BrowseNodeResponse {
                path: path.to_string(),
                domain: domain.to_string(),
                uri: node_snapshot.primary_uri.clone(),
                name: if path.contains('/') {
                    path.rsplit('/').next().unwrap_or(path).to_string()
                } else {
                    path.to_string()
                },
                content: node_snapshot.content,
                priority: node_snapshot.priority,
                disclosure: node_snapshot.disclosure,
                created_at,
                is_virtual: false,
                aliases,
                node_uuid: node_snapshot.node_uuid.clone(),
                glossary_keywords: node_snapshot.keywords,
                glossary_matches,
            },
            children: list_children(&conn, &config, domain, &row.node_uuid)?,
            breadcrumbs: build_breadcrumbs(path),
        })
    }

    pub fn update_node(
        &self,
        namespace: Option<&str>,
        domain: &str,
        path: &str,
        content: Option<String>,
        priority: Option<i64>,
        disclosure: Option<String>,
    ) -> Result<UpdateNodeResponse> {
        let config = self.config_for(namespace);
        let result = crate::service::execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::Update,
                uri: Some(format!("{domain}://{path}")),
                content,
                priority,
                disclosure,
                ..ZmemoryToolCallParam::default()
            },
        )?;
        Ok(UpdateNodeResponse {
            success: true,
            memory_id: result["result"]["newMemoryId"].as_i64(),
        })
    }

    pub fn list_glossary(&self, namespace: Option<&str>) -> Result<GlossaryListResponse> {
        let (conn, config) = self.connect(namespace)?;
        let mut stmt = conn.prepare(
            "SELECT g.keyword, g.node_uuid, p.domain, p.path, m.content
             FROM glossary_keywords g
             JOIN (
                 SELECT e.child_uuid,
                        MIN(p.domain || '://' || p.path) AS primary_uri
                 FROM edges e
                 JOIN paths p ON p.edge_id = e.id AND p.namespace = e.namespace
                 WHERE e.namespace = ?1
                 GROUP BY e.child_uuid
             ) AS primary_paths ON primary_paths.child_uuid = g.node_uuid
             JOIN paths p ON (p.domain || '://' || p.path) = primary_paths.primary_uri AND p.namespace = ?1
             JOIN edges e ON e.id = p.edge_id AND e.namespace = p.namespace
             JOIN memories m ON m.node_uuid = e.child_uuid AND m.namespace = e.namespace AND m.deprecated = FALSE
             WHERE g.namespace = ?1
             ORDER BY g.keyword ASC, p.domain ASC, p.path ASC",
        )?;
        let mut grouped = BTreeMap::<String, Vec<GlossaryNodeResponse>>::new();
        for row in stmt.query_map([config.namespace()], |row| {
            let domain: String = row.get(2)?;
            let path: String = row.get(3)?;
            let content: String = row.get(4)?;
            Ok((
                row.get::<_, String>(0)?,
                GlossaryNodeResponse {
                    node_uuid: row.get(1)?,
                    uri: format!("{domain}://{path}"),
                    content_snippet: snippet(&content),
                },
            ))
        })? {
            let (keyword, node) = row?;
            grouped.entry(keyword).or_default().push(node);
        }

        Ok(GlossaryListResponse {
            glossary: grouped
                .into_iter()
                .map(|(keyword, nodes)| GlossaryEntryResponse { keyword, nodes })
                .collect(),
        })
    }
}

fn list_children(
    conn: &Connection,
    config: &crate::config::ZmemoryConfig,
    domain: &str,
    node_uuid: &str,
) -> Result<Vec<BrowseChildResponse>> {
    let child_rows = common::list_children(conn, config, domain, node_uuid)?;
    let mut children = Vec::with_capacity(child_rows.len());
    for child in child_rows {
        let child_uri = ZmemoryUri::parse(&child.uri)?;
        let child_row = common::find_path_row(conn, config, &child_uri)?
            .ok_or_else(|| anyhow::anyhow!("memory not found: {}", child.uri))?;
        let active_memory =
            common::read_active_memory(conn, config.namespace(), &child_row.node_uuid)?
                .ok_or_else(|| anyhow::anyhow!("active memory not found for {}", child.uri))?;
        let approx_children_count = child_count(conn, config.namespace(), &child_row.node_uuid)?;
        children.push(BrowseChildResponse {
            domain: child_uri.domain,
            path: child_uri.path,
            uri: child.uri,
            name: child.name,
            priority: child.priority,
            disclosure: child.disclosure,
            content_snippet: snippet(&active_memory.content),
            approx_children_count,
        });
    }
    Ok(children)
}

fn child_count(conn: &Connection, namespace: &str, node_uuid: &str) -> Result<i64> {
    conn.query_row(
        "SELECT COUNT(DISTINCT child_uuid)
         FROM edges
         WHERE namespace = ?1 AND parent_uuid = ?2",
        params![namespace, node_uuid],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn active_memory_created_at(
    conn: &Connection,
    namespace: &str,
    node_uuid: &str,
) -> Result<Option<String>> {
    conn.query_row(
        "SELECT created_at
         FROM memories
         WHERE namespace = ?1 AND node_uuid = ?2 AND deprecated = FALSE
         ORDER BY id DESC
         LIMIT 1",
        params![namespace, node_uuid],
        |row| row.get(0),
    )
    .optional()
    .map_err(Into::into)
}

fn glossary_matches(
    conn: &Connection,
    config: &crate::config::ZmemoryConfig,
    content: &str,
) -> Result<Vec<GlossaryMatchResponse>> {
    let mut stmt = conn.prepare(
        "SELECT g.keyword, g.node_uuid, MIN(p.domain || '://' || p.path) AS uri
         FROM glossary_keywords g
         JOIN edges e ON e.child_uuid = g.node_uuid AND e.namespace = g.namespace
         JOIN paths p ON p.edge_id = e.id AND p.namespace = e.namespace
         WHERE g.namespace = ?1
         GROUP BY g.keyword, g.node_uuid
         ORDER BY g.keyword ASC, uri ASC",
    )?;
    let content_lower = content.to_lowercase();
    let mut grouped = BTreeMap::<String, Vec<GlossaryMatchNodeResponse>>::new();
    for row in stmt.query_map([config.namespace()], |row| {
        Ok((
            row.get::<_, String>(0)?,
            GlossaryMatchNodeResponse {
                node_uuid: row.get(1)?,
                uri: row.get(2)?,
            },
        ))
    })? {
        let (keyword, node) = row?;
        if content_lower.contains(&keyword.to_lowercase()) {
            grouped.entry(keyword).or_default().push(node);
        }
    }
    Ok(grouped
        .into_iter()
        .map(|(keyword, nodes)| GlossaryMatchResponse { keyword, nodes })
        .collect())
}

fn build_breadcrumbs(path: &str) -> Vec<BreadcrumbItem> {
    let mut breadcrumbs = vec![BreadcrumbItem {
        path: String::new(),
        label: "root".to_string(),
    }];
    if path.is_empty() {
        return breadcrumbs;
    }

    let mut current = String::new();
    for segment in path.split('/') {
        if !current.is_empty() {
            current.push('/');
        }
        current.push_str(segment);
        breadcrumbs.push(BreadcrumbItem {
            path: current.clone(),
            label: segment.to_string(),
        });
    }
    breadcrumbs
}

pub(crate) fn snippet(content: &str) -> String {
    const LIMIT: usize = 200;
    let mut value = content.chars().take(LIMIT).collect::<String>();
    if content.chars().count() > LIMIT {
        value.push_str("...");
    }
    value
}
