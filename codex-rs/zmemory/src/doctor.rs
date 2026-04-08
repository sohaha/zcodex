use crate::service::stats::StatsSnapshot;
use anyhow::Result;
use rusqlite::Connection;
use serde_json::Value;
use serde_json::json;

pub fn run_doctor(
    conn: &Connection,
    db_path: &str,
    namespace: &str,
    stats: &StatsSnapshot,
) -> Result<Value> {
    let search_count = stats.search_document_count;
    let fts_count = stats.fts_document_count;

    let mut issues = Vec::new();
    if search_count != fts_count {
        issues.push(json!({
            "code": "fts_count_mismatch",
            "message": format!("search_documents={search_count}, search_documents_fts={fts_count}"),
        }));
    }

    let active_memory_conflicts: i64 = conn.query_row(
        "SELECT COUNT(*) FROM (
            SELECT node_uuid
            FROM memories
            WHERE namespace = ?1 AND deprecated = FALSE
            GROUP BY node_uuid
            HAVING COUNT(*) > 1
        )",
        [namespace],
        |row| row.get(0),
    )?;
    if active_memory_conflicts > 0 {
        issues.push(json!({
            "code": "multiple_active_memories",
            "message": format!("{active_memory_conflicts} nodes have more than one active memory row"),
        }));
    }

    let dangling_keywords: i64 = conn.query_row(
        "SELECT COUNT(*) FROM glossary_keywords g
         WHERE g.namespace = ?1
           AND NOT EXISTS (
             SELECT 1
             FROM edges e
             JOIN paths p ON p.edge_id = e.id AND p.namespace = e.namespace
             WHERE e.namespace = ?1 AND e.child_uuid = g.node_uuid
         )",
        [namespace],
        |row| row.get(0),
    )?;
    if dangling_keywords > 0 {
        issues.push(json!({
            "code": "dangling_keywords",
            "message": format!("{dangling_keywords} glossary keyword rows point to nodes without any live path"),
        }));
    }

    let orphaned_memories = stats.orphaned_memory_count;
    if orphaned_memories > 0 {
        issues.push(json!({
            "code": "orphaned_memories",
            "message": format!("orphaned memories: {orphaned_memories}"),
        }));
    }

    let deprecated_memories = stats.deprecated_memory_count;
    if deprecated_memories > 0 {
        issues.push(json!({
            "code": "deprecated_memories_awaiting_review",
            "message": format!("deprecated memories awaiting review: {deprecated_memories}"),
        }));
    }

    let alias_nodes = stats.alias_node_count;
    let trigger_nodes = stats.trigger_node_count;
    let alias_nodes_missing = crate::service::stats::alias_nodes_missing_triggers(conn, namespace)?;
    let paths_missing_disclosure = stats.paths_missing_disclosure;
    let disclosures_needing_review = stats.disclosures_needing_review;
    if alias_nodes_missing > 0 {
        issues.push(json!({
            "code": "alias_nodes_missing_triggers",
            "message": format!("{alias_nodes_missing} alias nodes have no keywords"),
        }));
    }
    if disclosures_needing_review > 0 {
        issues.push(json!({
            "code": "disclosures_need_review",
            "message": format!("{disclosures_needing_review} disclosures look multi-trigger or ambiguous"),
        }));
    }

    Ok(json!({
        "healthy": issues.is_empty(),
        "dbPath": db_path,
        "searchDocumentCount": search_count,
        "ftsDocumentCount": fts_count,
        "orphanedMemoryCount": orphaned_memories,
        "deprecatedMemoryCount": deprecated_memories,
        "aliasNodeCount": alias_nodes,
        "triggerNodeCount": trigger_nodes,
        "aliasNodesMissingTriggers": alias_nodes_missing,
        "pathsMissingDisclosure": paths_missing_disclosure,
        "disclosuresNeedingReview": disclosures_needing_review,
        "issues": issues,
    }))
}
