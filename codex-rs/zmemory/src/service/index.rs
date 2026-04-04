//! Full-text search index management.

use anyhow::Result;
use rusqlite::Connection;
use rusqlite::params;

use super::search::build_search_terms;

pub(crate) fn rebuild_search_index(conn: &mut Connection) -> Result<i64> {
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM search_documents", [])?;
    tx.execute("DELETE FROM search_documents_fts", [])?;

    let rows = {
        let mut stmt = tx.prepare(
            "SELECT
                p.domain,
                p.path,
                e.child_uuid,
                m.id,
                m.content,
                e.disclosure,
                e.priority,
                COALESCE((
                    SELECT GROUP_CONCAT(keyword, ' ')
                    FROM glossary_keywords
                    WHERE node_uuid = e.child_uuid
                ), '')
             FROM paths p
             JOIN edges e ON e.id = p.edge_id
             JOIN memories m ON m.node_uuid = e.child_uuid AND m.deprecated = FALSE
             ORDER BY p.domain ASC, p.path ASC",
        )?;
        stmt.query_map([], |row| {
            let domain: String = row.get(0)?;
            let path: String = row.get(1)?;
            let node_uuid: String = row.get(2)?;
            let memory_id: i64 = row.get(3)?;
            let content: String = row.get(4)?;
            let disclosure: Option<String> = row.get(5)?;
            let priority: i64 = row.get(6)?;
            let keywords: String = row.get(7)?;
            Ok((
                domain, path, node_uuid, memory_id, content, disclosure, priority, keywords,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?
    };

    for (domain, path, node_uuid, memory_id, content, disclosure, priority, keywords) in rows {
        let uri = format!("{domain}://{path}");
        let search_terms = build_search_terms(&domain, &path, &content, &keywords);
        tx.execute(
            "INSERT INTO search_documents(
                domain, path, node_uuid, memory_id, uri, content, disclosure, search_terms, priority
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                domain,
                path,
                node_uuid,
                memory_id,
                uri,
                content,
                disclosure,
                search_terms,
                priority
            ],
        )?;
        tx.execute(
            "INSERT INTO search_documents_fts(domain, path, uri, content, disclosure, search_terms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![domain, path, uri, content, disclosure, search_terms],
        )?;
    }
    tx.commit()?;

    conn.query_row("SELECT COUNT(*) FROM search_documents", [], |row| {
        row.get(0)
    })
    .map_err(Into::into)
}
