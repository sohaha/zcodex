use super::CompatService;
use super::browse::snippet;
use super::contracts::AdminDoctorResponse;
use super::contracts::AdminStatsResponse;
use super::contracts::DeleteOrphanResponse;
use super::contracts::OrphanDetailResponse;
use super::contracts::OrphanListItemResponse;
use super::contracts::OrphanMigrationTargetDetailResponse;
use super::contracts::OrphanMigrationTargetSnippetResponse;
use super::contracts::RebuildSearchResponse;
use super::contracts::ReviewDeprecatedItemResponse;
use super::contracts::ReviewDeprecatedResponse;
use crate::doctor::run_doctor;
use crate::service::stats;
use crate::tool_api::ZmemoryToolAction;
use crate::tool_api::ZmemoryToolCallParam;
use anyhow::Result;
use rusqlite::Connection;
use rusqlite::OptionalExtension;
use rusqlite::params;

impl CompatService {
    pub fn admin_stats(&self, namespace: Option<&str>) -> Result<AdminStatsResponse> {
        let (conn, config) = self.connect(namespace)?;
        let snapshot = stats::collect_stats_snapshot(&conn, &config)?;
        Ok(AdminStatsResponse {
            generated_at: snapshot
                .latest_audit_at
                .clone()
                .unwrap_or_else(|| "unavailable".to_string()),
            active_paths: snapshot.path_count,
            unique_nodes: snapshot.node_count,
            glossary_keywords: snapshot.glossary_count,
            orphaned_memories: snapshot.orphaned_memory_count,
            deprecated_memories: snapshot.deprecated_memory_count,
        })
    }

    pub fn admin_doctor(&self, namespace: Option<&str>) -> Result<AdminDoctorResponse> {
        let (conn, config) = self.connect(namespace)?;
        let snapshot = stats::collect_stats_snapshot(&conn, &config)?;
        let doctor = run_doctor(&conn, config.namespace(), &snapshot)?;
        let checks = vec![
            format!("active paths: {}", snapshot.path_count),
            format!("unique nodes: {}", snapshot.node_count),
            format!("glossary keywords: {}", snapshot.glossary_count),
        ];
        let warnings = doctor
            .issues
            .iter()
            .map(|issue| issue.message.clone())
            .collect::<Vec<_>>();
        Ok(AdminDoctorResponse {
            generated_at: snapshot
                .latest_audit_at
                .unwrap_or_else(|| "unavailable".to_string()),
            status: if warnings.is_empty() {
                "ok".to_string()
            } else {
                "warn".to_string()
            },
            checks,
            warnings,
        })
    }

    pub fn list_orphans(&self, namespace: Option<&str>) -> Result<Vec<OrphanListItemResponse>> {
        let (conn, config) = self.connect(namespace)?;
        list_orphans(&conn, config.namespace())
    }

    pub fn orphan_detail(
        &self,
        namespace: Option<&str>,
        memory_id: i64,
    ) -> Result<Option<OrphanDetailResponse>> {
        let (conn, config) = self.connect(namespace)?;
        orphan_detail(&conn, config.namespace(), memory_id)
    }

    pub fn delete_orphan(
        &self,
        namespace: Option<&str>,
        memory_id: i64,
    ) -> Result<DeleteOrphanResponse> {
        let (mut conn, config) = self.connect(namespace)?;
        let tx = conn.transaction()?;
        let target = load_memory_row(&tx, config.namespace(), memory_id)?
            .ok_or_else(|| anyhow::anyhow!("Memory ID {memory_id} not found"))?;
        anyhow::ensure!(
            target.deprecated,
            "Memory {memory_id} is active (deprecated=False). Deletion aborted."
        );
        let chain_repaired_to = target.migrated_to;
        tx.execute(
            "UPDATE memories
             SET migrated_to = ?2
             WHERE namespace = ?1 AND migrated_to = ?3",
            params![config.namespace(), chain_repaired_to, memory_id],
        )?;
        tx.execute(
            "DELETE FROM memories WHERE namespace = ?1 AND id = ?2",
            params![config.namespace(), memory_id],
        )?;
        let remaining: i64 = tx.query_row(
            "SELECT COUNT(*) FROM memories WHERE namespace = ?1 AND node_uuid = ?2",
            params![config.namespace(), target.node_uuid.as_str()],
            |row| row.get(0),
        )?;
        if remaining == 0 {
            gc_memoryless_node(&tx, config.namespace(), &target.node_uuid)?;
        }
        tx.commit()?;
        let _ = crate::service::index::rebuild_search_index(&mut conn, config.namespace())?;

        Ok(DeleteOrphanResponse {
            deleted_memory_id: memory_id,
            chain_repaired_to,
        })
    }

    pub fn rebuild_search(&self, namespace: Option<&str>) -> Result<RebuildSearchResponse> {
        let config = self.config_for(namespace);
        let result = crate::service::execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::RebuildSearch,
                ..ZmemoryToolCallParam::default()
            },
        )?;
        Ok(RebuildSearchResponse {
            status: "ok".to_string(),
            rebuilt_nodes: result["result"]["documentCount"].as_i64().unwrap_or(0),
        })
    }

    pub fn review_deprecated(&self, namespace: Option<&str>) -> Result<ReviewDeprecatedResponse> {
        let (conn, config) = self.connect(namespace)?;
        let items = list_orphans(&conn, config.namespace())?
            .into_iter()
            .filter(|item| item.category == "deprecated")
            .map(|item| ReviewDeprecatedItemResponse {
                id: item.id,
                content_snippet: item.content_snippet,
                migrated_to: item.migrated_to,
                created_at: item.created_at,
            })
            .collect::<Vec<_>>();
        Ok(ReviewDeprecatedResponse {
            count: items.len(),
            memories: items,
        })
    }
}

#[derive(Debug, Clone)]
struct MemoryRowData {
    id: i64,
    node_uuid: String,
    content: String,
    deprecated: bool,
    migrated_to: Option<i64>,
    created_at: Option<String>,
}

fn load_memory_row(
    conn: &Connection,
    namespace: &str,
    memory_id: i64,
) -> Result<Option<MemoryRowData>> {
    conn.query_row(
        "SELECT id, node_uuid, content, deprecated, migrated_to, created_at
         FROM memories
         WHERE namespace = ?1 AND id = ?2",
        params![namespace, memory_id],
        |row| {
            Ok(MemoryRowData {
                id: row.get(0)?,
                node_uuid: row.get(1)?,
                content: row.get(2)?,
                deprecated: row.get(3)?,
                migrated_to: row.get(4)?,
                created_at: row.get(5)?,
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

fn list_orphans(conn: &Connection, namespace: &str) -> Result<Vec<OrphanListItemResponse>> {
    let mut stmt = conn.prepare(
        "SELECT id, node_uuid, content, deprecated, migrated_to, created_at
         FROM memories
         WHERE namespace = ?1 AND deprecated = TRUE
         ORDER BY id DESC",
    )?;
    let mut items = Vec::new();
    for row in stmt.query_map([namespace], |row| {
        Ok(MemoryRowData {
            id: row.get(0)?,
            node_uuid: row.get(1)?,
            content: row.get(2)?,
            deprecated: row.get(3)?,
            migrated_to: row.get(4)?,
            created_at: row.get(5)?,
        })
    })? {
        let memory = row?;
        let migration_target = if let Some(target_id) = memory.migrated_to {
            if let Some(target) = resolve_migration_chain(conn, namespace, target_id)? {
                Some(OrphanMigrationTargetSnippetResponse {
                    id: target.id,
                    paths: paths_for_node(conn, namespace, &target.node_uuid)?,
                    content_snippet: snippet(&target.content),
                })
            } else {
                None
            }
        } else {
            None
        };
        items.push(OrphanListItemResponse {
            id: memory.id,
            content_snippet: snippet(&memory.content),
            created_at: memory.created_at,
            deprecated: true,
            migrated_to: memory.migrated_to,
            category: if memory.migrated_to.is_some() {
                "deprecated".to_string()
            } else {
                "orphaned".to_string()
            },
            migration_target,
        });
    }
    Ok(items)
}

fn orphan_detail(
    conn: &Connection,
    namespace: &str,
    memory_id: i64,
) -> Result<Option<OrphanDetailResponse>> {
    let Some(memory) = load_memory_row(conn, namespace, memory_id)? else {
        return Ok(None);
    };
    let migration_target = if let Some(target_id) = memory.migrated_to {
        if let Some(target) = resolve_migration_chain(conn, namespace, target_id)? {
            Some(OrphanMigrationTargetDetailResponse {
                id: target.id,
                paths: paths_for_node(conn, namespace, &target.node_uuid)?,
                content: target.content,
                created_at: target.created_at,
            })
        } else {
            None
        }
    } else {
        None
    };
    Ok(Some(OrphanDetailResponse {
        id: memory.id,
        content: memory.content,
        created_at: memory.created_at,
        deprecated: memory.deprecated,
        migrated_to: memory.migrated_to,
        category: if memory.migrated_to.is_some() {
            "deprecated".to_string()
        } else {
            "orphaned".to_string()
        },
        migration_target,
    }))
}

fn resolve_migration_chain(
    conn: &Connection,
    namespace: &str,
    start_id: i64,
) -> Result<Option<MemoryRowData>> {
    let mut current_id = start_id;
    for _ in 0..50 {
        let Some(memory) = load_memory_row(conn, namespace, current_id)? else {
            return Ok(None);
        };
        if let Some(next_id) = memory.migrated_to {
            current_id = next_id;
        } else {
            return Ok(Some(memory));
        }
    }
    Ok(None)
}

fn paths_for_node(conn: &Connection, namespace: &str, node_uuid: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT p.domain, p.path
         FROM edges e
         JOIN paths p ON p.edge_id = e.id AND p.namespace = e.namespace
         WHERE e.namespace = ?1 AND e.child_uuid = ?2
         ORDER BY p.domain ASC, p.path ASC",
    )?;
    stmt.query_map(params![namespace, node_uuid], |row| {
        let domain: String = row.get(0)?;
        let path: String = row.get(1)?;
        Ok(format!("{domain}://{path}"))
    })?
    .collect::<rusqlite::Result<Vec<_>>>()
    .map_err(Into::into)
}

fn gc_memoryless_node(conn: &Connection, namespace: &str, node_uuid: &str) -> Result<()> {
    let mut edge_ids = Vec::new();
    let mut stmt = conn.prepare("SELECT id FROM edges WHERE namespace = ?1 AND child_uuid = ?2")?;
    for edge_id in stmt.query_map(params![namespace, node_uuid], |row| row.get::<_, i64>(0))? {
        edge_ids.push(edge_id?);
    }
    for edge_id in edge_ids {
        conn.execute(
            "DELETE FROM paths WHERE namespace = ?1 AND edge_id = ?2",
            params![namespace, edge_id],
        )?;
        conn.execute(
            "DELETE FROM edges WHERE namespace = ?1 AND id = ?2",
            params![namespace, edge_id],
        )?;
    }
    conn.execute(
        "DELETE FROM glossary_keywords WHERE namespace = ?1 AND node_uuid = ?2",
        params![namespace, node_uuid],
    )?;
    conn.execute("DELETE FROM nodes WHERE uuid = ?1", params![node_uuid])?;
    Ok(())
}
