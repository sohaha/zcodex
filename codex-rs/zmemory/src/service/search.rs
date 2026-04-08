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
const DEFAULT_SNIPPET_LIMIT: usize = 80;
const SNIPPET_CONTEXT: usize = 30;
const HIGHLIGHT_PREFIX: &str = "<mark>";
const HIGHLIGHT_SUFFIX: &str = "</mark>";

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
           ON sd.namespace = f.namespace AND sd.domain = f.domain AND sd.path = f.path
         WHERE f.namespace = ?1 AND search_documents_fts MATCH ?2",
    );

    let raw_matches = if let Some(scope) = scope {
        sql.push_str(" AND f.domain = ?3 AND (f.path = ?4 OR f.path LIKE ?5)");
        sql.push_str(SEARCH_ORDER_BY);
        let prefix = if scope.path.is_empty() {
            "%".to_string()
        } else {
            format!("{}/%", scope.path)
        };
        let mut stmt = conn.prepare(&sql)?;
        stmt.query_map(
            params![
                config.namespace(),
                normalized_query,
                scope.domain,
                scope.path,
                prefix
            ],
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
        stmt.query_map(params![config.namespace(), normalized_query], |row| {
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
    if lower_query.is_empty() {
        return snippet(content, DEFAULT_SNIPPET_LIMIT);
    }

    if let Some(highlight_start) = find_match_start(&lower_content, &lower_query) {
        let highlight_len = lower_query.chars().count();
        return highlighted_window(content, highlight_start, highlight_len, true);
    }

    for token in index::snippet_query_tokens(query) {
        if let Some(highlight_start) = find_match_start(&lower_content, &token) {
            let highlight_len = token.chars().count();
            return highlighted_window(content, highlight_start, highlight_len, false);
        }
    }

    snippet(content, DEFAULT_SNIPPET_LIMIT)
}

fn find_match_start(haystack: &str, needle: &str) -> Option<usize> {
    haystack
        .find(needle)
        .map(|pos| haystack[..pos].chars().count())
}

fn highlighted_window(
    content: &str,
    highlight_start: usize,
    highlight_len: usize,
    center_match: bool,
) -> String {
    let content_len = content.chars().count();
    let start = if center_match {
        highlight_start
    } else {
        highlight_start.saturating_sub(SNIPPET_CONTEXT)
    };
    let end = usize::min(
        content_len,
        highlight_start + highlight_len + SNIPPET_CONTEXT,
    );
    let relative_highlight_start = highlight_start.saturating_sub(start);
    let relative_highlight_end = relative_highlight_start + highlight_len;
    let prefix = if start > 0 { "..." } else { "" };
    let suffix = if end < content_len { "..." } else { "" };
    let snippet_body = slice_chars(content, start, end);
    format!(
        "{prefix}{}{suffix}",
        highlight_range(
            &snippet_body,
            relative_highlight_start,
            relative_highlight_end,
        )
    )
}

fn highlight_range(content: &str, start: usize, end: usize) -> String {
    let before = slice_chars(content, 0, start);
    let highlighted = slice_chars(content, start, end);
    let after = slice_chars(content, end, content.chars().count());
    format!("{before}{HIGHLIGHT_PREFIX}{highlighted}{HIGHLIGHT_SUFFIX}{after}")
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
