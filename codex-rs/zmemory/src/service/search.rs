use crate::config::ZmemoryConfig;
use crate::service::common;
use crate::service::index;
use crate::tool_api::SearchActionParams;
use anyhow::Result;
use rusqlite::Connection;
use rusqlite::params;
use serde_json::Value;
use serde_json::json;
use std::collections::HashSet;

const SEARCH_ORDER_BY: &str =
    " ORDER BY sd.priority ASC, bm25(search_documents_fts) ASC, length(f.path) ASC, f.uri ASC";

pub(crate) fn search_action(
    config: &ZmemoryConfig,
    conn: &Connection,
    args: &SearchActionParams,
) -> Result<Value> {
    let query = args.query.as_str();
    let limit = args.limit;
    let scope = args.uri.clone();
    if let Some(scope) = scope.as_ref() {
        common::ensure_readable_domain(config, conn, &scope.domain)?;
    }
    let normalized_query = index::normalize_search_query(query);

    let mut sql = String::from(
        "SELECT f.domain, f.path, f.uri, sd.content,
                sd.priority, sd.disclosure, sd.node_uuid
         FROM search_documents_fts f
         JOIN search_documents sd
           ON sd.domain = f.domain AND sd.path = f.path
         WHERE search_documents_fts MATCH ?1",
    );

    let raw_matches = if let Some(scope) = scope {
        sql.push_str(" AND f.domain = ?2 AND (f.path = ?3 OR f.path LIKE ?4)");
        sql.push_str(SEARCH_ORDER_BY);
        let prefix = if scope.path.is_empty() {
            "%".to_string()
        } else {
            format!("{}/%", scope.path)
        };
        let mut stmt = conn.prepare(&sql)?;
        stmt.query_map(
            params![normalized_query, scope.domain, scope.path, prefix],
            |row| {
                Ok(SearchMatch {
                    domain: row.get(0)?,
                    path: row.get(1)?,
                    uri: row.get(2)?,
                    content: row.get(3)?,
                    priority: row.get(4)?,
                    disclosure: row.get(5)?,
                    node_uuid: row.get(6)?,
                })
            },
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?
    } else {
        sql.push_str(SEARCH_ORDER_BY);
        let mut stmt = conn.prepare(&sql)?;
        stmt.query_map(params![normalized_query], |row| {
            Ok(SearchMatch {
                domain: row.get(0)?,
                path: row.get(1)?,
                uri: row.get(2)?,
                content: row.get(3)?,
                priority: row.get(4)?,
                disclosure: row.get(5)?,
                node_uuid: row.get(6)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?
    };
    let matches = dedupe_search_matches(raw_matches, query, limit);

    Ok(json!({
        "query": query,
        "matchCount": matches.len(),
        "matches": matches,
    }))
}

#[derive(Debug)]
struct SearchMatch {
    domain: String,
    path: String,
    uri: String,
    content: String,
    priority: i64,
    disclosure: Option<String>,
    node_uuid: String,
}

fn dedupe_search_matches(matches: Vec<SearchMatch>, query: &str, limit: usize) -> Vec<Value> {
    let mut seen = HashSet::new();
    matches
        .into_iter()
        .filter(|item| seen.insert(item.node_uuid.clone()))
        .take(limit)
        .map(|item| {
            json!({
                "domain": item.domain,
                "path": item.path,
                "uri": item.uri,
                "snippet": make_search_snippet(&item.content, query),
                "priority": item.priority,
                "disclosure": item.disclosure,
            })
        })
        .collect()
}

fn make_search_snippet(content: &str, query: &str) -> String {
    if content.is_empty() {
        return String::new();
    }
    let lower_content = content.to_lowercase();
    let lower_query = query.trim().to_lowercase();
    let mut match_len = lower_query.chars().count();
    if lower_query.is_empty() {
        return snippet(content, 80);
    }

    if let Some(pos) = lower_content.find(&lower_query) {
        let start = lower_content[..pos].chars().count();
        let end = usize::min(content.chars().count(), start + match_len + 30);
        let prefix = if start > 0 { "..." } else { "" };
        let suffix = if end < content.chars().count() {
            "..."
        } else {
            ""
        };
        return format!("{prefix}{}{suffix}", slice_chars(content, start, end));
    }

    for token in index::snippet_query_tokens(query) {
        if let Some(pos) = lower_content.find(&token) {
            if content.chars().count() <= 80 {
                return content.to_string();
            }
            match_len = token.chars().count();
            let rune_pos = lower_content[..pos].chars().count();
            let start = rune_pos.saturating_sub(30);
            let end = usize::min(content.chars().count(), rune_pos + match_len + 30);
            let prefix = if start > 0 { "..." } else { "" };
            let suffix = if end < content.chars().count() {
                "..."
            } else {
                ""
            };
            return format!("{prefix}{}{suffix}", slice_chars(content, start, end));
        }
    }

    snippet(content, 80)
}

fn snippet(content: &str, limit: usize) -> String {
    if content.chars().count() <= limit {
        return content.to_string();
    }
    format!("{}...", content.chars().take(limit).collect::<String>())
}

fn slice_chars(content: &str, start: usize, end: usize) -> String {
    content
        .chars()
        .skip(start)
        .take(end.saturating_sub(start))
        .collect()
}
