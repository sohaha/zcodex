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
        ParsedSystemView::Alias { limit } => read_alias_view(conn, limit),
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
    Alias {
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
            Self::Alias { .. } => "alias",
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
        "alias" if tail.is_empty() => Ok(ParsedSystemView::Alias {
            limit: default_limit,
        }),
        "alias" if tail.len() == 1 => Ok(ParsedSystemView::Alias {
            limit: tail[0]
                .parse::<usize>()
                .map_err(|err| anyhow!("invalid system alias limit `{}`: {err}", tail[0]))?,
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

fn read_alias_view(conn: &Connection, limit: usize) -> Result<Value> {
    let alias_nodes = alias_node_count(conn)?;
    let trigger_nodes = trigger_node_count(conn)?;
    let alias_nodes_missing = alias_nodes_missing_triggers(conn)?;
    let entries = alias_entries(conn, limit)?;

    let coverage_percent = if alias_nodes == 0 {
        100
    } else {
        (((alias_nodes - alias_nodes_missing) * 100) / alias_nodes).clamp(0, 100)
    };
    let recommendations: Vec<Value> = entries
        .iter()
        .filter(|entry| entry["missingTriggers"].as_bool().unwrap_or(false))
        .take(3)
        .map(|entry| {
            let node_uri = entry["nodeUri"].as_str().unwrap_or_default();
            json!({
                "nodeUri": node_uri,
                "missingTriggers": entry["missingTriggers"],
                "priorityScore": entry["priorityScore"],
                "reviewPriority": entry["reviewPriority"],
                "action": "manage-triggers",
                "advice": "add specific trigger keywords to this alias node",
                "command": format!("codex zmemory manage-triggers {node_uri} --add <keyword> --json")
            })
        })
        .collect();

    Ok(json!({
        "view": "alias",
        "entryCount": entries.len(),
        "aliasNodeCount": alias_nodes,
        "triggerNodeCount": trigger_nodes,
        "aliasNodesMissingTriggers": alias_nodes_missing,
        "coveragePercent": coverage_percent,
        "recommendations": recommendations,
        "entries": entries,
    }))
}

fn alias_entries(conn: &Connection, limit: usize) -> Result<Vec<Value>> {
    let mut stmt = conn.prepare(
        "SELECT alias.node_uuid,
                alias.domain,
                alias.path,
                alias.alias_count,
                COALESCE(trigger_counts.count, 0) AS trigger_count
         FROM (
             SELECT e.child_uuid AS node_uuid,
                    MIN(p.domain) AS domain,
                    MIN(p.path) AS path,
                    COUNT(*) AS alias_count
             FROM edges e
             JOIN paths p ON p.edge_id = e.id
             GROUP BY e.child_uuid
             HAVING COUNT(*) > 1
             ORDER BY alias_count DESC, domain ASC, path ASC
             LIMIT ?1
         ) alias
         LEFT JOIN (
             SELECT node_uuid, COUNT(*) AS count
             FROM glossary_keywords
             GROUP BY node_uuid
         ) trigger_counts ON trigger_counts.node_uuid = alias.node_uuid",
    )?;

    let entries = stmt
        .query_map([limit as i64], |row| {
            let trigger_count: i64 = row.get(4)?;
            let domain: String = row.get(1)?;
            let path: String = row.get(2)?;
            let node_uri = format!("{domain}://{path}");
            let alias_count = row.get::<_, i64>(3)?;
            let missing_triggers = trigger_count == 0;
            let (review_priority, priority_score) =
                alias_review_priority(alias_count, missing_triggers);
            Ok(json!({
                "nodeUuid": row.get::<_, String>(0)?,
                "domain": domain,
                "path": path,
                "aliasCount": alias_count,
                "triggerCount": trigger_count,
                "missingTriggers": missing_triggers,
                "reviewPriority": review_priority,
                "priorityScore": priority_score,
                "nodeUri": node_uri,
            }))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut entries = entries;
    entries.sort_by(|left, right| {
        let right_score = right["priorityScore"].as_i64().unwrap_or(0);
        let left_score = left["priorityScore"].as_i64().unwrap_or(0);
        right_score
            .cmp(&left_score)
            .then_with(|| {
                let right_aliases = right["aliasCount"].as_i64().unwrap_or(0);
                let left_aliases = left["aliasCount"].as_i64().unwrap_or(0);
                right_aliases.cmp(&left_aliases)
            })
            .then_with(|| {
                let left_uri = left["nodeUri"].as_str().unwrap_or_default();
                let right_uri = right["nodeUri"].as_str().unwrap_or_default();
                left_uri.cmp(right_uri)
            })
    });

    Ok(entries)
}

fn alias_review_priority(alias_count: i64, missing_triggers: bool) -> (&'static str, i64) {
    if missing_triggers {
        let priority = if alias_count >= 3 { "high" } else { "medium" };
        let score = 100 + alias_count;
        (priority, score)
    } else {
        let priority = if alias_count >= 4 { "medium" } else { "low" };
        (priority, alias_count)
    }
}

fn alias_node_count(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM (
             SELECT child_uuid
             FROM edges
             GROUP BY child_uuid
             HAVING COUNT(*) > 1
         )",
        [],
        |row| row.get(0),
    )?)
}

fn trigger_node_count(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(DISTINCT node_uuid) FROM glossary_keywords",
        [],
        |row| row.get(0),
    )?)
}

fn alias_nodes_missing_triggers(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM (
             SELECT child_uuid
             FROM edges
             GROUP BY child_uuid
             HAVING COUNT(*) > 1
         ) AS alias_nodes
         WHERE alias_nodes.child_uuid NOT IN (
             SELECT DISTINCT node_uuid FROM glossary_keywords
         )",
        [],
        |row| row.get(0),
    )?)
}
