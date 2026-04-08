use super::CompatService;
use super::contracts::GlossaryChangeResponse;
use super::contracts::PathChangeResponse;
use super::contracts::ReviewDiffResponse;
use super::contracts::ReviewGroupItemResponse;
use super::contracts::ReviewRollbackActionResponse;
use super::contracts::StateMetaResponse;
use crate::service::contracts::ReviewGroupDiffContract;
use crate::service::index;
use crate::tool_api::ZmemoryUri;
use anyhow::Result;
use rusqlite::Connection;
use rusqlite::OptionalExtension;
use rusqlite::Transaction;
use rusqlite::params;
use serde_json::Value;
use serde_json::json;

impl CompatService {
    pub fn review_groups(&self, namespace: Option<&str>) -> Result<Vec<ReviewGroupItemResponse>> {
        let (conn, config) = self.connect(namespace)?;
        reviewable_pending_node_uuids(&conn, &config)?
            .into_iter()
            .map(|node_uuid| {
                let diff = crate::service::review::review_group_diff_for_node_uuid(
                    &conn, &config, &node_uuid,
                )?;
                let entries = pending_audit_entries(&conn, config.namespace(), &node_uuid)?;
                let namespaces = if config.namespace().is_empty() {
                    None
                } else {
                    Some(vec![config.namespace().to_string()])
                };
                Ok(ReviewGroupItemResponse {
                    node_uuid,
                    display_uri: diff.group.node_uri,
                    top_level_table: top_level_table(&entries).to_string(),
                    action: review_action(&entries).to_string(),
                    row_count: entries.len() as i64,
                    namespaces,
                })
            })
            .collect()
    }

    pub fn review_group_diff(
        &self,
        namespace: Option<&str>,
        node_uuid: &str,
    ) -> Result<ReviewDiffResponse> {
        let (conn, config) = self.connect(namespace)?;
        let pending_entries = pending_audit_entries(&conn, config.namespace(), node_uuid)?;
        anyhow::ensure!(
            !pending_entries.is_empty(),
            "not found: no pending review group for {node_uuid}"
        );
        let diff =
            crate::service::review::review_group_diff_for_node_uuid(&conn, &config, node_uuid)?;
        Ok(review_diff_response(diff, &pending_entries))
    }

    pub fn approve_review_group(&self, namespace: Option<&str>, node_uuid: &str) -> Result<String> {
        let (mut conn, config) = self.connect(namespace)?;
        let pending_entries = pending_audit_entries(&conn, config.namespace(), node_uuid)?;
        anyhow::ensure!(
            !pending_entries.is_empty(),
            "not found: no pending review group for {node_uuid}"
        );
        let diff =
            crate::service::review::review_group_diff_for_node_uuid(&conn, &config, node_uuid)?;
        let tx = conn.transaction()?;
        crate::service::common::insert_audit_log(
            &tx,
            config.namespace(),
            "review-approve",
            Some(&diff.snapshot.uri),
            Some(node_uuid),
            json!({
                "clearedActions": pending_entries.len(),
            }),
        )?;
        tx.commit()?;
        Ok(format!(
            "Approved node '{node_uuid}' ({} actions cleared)",
            pending_entries.len()
        ))
    }

    pub fn clear_review_groups(&self, namespace: Option<&str>) -> Result<String> {
        let (mut conn, config) = self.connect(namespace)?;
        let pending_nodes = reviewable_pending_node_uuids(&conn, &config)?;
        anyhow::ensure!(
            !pending_nodes.is_empty(),
            "not found: no pending review groups"
        );
        let tx = conn.transaction()?;
        for node_uuid in &pending_nodes {
            let diff =
                crate::service::review::review_group_diff_for_node_uuid(&tx, &config, node_uuid)?;
            let pending_entries = pending_audit_entries(&tx, config.namespace(), node_uuid)?;
            crate::service::common::insert_audit_log(
                &tx,
                config.namespace(),
                "review-approve",
                Some(&diff.snapshot.uri),
                Some(node_uuid),
                json!({
                    "clearedActions": pending_entries.len(),
                    "clearAll": true,
                }),
            )?;
        }
        tx.commit()?;
        Ok(format!(
            "All changes integrated ({} groups cleared)",
            pending_nodes.len()
        ))
    }

    pub fn rollback_review_group(
        &self,
        namespace: Option<&str>,
        node_uuid: &str,
    ) -> Result<ReviewRollbackActionResponse> {
        let (mut conn, config) = self.connect(namespace)?;
        let pending_entries = pending_audit_entries(&conn, config.namespace(), node_uuid)?;
        anyhow::ensure!(
            !pending_entries.is_empty(),
            "not found: no pending review group for {node_uuid}"
        );
        let diff =
            crate::service::review::review_group_diff_for_node_uuid(&conn, &config, node_uuid)?;

        let tx = conn.transaction()?;
        if created_node(&pending_entries, &diff.snapshot.uri) {
            delete_created_node(&tx, config.namespace(), node_uuid)?;
        } else {
            rollback_pending_entries(
                &tx,
                config.namespace(),
                node_uuid,
                &diff.snapshot.uri,
                &pending_entries,
            )?;
        }
        crate::service::common::insert_audit_log(
            &tx,
            config.namespace(),
            "review-rollback",
            Some(&diff.snapshot.uri),
            Some(node_uuid),
            json!({
                "revertedActions": pending_entries.len(),
            }),
        )?;
        tx.commit()?;
        let _ = index::rebuild_search_index(&mut conn, config.namespace())?;

        Ok(ReviewRollbackActionResponse {
            node_uuid: node_uuid.to_string(),
            success: true,
            message: format!("Rolled back node '{node_uuid}'"),
        })
    }
}

fn review_diff_response(
    diff: ReviewGroupDiffContract,
    pending_entries: &[crate::service::contracts::AuditEntryContract],
) -> ReviewDiffResponse {
    let before_content = diff
        .changeset
        .versions
        .iter()
        .find(|version| version.id != diff.snapshot.memory_id)
        .map(|version| version.content.clone());
    let current_meta = StateMetaResponse {
        priority: Some(diff.snapshot.priority),
        disclosure: diff.snapshot.disclosure.clone(),
    };
    let before_meta = current_meta.clone();
    let path_changes = path_changes(pending_entries);
    let glossary_changes = glossary_changes(pending_entries);
    let active_paths = std::iter::once(diff.snapshot.uri.clone())
        .chain(diff.snapshot.aliases.iter().map(|alias| alias.uri.clone()))
        .collect::<Vec<_>>();
    ReviewDiffResponse {
        uri: diff.snapshot.uri,
        change_type: top_level_table(pending_entries).to_string(),
        action: review_action(pending_entries).to_string(),
        before_content,
        current_content: Some(diff.snapshot.content),
        before_meta,
        current_meta,
        path_changes,
        glossary_changes,
        active_paths,
        has_changes: true,
    }
}

fn pending_review_node_uuids(conn: &Connection, namespace: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT node_uuid, MAX(id) AS latest_id
         FROM audit_log
         WHERE namespace = ?1 AND node_uuid IS NOT NULL
         GROUP BY node_uuid
         ORDER BY latest_id DESC",
    )?;
    let node_uuids = stmt
        .query_map([namespace], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut pending = Vec::new();
    for node_uuid in node_uuids {
        if !pending_audit_entries(conn, namespace, &node_uuid)?.is_empty() {
            pending.push(node_uuid);
        }
    }
    Ok(pending)
}

fn reviewable_pending_node_uuids(
    conn: &Connection,
    config: &crate::config::ZmemoryConfig,
) -> Result<Vec<String>> {
    let mut reviewable = Vec::new();
    for node_uuid in pending_review_node_uuids(conn, config.namespace())? {
        if crate::service::review::review_group_diff_for_node_uuid(conn, config, &node_uuid).is_ok()
        {
            reviewable.push(node_uuid);
        }
    }
    Ok(reviewable)
}

fn pending_audit_entries(
    conn: &Connection,
    namespace: &str,
    node_uuid: &str,
) -> Result<Vec<crate::service::contracts::AuditEntryContract>> {
    let mut stmt = conn.prepare(
        "SELECT id, action, uri, node_uuid, details, created_at
         FROM audit_log
         WHERE namespace = ?1 AND node_uuid = ?2
         ORDER BY id DESC",
    )?;
    let rows = stmt
        .query_map(params![namespace, node_uuid], |row| {
            let details = row.get::<_, String>(4)?;
            Ok(crate::service::contracts::AuditEntryContract {
                id: row.get(0)?,
                action: row.get(1)?,
                uri: row.get(2)?,
                node_uuid: row.get(3)?,
                details: serde_json::from_str::<Value>(&details).unwrap_or(Value::String(details)),
                created_at: row.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut pending = Vec::new();
    for entry in rows {
        if is_review_resolution_action(&entry.action) {
            break;
        }
        pending.push(entry);
    }
    pending.reverse();
    Ok(pending)
}

fn is_review_resolution_action(action: &str) -> bool {
    matches!(action, "review-approve" | "review-rollback")
}

fn top_level_table(entries: &[crate::service::contracts::AuditEntryContract]) -> &'static str {
    if entries
        .iter()
        .any(|entry| entry.action == "manage-triggers")
    {
        "glossary_keywords"
    } else if entries.iter().any(|entry| entry.action == "update") {
        "memories"
    } else {
        "paths"
    }
}

fn review_action(entries: &[crate::service::contracts::AuditEntryContract]) -> &'static str {
    if entries.iter().all(|entry| entry.action == "create") {
        "created"
    } else if entries.iter().all(|entry| entry.action == "delete-path") {
        "deleted"
    } else {
        "modified"
    }
}

fn created_node(
    entries: &[crate::service::contracts::AuditEntryContract],
    snapshot_uri: &str,
) -> bool {
    entries
        .iter()
        .any(|entry| entry.action == "create" && entry.uri.as_deref() == Some(snapshot_uri))
}

fn rollback_pending_entries(
    tx: &Transaction<'_>,
    namespace: &str,
    node_uuid: &str,
    snapshot_uri: &str,
    entries: &[crate::service::contracts::AuditEntryContract],
) -> Result<()> {
    for entry in entries.iter().rev() {
        match entry.action.as_str() {
            "manage-triggers" => rollback_manage_triggers(tx, namespace, node_uuid, entry)?,
            "add-alias" => rollback_added_alias(tx, namespace, entry)?,
            "update" => rollback_update(tx, namespace, node_uuid, entry)?,
            "delete-path" => rollback_deleted_path(tx, namespace, node_uuid, snapshot_uri, entry)?,
            "create" => {}
            _ => {}
        }
    }
    Ok(())
}

fn rollback_manage_triggers(
    tx: &Transaction<'_>,
    namespace: &str,
    node_uuid: &str,
    entry: &crate::service::contracts::AuditEntryContract,
) -> Result<()> {
    let Some(details) = entry.details.as_object() else {
        return Ok(());
    };
    if let Some(added) = details.get("added").and_then(Value::as_array) {
        for keyword in added.iter().filter_map(Value::as_str) {
            tx.execute(
                "DELETE FROM glossary_keywords
                 WHERE namespace = ?1 AND node_uuid = ?2 AND keyword = ?3",
                params![namespace, node_uuid, keyword],
            )?;
        }
    }
    if let Some(removed) = details.get("removed").and_then(Value::as_array) {
        for keyword in removed.iter().filter_map(Value::as_str) {
            tx.execute(
                "INSERT OR IGNORE INTO glossary_keywords(keyword, node_uuid, namespace)
                 VALUES (?1, ?2, ?3)",
                params![keyword, node_uuid, namespace],
            )?;
        }
    }
    Ok(())
}

fn rollback_added_alias(
    tx: &Transaction<'_>,
    namespace: &str,
    entry: &crate::service::contracts::AuditEntryContract,
) -> Result<()> {
    let Some(uri) = entry.uri.as_deref() else {
        return Ok(());
    };
    let uri = ZmemoryUri::parse(uri)?;
    delete_path_without_audit(tx, namespace, &uri)
}

fn rollback_update(
    tx: &Transaction<'_>,
    namespace: &str,
    node_uuid: &str,
    entry: &crate::service::contracts::AuditEntryContract,
) -> Result<()> {
    let Some(details) = entry.details.as_object() else {
        return Ok(());
    };
    let Some(old_memory_id) = details.get("oldMemoryId").and_then(Value::as_i64) else {
        return Ok(());
    };

    tx.execute(
        "UPDATE memories
         SET deprecated = FALSE, migrated_to = NULL
         WHERE namespace = ?1 AND id = ?2",
        params![namespace, old_memory_id],
    )?;
    tx.execute(
        "UPDATE memories
         SET deprecated = TRUE, migrated_to = ?3
         WHERE namespace = ?1 AND node_uuid = ?2 AND deprecated = FALSE AND id != ?3",
        params![namespace, node_uuid, old_memory_id],
    )?;
    Ok(())
}

fn rollback_deleted_path(
    tx: &Transaction<'_>,
    namespace: &str,
    node_uuid: &str,
    snapshot_uri: &str,
    entry: &crate::service::contracts::AuditEntryContract,
) -> Result<()> {
    let Some(uri) = entry.uri.as_deref() else {
        return Ok(());
    };
    let uri = ZmemoryUri::parse(uri)?;
    let existing_path = tx
        .query_row(
            "SELECT 1
             FROM paths
             WHERE namespace = ?1 AND domain = ?2 AND path = ?3",
            params![namespace, uri.domain, uri.path],
            |_| Ok(()),
        )
        .optional()?;
    if existing_path.is_some() {
        return Ok(());
    }

    let parent_uri = uri.parent();
    let parent_uuid = if parent_uri.is_root() {
        crate::schema::ROOT_NODE_UUID.to_string()
    } else if let Some(parent_row) = tx
        .query_row(
            "SELECT e.child_uuid
             FROM paths p
             JOIN edges e ON e.id = p.edge_id AND e.namespace = p.namespace
             WHERE p.namespace = ?1 AND p.domain = ?2 AND p.path = ?3",
            params![namespace, parent_uri.domain, parent_uri.path],
            |row| row.get::<_, String>(0),
        )
        .optional()?
    {
        parent_row
    } else {
        crate::schema::ROOT_NODE_UUID.to_string()
    };

    let (priority, disclosure) = edge_metadata_for_node(tx, namespace, node_uuid, snapshot_uri)?;
    let edge_name = uri.leaf_name()?;
    let edge_id = tx
        .query_row(
            "SELECT id
             FROM edges
             WHERE namespace = ?1 AND parent_uuid = ?2 AND child_uuid = ?3 AND name = ?4",
            params![namespace, parent_uuid, node_uuid, edge_name],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    let edge_id = if let Some(edge_id) = edge_id {
        edge_id
    } else {
        tx.execute(
            "INSERT INTO edges(namespace, parent_uuid, child_uuid, name, priority, disclosure)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                namespace,
                parent_uuid,
                node_uuid,
                edge_name,
                priority,
                disclosure
            ],
        )?;
        tx.last_insert_rowid()
    };
    tx.execute(
        "INSERT OR IGNORE INTO paths(namespace, domain, path, edge_id)
         VALUES (?1, ?2, ?3, ?4)",
        params![namespace, uri.domain, uri.path, edge_id],
    )?;
    Ok(())
}

fn edge_metadata_for_node(
    tx: &Transaction<'_>,
    namespace: &str,
    node_uuid: &str,
    snapshot_uri: &str,
) -> Result<(i64, Option<String>)> {
    let snapshot_uri = ZmemoryUri::parse(snapshot_uri)?;
    if let Some((priority, disclosure)) = tx
        .query_row(
            "SELECT e.priority, e.disclosure
             FROM paths p
             JOIN edges e ON e.id = p.edge_id AND e.namespace = p.namespace
             WHERE p.namespace = ?1 AND p.domain = ?2 AND p.path = ?3",
            params![namespace, snapshot_uri.domain, snapshot_uri.path],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?)),
        )
        .optional()?
    {
        return Ok((priority, disclosure));
    }

    tx.query_row(
        "SELECT priority, disclosure
         FROM edges
         WHERE namespace = ?1 AND child_uuid = ?2
         ORDER BY id DESC
         LIMIT 1",
        params![namespace, node_uuid],
        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?)),
    )
    .optional()
    .map(|result| result.unwrap_or((0, None)))
    .map_err(Into::into)
}

fn delete_created_node(tx: &Transaction<'_>, namespace: &str, node_uuid: &str) -> Result<()> {
    let mut edge_ids = Vec::new();
    let mut stmt = tx.prepare(
        "SELECT id
         FROM edges
         WHERE namespace = ?1 AND (child_uuid = ?2 OR parent_uuid = ?2)",
    )?;
    for edge_id in stmt.query_map(params![namespace, node_uuid], |row| row.get::<_, i64>(0))? {
        edge_ids.push(edge_id?);
    }
    for edge_id in edge_ids {
        tx.execute(
            "DELETE FROM paths WHERE namespace = ?1 AND edge_id = ?2",
            params![namespace, edge_id],
        )?;
        tx.execute(
            "DELETE FROM edges WHERE namespace = ?1 AND id = ?2",
            params![namespace, edge_id],
        )?;
    }
    tx.execute(
        "DELETE FROM glossary_keywords WHERE namespace = ?1 AND node_uuid = ?2",
        params![namespace, node_uuid],
    )?;
    tx.execute(
        "DELETE FROM memories WHERE namespace = ?1 AND node_uuid = ?2",
        params![namespace, node_uuid],
    )?;
    tx.execute("DELETE FROM nodes WHERE uuid = ?1", params![node_uuid])?;
    Ok(())
}

fn delete_path_without_audit(
    tx: &Transaction<'_>,
    namespace: &str,
    uri: &ZmemoryUri,
) -> Result<()> {
    let row = tx
        .query_row(
            "SELECT p.edge_id, e.child_uuid
             FROM paths p
             JOIN edges e ON e.id = p.edge_id AND e.namespace = p.namespace
             WHERE p.namespace = ?1 AND p.domain = ?2 AND p.path = ?3",
            params![namespace, uri.domain, uri.path],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;
    let Some((edge_id, node_uuid)) = row else {
        return Ok(());
    };

    tx.execute(
        "DELETE FROM paths WHERE namespace = ?1 AND domain = ?2 AND path = ?3",
        params![namespace, uri.domain, uri.path],
    )?;
    let remaining_edge_paths: i64 = tx.query_row(
        "SELECT COUNT(*) FROM paths WHERE namespace = ?1 AND edge_id = ?2",
        params![namespace, edge_id],
        |row| row.get(0),
    )?;
    if remaining_edge_paths == 0 {
        tx.execute(
            "DELETE FROM edges WHERE namespace = ?1 AND id = ?2",
            params![namespace, edge_id],
        )?;
    }

    let remaining_refs: i64 = tx.query_row(
        "SELECT COUNT(*)
         FROM edges e
         JOIN paths p ON p.edge_id = e.id AND p.namespace = e.namespace
         WHERE e.namespace = ?1 AND e.child_uuid = ?2",
        params![namespace, node_uuid],
        |row| row.get(0),
    )?;
    if remaining_refs == 0 {
        tx.execute(
            "UPDATE memories
             SET deprecated = TRUE
             WHERE namespace = ?1 AND node_uuid = ?2 AND deprecated = FALSE",
            params![namespace, node_uuid],
        )?;
    }
    Ok(())
}

fn path_changes(
    entries: &[crate::service::contracts::AuditEntryContract],
) -> Vec<PathChangeResponse> {
    let mut changes = Vec::new();
    for entry in entries {
        match entry.action.as_str() {
            "add-alias" | "create" => {
                if let Some(uri) = entry.uri.as_ref().filter(|uri| !uri.is_empty()) {
                    changes.push(PathChangeResponse {
                        action: "created".to_string(),
                        uri: uri.clone(),
                        namespace: String::new(),
                    });
                }
            }
            "delete-path" => {
                if let Some(uri) = entry.uri.as_ref().filter(|uri| !uri.is_empty()) {
                    changes.push(PathChangeResponse {
                        action: "deleted".to_string(),
                        uri: uri.clone(),
                        namespace: String::new(),
                    });
                }
            }
            _ => {}
        }
    }
    changes
}

fn glossary_changes(
    entries: &[crate::service::contracts::AuditEntryContract],
) -> Vec<GlossaryChangeResponse> {
    let mut changes = Vec::new();
    for entry in entries {
        if entry.action != "manage-triggers" {
            continue;
        }
        let Some(object) = entry.details.as_object() else {
            continue;
        };
        if let Some(added) = object.get("added").and_then(Value::as_array) {
            for keyword in added.iter().filter_map(Value::as_str) {
                changes.push(GlossaryChangeResponse {
                    action: "created".to_string(),
                    keyword: keyword.to_string(),
                });
            }
        }
        if let Some(removed) = object.get("removed").and_then(Value::as_array) {
            for keyword in removed.iter().filter_map(Value::as_str) {
                changes.push(GlossaryChangeResponse {
                    action: "deleted".to_string(),
                    keyword: keyword.to_string(),
                });
            }
        }
    }
    changes
}
