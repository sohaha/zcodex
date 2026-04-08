use crate::config::ZmemoryConfig;
use crate::config::boot_role_bindings_for_uris;
use crate::config::default_boot_role_bindings;
use crate::config::default_core_memory_uris;
use crate::config::default_valid_domains;
use crate::config::project_key_for_workspace;
use crate::config::unassigned_boot_uris;
use crate::config::zmemory_db_path;
use anyhow::Result;
use anyhow::anyhow;
use rusqlite::Connection;
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
        ParsedSystemView::Defaults => read_defaults_view(config),
        ParsedSystemView::Workspace => read_workspace_view(conn, config),
        ParsedSystemView::Index { domain, limit } => {
            read_index_view(conn, config, domain.as_deref(), limit)
        }
        ParsedSystemView::Paths { domain, limit } => {
            read_paths_view(conn, config, domain.as_deref(), limit)
        }
        ParsedSystemView::Recent { limit } => read_recent_view(conn, config, limit),
        ParsedSystemView::Glossary { limit } => read_glossary_view(conn, config, limit),
        ParsedSystemView::Alias { limit } => read_alias_view(conn, config, limit),
        ParsedSystemView::Unknown { raw } => Err(anyhow!(
            "unknown system view `{raw}`. supported views: boot, defaults, workspace, index, index/<domain>, paths, paths/<domain>, recent, recent/<n>, glossary, alias, alias/<n>"
        )),
    }
}

enum ParsedSystemView {
    Boot {
        limit: usize,
    },
    Defaults,
    Workspace,
    Index {
        domain: Option<String>,
        limit: usize,
    },
    Paths {
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

fn parse_system_view(view: &str, default_limit: usize) -> Result<ParsedSystemView> {
    let default_limit = default_limit.clamp(1, 100);
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
        "defaults" if tail.is_empty() => Ok(ParsedSystemView::Defaults),
        "workspace" if tail.is_empty() => Ok(ParsedSystemView::Workspace),
        "index" if tail.is_empty() => Ok(ParsedSystemView::Index {
            domain: None,
            limit: default_limit,
        }),
        "index" if tail.len() == 1 => Ok(ParsedSystemView::Index {
            domain: Some(tail[0].to_string()),
            limit: default_limit,
        }),
        "paths" if tail.is_empty() => Ok(ParsedSystemView::Paths {
            domain: None,
            limit: default_limit,
        }),
        "paths" if tail.len() == 1 => Ok(ParsedSystemView::Paths {
            domain: Some(tail[0].to_string()),
            limit: default_limit,
        }),
        "recent" if tail.is_empty() => Ok(ParsedSystemView::Recent {
            limit: default_limit,
        }),
        "recent" if tail.len() == 1 => Ok(ParsedSystemView::Recent {
            limit: tail[0]
                .parse::<usize>()
                .map_err(|err| anyhow!("invalid system recent limit `{}`: {err}", tail[0]))?
                .clamp(1, 100),
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
                .map_err(|err| anyhow!("invalid system alias limit `{}`: {err}", tail[0]))?
                .clamp(1, 100),
        }),
        _ => Ok(ParsedSystemView::Unknown {
            raw: trimmed.to_string(),
        }),
    }
}

fn read_boot_view(conn: &Connection, config: &ZmemoryConfig, limit: usize) -> Result<Value> {
    let configured_uris = config.core_memory_uris();
    let boot_roles = boot_role_bindings_for_uris(configured_uris);
    let unassigned_uris = unassigned_boot_uris(configured_uris);
    let role_by_uri = boot_roles
        .iter()
        .filter_map(|binding| binding.uri.as_deref().map(|uri| (uri, binding.role)))
        .collect::<BTreeMap<_, _>>();
    let scoped_uris = configured_uris
        .iter()
        .take(limit)
        .cloned()
        .collect::<Vec<_>>();
    let mut entries = Vec::new();
    let mut present_uris = Vec::new();
    let mut missing_uris = Vec::new();
    let mut anchors = Vec::new();

    let indexed_entries = search_documents_by_uri(conn, config.namespace(), &scoped_uris)?;
    for uri in scoped_uris {
        let role = role_by_uri.get(uri.as_str()).map(|role| role.key());
        if let Some(entry) = indexed_entries.get(&uri) {
            entries.push(entry.clone());
            present_uris.push(uri.clone());
            anchors.push(json!({
                "uri": uri,
                "role": role,
                "exists": true,
                "priority": entry["priority"].clone(),
                "updatedAt": entry["updatedAt"].clone(),
            }));
        } else {
            missing_uris.push(uri.clone());
            anchors.push(json!({
                "uri": uri,
                "role": role,
                "exists": false,
            }));
        }
    }
    let boot_healthy = missing_uris.is_empty();
    let missing_uri_count = missing_uris.len();

    Ok(json!({
        "view": "boot",
        "configuredUriCount": configured_uris.len(),
        "configuredUris": configured_uris,
        "bootRoles": role_bindings_json(&boot_roles),
        "unassignedUris": unassigned_uris,
        "presentUris": present_uris,
        "missingUris": missing_uris,
        "missingUriCount": missing_uri_count,
        "bootHealthy": boot_healthy,
        "entryCount": entries.len(),
        "entries": entries,
        "anchors": anchors,
    }))
}

fn search_documents_by_uri(
    conn: &Connection,
    namespace: &str,
    uris: &[String],
) -> Result<BTreeMap<String, Value>> {
    if uris.is_empty() {
        return Ok(BTreeMap::new());
    }

    let placeholders = (0..uris.len()).map(|_| "?").collect::<Vec<_>>().join(", ");
    let sql = format!(
        "SELECT uri, priority, updated_at
         FROM search_documents
         WHERE namespace = ?1 AND uri IN ({placeholders})"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(
            rusqlite::params_from_iter(
                std::iter::once(namespace).chain(uris.iter().map(String::as_str)),
            ),
            |row| {
                let uri = row.get::<_, String>(0)?;
                Ok((
                    uri.clone(),
                    json!({
                        "uri": uri,
                        "priority": row.get::<_, i64>(1)?,
                        "updatedAt": row.get::<_, String>(2)?,
                    }),
                ))
            },
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(rows.into_iter().collect())
}

fn role_bindings_json(bindings: &[crate::config::BootRoleBinding]) -> Vec<Value> {
    bindings
        .iter()
        .map(|binding| {
            json!({
                "role": binding.role.key(),
                "uri": binding.uri.clone(),
                "configured": binding.uri.is_some(),
                "description": binding.role.description(),
            })
        })
        .collect()
}

fn read_defaults_view(config: &ZmemoryConfig) -> Result<Value> {
    let default_workspace_key = project_key_for_workspace(config.workspace_base());
    let default_db_path = zmemory_db_path(config.codex_home(), config.workspace_base());
    let boot_roles = default_boot_role_bindings();
    Ok(json!({
        "view": "defaults",
        "validDomains": default_valid_domains(),
        "coreMemoryUris": default_core_memory_uris(),
        "namespace": config.namespace(),
        "namespaceSource": config.namespace_source(),
        "supportsNamespaceSelection": config.supports_namespace_selection(),
        "bootRoles": role_bindings_json(&boot_roles),
        "unassignedUris": Vec::<String>::new(),
        "defaultPathPolicy": {
            "mode": "projectScoped",
            "dbPath": default_db_path.display().to_string(),
            "workspaceKey": default_workspace_key,
            "source": "projectScoped",
            "reason": format!("defaulted to project scope {}", default_db_path.display()),
        },
        "recommendedDomains": default_valid_domains(),
        "recommendedBootUris": default_core_memory_uris(),
        "recommendedBootRoles": role_bindings_json(&boot_roles),
        "bootContract": {
            "view": "boot",
            "limitControlsAnchors": true,
            "entriesListOnlyPresentAnchors": true,
            "missingUrisAreAuthoritative": true,
            "anchors": default_core_memory_uris(),
            "roles": role_bindings_json(&boot_roles),
            "unassignedUris": Vec::<String>::new(),
        },
    }))
}

fn read_workspace_view(conn: &Connection, config: &ZmemoryConfig) -> Result<Value> {
    let resolution = config.path_resolution();
    let default_workspace_key = project_key_for_workspace(config.workspace_base());
    let default_db_path = zmemory_db_path(config.codex_home(), config.workspace_base());
    let boot_roles = boot_role_bindings_for_uris(config.core_memory_uris());
    let unassigned_uris = unassigned_boot_uris(config.core_memory_uris());
    let boot = read_boot_view(conn, config, usize::MAX)?;
    let boot_healthy = boot
        .get("bootHealthy")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    Ok(json!({
        "view": "workspace",
        "workspaceBase": config.workspace_base().display().to_string(),
        "dbPath": resolution.db_path.display().to_string(),
        "workspaceKey": resolution.workspace_key.clone(),
        "source": resolution.source,
        "reason": resolution.reason.clone(),
        "namespace": config.namespace(),
        "namespaceSource": config.namespace_source(),
        "supportsNamespaceSelection": config.supports_namespace_selection(),
        "hasExplicitZmemoryPath": matches!(resolution.source, crate::path_resolution::ZmemoryPathSource::Explicit),
        "defaultWorkspaceKey": default_workspace_key,
        "defaultDbPath": default_db_path.display().to_string(),
        "dbPathDiffers": resolution.db_path != default_db_path,
        "validDomains": config.valid_domains(),
        "coreMemoryUris": config.core_memory_uris(),
        "bootRoles": role_bindings_json(&boot_roles),
        "unassignedUris": unassigned_uris,
        "boot": boot,
        "bootHealthy": boot_healthy,
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
            "SELECT COUNT(*) FROM search_documents WHERE namespace = ?1 AND domain = ?2",
            rusqlite::params![config.namespace(), domain],
            |row| row.get(0),
        )?;
        let mut stmt = conn.prepare(
            "SELECT uri, priority
             FROM search_documents
             WHERE namespace = ?1 AND domain = ?2
             ORDER BY uri ASC
             LIMIT ?3",
        )?;
        let entries = stmt
            .query_map(
                rusqlite::params![config.namespace(), domain, limit as i64],
                |row| {
                    Ok(json!({
                        "uri": row.get::<_, String>(0)?,
                        "priority": row.get::<_, i64>(1)?,
                    }))
                },
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        (total, entries)
    } else {
        let total: i64 = conn.query_row(
            "SELECT COUNT(*) FROM search_documents WHERE namespace = ?1",
            [config.namespace()],
            |row| row.get(0),
        )?;
        let mut stmt = conn.prepare(
            "SELECT uri, priority
             FROM search_documents
             WHERE namespace = ?1
             ORDER BY uri ASC
             LIMIT ?2",
        )?;
        let entries = stmt
            .query_map(rusqlite::params![config.namespace(), limit as i64], |row| {
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

fn read_paths_view(
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
            "SELECT COUNT(*) FROM search_documents WHERE namespace = ?1 AND domain = ?2",
            rusqlite::params![config.namespace(), domain],
            |row| row.get(0),
        )?;
        let mut stmt = conn.prepare(
            "SELECT domain, path, uri, node_uuid, priority
             FROM search_documents
             WHERE namespace = ?1 AND domain = ?2
             ORDER BY path ASC, uri ASC
             LIMIT ?3",
        )?;
        let entries = stmt
            .query_map(
                rusqlite::params![config.namespace(), domain, limit as i64],
                |row| {
                    Ok(json!({
                        "domain": row.get::<_, String>(0)?,
                        "path": row.get::<_, String>(1)?,
                        "uri": row.get::<_, String>(2)?,
                        "nodeUuid": row.get::<_, String>(3)?,
                        "priority": row.get::<_, i64>(4)?,
                    }))
                },
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        (total, entries)
    } else {
        let total: i64 = conn.query_row(
            "SELECT COUNT(*) FROM search_documents WHERE namespace = ?1",
            [config.namespace()],
            |row| row.get(0),
        )?;
        let mut stmt = conn.prepare(
            "SELECT domain, path, uri, node_uuid, priority
             FROM search_documents
             WHERE namespace = ?1
             ORDER BY domain ASC, path ASC, uri ASC
             LIMIT ?2",
        )?;
        let entries = stmt
            .query_map(rusqlite::params![config.namespace(), limit as i64], |row| {
                Ok(json!({
                    "domain": row.get::<_, String>(0)?,
                    "path": row.get::<_, String>(1)?,
                    "uri": row.get::<_, String>(2)?,
                    "nodeUuid": row.get::<_, String>(3)?,
                    "priority": row.get::<_, i64>(4)?,
                }))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        (total, entries)
    };

    Ok(match domain {
        Some(domain) => json!({
            "view": "paths",
            "domain": domain,
            "totalCount": total,
            "entryCount": entries.len(),
            "entries": entries,
        }),
        None => json!({
            "view": "paths",
            "totalCount": total,
            "entryCount": entries.len(),
            "entries": entries,
        }),
    })
}

fn read_recent_view(conn: &Connection, config: &ZmemoryConfig, limit: usize) -> Result<Value> {
    let mut stmt = conn.prepare(
        "SELECT MIN(uri) AS uri, MAX(updated_at) AS updated_at
         FROM search_documents
         WHERE namespace = ?1
         GROUP BY node_uuid
         ORDER BY updated_at DESC, uri ASC
         LIMIT ?2",
    )?;
    let entries = stmt
        .query_map(rusqlite::params![config.namespace(), limit as i64], |row| {
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

fn read_glossary_view(conn: &Connection, config: &ZmemoryConfig, limit: usize) -> Result<Value> {
    let mut stmt = conn.prepare(
        "SELECT g.keyword, p.domain, p.path
         FROM glossary_keywords g
         JOIN edges e ON e.namespace = g.namespace AND e.child_uuid = g.node_uuid
         JOIN paths p ON p.edge_id = e.id AND p.namespace = e.namespace
         WHERE g.namespace = ?1
         ORDER BY g.keyword ASC, p.domain ASC, p.path ASC",
    )?;
    let rows = stmt
        .query_map([config.namespace()], |row| {
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

fn read_alias_view(conn: &Connection, config: &ZmemoryConfig, limit: usize) -> Result<Value> {
    let alias_nodes = crate::service::stats::alias_node_count(conn, config.namespace())?;
    let trigger_nodes = crate::service::stats::trigger_node_count(conn, config.namespace())?;
    let alias_nodes_missing =
        crate::service::stats::alias_nodes_missing_triggers(conn, config.namespace())?;
    let entries = alias_entries(conn, config.namespace(), limit)?;

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

fn alias_entries(conn: &Connection, namespace: &str, limit: usize) -> Result<Vec<Value>> {
    let mut stmt = conn.prepare(
        "SELECT alias.node_uuid,
                alias.alias_count,
                COALESCE(trigger_counts.count, 0) AS trigger_count,
                p.domain,
                p.path
         FROM (
             SELECT e.child_uuid AS node_uuid,
                    COUNT(*) AS alias_count
             FROM edges e
             JOIN paths p ON p.edge_id = e.id AND p.namespace = e.namespace
             WHERE e.namespace = ?1
             GROUP BY e.child_uuid
             HAVING COUNT(*) > 1
         ) alias
         JOIN edges e ON e.namespace = ?1 AND e.child_uuid = alias.node_uuid
         JOIN paths p ON p.edge_id = e.id AND p.namespace = e.namespace
         LEFT JOIN (
             SELECT node_uuid, COUNT(*) AS count
             FROM glossary_keywords
             WHERE namespace = ?1
             GROUP BY node_uuid
         ) trigger_counts ON trigger_counts.node_uuid = alias.node_uuid
         WHERE e.namespace = ?1
         ORDER BY alias.alias_count DESC, alias.node_uuid ASC, p.domain ASC, p.path ASC",
    )?;

    let rows = stmt
        .query_map([namespace], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut grouped = BTreeMap::<String, AliasNodeAggregate>::new();
    for (node_uuid, alias_count, trigger_count, domain, path) in rows {
        let aggregate = grouped
            .entry(node_uuid.clone())
            .or_insert_with(|| AliasNodeAggregate {
                node_uuid,
                alias_count,
                trigger_count,
                paths: Vec::new(),
            });
        aggregate.paths.push((domain, path));
    }

    let mut entries = grouped
        .into_values()
        .map(AliasNodeAggregate::into_json)
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

    entries.truncate(limit);

    Ok(entries)
}

struct AliasNodeAggregate {
    node_uuid: String,
    alias_count: i64,
    trigger_count: i64,
    paths: Vec<(String, String)>,
}

impl AliasNodeAggregate {
    fn into_json(self) -> Result<Value> {
        let (domain, path) = self
            .paths
            .first()
            .cloned()
            .ok_or_else(|| anyhow!("alias node {} has no active paths", self.node_uuid))?;
        let node_uri = format!("{domain}://{path}");
        let missing_triggers = self.trigger_count == 0;
        let (review_priority, priority_score) =
            alias_review_priority(self.alias_count, missing_triggers);
        let priority_reason = alias_priority_reason(self.alias_count, missing_triggers);
        let suggested_keywords = if missing_triggers {
            infer_alias_keywords_from_paths(&self.paths)
        } else {
            Vec::new()
        };

        Ok(json!({
            "nodeUuid": self.node_uuid,
            "domain": domain,
            "path": path,
            "aliasCount": self.alias_count,
            "triggerCount": self.trigger_count,
            "missingTriggers": missing_triggers,
            "reviewPriority": review_priority,
            "priorityScore": priority_score,
            "priorityReason": priority_reason,
            "suggestedKeywords": suggested_keywords,
            "nodeUri": node_uri,
        }))
    }
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

fn infer_alias_keywords_from_paths(paths: &[(String, String)]) -> Vec<String> {
    let mut keywords = BTreeSet::new();
    for (_, path) in paths {
        for segment in path.split('/') {
            for token in segment.split(['-', '_']) {
                let candidate = token.trim().to_lowercase();
                if candidate.len() >= 2 && candidate.chars().any(|ch| ch.is_ascii_alphabetic()) {
                    keywords.insert(candidate);
                }
            }
        }
    }

    keywords.into_iter().take(3).collect()
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
