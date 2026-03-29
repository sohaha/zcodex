use anyhow::Result;
use rusqlite::Connection;
use serde_json::Value;
use serde_json::json;

pub fn run_doctor(conn: &Connection, db_path: &str) -> Result<Value> {
    let search_count: i64 = conn.query_row("SELECT COUNT(*) FROM search_documents", [], |row| {
        row.get(0)
    })?;
    let fts_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM search_documents_fts", [], |row| {
            row.get(0)
        })?;

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
            WHERE deprecated = FALSE
            GROUP BY node_uuid
            HAVING COUNT(*) > 1
        )",
        [],
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
         WHERE NOT EXISTS (
             SELECT 1
             FROM edges e
             JOIN paths p ON p.edge_id = e.id
             WHERE e.child_uuid = g.node_uuid
         )",
        [],
        |row| row.get(0),
    )?;
    if dangling_keywords > 0 {
        issues.push(json!({
            "code": "dangling_keywords",
            "message": format!("{dangling_keywords} glossary keyword rows point to nodes without any live path"),
        }));
    }

    let orphaned_memories: i64 = conn.query_row(
        "SELECT COUNT(*) FROM memories WHERE deprecated = TRUE AND migrated_to IS NULL",
        [],
        |row| row.get(0),
    )?;
    if orphaned_memories > 0 {
        issues.push(json!({
            "code": "orphaned_memories",
            "message": format!("orphaned memories: {orphaned_memories}"),
        }));
    }

    let deprecated_memories: i64 = conn.query_row(
        "SELECT COUNT(*) FROM memories WHERE deprecated = TRUE AND migrated_to IS NOT NULL",
        [],
        |row| row.get(0),
    )?;
    if deprecated_memories > 0 {
        issues.push(json!({
            "code": "deprecated_memories_awaiting_review",
            "message": format!("deprecated memories awaiting review: {deprecated_memories}"),
        }));
    }

    Ok(json!({
        "healthy": issues.is_empty(),
        "dbPath": db_path,
        "searchDocumentCount": search_count,
        "ftsDocumentCount": fts_count,
        "orphanedMemoryCount": orphaned_memories,
        "deprecatedMemoryCount": deprecated_memories,
        "issues": issues,
    }))
}
