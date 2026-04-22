use crate::config::ZmemoryConfig;
use crate::doctor::run_doctor;
use crate::service::common;
use crate::service::contracts::AuditEntryContract;
use crate::service::contracts::AuditResultContract;
use crate::service::contracts::ContentGovernanceResultContract;
use crate::service::contracts::MaintenanceDoctorContract;
use crate::service::contracts::MaintenanceStatsContract;
use crate::service::contracts::RebuildSearchResultContract;
use crate::service::governance;
use crate::service::index;
use crate::tool_api::AuditActionParams;
use crate::tool_api::ZmemoryUri;
use anyhow::Result;
use rusqlite::Connection;
use serde_json::Value;
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
    pub(crate) content_governance_issue_count: i64,
    pub(crate) content_governance_conflict_count: i64,
    pub(crate) orphaned_memory_count: i64,
    pub(crate) deprecated_memory_count: i64,
    pub(crate) search_document_count: i64,
    pub(crate) fts_document_count: i64,
    pub(crate) audit_log_count: i64,
    pub(crate) latest_audit_at: Option<String>,
    pub(crate) audit_action_counts: BTreeMap<String, i64>,
    pub(crate) content_governance_results: Vec<ContentGovernanceResultContract>,
}

pub(crate) fn stats_action(conn: &Connection, config: &ZmemoryConfig) -> Result<Value> {
    let stats = collect_stats_snapshot(conn, config)?;
    serde_json::to_value(stats_contract(config, &stats)).map_err(Into::into)
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
                Ok(AuditEntryContract {
                    id: row.get(0)?,
                    action: row.get(1)?,
                    uri: row.get(2)?,
                    node_uuid: row.get(3)?,
                    details: serde_json::from_str::<Value>(&details)
                        .unwrap_or(Value::String(details)),
                    created_at: row.get(5)?,
                })
            },
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    serde_json::to_value(AuditResultContract {
        count: entries.len(),
        limit: args.limit,
        audit_action: args.audit_action.clone(),
        uri: args.uri.as_ref().map(ToString::to_string),
        entries,
    })
    .map_err(Into::into)
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
                content_governance_issue_count: 0,
                content_governance_conflict_count: 0,
                orphaned_memory_count: row.get(10)?,
                deprecated_memory_count: row.get(11)?,
                search_document_count: row.get(12)?,
                fts_document_count: row.get(13)?,
                audit_log_count: row.get(14)?,
                latest_audit_at: row.get(15)?,
                audit_action_counts: BTreeMap::new(),
                content_governance_results: Vec::new(),
            })
        },
    )?;
    let audit_action_counts = collect_audit_action_counts(conn, namespace)?;
    let content_governance_results = collect_content_governance_results(conn, config)?;
    let content_governance_issue_count = content_governance_results
        .iter()
        .filter(|result| result.status != "accepted")
        .count() as i64;
    let content_governance_conflict_count = content_governance_results
        .iter()
        .filter(|result| result.status == "conflict")
        .count() as i64;

    Ok(StatsSnapshot {
        audit_action_counts,
        content_governance_issue_count,
        content_governance_conflict_count,
        content_governance_results,
        ..stats_row
    })
}

fn collect_content_governance_results(
    conn: &Connection,
    config: &ZmemoryConfig,
) -> Result<Vec<ContentGovernanceResultContract>> {
    let mut results = Vec::new();
    for raw_uri in governance::governed_uris() {
        let uri = ZmemoryUri::parse(raw_uri)?;
        let Some(row) = common::find_path_row(conn, config, &uri)? else {
            continue;
        };
        let Some(memory) = common::read_active_memory(conn, config.namespace(), &row.node_uuid)?
        else {
            continue;
        };
        results.push(governance::evaluate_content(&uri, &memory.content));
    }
    Ok(results)
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
    let stats = stats_contract(config, &stats_snapshot);
    let doctor = run_doctor(conn, config.namespace(), &stats_snapshot)?;
    let path_resolution = common::path_resolution_contract(config);

    serde_json::to_value(MaintenanceDoctorContract {
        db_path: path_resolution.db_path.clone(),
        workspace_key: path_resolution.workspace_key.clone(),
        source: path_resolution.source,
        reason: path_resolution.reason.clone(),
        namespace: path_resolution.namespace.clone(),
        namespace_source: path_resolution.namespace_source.clone(),
        supports_namespace_selection: path_resolution.supports_namespace_selection,
        healthy: doctor.healthy,
        orphaned_memory_count: doctor.orphaned_memory_count,
        deprecated_memory_count: doctor.deprecated_memory_count,
        alias_node_count: doctor.alias_node_count,
        trigger_node_count: doctor.trigger_node_count,
        alias_nodes_missing_triggers: doctor.alias_nodes_missing_triggers,
        paths_missing_disclosure: doctor.paths_missing_disclosure,
        disclosures_needing_review: doctor.disclosures_needing_review,
        content_governance_issue_count: doctor.content_governance_issue_count,
        content_governance_conflict_count: doctor.content_governance_conflict_count,
        issues: doctor.issues,
        stats,
        path_resolution,
    })
    .map_err(Into::into)
}

fn stats_contract(config: &ZmemoryConfig, stats: &StatsSnapshot) -> MaintenanceStatsContract {
    let path_resolution = common::path_resolution_contract(config);
    MaintenanceStatsContract {
        db_path: path_resolution.db_path.clone(),
        workspace_key: path_resolution.workspace_key.clone(),
        source: path_resolution.source,
        reason: path_resolution.reason.clone(),
        namespace: path_resolution.namespace.clone(),
        namespace_source: path_resolution.namespace_source.clone(),
        supports_namespace_selection: path_resolution.supports_namespace_selection,
        path_resolution,
        node_count: stats.node_count,
        memory_count: stats.memory_count,
        path_count: stats.path_count,
        glossary_keyword_count: stats.glossary_count,
        orphaned_memory_count: stats.orphaned_memory_count,
        deprecated_memory_count: stats.deprecated_memory_count,
        alias_node_count: stats.alias_node_count,
        trigger_node_count: stats.trigger_node_count,
        alias_nodes_missing_triggers: stats.alias_nodes_missing_triggers,
        disclosure_path_count: stats.disclosure_path_count,
        paths_missing_disclosure: stats.paths_missing_disclosure,
        disclosures_needing_review: stats.disclosures_needing_review,
        content_governance_issue_count: stats.content_governance_issue_count,
        content_governance_conflict_count: stats.content_governance_conflict_count,
        search_document_count: stats.search_document_count,
        fts_document_count: stats.fts_document_count,
        audit_log_count: stats.audit_log_count,
        latest_audit_at: stats.latest_audit_at.clone(),
        audit_action_counts: stats.audit_action_counts.clone(),
    }
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

    serde_json::to_value(RebuildSearchResultContract {
        document_count: count,
        fts_document_count: fts_count,
    })
    .map_err(Into::into)
}
