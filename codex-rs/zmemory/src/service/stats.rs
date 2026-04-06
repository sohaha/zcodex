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
    let stats = collect_stats_snapshot(conn)?;
    Ok(stats_action_with_snapshot(config, &stats))
}

pub(crate) fn audit_action(conn: &Connection, args: &AuditActionParams) -> Result<Value> {
    let mut stmt = conn.prepare(
        "SELECT id, action, uri, node_uuid, details, created_at
         FROM audit_log
         WHERE (?1 IS NULL OR action = ?1)
           AND (?2 IS NULL OR uri = ?2)
         ORDER BY id DESC
         LIMIT ?3",
    )?;
    let entries = stmt
        .query_map(
            rusqlite::params![
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

pub(crate) fn collect_stats_snapshot(conn: &Connection) -> Result<StatsSnapshot> {
    let node_count: i64 = conn.query_row("SELECT COUNT(*) FROM nodes", [], |row| row.get(0))?;
    let memory_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM memories WHERE deprecated = FALSE",
        [],
        |row| row.get(0),
    )?;
    let path_count: i64 = conn.query_row("SELECT COUNT(*) FROM paths", [], |row| row.get(0))?;
    let glossary_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM glossary_keywords", [], |row| {
            row.get(0)
        })?;
    let alias_node_count = alias_node_count(conn)?;
    let trigger_node_count = trigger_node_count(conn)?;
    let disclosure_path_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM edges e
         JOIN paths p ON p.edge_id = e.id
         WHERE e.disclosure IS NOT NULL AND TRIM(e.disclosure) != ''",
        [],
        |row| row.get(0),
    )?;
    let paths_missing_disclosure = paths_missing_disclosure(conn)?;
    let disclosures_needing_review = disclosures_needing_review(conn)?;
    let orphaned_memory_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM memories WHERE deprecated = TRUE AND migrated_to IS NULL",
        [],
        |row| row.get(0),
    )?;
    let deprecated_memory_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM memories WHERE deprecated = TRUE AND migrated_to IS NOT NULL",
        [],
        |row| row.get(0),
    )?;
    let search_document_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM search_documents", [], |row| {
            row.get(0)
        })?;
    let fts_document_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM search_documents_fts", [], |row| {
            row.get(0)
        })?;
    let audit_log_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM audit_log", [], |row| row.get(0))?;
    let latest_audit_at: Option<String> =
        conn.query_row("SELECT MAX(created_at) FROM audit_log", [], |row| {
            row.get(0)
        })?;
    let audit_action_counts = collect_audit_action_counts(conn)?;

    Ok(StatsSnapshot {
        node_count,
        memory_count,
        path_count,
        glossary_count,
        alias_node_count,
        trigger_node_count,
        disclosure_path_count,
        paths_missing_disclosure,
        disclosures_needing_review,
        orphaned_memory_count,
        deprecated_memory_count,
        search_document_count,
        fts_document_count,
        audit_log_count,
        latest_audit_at,
        audit_action_counts,
    })
}

fn collect_audit_action_counts(conn: &Connection) -> Result<BTreeMap<String, i64>> {
    let mut stmt = conn.prepare(
        "SELECT action, COUNT(*)
         FROM audit_log
         GROUP BY action
         ORDER BY action ASC",
    )?;
    stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?
    .collect::<rusqlite::Result<BTreeMap<_, _>>>()
    .map_err(Into::into)
}

pub(crate) fn alias_node_count(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM (
             SELECT e.child_uuid
             FROM edges e
             JOIN paths p ON p.edge_id = e.id
             GROUP BY e.child_uuid
             HAVING COUNT(*) > 1
         )",
        [],
        |row| row.get(0),
    )?)
}

pub(crate) fn trigger_node_count(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(DISTINCT node_uuid) FROM glossary_keywords",
        [],
        |row| row.get(0),
    )?)
}

pub(crate) fn alias_nodes_missing_triggers(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM (
             SELECT e.child_uuid
             FROM edges e
             JOIN paths p ON p.edge_id = e.id
             GROUP BY e.child_uuid
             HAVING COUNT(*) > 1
         ) AS alias_nodes
         WHERE alias_nodes.child_uuid NOT IN (
             SELECT DISTINCT node_uuid FROM glossary_keywords
         )",
        [],
        |row| row.get(0),
    )?)
}

pub(crate) fn paths_missing_disclosure(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM edges e
         JOIN paths p ON p.edge_id = e.id
         WHERE e.disclosure IS NULL OR TRIM(e.disclosure) = ''",
        [],
        |row| row.get(0),
    )?)
}

pub(crate) fn disclosures_needing_review(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM edges e
         JOIN paths p ON p.edge_id = e.id
         WHERE e.disclosure IS NOT NULL
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
           )",
        [],
        |row| row.get(0),
    )?)
}

pub(crate) fn doctor_action(conn: &Connection, config: &ZmemoryConfig) -> Result<Value> {
    let stats_snapshot = collect_stats_snapshot(conn)?;
    let doctor = run_doctor(
        conn,
        &config.db_path().display().to_string(),
        &stats_snapshot,
    )?;
    let stats = stats_action_with_snapshot(config, &stats_snapshot);
    let path_resolution = common::path_resolution_payload(config);
    Ok(json!({
        "dbPath": path_resolution["dbPath"].clone(),
        "workspaceKey": path_resolution["workspaceKey"].clone(),
        "source": path_resolution["source"].clone(),
        "reason": path_resolution["reason"].clone(),
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
        "pathResolution": path_resolution,
        "nodeCount": stats.node_count,
        "memoryCount": stats.memory_count,
        "pathCount": stats.path_count,
        "glossaryKeywordCount": stats.glossary_count,
        "orphanedMemoryCount": stats.orphaned_memory_count,
        "deprecatedMemoryCount": stats.deprecated_memory_count,
        "aliasNodeCount": stats.alias_node_count,
        "triggerNodeCount": stats.trigger_node_count,
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

pub(crate) fn rebuild_search_action(conn: &mut Connection) -> Result<Value> {
    let count = index::rebuild_search_index(conn)?;
    let fts_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM search_documents_fts", [], |row| {
            row.get(0)
        })?;
    Ok(json!({
        "documentCount": count,
        "ftsDocumentCount": fts_count,
    }))
}
