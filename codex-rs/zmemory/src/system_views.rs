use crate::config::ZmemoryConfig;
use anyhow::Result;
use anyhow::anyhow;
use rusqlite::Connection;
use rusqlite::OptionalExtension;
use serde_json::Value;
use serde_json::json;
use std::collections::BTreeMap;
use std::collections::BTreeSet;

pub fn read_system_view(
    conn: &Connection,
    config: &ZmemoryConfig,
    view: &str,
    limit: usize,
) -> Result<Value> {
    match parse_system_view(view, limit)? {
        ParsedSystemView::Boot { limit } => read_boot_view(conn, config, limit),
        ParsedSystemView::Index { domain, limit } => {
            read_index_view(conn, config, domain.as_deref(), limit)
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

fn read_boot_view(conn: &Connection, config: &ZmemoryConfig, limit: usize) -> Result<Value> {
    let configured_uris = config.core_memory_uris();
    let mut entries = Vec::new();
    let mut missing_uris = Vec::new();

    for uri in configured_uris.iter().take(limit) {
        let row = conn
            .query_row(
                "SELECT uri, priority, updated_at
                 FROM search_documents
                 WHERE uri = ?1",
                [uri],
                |row| {
                    Ok(json!({
                        "uri": row.get::<_, String>(0)?,
                        "priority": row.get::<_, i64>(1)?,
                        "updatedAt": row.get::<_, String>(2)?,
                    }))
                },
            )
            .optional()?;
        if let Some(entry) = row {
            entries.push(entry);
        } else {
            missing_uris.push(uri.clone());
        }
    }

    Ok(json!({
        "view": "boot",
        "configuredUriCount": configured_uris.len(),
        "configuredUris": configured_uris,
        "missingUris": missing_uris,
        "entryCount": entries.len(),
        "entries": entries,
    }))
}

fn read_index_view(
    conn: &Connection,
    config: &ZmemoryConfig,
    domain: Option<&str>,
    limit: usize,
) -> Result<Value> {
    if let Some(domain) = domain {
        anyhow::ensure!(
            config.is_valid_domain(domain),
            "unknown domain '{domain}'. valid domains: {}",
            config.valid_domains_for_display().join(", ")
        );
    }
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
                "priorityReason": entry["priorityReason"],
                "suggestedKeywords": entry["suggestedKeywords"],
                "action": "manage-triggers",
                "advice": "add specific trigger keywords to this alias node",
                "command": suggestion_command(node_uri, &entry["suggestedKeywords"]),
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

    let rows = stmt
        .query_map([limit as i64], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut entries = rows
        .into_iter()
        .map(|(node_uuid, domain, path, alias_count, trigger_count)| {
            let node_uri = format!("{domain}://{path}");
            let missing_triggers = trigger_count == 0;
            let (review_priority, priority_score) =
                alias_review_priority(alias_count, missing_triggers);
            let priority_reason = alias_priority_reason(alias_count, missing_triggers);
            let suggested_keywords = if missing_triggers {
                infer_alias_keywords(conn, &node_uuid)?
            } else {
                Vec::new()
            };
            Ok(json!({
                "nodeUuid": node_uuid,
                "domain": domain,
                "path": path,
                "aliasCount": alias_count,
                "triggerCount": trigger_count,
                "missingTriggers": missing_triggers,
                "reviewPriority": review_priority,
                "priorityScore": priority_score,
                "priorityReason": priority_reason,
                "suggestedKeywords": suggested_keywords,
                "nodeUri": node_uri,
            }))
        })
        .collect::<Result<Vec<_>>>()?;

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

fn alias_priority_reason(alias_count: i64, missing_triggers: bool) -> String {
    if missing_triggers {
        format!("missing triggers across {alias_count} alias paths")
    } else {
        format!("covered by triggers across {alias_count} alias paths")
    }
}

fn infer_alias_keywords(conn: &Connection, node_uuid: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT p.path
         FROM edges e
         JOIN paths p ON p.edge_id = e.id
         WHERE e.child_uuid = ?1
         ORDER BY p.domain ASC, p.path ASC",
    )?;
    let paths = stmt
        .query_map([node_uuid], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut keywords = BTreeSet::new();
    for path in paths {
        for segment in path.split('/') {
            for token in segment.split(['-', '_']) {
                let candidate = token.trim().to_lowercase();
                if candidate.len() >= 2 && candidate.chars().any(|ch| ch.is_ascii_alphabetic()) {
                    keywords.insert(candidate);
                }
            }
        }
    }

    Ok(keywords.into_iter().take(3).collect())
}

fn suggestion_command(node_uri: &str, suggested_keywords: &Value) -> String {
    let Some(suggested_keywords) = suggested_keywords.as_array() else {
        return format!("codex zmemory manage-triggers {node_uri} --add <keyword> --json");
    };
    if suggested_keywords.is_empty() {
        return format!("codex zmemory manage-triggers {node_uri} --add <keyword> --json");
    }

    let args = suggested_keywords
        .iter()
        .filter_map(Value::as_str)
        .map(|keyword| format!("--add {keyword}"))
        .collect::<Vec<_>>()
        .join(" ");
    format!("codex zmemory manage-triggers {node_uri} {args} --json")
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
