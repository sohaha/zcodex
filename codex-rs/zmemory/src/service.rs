use crate::config::ZmemoryConfig;
use crate::doctor::run_doctor;
use crate::repository::ZmemoryRepository;
use crate::schema::ROOT_NODE_UUID;
use crate::schema::active_memory_id_for_node;
use crate::schema::ensure_domain_root;
use crate::schema::mark_other_memories_deprecated;
use crate::system_views::read_system_view;
use crate::tool_api::ZmemoryToolAction;
use crate::tool_api::ZmemoryToolCallParam;
use crate::tool_api::ZmemoryUri;
use anyhow::Result;
use anyhow::bail;
use rusqlite::Connection;
use rusqlite::OptionalExtension;
use rusqlite::params;
use serde_json::Value;
use serde_json::json;
use uuid::Uuid;

pub(crate) fn execute_action(config: &ZmemoryConfig, args: &ZmemoryToolCallParam) -> Result<Value> {
    let repository = ZmemoryRepository::new(config.clone());
    let mut conn = repository.connect()?;
    let result = match args.action {
        ZmemoryToolAction::Read => read_action(config, &conn, args)?,
        ZmemoryToolAction::Search => search_action(config, &conn, args)?,
        ZmemoryToolAction::Create => create_action(config, &mut conn, args)?,
        ZmemoryToolAction::Update => update_action(config, &mut conn, args)?,
        ZmemoryToolAction::DeletePath => delete_path_action(config, &mut conn, args)?,
        ZmemoryToolAction::AddAlias => add_alias_action(config, &mut conn, args)?,
        ZmemoryToolAction::ManageTriggers => manage_triggers_action(config, &mut conn, args)?,
        ZmemoryToolAction::Stats => stats_action(&conn, config)?,
        ZmemoryToolAction::Doctor => doctor_action(&conn, config)?,
        ZmemoryToolAction::RebuildSearch => rebuild_search_action(&mut conn)?,
    };
    Ok(json!({
        "action": action_name(args.action.clone()),
        "result": result,
    }))
}

fn action_name(action: ZmemoryToolAction) -> &'static str {
    match action {
        ZmemoryToolAction::Read => "read",
        ZmemoryToolAction::Search => "search",
        ZmemoryToolAction::Create => "create",
        ZmemoryToolAction::Update => "update",
        ZmemoryToolAction::DeletePath => "delete-path",
        ZmemoryToolAction::AddAlias => "add-alias",
        ZmemoryToolAction::ManageTriggers => "manage-triggers",
        ZmemoryToolAction::Stats => "stats",
        ZmemoryToolAction::Doctor => "doctor",
        ZmemoryToolAction::RebuildSearch => "rebuild-search",
    }
}

fn read_action(
    config: &ZmemoryConfig,
    conn: &Connection,
    args: &ZmemoryToolCallParam,
) -> Result<Value> {
    let uri = parse_required_uri(args.uri.as_deref())?;
    if uri.domain == "system" {
        return read_system_view(conn, config, &uri.path, args.limit.unwrap_or(20))
            .map(|view| json!({ "uri": uri.to_string(), "view": view }));
    }
    ensure_readable_domain(config, conn, &uri.domain)?;

    let row =
        find_path_row(conn, &uri)?.ok_or_else(|| anyhow::anyhow!("memory not found: {uri}"))?;
    let memory = read_active_memory(conn, &row.node_uuid)?
        .ok_or_else(|| anyhow::anyhow!("active memory not found for {uri}"))?;
    let keywords = load_keywords(conn, &row.node_uuid)?;
    let children = list_children(conn, &uri, &row.node_uuid)?;
    let alias_count = count_aliases(conn, &row.node_uuid)?;

    Ok(json!({
        "uri": uri.to_string(),
        "nodeUuid": row.node_uuid,
        "memoryId": memory.id,
        "content": memory.content,
        "priority": row.priority,
        "disclosure": row.disclosure,
        "keywords": keywords,
        "children": children,
        "aliasCount": alias_count,
    }))
}

fn search_action(
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
        ensure_readable_domain(config, conn, &scope.domain)?;
    }

    let mut sql = String::from(
        "SELECT f.domain, f.path, f.uri, snippet(search_documents_fts, 3, '[', ']', '...', 12) AS snippet,
                sd.priority, sd.disclosure
         FROM search_documents_fts f
         JOIN search_documents sd
           ON sd.domain = f.domain AND sd.path = f.path
         WHERE search_documents_fts MATCH ?1",
    );

    let matches = if let Some(scope) = scope {
        sql.push_str(" AND f.domain = ?2 AND (f.path = ?3 OR f.path LIKE ?4) ORDER BY sd.priority DESC, bm25(search_documents_fts) ASC, f.uri ASC LIMIT ?5");
        let prefix = if scope.path.is_empty() {
            "%".to_string()
        } else {
            format!("{}/%", scope.path)
        };
        let mut stmt = conn.prepare(&sql)?;
        stmt.query_map(
            params![query, scope.domain, scope.path, prefix, limit as i64],
            |row| {
                Ok(json!({
                    "domain": row.get::<_, String>(0)?,
                    "path": row.get::<_, String>(1)?,
                    "uri": row.get::<_, String>(2)?,
                    "snippet": row.get::<_, String>(3)?,
                    "priority": row.get::<_, i64>(4)?,
                    "disclosure": row.get::<_, Option<String>>(5)?,
                }))
            },
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?
    } else {
        sql.push_str(
            " ORDER BY sd.priority DESC, bm25(search_documents_fts) ASC, f.uri ASC LIMIT ?2",
        );
        let mut stmt = conn.prepare(&sql)?;
        stmt.query_map(params![query, limit as i64], |row| {
            Ok(json!({
                "domain": row.get::<_, String>(0)?,
                "path": row.get::<_, String>(1)?,
                "uri": row.get::<_, String>(2)?,
                "snippet": row.get::<_, String>(3)?,
                "priority": row.get::<_, i64>(4)?,
                "disclosure": row.get::<_, Option<String>>(5)?,
            }))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?
    };

    Ok(json!({
        "query": query,
        "matchCount": matches.len(),
        "matches": matches,
    }))
}

fn create_action(
    config: &ZmemoryConfig,
    conn: &mut Connection,
    args: &ZmemoryToolCallParam,
) -> Result<Value> {
    let uri = resolve_create_uri(conn, args)?;
    let content = required_content(args.content.as_deref())?;
    ensure_writable_domain(config, conn, &uri.domain)?;
    anyhow::ensure!(
        find_path_row(conn, &uri)?.is_none(),
        "memory already exists at {uri}"
    );

    let parent_uri = uri.parent();
    let parent = if parent_uri.is_root() {
        PathRow::root(parent_uri.domain)
    } else {
        find_path_row(conn, &parent_uri)?
            .ok_or_else(|| anyhow::anyhow!("parent path does not exist: {parent_uri}"))?
    };
    let node_uuid = Uuid::new_v4().to_string();
    let priority = args.priority.unwrap_or_default();
    let disclosure = normalize_optional_text(args.disclosure.as_deref());

    let tx = conn.transaction()?;
    tx.execute("INSERT INTO nodes(uuid) VALUES (?1)", [node_uuid.as_str()])?;
    tx.execute(
        "INSERT INTO memories(node_uuid, content) VALUES (?1, ?2)",
        params![node_uuid, content],
    )?;
    let memory_id = tx.last_insert_rowid();
    tx.execute(
        "INSERT INTO edges(parent_uuid, child_uuid, name, priority, disclosure) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![parent.node_uuid, node_uuid, uri.leaf_name()?, priority, disclosure],
    )?;
    let edge_id = tx.last_insert_rowid();
    tx.execute(
        "INSERT INTO paths(domain, path, edge_id) VALUES (?1, ?2, ?3)",
        params![uri.domain, uri.path, edge_id],
    )?;
    tx.commit()?;

    let rebuild = rebuild_search_index(conn)?;
    Ok(json!({
        "uri": uri.to_string(),
        "nodeUuid": node_uuid,
        "memoryId": memory_id,
        "priority": priority,
        "disclosure": disclosure,
        "documentCount": rebuild,
    }))
}

fn update_action(
    config: &ZmemoryConfig,
    conn: &mut Connection,
    args: &ZmemoryToolCallParam,
) -> Result<Value> {
    let uri = parse_required_uri(args.uri.as_deref())?;
    anyhow::ensure!(!uri.is_root(), "cannot update root path");
    ensure_writable_domain(config, conn, &uri.domain)?;
    let row =
        find_path_row(conn, &uri)?.ok_or_else(|| anyhow::anyhow!("memory not found: {uri}"))?;
    let current_memory = read_active_memory(conn, &row.node_uuid)?
        .ok_or_else(|| anyhow::anyhow!("active memory not found: {uri}"))?;

    let mut content_changed = false;
    let mut new_memory_id = None;
    let mut disclosure = row.disclosure.clone();
    let mut priority = row.priority;
    let updated_content = resolve_updated_content(args, &current_memory.content)?;

    let tx = conn.transaction()?;
    if let Some(content) = updated_content
        && content != current_memory.content
    {
        tx.execute(
            "INSERT INTO memories(node_uuid, content) VALUES (?1, ?2)",
            params![row.node_uuid, content],
        )?;
        let replacement_id = tx.last_insert_rowid();
        mark_other_memories_deprecated(&tx, &row.node_uuid, replacement_id)?;
        new_memory_id = Some(replacement_id);
        content_changed = true;
    }

    if let Some(updated_priority) = args.priority {
        priority = updated_priority;
        tx.execute(
            "UPDATE edges SET priority = ?2 WHERE id = ?1",
            params![row.edge_id, updated_priority],
        )?;
    }

    if let Some(updated_disclosure) = args.disclosure.clone() {
        disclosure = Some(updated_disclosure.clone());
        tx.execute(
            "UPDATE edges SET disclosure = ?2 WHERE id = ?1",
            params![row.edge_id, updated_disclosure],
        )?;
    }

    if !content_changed && args.priority.is_none() && args.disclosure.is_none() {
        bail!("no changes requested");
    }

    tx.commit()?;
    let rebuild = rebuild_search_index(conn)?;
    Ok(json!({
        "uri": uri.to_string(),
        "nodeUuid": row.node_uuid,
        "oldMemoryId": current_memory.id,
        "newMemoryId": new_memory_id,
        "priority": priority,
        "disclosure": disclosure,
        "contentChanged": content_changed,
        "documentCount": rebuild,
    }))
}

fn delete_path_action(
    config: &ZmemoryConfig,
    conn: &mut Connection,
    args: &ZmemoryToolCallParam,
) -> Result<Value> {
    let uri = parse_required_uri(args.uri.as_deref())?;
    anyhow::ensure!(!uri.is_root(), "cannot delete root path");
    ensure_writable_domain(config, conn, &uri.domain)?;
    let row =
        find_path_row(conn, &uri)?.ok_or_else(|| anyhow::anyhow!("memory not found: {uri}"))?;

    let tx = conn.transaction()?;
    let deleted_paths = tx.execute(
        "DELETE FROM paths WHERE domain = ?1 AND path = ?2",
        params![uri.domain, uri.path],
    )?;
    let deleted_edges = tx.execute("DELETE FROM edges WHERE id = ?1", [row.edge_id])?;
    let remaining_refs: i64 = tx.query_row(
        "SELECT COUNT(*) FROM edges e JOIN paths p ON p.edge_id = e.id WHERE e.child_uuid = ?1",
        [row.node_uuid.as_str()],
        |stmt| stmt.get(0),
    )?;
    let deprecated_nodes = if remaining_refs == 0 {
        tx.execute(
            "UPDATE memories SET deprecated = TRUE WHERE node_uuid = ?1 AND deprecated = FALSE",
            [row.node_uuid.as_str()],
        )?
    } else {
        0
    };
    tx.commit()?;

    let rebuild = rebuild_search_index(conn)?;
    Ok(json!({
        "uri": uri.to_string(),
        "nodeUuid": row.node_uuid,
        "deletedPaths": deleted_paths,
        "deletedEdges": deleted_edges,
        "deprecatedNodes": deprecated_nodes,
        "documentCount": rebuild,
    }))
}

fn add_alias_action(
    config: &ZmemoryConfig,
    conn: &mut Connection,
    args: &ZmemoryToolCallParam,
) -> Result<Value> {
    let new_uri = parse_required_uri(args.new_uri.as_deref())?;
    let target_uri = parse_required_uri(args.target_uri.as_deref())?;
    anyhow::ensure!(!new_uri.is_root(), "cannot alias root path");
    anyhow::ensure!(!target_uri.is_root(), "cannot alias the root node");
    ensure_writable_domain(config, conn, &new_uri.domain)?;
    ensure_readable_domain(config, conn, &target_uri.domain)?;
    anyhow::ensure!(
        find_path_row(conn, &new_uri)?.is_none(),
        "alias path already exists: {new_uri}"
    );

    let target = find_path_row(conn, &target_uri)?
        .ok_or_else(|| anyhow::anyhow!("target path does not exist: {target_uri}"))?;
    let parent_uri = new_uri.parent();
    let parent = if parent_uri.is_root() {
        PathRow::root(new_uri.domain.clone())
    } else {
        find_path_row(conn, &parent_uri)?
            .ok_or_else(|| anyhow::anyhow!("parent path does not exist: {parent_uri}"))?
    };
    let priority = args.priority.unwrap_or(target.priority);
    let disclosure = args.disclosure.clone();

    let tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO edges(parent_uuid, child_uuid, name, priority, disclosure) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![parent.node_uuid, target.node_uuid, new_uri.leaf_name()?, priority, disclosure],
    )?;
    let edge_id = tx.last_insert_rowid();
    tx.execute(
        "INSERT INTO paths(domain, path, edge_id) VALUES (?1, ?2, ?3)",
        params![new_uri.domain, new_uri.path, edge_id],
    )?;
    tx.commit()?;

    let rebuild = rebuild_search_index(conn)?;
    Ok(json!({
        "uri": new_uri.to_string(),
        "targetUri": target_uri.to_string(),
        "nodeUuid": target.node_uuid,
        "edgeId": edge_id,
        "priority": priority,
        "disclosure": disclosure,
        "documentCount": rebuild,
    }))
}

fn manage_triggers_action(
    config: &ZmemoryConfig,
    conn: &mut Connection,
    args: &ZmemoryToolCallParam,
) -> Result<Value> {
    let uri = parse_required_uri(args.uri.as_deref())?;
    anyhow::ensure!(!uri.is_root(), "cannot manage triggers for root path");
    ensure_writable_domain(config, conn, &uri.domain)?;
    let row =
        find_path_row(conn, &uri)?.ok_or_else(|| anyhow::anyhow!("memory not found: {uri}"))?;
    let add = normalize_keywords(args.add.clone().unwrap_or_default());
    let remove = normalize_keywords(args.remove.clone().unwrap_or_default());
    anyhow::ensure!(
        !(add.is_empty() && remove.is_empty()),
        "no changes requested"
    );

    let tx = conn.transaction()?;
    for keyword in &add {
        tx.execute(
            "INSERT OR IGNORE INTO glossary_keywords(keyword, node_uuid) VALUES (?1, ?2)",
            params![keyword, row.node_uuid],
        )?;
    }
    for keyword in &remove {
        tx.execute(
            "DELETE FROM glossary_keywords WHERE keyword = ?1 AND node_uuid = ?2",
            params![keyword, row.node_uuid],
        )?;
    }
    tx.commit()?;
    let rebuild = rebuild_search_index(conn)?;
    let current = load_keywords(conn, &row.node_uuid)?;

    Ok(json!({
        "uri": uri.to_string(),
        "nodeUuid": row.node_uuid,
        "added": add,
        "removed": remove,
        "current": current,
        "documentCount": rebuild,
    }))
}

fn stats_action(conn: &Connection, config: &ZmemoryConfig) -> Result<Value> {
    let node_count: i64 = conn.query_row("SELECT COUNT(*) FROM nodes", [], |row| row.get(0))?;
    let memory_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM memories WHERE deprecated = FALSE",
        [],
        |row| row.get(0),
    )?;
    let path_count: i64 = conn.query_row("SELECT COUNT(*) FROM paths", [], |row| row.get(0))?;
    let glossary_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM glossary_keywords", [], |row| {
            row.get(0)
        })?;
    let alias_node_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM (
             SELECT e.child_uuid
             FROM edges e
             JOIN paths p ON p.edge_id = e.id
             GROUP BY e.child_uuid
             HAVING COUNT(*) > 1
         )",
        [],
        |row| row.get(0),
    )?;
    let trigger_node_count: i64 = conn.query_row(
        "SELECT COUNT(DISTINCT node_uuid) FROM glossary_keywords",
        [],
        |row| row.get(0),
    )?;
    let disclosure_path_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM edges e
         JOIN paths p ON p.edge_id = e.id
         WHERE e.disclosure IS NOT NULL AND TRIM(e.disclosure) != ''",
        [],
        |row| row.get(0),
    )?;
    let paths_missing_disclosure: i64 = conn.query_row(
        "SELECT COUNT(*) FROM edges e
         JOIN paths p ON p.edge_id = e.id
         WHERE e.disclosure IS NULL OR TRIM(e.disclosure) = ''",
        [],
        |row| row.get(0),
    )?;
    let disclosures_needing_review: i64 = conn.query_row(
        "SELECT COUNT(*) FROM edges e
         JOIN paths p ON p.edge_id = e.id
         WHERE e.disclosure IS NOT NULL
           AND TRIM(e.disclosure) != ''
           AND (
             INSTR(LOWER(e.disclosure), ' or ') > 0
             OR INSTR(LOWER(e.disclosure), ' and ') > 0
             OR INSTR(e.disclosure, ',') > 0
             OR INSTR(e.disclosure, '，') > 0
             OR INSTR(e.disclosure, '、') > 0
             OR INSTR(e.disclosure, ';') > 0
             OR INSTR(e.disclosure, '；') > 0
             OR INSTR(e.disclosure, '/') > 0
             OR INSTR(e.disclosure, '&') > 0
             OR INSTR(e.disclosure, '+') > 0
             OR INSTR(e.disclosure, '|') > 0
             OR INSTR(e.disclosure, '或') > 0
           )",
        [],
        |row| row.get(0),
    )?;
    let orphaned_memory_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM memories WHERE deprecated = TRUE AND migrated_to IS NULL",
        [],
        |row| row.get(0),
    )?;
    let deprecated_memory_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM memories WHERE deprecated = TRUE AND migrated_to IS NOT NULL",
        [],
        |row| row.get(0),
    )?;
    let search_document_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM search_documents", [], |row| {
            row.get(0)
        })?;
    let fts_document_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM search_documents_fts", [], |row| {
            row.get(0)
        })?;

    Ok(json!({
        "dbPath": config.db_path().display().to_string(),
        "nodeCount": node_count,
        "memoryCount": memory_count,
        "pathCount": path_count,
        "glossaryKeywordCount": glossary_count,
        "orphanedMemoryCount": orphaned_memory_count,
        "deprecatedMemoryCount": deprecated_memory_count,
        "aliasNodeCount": alias_node_count,
        "triggerNodeCount": trigger_node_count,
        "disclosurePathCount": disclosure_path_count,
        "pathsMissingDisclosure": paths_missing_disclosure,
        "disclosuresNeedingReview": disclosures_needing_review,
        "searchDocumentCount": search_document_count,
        "ftsDocumentCount": fts_document_count,
    }))
}

fn doctor_action(conn: &Connection, config: &ZmemoryConfig) -> Result<Value> {
    let doctor = run_doctor(conn, &config.db_path().display().to_string())?;
    let stats = stats_action(conn, config)?;
    Ok(json!({
        "healthy": doctor.get("healthy").and_then(serde_json::Value::as_bool).unwrap_or(false),
        "orphanedMemoryCount": doctor.get("orphanedMemoryCount").cloned().unwrap_or_else(|| json!(0)),
        "deprecatedMemoryCount": doctor.get("deprecatedMemoryCount").cloned().unwrap_or_else(|| json!(0)),
        "aliasNodeCount": doctor.get("aliasNodeCount").cloned().unwrap_or_else(|| json!(0)),
        "triggerNodeCount": doctor.get("triggerNodeCount").cloned().unwrap_or_else(|| json!(0)),
        "aliasNodesMissingTriggers": doctor
            .get("aliasNodesMissingTriggers")
            .cloned()
            .unwrap_or_else(|| json!(0)),
        "pathsMissingDisclosure": doctor
            .get("pathsMissingDisclosure")
            .cloned()
            .unwrap_or_else(|| json!(0)),
        "disclosuresNeedingReview": doctor
            .get("disclosuresNeedingReview")
            .cloned()
            .unwrap_or_else(|| json!(0)),
        "issues": doctor.get("issues").cloned().unwrap_or_else(|| json!([])),
        "stats": stats,
        "dbPath": config.db_path().display().to_string(),
    }))
}

fn rebuild_search_action(conn: &mut Connection) -> Result<Value> {
    let count = rebuild_search_index(conn)?;
    let fts_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM search_documents_fts", [], |row| {
            row.get(0)
        })?;
    Ok(json!({
        "documentCount": count,
        "ftsDocumentCount": fts_count,
    }))
}

fn rebuild_search_index(conn: &mut Connection) -> Result<i64> {
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
        let search_terms = build_search_terms(&domain, &path, &keywords);
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

fn build_search_terms(domain: &str, path: &str, keywords: &str) -> String {
    let mut terms = vec![domain.to_string(), path.replace('/', " ")];
    if !keywords.trim().is_empty() {
        terms.push(keywords.trim().to_string());
    }
    terms.join(" ")
}

fn parse_required_uri(raw: Option<&str>) -> Result<ZmemoryUri> {
    let raw = raw.ok_or_else(|| anyhow::anyhow!("`uri` is required"))?;
    ZmemoryUri::parse(raw)
}

fn required_content(content: Option<&str>) -> Result<String> {
    let content = content
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("`content` is required"))?;
    Ok(content.to_string())
}

fn resolve_create_uri(conn: &Connection, args: &ZmemoryToolCallParam) -> Result<ZmemoryUri> {
    anyhow::ensure!(
        !(args.uri.is_some() && (args.parent_uri.is_some() || args.title.is_some())),
        "`uri` cannot be combined with `parentUri` or `title`",
    );

    if let Some(uri) = args.uri.as_deref() {
        let uri = ZmemoryUri::parse(uri)?;
        anyhow::ensure!(!uri.is_root(), "cannot create root path");
        return Ok(uri);
    }

    let parent_uri = parse_parent_uri(args.parent_uri.as_deref())?;
    let title = normalize_optional_text(args.title.as_deref());
    let name = match title {
        Some(title) => {
            validate_title(&title)?;
            title
        }
        None => next_auto_child_name(conn, &parent_uri)?,
    };
    let path = if parent_uri.path.is_empty() {
        name
    } else {
        format!("{}/{name}", parent_uri.path)
    };
    Ok(ZmemoryUri {
        domain: parent_uri.domain,
        path,
    })
}

fn parse_parent_uri(raw: Option<&str>) -> Result<ZmemoryUri> {
    let raw = raw.ok_or_else(|| anyhow::anyhow!("`uri` or `parentUri` is required"))?;
    ZmemoryUri::parse(raw)
}

fn validate_title(title: &str) -> Result<()> {
    anyhow::ensure!(
        title
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-'),
        "`title` may only contain ASCII letters, numbers, `_`, or `-`",
    );
    Ok(())
}

fn next_auto_child_name(conn: &Connection, parent_uri: &ZmemoryUri) -> Result<String> {
    if !parent_uri.is_root() {
        find_path_row(conn, parent_uri)?
            .ok_or_else(|| anyhow::anyhow!("parent path does not exist: {parent_uri}"))?;
    }

    let mut next_index = 1_u64;
    loop {
        let child_path = if parent_uri.path.is_empty() {
            next_index.to_string()
        } else {
            format!("{}/{next_index}", parent_uri.path)
        };
        let candidate = ZmemoryUri {
            domain: parent_uri.domain.clone(),
            path: child_path,
        };
        if find_path_row(conn, &candidate)?.is_none() {
            return Ok(next_index.to_string());
        }
        next_index += 1;
    }
}

fn normalize_optional_text(raw: Option<&str>) -> Option<String> {
    raw.map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn resolve_updated_content(
    args: &ZmemoryToolCallParam,
    current_content: &str,
) -> Result<Option<String>> {
    let has_patch_fields = args.old_string.is_some() || args.new_string.is_some();
    let has_append = args.append.is_some();
    let has_content = args.content.is_some();

    anyhow::ensure!(
        !(has_content && (has_patch_fields || has_append)),
        "`content` cannot be combined with `oldString`/`newString`/`append`",
    );
    anyhow::ensure!(
        !(has_patch_fields && has_append),
        "`oldString`/`newString` cannot be combined with `append`",
    );

    match (
        args.content.as_deref(),
        args.old_string.as_deref(),
        args.new_string.as_deref(),
        args.append.as_deref(),
    ) {
        (Some(content), None, None, None) => Ok(Some(required_content(Some(content))?)),
        (None, Some(_), None, None) => {
            bail!("`newString` is required when `oldString` is provided")
        }
        (None, None, Some(_), None) => {
            bail!("`oldString` is required when `newString` is provided")
        }
        (None, Some(old_string), Some(new_string), None) => {
            patch_content(current_content, old_string, new_string).map(Some)
        }
        (None, None, None, Some(append)) => append_content(current_content, append).map(Some),
        (None, None, None, None) => Ok(None),
        _ => unreachable!("conflicting update modes should already be rejected"),
    }
}

fn patch_content(current_content: &str, old_string: &str, new_string: &str) -> Result<String> {
    anyhow::ensure!(!old_string.is_empty(), "`oldString` cannot be empty");
    anyhow::ensure!(
        old_string != new_string,
        "`oldString` and `newString` must differ"
    );
    let match_count = current_content.matches(old_string).count();
    anyhow::ensure!(
        match_count > 0,
        "`oldString` was not found in the current content"
    );
    anyhow::ensure!(
        match_count == 1,
        "`oldString` matched multiple locations; provide a more specific value",
    );
    Ok(current_content.replacen(old_string, new_string, 1))
}

fn append_content(current_content: &str, append: &str) -> Result<String> {
    anyhow::ensure!(!append.is_empty(), "`append` cannot be empty");
    Ok(format!("{current_content}{append}"))
}

fn normalize_keywords(keywords: Vec<String>) -> Vec<String> {
    let mut normalized = keywords
        .into_iter()
        .map(|keyword| keyword.trim().to_lowercase())
        .filter(|keyword| !keyword.is_empty())
        .collect::<Vec<_>>();
    normalized.sort();
    normalized.dedup();
    normalized
}

#[derive(Debug, Clone)]
struct PathRow {
    edge_id: i64,
    node_uuid: String,
    priority: i64,
    disclosure: Option<String>,
}

impl PathRow {
    fn root(domain: String) -> Self {
        let _ = domain;
        Self {
            edge_id: 0,
            node_uuid: ROOT_NODE_UUID.to_string(),
            priority: 0,
            disclosure: None,
        }
    }
}

#[derive(Debug, Clone)]
struct MemoryRow {
    id: i64,
    content: String,
}

fn find_path_row(conn: &Connection, uri: &ZmemoryUri) -> Result<Option<PathRow>> {
    if uri.is_root() {
        return Ok(Some(PathRow::root(uri.domain.clone())));
    }
    conn.query_row(
        "SELECT p.edge_id, e.child_uuid, e.priority, e.disclosure
         FROM paths p
         JOIN edges e ON e.id = p.edge_id
         WHERE p.domain = ?1 AND p.path = ?2",
        params![uri.domain, uri.path],
        |row| {
            Ok(PathRow {
                edge_id: row.get(0)?,
                node_uuid: row.get(1)?,
                priority: row.get(2)?,
                disclosure: row.get(3)?,
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

fn ensure_readable_domain(config: &ZmemoryConfig, conn: &Connection, domain: &str) -> Result<()> {
    anyhow::ensure!(
        config.is_valid_domain(domain),
        "unsupported domain: {domain}"
    );
    if domain != "system" {
        ensure_domain_root(conn, domain)?;
    }
    Ok(())
}

fn ensure_writable_domain(config: &ZmemoryConfig, conn: &Connection, domain: &str) -> Result<()> {
    anyhow::ensure!(domain != "system", "system domain is read-only");
    ensure_readable_domain(config, conn, domain)
}

fn read_active_memory(conn: &Connection, node_uuid: &str) -> Result<Option<MemoryRow>> {
    let active_memory_id = active_memory_id_for_node(conn, node_uuid)?;
    let Some(active_memory_id) = active_memory_id else {
        return Ok(None);
    };
    conn.query_row(
        "SELECT id, content FROM memories WHERE id = ?1",
        [active_memory_id],
        |row| {
            Ok(MemoryRow {
                id: row.get(0)?,
                content: row.get(1)?,
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

fn list_children(conn: &Connection, uri: &ZmemoryUri, node_uuid: &str) -> Result<Vec<Value>> {
    let mut stmt = conn.prepare(
        "SELECT p.path, e.name, e.priority, e.disclosure
         FROM edges e
         JOIN paths p ON p.edge_id = e.id
         WHERE e.parent_uuid = ?1 AND p.domain = ?2
         ORDER BY e.priority DESC, e.name ASC",
    )?;
    stmt.query_map(params![node_uuid, uri.domain.as_str()], |row| {
        let path: String = row.get(0)?;
        Ok(json!({
            "name": row.get::<_, String>(1)?,
            "priority": row.get::<_, i64>(2)?,
            "disclosure": row.get::<_, Option<String>>(3)?,
            "uri": format!("{}://{}", uri.domain, path),
        }))
    })?
    .collect::<rusqlite::Result<Vec<_>>>()
    .map_err(Into::into)
}

fn load_keywords(conn: &Connection, node_uuid: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT keyword FROM glossary_keywords WHERE node_uuid = ?1 ORDER BY keyword ASC",
    )?;
    stmt.query_map([node_uuid], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

fn count_aliases(conn: &Connection, node_uuid: &str) -> Result<i64> {
    conn.query_row(
        "SELECT COUNT(*) FROM edges e JOIN paths p ON p.edge_id = e.id WHERE e.child_uuid = ?1",
        [node_uuid],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::execute_action;
    use crate::config::ZmemoryConfig;
    use crate::config::ZmemorySettings;
    use crate::tool_api::ZmemoryToolAction;
    use crate::tool_api::ZmemoryToolCallParam;
    use pretty_assertions::assert_eq;
    use rusqlite::Connection;
    use rusqlite::params;
    use serde_json::json;
    use tempfile::TempDir;

    fn config() -> (TempDir, ZmemoryConfig) {
        let dir = TempDir::new().expect("tempdir");
        let config = ZmemoryConfig::new(dir.path().to_path_buf());
        (dir, config)
    }

    fn config_with_settings(settings: ZmemorySettings) -> (TempDir, ZmemoryConfig) {
        let dir = TempDir::new().expect("tempdir");
        let config = ZmemoryConfig::new_with_settings(dir.path().to_path_buf(), settings);
        (dir, config)
    }

    #[test]
    fn create_read_search_and_rebuild_round_trip() {
        let (_dir, config) = config();
        let create = execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::Create,
                uri: Some("core://agent-profile".to_string()),
                content: Some("Stores agent profile memory".to_string()),
                priority: Some(5),
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect("create should succeed");
        assert_eq!(create["action"], "create");
        assert_eq!(create["result"]["uri"], "core://agent-profile");

        let read = execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::Read,
                uri: Some("core://agent-profile".to_string()),
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect("read should succeed");
        assert_eq!(read["result"]["content"], "Stores agent profile memory");

        let search = execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::Search,
                query: Some("profile".to_string()),
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect("search should succeed");
        assert_eq!(search["result"]["matchCount"], 1);

        let rebuild = execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::RebuildSearch,
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect("rebuild should succeed");
        assert_eq!(rebuild["result"]["documentCount"], 1);
    }

    #[test]
    fn create_supports_parent_uri_and_auto_numbering() {
        let (_dir, config) = config();
        execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::Create,
                parent_uri: Some("core://".to_string()),
                title: Some("agent".to_string()),
                content: Some("Stores agent profile memory".to_string()),
                priority: Some(5),
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect("named create should succeed");

        let numbered = execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::Create,
                parent_uri: Some("core://".to_string()),
                content: Some("Auto numbered memory".to_string()),
                priority: Some(3),
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect("auto-numbered create should succeed");

        assert_eq!(numbered["result"]["uri"], "core://1");

        let read = execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::Read,
                uri: Some("core://agent".to_string()),
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect("read should succeed");
        assert_eq!(read["result"]["content"], "Stores agent profile memory");
    }

    #[test]
    fn create_rejects_conflicting_uri_modes_and_invalid_title() {
        let (_dir, config) = config();
        let conflict = execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::Create,
                uri: Some("core://agent".to_string()),
                parent_uri: Some("core://".to_string()),
                content: Some("Stores agent profile memory".to_string()),
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect_err("uri and parentUri should conflict");
        assert_eq!(
            conflict.to_string(),
            "`uri` cannot be combined with `parentUri` or `title`"
        );

        let invalid_title = execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::Create,
                parent_uri: Some("core://".to_string()),
                title: Some("bad title".to_string()),
                content: Some("Stores agent profile memory".to_string()),
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect_err("invalid title should fail");
        assert_eq!(
            invalid_title.to_string(),
            "`title` may only contain ASCII letters, numbers, `_`, or `-`"
        );
    }

    #[test]
    fn alias_and_manage_triggers_are_visible_in_read() {
        let (_dir, config) = config();
        execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::Create,
                uri: Some("core://team".to_string()),
                content: Some("Team root".to_string()),
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect("parent create should succeed");
        execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::Create,
                uri: Some("core://salem".to_string()),
                content: Some("Profile for Salem".to_string()),
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect("create should succeed");
        execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::AddAlias,
                new_uri: Some("core://team/salem".to_string()),
                target_uri: Some("core://salem".to_string()),
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect("alias should succeed");
        execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::ManageTriggers,
                uri: Some("core://salem".to_string()),
                add: Some(vec!["Profile".to_string(), "Agent".to_string()]),
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect("manage triggers should succeed");

        let read = execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::Read,
                uri: Some("core://salem".to_string()),
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect("read should succeed");

        assert_eq!(read["result"]["aliasCount"], 2);
        assert_eq!(read["result"]["keywords"].as_array().map(Vec::len), Some(2));
    }

    #[test]
    fn update_supports_patch_append_and_metadata_only_modes() {
        let (_dir, config) = config();
        execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::Create,
                uri: Some("core://agent".to_string()),
                content: Some("Original memory".to_string()),
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect("create should succeed");

        let update = execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::Update,
                uri: Some("core://agent".to_string()),
                old_string: Some("Original".to_string()),
                new_string: Some("Updated".to_string()),
                priority: Some(5),
                disclosure: Some("team".to_string()),
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect("update should succeed");

        assert_eq!(update["result"]["contentChanged"], true);
        assert_eq!(update["result"]["priority"], 5);
        assert_eq!(update["result"]["disclosure"], "team");
        assert!(update["result"]["newMemoryId"].as_i64().is_some());

        let metadata_only = execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::Update,
                uri: Some("core://agent".to_string()),
                priority: Some(9),
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect("metadata-only update should succeed");
        assert_eq!(metadata_only["result"]["contentChanged"], false);
        assert!(metadata_only["result"]["newMemoryId"].is_null());
        assert_eq!(metadata_only["result"]["priority"], 9);

        let append = execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::Update,
                uri: Some("core://agent".to_string()),
                append: Some("   ".to_string()),
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect("append update should succeed");
        assert_eq!(append["result"]["contentChanged"], true);

        let read = execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::Read,
                uri: Some("core://agent".to_string()),
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect("read should succeed");
        assert_eq!(read["result"]["content"], "Updated memory   ");
        assert_eq!(read["result"]["priority"], 9);

        let search = execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::Search,
                query: Some("Updated".to_string()),
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect("search should succeed");
        assert_eq!(search["result"]["matchCount"], 1);
    }

    #[test]
    fn update_rejects_conflicting_or_invalid_patch_modes() {
        let (_dir, config) = config();
        execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::Create,
                uri: Some("core://agent".to_string()),
                content: Some("Original memory".to_string()),
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect("create should succeed");

        let conflict = execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::Update,
                uri: Some("core://agent".to_string()),
                content: Some("Replacement".to_string()),
                append: Some("suffix".to_string()),
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect_err("content and append should conflict");
        assert_eq!(
            conflict.to_string(),
            "`content` cannot be combined with `oldString`/`newString`/`append`"
        );

        let missing_new = execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::Update,
                uri: Some("core://agent".to_string()),
                old_string: Some("Original".to_string()),
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect_err("oldString without newString should fail");
        assert_eq!(
            missing_new.to_string(),
            "`newString` is required when `oldString` is provided"
        );

        let duplicate_patch = execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::Update,
                uri: Some("core://agent".to_string()),
                old_string: Some("m".to_string()),
                new_string: Some("M".to_string()),
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect_err("ambiguous patch should fail");
        assert_eq!(
            duplicate_patch.to_string(),
            "`oldString` matched multiple locations; provide a more specific value"
        );

        let empty_append = execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::Update,
                uri: Some("core://agent".to_string()),
                append: Some(String::new()),
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect_err("empty append should fail");
        assert_eq!(empty_append.to_string(), "`append` cannot be empty");
    }

    #[test]
    fn delete_path_removes_last_reference_from_search() {
        let (_dir, config) = config();
        execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::Create,
                uri: Some("core://obsolete".to_string()),
                content: Some("Obsolete memory".to_string()),
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect("create should succeed");

        let delete = execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::DeletePath,
                uri: Some("core://obsolete".to_string()),
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect("delete should succeed");
        assert_eq!(delete["result"]["deletedPaths"], 1);
        assert_eq!(delete["result"]["deletedEdges"], 1);
        assert_eq!(delete["result"]["deprecatedNodes"], 1);

        let search = execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::Search,
                query: Some("Obsolete".to_string()),
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect("search should succeed");
        assert_eq!(search["result"]["matchCount"], 0);
    }

    #[test]
    fn system_views_reflect_index_recent_and_glossary() {
        let (_dir, config) = config();
        execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::Create,
                uri: Some("core://agent".to_string()),
                content: Some("Profile for agent".to_string()),
                priority: Some(7),
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect("create should succeed");
        execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::ManageTriggers,
                uri: Some("core://agent".to_string()),
                add: Some(vec!["profile".to_string(), "agent".to_string()]),
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect("manage triggers should succeed");

        let boot = execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::Read,
                uri: Some("system://boot".to_string()),
                limit: Some(10),
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect("boot view should succeed");
        assert_eq!(boot["result"]["view"]["view"], "boot");
        assert_eq!(boot["result"]["view"]["entryCount"], 1);
        assert_eq!(boot["result"]["view"]["entries"][0]["uri"], "core://agent");
        assert_eq!(boot["result"]["view"]["missingUris"][0], "core://my_user");
        assert_eq!(
            boot["result"]["view"]["missingUris"][1],
            "core://agent/my_user"
        );

        let index = execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::Read,
                uri: Some("system://index".to_string()),
                limit: Some(10),
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect("index view should succeed");
        assert_eq!(index["result"]["view"]["totalCount"], 1);

        let recent = execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::Read,
                uri: Some("system://recent".to_string()),
                limit: Some(10),
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect("recent view should succeed");
        assert_eq!(recent["result"]["view"]["entryCount"], 1);

        let glossary = execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::Read,
                uri: Some("system://glossary".to_string()),
                limit: Some(10),
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect("glossary view should succeed");
        assert_eq!(glossary["result"]["view"]["entryCount"], 2);

        let index_by_domain = execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::Read,
                uri: Some("system://index/core".to_string()),
                limit: Some(10),
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect("domain-scoped index should succeed");
        assert_eq!(index_by_domain["result"]["view"]["view"], "index");
        assert_eq!(index_by_domain["result"]["view"]["domain"], "core");
        assert_eq!(index_by_domain["result"]["view"]["entryCount"], 1);

        let recent_with_path_limit = execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::Read,
                uri: Some("system://recent/1".to_string()),
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect("path-limited recent view should succeed");
        assert_eq!(recent_with_path_limit["result"]["view"]["view"], "recent");
        assert_eq!(recent_with_path_limit["result"]["view"]["entryCount"], 1);
    }

    #[test]
    fn invalid_domains_are_rejected_and_system_writes_are_blocked() {
        let (_dir, config) = config_with_settings(ZmemorySettings::from_env_vars(
            Some("core,notes".to_string()),
            None,
        ));

        let invalid_domain = execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::Create,
                uri: Some("writer://draft".to_string()),
                content: Some("unsupported".to_string()),
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect_err("invalid domain should fail");
        assert_eq!(invalid_domain.to_string(), "unsupported domain: writer");

        let invalid_index = execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::Read,
                uri: Some("system://index/writer".to_string()),
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect_err("invalid index domain should fail");
        assert_eq!(invalid_index.to_string(), "unsupported domain: writer");

        let system_write = execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::Create,
                uri: Some("system://boot-note".to_string()),
                content: Some("forbidden".to_string()),
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect_err("system writes should fail");
        assert_eq!(system_write.to_string(), "system domain is read-only");
    }

    #[test]
    fn stats_and_doctor_surface_review_pressure() {
        let (_dir, config) = config();
        execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::Create,
                uri: Some("core://legacy".to_string()),
                content: Some("Original profile memory".to_string()),
                disclosure: Some("review/handoff".to_string()),
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect("create should succeed");
        execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::Update,
                uri: Some("core://legacy".to_string()),
                append: Some(" with fresh note".to_string()),
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect("update should succeed");
        execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::Create,
                uri: Some("core://orphan".to_string()),
                content: Some("Orphaned review source".to_string()),
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect("create should succeed");
        execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::DeletePath,
                uri: Some("core://orphan".to_string()),
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect("delete-path should succeed");
        execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::Create,
                uri: Some("core://undisclosed".to_string()),
                content: Some("Missing disclosure".to_string()),
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect("undisclosed create should succeed");

        let stats = execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::Stats,
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect("stats should succeed");
        assert_eq!(stats["result"]["deprecatedMemoryCount"], 1);
        assert_eq!(stats["result"]["orphanedMemoryCount"], 1);
        assert_eq!(stats["result"]["pathsMissingDisclosure"], 1);
        assert_eq!(stats["result"]["disclosuresNeedingReview"], 1);

        let doctor = execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::Doctor,
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect("doctor should succeed");
        assert_eq!(doctor["result"]["healthy"], false);
        let issues = doctor["result"]["issues"]
            .as_array()
            .expect("issues should be an array");
        assert!(
            issues
                .iter()
                .any(|issue| issue["code"] == "deprecated_memories_awaiting_review")
        );
        assert!(
            issues
                .iter()
                .any(|issue| issue["code"] == "orphaned_memories")
        );
        assert!(
            issues
                .iter()
                .any(|issue| issue["code"] == "paths_missing_disclosure")
        );
        assert!(
            issues
                .iter()
                .any(|issue| issue["code"] == "disclosures_need_review")
        );
    }

    #[test]
    fn alias_view_includes_priority_reasons_and_suggested_keywords() {
        let (_dir, config) = config();
        execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::Create,
                uri: Some("core://hub".to_string()),
                content: Some("Hub".to_string()),
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect("hub create should succeed");
        execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::Create,
                uri: Some("core://zone".to_string()),
                content: Some("Zone".to_string()),
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect("zone create should succeed");
        execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::Create,
                uri: Some("core://project-alpha".to_string()),
                content: Some("Project note".to_string()),
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect("create should succeed");
        execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::AddAlias,
                new_uri: Some("core://hub/launch-plan".to_string()),
                target_uri: Some("core://project-alpha".to_string()),
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect("first alias should succeed");
        execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::AddAlias,
                new_uri: Some("core://zone/release_plan".to_string()),
                target_uri: Some("core://project-alpha".to_string()),
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect("second alias should succeed");

        let alias_view = execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::Read,
                uri: Some("system://alias".to_string()),
                limit: Some(10),
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect("alias view should succeed");

        let view = &alias_view["result"]["view"];
        let recommendations = view["recommendations"]
            .as_array()
            .expect("recommendations should be an array");
        assert_eq!(recommendations.len(), 1);
        assert_eq!(recommendations[0]["reviewPriority"], "high");
        assert_eq!(
            recommendations[0]["priorityReason"],
            "missing triggers across 3 alias paths"
        );
        assert_eq!(
            recommendations[0]["suggestedKeywords"],
            json!(["alpha", "hub", "launch"])
        );
        assert_eq!(
            recommendations[0]["command"],
            "codex zmemory manage-triggers core://hub/launch-plan --add alpha --add hub --add launch --json"
        );

        let entries = view["entries"]
            .as_array()
            .expect("entries should be an array");
        assert_eq!(entries[0]["nodeUri"], "core://hub/launch-plan");
        assert_eq!(entries[0]["reviewPriority"], "high");
        assert_eq!(
            entries[0]["priorityReason"],
            "missing triggers across 3 alias paths"
        );
        assert_eq!(
            entries[0]["suggestedKeywords"],
            json!(["alpha", "hub", "launch"])
        );
    }

    #[test]
    fn doctor_reports_fts_and_keyword_inconsistencies() {
        let (_dir, config) = config();
        execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::Create,
                uri: Some("core://salem".to_string()),
                content: Some("Profile for Salem".to_string()),
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect("create should succeed");
        execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::ManageTriggers,
                uri: Some("core://salem".to_string()),
                add: Some(vec!["profile".to_string()]),
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect("manage triggers should succeed");

        let conn = Connection::open(config.db_path()).expect("db should open");
        conn.execute("DELETE FROM search_documents_fts", [])
            .expect("fts delete should succeed");
        conn.execute(
            "DELETE FROM paths WHERE domain = ?1 AND path = ?2",
            params!["core", "salem"],
        )
        .expect("path delete should succeed");

        let doctor = execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::Doctor,
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect("doctor should succeed");

        assert_eq!(doctor["result"]["healthy"], false);
        let issues = doctor["result"]["issues"]
            .as_array()
            .expect("issues should be an array");
        assert!(
            issues
                .iter()
                .any(|issue| issue["code"] == "fts_count_mismatch")
        );
        assert!(
            issues
                .iter()
                .any(|issue| issue["code"] == "dangling_keywords")
        );
    }
}
