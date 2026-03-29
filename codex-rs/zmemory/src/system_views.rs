use anyhow::Result;
use anyhow::anyhow;
use rusqlite::Connection;
use serde_json::Value;
use serde_json::json;
use std::collections::BTreeMap;

pub fn read_system_view(conn: &Connection, view: &str, limit: usize) -> Result<Value> {
    match parse_system_view(view, limit)? {
        ParsedSystemView::Boot { limit } => read_boot_view(conn, limit),
        ParsedSystemView::Index { domain, limit } => {
            read_index_view(conn, domain.as_deref(), limit)
        }
        ParsedSystemView::Recent { limit } => read_recent_view(conn, limit),
        ParsedSystemView::Glossary { limit } => read_glossary_view(conn, limit),
        other => Ok(json!({
            "view": other.raw(),
            "entryCount": 0,
            "entries": [],
        })),
    }
}

enum ParsedSystemView {
    Boot {
        limit: usize,
    },
    Index {
        domain: Option<String>,
        limit: usize,
    },
    Recent {
        limit: usize,
    },
    Glossary {
        limit: usize,
    },
    Unknown {
        raw: String,
    },
}

impl ParsedSystemView {
    fn raw(&self) -> &str {
        match self {
            Self::Boot { .. } => "boot",
            Self::Index { .. } => "index",
            Self::Recent { .. } => "recent",
            Self::Glossary { .. } => "glossary",
            Self::Unknown { raw } => raw,
        }
    }
}

fn parse_system_view(view: &str, default_limit: usize) -> Result<ParsedSystemView> {
    let trimmed = view.trim_matches('/');
    if trimmed.is_empty() {
        return Ok(ParsedSystemView::Unknown { raw: String::new() });
    }

    let mut segments = trimmed.split('/');
    let head = segments.next().unwrap_or_default();
    let tail = segments.collect::<Vec<_>>();

    match head {
        "boot" if tail.is_empty() => Ok(ParsedSystemView::Boot {
            limit: default_limit,
        }),
        "index" if tail.is_empty() => Ok(ParsedSystemView::Index {
            domain: None,
            limit: default_limit,
        }),
        "index" if tail.len() == 1 => Ok(ParsedSystemView::Index {
            domain: Some(tail[0].to_string()),
            limit: default_limit,
        }),
        "recent" if tail.is_empty() => Ok(ParsedSystemView::Recent {
            limit: default_limit,
        }),
        "recent" if tail.len() == 1 => Ok(ParsedSystemView::Recent {
            limit: tail[0]
                .parse::<usize>()
                .map_err(|err| anyhow!("invalid system recent limit `{}`: {err}", tail[0]))?,
        }),
        "glossary" if tail.is_empty() => Ok(ParsedSystemView::Glossary {
            limit: default_limit,
        }),
        _ => Ok(ParsedSystemView::Unknown {
            raw: trimmed.to_string(),
        }),
    }
}

fn read_boot_view(conn: &Connection, limit: usize) -> Result<Value> {
    let mut stmt = conn.prepare(
        "SELECT uri, priority, updated_at
         FROM search_documents
         ORDER BY priority DESC, updated_at DESC, uri ASC
         LIMIT ?1",
    )?;
    let entries = stmt
        .query_map([limit as i64], |row| {
            Ok(json!({
                "uri": row.get::<_, String>(0)?,
                "priority": row.get::<_, i64>(1)?,
                "updatedAt": row.get::<_, String>(2)?,
            }))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(json!({
        "view": "boot",
        "entryCount": entries.len(),
        "entries": entries,
    }))
}

fn read_index_view(conn: &Connection, domain: Option<&str>, limit: usize) -> Result<Value> {
    let (total, entries) = if let Some(domain) = domain {
        let total: i64 = conn.query_row(
            "SELECT COUNT(*) FROM search_documents WHERE domain = ?1",
            [domain],
            |row| row.get(0),
        )?;
        let mut stmt = conn.prepare(
            "SELECT uri, priority
             FROM search_documents
             WHERE domain = ?1
             ORDER BY uri ASC
             LIMIT ?2",
        )?;
        let entries = stmt
            .query_map((domain, limit as i64), |row| {
                Ok(json!({
                    "uri": row.get::<_, String>(0)?,
                    "priority": row.get::<_, i64>(1)?,
                }))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        (total, entries)
    } else {
        let total: i64 = conn.query_row("SELECT COUNT(*) FROM search_documents", [], |row| {
            row.get(0)
        })?;
        let mut stmt = conn.prepare(
            "SELECT uri, priority
             FROM search_documents
             ORDER BY uri ASC
             LIMIT ?1",
        )?;
        let entries = stmt
            .query_map([limit as i64], |row| {
                Ok(json!({
                    "uri": row.get::<_, String>(0)?,
                    "priority": row.get::<_, i64>(1)?,
                }))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        (total, entries)
    };

    Ok(match domain {
        Some(domain) => json!({
            "view": "index",
            "domain": domain,
            "totalCount": total,
            "entryCount": entries.len(),
            "entries": entries,
        }),
        None => json!({
            "view": "index",
            "totalCount": total,
            "entryCount": entries.len(),
            "entries": entries,
        }),
    })
}

fn read_recent_view(conn: &Connection, limit: usize) -> Result<Value> {
    let mut stmt = conn.prepare(
        "SELECT uri, updated_at
         FROM search_documents
         ORDER BY updated_at DESC, uri ASC
         LIMIT ?1",
    )?;
    let entries = stmt
        .query_map([limit as i64], |row| {
            Ok(json!({
                "uri": row.get::<_, String>(0)?,
                "updatedAt": row.get::<_, String>(1)?,
            }))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(json!({
        "view": "recent",
        "entryCount": entries.len(),
        "entries": entries,
    }))
}

fn read_glossary_view(conn: &Connection, limit: usize) -> Result<Value> {
    let mut stmt = conn.prepare(
        "SELECT g.keyword, p.domain, p.path
         FROM glossary_keywords g
         JOIN edges e ON e.child_uuid = g.node_uuid
         JOIN paths p ON p.edge_id = e.id
         ORDER BY g.keyword ASC, p.domain ASC, p.path ASC",
    )?;
    let rows = stmt
        .query_map([], |row| {
            let keyword: String = row.get(0)?;
            let domain: String = row.get(1)?;
            let path: String = row.get(2)?;
            Ok((keyword, format!("{domain}://{path}")))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut grouped: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (keyword, uri) in rows {
        grouped.entry(keyword).or_default().push(uri);
    }

    let entries = grouped
        .into_iter()
        .take(limit)
        .map(|(keyword, uris)| {
            json!({
                "keyword": keyword,
                "uris": uris,
            })
        })
        .collect::<Vec<_>>();

    Ok(json!({
        "view": "glossary",
        "entryCount": entries.len(),
        "entries": entries,
    }))
}
