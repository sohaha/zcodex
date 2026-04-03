use crate::config::ZmemoryConfig;
use crate::doctor::run_doctor;
use crate::service::common;
use crate::service::index;
use anyhow::Result;
use rusqlite::Connection;
use serde_json::Value;
use serde_json::json;

pub(crate) fn stats_action(conn: &Connection, config: &ZmemoryConfig) -> Result<Value> {
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
    let alias_node_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM (
             SELECT e.child_uuid
             FROM edges e
             JOIN paths p ON p.edge_id = e.id
             GROUP BY e.child_uuid
             HAVING COUNT(*) > 1
         )",
        [],
        |row| row.get(0),
    )?;
    let trigger_node_count: i64 = conn.query_row(
        "SELECT COUNT(DISTINCT node_uuid) FROM glossary_keywords",
        [],
        |row| row.get(0),
    )?;
    let disclosure_path_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM edges e
         JOIN paths p ON p.edge_id = e.id
         WHERE e.disclosure IS NOT NULL AND TRIM(e.disclosure) != ''",
        [],
        |row| row.get(0),
    )?;
    let paths_missing_disclosure: i64 = conn.query_row(
        "SELECT COUNT(*) FROM edges e
         JOIN paths p ON p.edge_id = e.id
         WHERE e.disclosure IS NULL OR TRIM(e.disclosure) = ''",
        [],
        |row| row.get(0),
    )?;
    let disclosures_needing_review: i64 = conn.query_row(
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
    )?;
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
    let path_resolution = common::path_resolution_payload(config);

    Ok(json!({
        "dbPath": path_resolution["dbPath"].clone(),
        "workspaceKey": path_resolution["workspaceKey"].clone(),
        "source": path_resolution["source"].clone(),
        "reason": path_resolution["reason"].clone(),
        "pathResolution": path_resolution,
        "nodeCount": node_count,
        "memoryCount": memory_count,
        "pathCount": path_count,
        "glossaryKeywordCount": glossary_count,
        "orphanedMemoryCount": orphaned_memory_count,
        "deprecatedMemoryCount": deprecated_memory_count,
        "aliasNodeCount": alias_node_count,
        "triggerNodeCount": trigger_node_count,
        "disclosurePathCount": disclosure_path_count,
        "pathsMissingDisclosure": paths_missing_disclosure,
        "disclosuresNeedingReview": disclosures_needing_review,
        "searchDocumentCount": search_document_count,
        "ftsDocumentCount": fts_document_count,
    }))
}

pub(crate) fn doctor_action(conn: &Connection, config: &ZmemoryConfig) -> Result<Value> {
    let doctor = run_doctor(conn, &config.db_path().display().to_string())?;
    let stats = stats_action(conn, config)?;
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
