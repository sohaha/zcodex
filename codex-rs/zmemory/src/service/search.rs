use crate::config::ZmemoryConfig;
use crate::service::common::domain_checks;
use crate::tool_api::ZmemoryToolCallParam;
use crate::tool_api::ZmemoryUri;
use anyhow::Result;
use rusqlite::Connection;
use rusqlite::params;
use serde_json::Value;
use serde_json::json;
use std::collections::HashSet;

pub(crate) fn search_action(
    config: &ZmemoryConfig,
    conn: &Connection,
    args: &ZmemoryToolCallParam,
) -> Result<Value> {
    let query = args
        .query
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("`query` is required for action=search"))?;
    let limit = args.limit.unwrap_or(10);
    let scope = args.uri.as_deref().map(ZmemoryUri::parse).transpose()?;
    if let Some(scope) = scope.as_ref() {
        domain_checks::ensure_readable_domain(config, conn, &scope.domain)?;
    }
    let normalized_query = normalize_search_query(query);

    let mut sql = String::from(
        "SELECT f.domain, f.path, f.uri, sd.content,
                sd.priority, sd.disclosure, sd.node_uuid
         FROM search_documents_fts f
         JOIN search_documents sd
           ON sd.domain = f.domain AND sd.path = f.path
         WHERE search_documents_fts MATCH ?1",
    );

    let row_mapper = |row: &rusqlite::Row| -> rusqlite::Result<SearchMatch> {
        Ok(SearchMatch {
            domain: row.get(0)?,
            path: row.get(1)?,
            uri: row.get(2)?,
            content: row.get(3)?,
            priority: row.get(4)?,
            disclosure: row.get(5)?,
            node_uuid: row.get(6)?,
        })
    };

    let raw_matches = if let Some(scope) = scope {
        sql.push_str(" AND f.domain = ?2 AND (f.path = ?3 OR f.path LIKE ?4) ORDER BY bm25(search_documents_fts) ASC, f.uri ASC");
        let prefix = if scope.path.is_empty() {
            "%".to_string()
        } else {
            format!("{}/%", scope.path)
        };
        let mut stmt = conn.prepare(&sql)?;
        stmt.query_map(
            params![normalized_query, scope.domain, scope.path, prefix],
            row_mapper,
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?
    } else {
        sql.push_str(" ORDER BY bm25(search_documents_fts) ASC, f.uri ASC");
        let mut stmt = conn.prepare(&sql)?;
        stmt.query_map(params![normalized_query], row_mapper)?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    let matches = dedupe_and_sort_search_matches(raw_matches, query, limit);

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

fn dedupe_and_sort_search_matches(
    matches: Vec<SearchMatch>,
    query: &str,
    limit: usize,
) -> Vec<Value> {
    let mut matches = matches;
    matches.sort_by(|left, right| {
        left.priority
            .cmp(&right.priority)
            .then_with(|| left.path.len().cmp(&right.path.len()))
            .then_with(|| left.uri.cmp(&right.uri))
    });

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

    for token in snippet_query_tokens(query) {
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

fn normalize_search_query(query: &str) -> String {
    if query.chars().any(is_cjk_rune) {
        normalize_search_field(query)
    } else {
        let tokens = snippet_query_tokens(query);
        if tokens.is_empty() {
            normalize_search_field(query)
        } else {
            tokens
                .into_iter()
                .map(|token| escape_fts5_token(&token))
                .collect::<Vec<_>>()
                .join(" ")
        }
    }
}

/// Escape FTS5 special syntax tokens (AND, OR, NOT, NEAR, *, ", parentheses)
/// by wrapping them in double-quotes to prevent query injection.
fn escape_fts5_token(token: &str) -> String {
    let upper = token.to_uppercase();
    if matches!(upper.as_str(), "AND" | "OR" | "NOT" | "NEAR") || token == "*" {
        format!("\"{token}\"")
    } else if token.contains('"') || token.contains('(') || token.contains(')') {
        let escaped = token.replace('"', "\"\"");
        format!("\"{escaped}\"")
    } else {
        token.to_string()
    }
}

fn normalize_search_field(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            ':' | '/' | '.' | '-' => ' ',
            _ => ch.to_ascii_lowercase(),
        })
        .collect::<String>()
}

fn ascii_search_tokens(value: &str) -> Vec<String> {
    snippet_query_tokens(value)
        .into_iter()
        .filter(|token| {
            token
                .chars()
                .any(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        })
        .collect()
}

fn build_search_terms(domain: &str, path: &str, content: &str, keywords: &str) -> String {
    let mut terms = vec![
        domain.to_string(),
        normalize_search_field(domain),
        normalize_search_field(path),
        normalize_search_field(&format!("{domain}://{path}")),
        normalize_search_field(content),
        ascii_search_tokens(content).join(" "),
    ];
    if !keywords.trim().is_empty() {
        terms.push(normalize_search_field(keywords.trim()));
    }
    terms.join(" ")
}

pub(crate) fn build_index_search_terms(
    domain: &str,
    path: &str,
    content: &str,
    keywords: &str,
) -> String {
    build_search_terms(domain, path, content, keywords)
}

fn snippet_query_tokens(query: &str) -> Vec<String> {
    let normalized: String = query
        .chars()
        .map(|ch| match ch {
            ':' | '/' | '.' | '-' => ' ',
            _ => ch,
        })
        .collect();
    let mut tokens = Vec::new();
    let mut ascii_run = String::new();
    let mut cjk_run = String::new();

    let flush_ascii = |tokens: &mut Vec<String>, ascii_run: &mut String| {
        if ascii_run.is_empty() {
            return;
        }
        tokens.push(ascii_run.to_lowercase());
        ascii_run.clear();
    };
    let flush_cjk = |tokens: &mut Vec<String>, cjk_run: &mut String| {
        if cjk_run.is_empty() {
            return;
        }
        tokens.extend(split_cjk_run(cjk_run));
        cjk_run.clear();
    };

    for ch in normalized.chars() {
        if ch == '_' || ch.is_ascii_alphanumeric() {
            flush_cjk(&mut tokens, &mut cjk_run);
            if should_split_ascii_snippet_run(&ascii_run, ch) {
                flush_ascii(&mut tokens, &mut ascii_run);
            }
            ascii_run.push(ch);
        } else if is_cjk_rune(ch) {
            flush_ascii(&mut tokens, &mut ascii_run);
            cjk_run.push(ch);
        } else {
            flush_ascii(&mut tokens, &mut ascii_run);
            flush_cjk(&mut tokens, &mut cjk_run);
        }
    }
    flush_ascii(&mut tokens, &mut ascii_run);
    flush_cjk(&mut tokens, &mut cjk_run);

    let mut seen = std::collections::BTreeSet::new();
    tokens
        .into_iter()
        .filter(|token| !token.is_empty() && seen.insert(token.clone()))
        .collect()
}

fn should_split_ascii_snippet_run(current: &str, next: char) -> bool {
    if current.is_empty() || !next.is_ascii_uppercase() {
        return false;
    }
    current
        .chars()
        .last()
        .is_some_and(|last| last.is_ascii_lowercase() || last.is_ascii_digit())
}

fn split_cjk_run(run: &str) -> Vec<String> {
    let chars = run.chars().collect::<Vec<_>>();
    if chars.len() <= 1 {
        return vec![run.to_string()];
    }
    let mut tokens = Vec::new();
    for width in [3_usize, 2, 1] {
        if chars.len() < width {
            continue;
        }
        for start in 0..=chars.len() - width {
            tokens.push(chars[start..start + width].iter().collect::<String>());
        }
    }
    tokens
}

fn is_cjk_rune(ch: char) -> bool {
    matches!(
        ch as u32,
        0x4E00..=0x9FFF
            | 0x3400..=0x4DBF
            | 0x20000..=0x2A6DF
            | 0x2A700..=0x2B73F
            | 0x2B740..=0x2B81F
            | 0x2B820..=0x2CEAF
            | 0xF900..=0xFAFF
            | 0x2F800..=0x2FA1F
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn escape_fts5_queries_special_tokens() {
        // FTS5 operators should be quoted
        assert_eq!(escape_fts5_token("AND"), "\"AND\"");
        assert_eq!(escape_fts5_token("or"), "\"or\"");
        assert_eq!(escape_fts5_token("NOT"), "\"NOT\"");
        assert_eq!(escape_fts5_token("near"), "\"near\"");
        assert_eq!(escape_fts5_token("*"), "\"*\"");

        // Normal tokens should be untouched
        assert_eq!(escape_fts5_token("hello"), "hello");
        assert_eq!(escape_fts5_token("search_query"), "search_query");

        // Tokens with quotes should be quoted with escaped internal quotes
        assert_eq!(
            escape_fts5_token("test(\"quoted\")"),
            "\"test(\"\"quoted\"\")\""
        );
    }

    #[test]
    fn normalize_search_query_escapes_fts5_operators() {
        let result = normalize_search_query("hello AND world OR test");
        // snippet_query_tokens lowercases, so FTS5 operators appear quoted but lowercase
        assert_eq!(result, "hello \"and\" world \"or\" test");
    }

    #[test]
    fn normalize_search_query_normal_ascii() {
        let result = normalize_search_query("hello world");
        assert_eq!(result, "hello world");
    }

    #[test]
    fn normalize_search_query_cjk() {
        let result = normalize_search_query("中文搜索");
        assert_eq!(result, "中文搜索");
    }
}
