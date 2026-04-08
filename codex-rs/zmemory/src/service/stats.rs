use crate::config::ZmemoryConfig;
use crate::doctor::run_doctor;
use crate::service::common;
use crate::service::index;
use crate::tool_api::AuditActionParams;
use anyhow::Result;
use rusqlite::Connection;
use serde_json::Value;
use serde_json::json;
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub(crate) struct StatsSnapshot {
    pub(crate) node_count: i64,
    pub(crate) memory_count: i64,
    pub(crate) path_count: i64,
    pub(crate) glossary_count: i64,
    pub(crate) alias_node_count: i64,
    pub(crate) trigger_node_count: i64,
    pub(crate) alias_nodes_missing_triggers: i64,
    pub(crate) disclosure_path_count: i64,
    pub(crate) paths_missing_disclosure: i64,
    pub(crate) disclosures_needing_review: i64,
    pub(crate) orphaned_memory_count: i64,
    pub(crate) deprecated_memory_count: i64,
    pub(crate) search_document_count: i64,
    pub(crate) fts_document_count: i64,
    pub(crate) audit_log_count: i64,
    pub(crate) latest_audit_at: Option<String>,
    pub(crate) audit_action_counts: BTreeMap<String, i64>,
}

pub(crate) fn stats_action(conn: &Connection, config: &ZmemoryConfig) -> Result<Value> {
    let stats = collect_stats_snapshot(conn, config)?;
    Ok(stats_action_with_snapshot(config, &stats))
}

pub(crate) fn audit_action(
    conn: &Connection,
    config: &ZmemoryConfig,
    args: &AuditActionParams,
) -> Result<Value> {
    let mut stmt = conn.prepare(
        "SELECT id, action, uri, node_uuid, details, created_at
         FROM audit_log
         WHERE namespace = ?1
           AND (?2 IS NULL OR action = ?2)
           AND (?3 IS NULL OR uri = ?3)
         ORDER BY id DESC
         LIMIT ?4",
    )?;
    let entries = stmt
        .query_map(
            rusqlite::params![
                config.namespace(),
                args.audit_action.as_deref(),
                args.uri.as_ref().map(ToString::to_string),
                args.limit
            ],
            |row| {
                let details = row.get::<_, String>(4)?;
                Ok(json!({
                    "id": row.get::<_, i64>(0)?,
                    "action": row.get::<_, String>(1)?,
                    "uri": row.get::<_, Option<String>>(2)?,
                    "nodeUuid": row.get::<_, Option<String>>(3)?,
                    "details": serde_json::from_str::<Value>(&details)
                        .unwrap_or(Value::String(details)),
                    "createdAt": row.get::<_, String>(5)?,
                }))
            },
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(json!({
        "count": entries.len(),
        "limit": args.limit,
        "auditAction": args.audit_action,
        "uri": args.uri.as_ref().map(ToString::to_string),
        "entries": entries,
    }))
}

pub(crate) fn collect_stats_snapshot(
    conn: &Connection,
    config: &ZmemoryConfig,
) -> Result<StatsSnapshot> {
    let namespace = config.namespace();
    let stats_row = conn.query_row(
        "WITH alias_nodes AS (
             SELECT e.child_uuid
             FROM edges e
             JOIN paths p ON p.edge_id = e.id AND p.namespace = e.namespace
             WHERE e.namespace = ?1
             GROUP BY e.child_uuid
             HAVING COUNT(*) > 1
         ),
         trigger_nodes AS (
             SELECT DISTINCT node_uuid
             FROM glossary_keywords
             WHERE namespace = ?1
         )
         SELECT
             (
                 SELECT COUNT(DISTINCT COALESCE(e.child_uuid, ?2))
                 FROM paths p
                 LEFT JOIN edges e ON e.id = p.edge_id AND e.namespace = p.namespace
                 WHERE p.namespace = ?1
             ),
             (
                 SELECT COUNT(DISTINCT m.id)
                 FROM memories m
                 JOIN edges e ON e.child_uuid = m.node_uuid AND e.namespace = m.namespace
                 JOIN paths p ON p.edge_id = e.id AND p.namespace = e.namespace
                 WHERE m.namespace = ?1 AND m.deprecated = FALSE
             ),
             (SELECT COUNT(*) FROM paths WHERE namespace = ?1),
             (SELECT COUNT(*) FROM glossary_keywords WHERE namespace = ?1),
             (SELECT COUNT(*) FROM alias_nodes),
             (SELECT COUNT(*) FROM trigger_nodes),
             (SELECT COUNT(*) FROM alias_nodes WHERE child_uuid NOT IN trigger_nodes),
             (
                 SELECT COUNT(*)
                 FROM edges e
                 JOIN paths p ON p.edge_id = e.id AND p.namespace = e.namespace
                 WHERE e.namespace = ?1
                   AND e.disclosure IS NOT NULL
                   AND TRIM(e.disclosure) != ''
             ),
             (
                 SELECT COUNT(*)
                 FROM edges e
                 JOIN paths p ON p.edge_id = e.id AND p.namespace = e.namespace
                 WHERE e.namespace = ?1
                   AND (e.disclosure IS NULL OR TRIM(e.disclosure) = '')
             ),
             (
                 SELECT COUNT(*)
                 FROM edges e
                 JOIN paths p ON p.edge_id = e.id AND p.namespace = e.namespace
                 WHERE e.namespace = ?1
                   AND e.disclosure IS NOT NULL
                   AND TRIM(e.disclosure) != ''
                   AND (
                     INSTR(LOWER(e.disclosure), ' or ') > 0
                     OR INSTR(LOWER(e.disclosure), ' and ') > 0
                     OR INSTR(e.disclosure, ',') > 0
                     OR INSTR(e.disclosure, '，') > 0
                     OR INSTR(e.disclosure, '、') > 0
                     OR INSTR(e.disclosure, ';') > 0
                     OR INSTR(e.disclosure, '；') > 0
                     OR INSTR(e.disclosure, '/') > 0
                     OR INSTR(e.disclosure, '&') > 0
                     OR INSTR(e.disclosure, '+') > 0
                     OR INSTR(e.disclosure, '|') > 0
                     OR INSTR(e.disclosure, '或') > 0
                   )
             ),
             (
                 SELECT COUNT(*)
                 FROM memories
                 WHERE namespace = ?1 AND deprecated = TRUE AND migrated_to IS NULL
             ),
             (
                 SELECT COUNT(*)
                 FROM memories
                 WHERE namespace = ?1 AND deprecated = TRUE AND migrated_to IS NOT NULL
             ),
             (SELECT COUNT(*) FROM search_documents WHERE namespace = ?1),
             (SELECT COUNT(*) FROM search_documents_fts WHERE namespace = ?1),
             (SELECT COUNT(*) FROM audit_log WHERE namespace = ?1),
             (SELECT MAX(created_at) FROM audit_log WHERE namespace = ?1)",
        rusqlite::params![namespace, crate::schema::ROOT_NODE_UUID],
        |row| {
            Ok(StatsSnapshot {
                node_count: row.get(0)?,
                memory_count: row.get(1)?,
                path_count: row.get(2)?,
                glossary_count: row.get(3)?,
                alias_node_count: row.get(4)?,
                trigger_node_count: row.get(5)?,
                alias_nodes_missing_triggers: row.get(6)?,
                disclosure_path_count: row.get(7)?,
                paths_missing_disclosure: row.get(8)?,
                disclosures_needing_review: row.get(9)?,
                orphaned_memory_count: row.get(10)?,
                deprecated_memory_count: row.get(11)?,
                search_document_count: row.get(12)?,
                fts_document_count: row.get(13)?,
                audit_log_count: row.get(14)?,
                latest_audit_at: row.get(15)?,
                audit_action_counts: BTreeMap::new(),
            })
        },
    )?;
    let audit_action_counts = collect_audit_action_counts(conn, namespace)?;

    Ok(StatsSnapshot {
        audit_action_counts,
        ..stats_row
    })
}

fn collect_audit_action_counts(
    conn: &Connection,
    namespace: &str,
) -> Result<BTreeMap<String, i64>> {
    let mut stmt = conn.prepare(
        "SELECT action, COUNT(*)
         FROM audit_log
         WHERE namespace = ?1
         GROUP BY action
         ORDER BY action ASC",
    )?;
    stmt.query_map([namespace], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?
    .collect::<rusqlite::Result<BTreeMap<_, _>>>()
    .map_err(Into::into)
}

pub(crate) fn alias_node_count(conn: &Connection, namespace: &str) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM (
             SELECT e.child_uuid
             FROM edges e
             JOIN paths p ON p.edge_id = e.id AND p.namespace = e.namespace
             WHERE e.namespace = ?1
             GROUP BY e.child_uuid
             HAVING COUNT(*) > 1
         )",
        [namespace],
        |row| row.get(0),
    )?)
}

pub(crate) fn trigger_node_count(conn: &Connection, namespace: &str) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(DISTINCT node_uuid) FROM glossary_keywords WHERE namespace = ?1",
        [namespace],
        |row| row.get(0),
    )?)
}

pub(crate) fn alias_nodes_missing_triggers(conn: &Connection, namespace: &str) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM (
             SELECT e.child_uuid
             FROM edges e
             JOIN paths p ON p.edge_id = e.id AND p.namespace = e.namespace
             WHERE e.namespace = ?1
             GROUP BY e.child_uuid
             HAVING COUNT(*) > 1
         ) AS alias_nodes
         WHERE alias_nodes.child_uuid NOT IN (
             SELECT DISTINCT node_uuid FROM glossary_keywords WHERE namespace = ?1
         )",
        [namespace],
        |row| row.get(0),
    )?)
}

pub(crate) fn doctor_action(conn: &Connection, config: &ZmemoryConfig) -> Result<Value> {
    let stats_snapshot = collect_stats_snapshot(conn, config)?;
    let doctor = run_doctor(
        conn,
        &config.db_path().display().to_string(),
        config.namespace(),
        &stats_snapshot,
    )?;
    let stats = stats_action_with_snapshot(config, &stats_snapshot);
    let path_resolution = common::path_resolution_payload(config);
    Ok(json!({
        "dbPath": path_resolution["dbPath"].clone(),
        "workspaceKey": path_resolution["workspaceKey"].clone(),
        "source": path_resolution["source"].clone(),
        "reason": path_resolution["reason"].clone(),
        "namespace": path_resolution["namespace"].clone(),
        "namespaceSource": path_resolution["namespaceSource"].clone(),
        "supportsNamespaceSelection": path_resolution["supportsNamespaceSelection"].clone(),
        "healthy": doctor.get("healthy").and_then(serde_json::Value::as_bool).unwrap_or(false),
        "orphanedMemoryCount": doctor.get("orphanedMemoryCount").cloned().unwrap_or_else(|| json!(0)),
        "deprecatedMemoryCount": doctor.get("deprecatedMemoryCount").cloned().unwrap_or_else(|| json!(0)),
        "aliasNodeCount": doctor.get("aliasNodeCount").cloned().unwrap_or_else(|| json!(0)),
        "triggerNodeCount": doctor.get("triggerNodeCount").cloned().unwrap_or_else(|| json!(0)),
        "aliasNodesMissingTriggers": doctor
            .get("aliasNodesMissingTriggers")
            .cloned()
            .unwrap_or_else(|| json!(0)),
        "pathsMissingDisclosure": doctor
            .get("pathsMissingDisclosure")
            .cloned()
            .unwrap_or_else(|| json!(0)),
        "disclosuresNeedingReview": doctor
            .get("disclosuresNeedingReview")
            .cloned()
            .unwrap_or_else(|| json!(0)),
        "issues": doctor.get("issues").cloned().unwrap_or_else(|| json!([])),
        "stats": stats,
        "pathResolution": path_resolution,
    }))
}

fn stats_action_with_snapshot(config: &ZmemoryConfig, stats: &StatsSnapshot) -> Value {
    let path_resolution = common::path_resolution_payload(config);
    json!({
        "dbPath": path_resolution["dbPath"].clone(),
        "workspaceKey": path_resolution["workspaceKey"].clone(),
        "source": path_resolution["source"].clone(),
        "reason": path_resolution["reason"].clone(),
        "namespace": path_resolution["namespace"].clone(),
        "namespaceSource": path_resolution["namespaceSource"].clone(),
        "supportsNamespaceSelection": path_resolution["supportsNamespaceSelection"].clone(),
        "pathResolution": path_resolution,
        "nodeCount": stats.node_count,
        "memoryCount": stats.memory_count,
        "pathCount": stats.path_count,
        "glossaryKeywordCount": stats.glossary_count,
        "orphanedMemoryCount": stats.orphaned_memory_count,
        "deprecatedMemoryCount": stats.deprecated_memory_count,
        "aliasNodeCount": stats.alias_node_count,
        "triggerNodeCount": stats.trigger_node_count,
        "aliasNodesMissingTriggers": stats.alias_nodes_missing_triggers,
        "disclosurePathCount": stats.disclosure_path_count,
        "pathsMissingDisclosure": stats.paths_missing_disclosure,
        "disclosuresNeedingReview": stats.disclosures_needing_review,
        "searchDocumentCount": stats.search_document_count,
        "ftsDocumentCount": stats.fts_document_count,
        "auditLogCount": stats.audit_log_count,
        "latestAuditAt": stats.latest_audit_at,
        "auditActionCounts": stats.audit_action_counts,
    })
}

pub(crate) fn rebuild_search_action(
    conn: &mut Connection,
    config: &ZmemoryConfig,
) -> Result<Value> {
    let count = index::rebuild_search_index(conn, config.namespace())?;
    let fts_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM search_documents_fts WHERE namespace = ?1",
        [config.namespace()],
        |row| row.get(0),
    )?;
    Ok(json!({
        "documentCount": count,
        "ftsDocumentCount": fts_count,
    }))
}
