use crate::config::ZmemoryConfig;
use crate::service::contracts::AuditEntryContract;
use crate::service::contracts::ContentGovernanceResultContract;
use crate::service::contracts::ReviewGroupContract;
use crate::service::contracts::ReviewGroupDiffContract;
use crate::service::contracts::ReviewNodeSnapshotContract;
use crate::service::contracts::ReviewRollbackTargetContract;
use crate::service::governance;
use crate::service::history;
use crate::service::snapshot;
use crate::tool_api::ZmemoryUri;
use anyhow::Result;
use rusqlite::Connection;
use rusqlite::params;
use serde_json::Value;

pub(crate) fn review_groups(
    conn: &Connection,
    config: &ZmemoryConfig,
    limit: usize,
) -> Result<Vec<ReviewGroupContract>> {
    let mut stmt = conn.prepare(
        "SELECT e.child_uuid, COUNT(*) AS alias_count
         FROM edges e
         JOIN paths p ON p.edge_id = e.id AND p.namespace = e.namespace
         WHERE e.namespace = ?1
         GROUP BY e.child_uuid
         HAVING COUNT(*) > 1
         ORDER BY alias_count DESC, e.child_uuid ASC",
    )?;
    let node_rows = stmt
        .query_map([config.namespace()], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut groups = node_rows
        .into_iter()
        .map(|(node_uuid, alias_count)| {
            review_group_for_node_uuid(conn, config, &node_uuid, alias_count)
        })
        .collect::<Result<Vec<_>>>()?;

    groups.sort_by(|left, right| {
        right
            .priority_score
            .cmp(&left.priority_score)
            .then_with(|| right.alias_count.cmp(&left.alias_count))
            .then_with(|| left.node_uri.cmp(&right.node_uri))
    });
    groups.truncate(limit);
    Ok(groups)
}

pub(crate) fn review_group_diff_for_node_uuid(
    conn: &Connection,
    config: &ZmemoryConfig,
    node_uuid: &str,
) -> Result<ReviewGroupDiffContract> {
    let alias_count = alias_count_for_node(conn, config.namespace(), node_uuid)?;
    let group = review_group_for_node_uuid(conn, config, node_uuid, alias_count)?;
    let node_snapshot = snapshot::load_node_snapshot_for_node(config, conn, node_uuid, None, None)?;
    let content_governance = evaluate_snapshot_governance(&node_snapshot);
    let review_snapshot = ReviewNodeSnapshotContract {
        uri: node_snapshot.primary_uri.clone(),
        node_uuid: node_snapshot.node_uuid.clone(),
        memory_id: node_snapshot.memory_id,
        content: node_snapshot.content,
        priority: node_snapshot.priority,
        disclosure: node_snapshot.disclosure,
        keywords: node_snapshot.keywords,
        aliases: node_snapshot.aliases,
        children: node_snapshot.children,
        alias_count: node_snapshot.alias_count,
    };
    let changeset = history::changeset_for_node(
        conn,
        config.namespace(),
        node_snapshot.primary_uri,
        node_snapshot.node_uuid,
    )?;
    let rollback_targets = changeset
        .versions
        .iter()
        .filter(|version| version.id != review_snapshot.memory_id)
        .map(|version| ReviewRollbackTargetContract {
            id: version.id,
            content: version.content.clone(),
            deprecated: version.deprecated,
            migrated_to: version.migrated_to,
            created_at: version.created_at.clone(),
        })
        .collect::<Vec<_>>();
    let recent_audit_entries = recent_audit_entries(conn, config.namespace(), node_uuid, 10)?;

    Ok(ReviewGroupDiffContract {
        group,
        snapshot: review_snapshot,
        changeset,
        rollback_targets,
        recent_audit_entries,
        content_governance,
    })
}

fn review_group_for_node_uuid(
    conn: &Connection,
    config: &ZmemoryConfig,
    node_uuid: &str,
    alias_count: i64,
) -> Result<ReviewGroupContract> {
    let node_snapshot = snapshot::load_node_snapshot_for_node(config, conn, node_uuid, None, None)?;
    let primary_uri = ZmemoryUri::parse(&node_snapshot.primary_uri)?;
    let content_governance = evaluate_snapshot_governance(&node_snapshot);
    let content_governance_status = summarize_governance_status(&content_governance);
    let content_governance_issue_count = content_governance
        .iter()
        .filter(|result| result.status != "accepted")
        .count() as i64;
    let trigger_count = node_snapshot.keywords.len() as i64;
    let missing_triggers = trigger_count == 0;
    let (review_priority, priority_score) =
        alias_review_priority(alias_count, missing_triggers, &content_governance_status);
    let priority_reason =
        alias_priority_reason(alias_count, missing_triggers, &content_governance_status);
    let suggested_keywords = if missing_triggers {
        infer_alias_keywords(&node_snapshot.primary_uri, &node_snapshot.aliases)
    } else {
        Vec::new()
    };

    Ok(ReviewGroupContract {
        node_uuid: node_uuid.to_string(),
        domain: primary_uri.domain,
        path: primary_uri.path,
        alias_count,
        trigger_count,
        missing_triggers,
        review_priority: review_priority.to_string(),
        priority_score,
        priority_reason,
        suggested_keywords,
        node_uri: node_snapshot.primary_uri,
        content_governance_status,
        content_governance_issue_count,
    })
}

fn alias_count_for_node(conn: &Connection, namespace: &str, node_uuid: &str) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(*)
         FROM edges e
         JOIN paths p ON p.edge_id = e.id AND p.namespace = e.namespace
         WHERE e.namespace = ?1 AND e.child_uuid = ?2",
        params![namespace, node_uuid],
        |row| row.get(0),
    )?)
}

fn recent_audit_entries(
    conn: &Connection,
    namespace: &str,
    node_uuid: &str,
    limit: usize,
) -> Result<Vec<AuditEntryContract>> {
    let mut stmt = conn.prepare(
        "SELECT id, action, uri, node_uuid, details, created_at
         FROM audit_log
         WHERE namespace = ?1 AND node_uuid = ?2
         ORDER BY id DESC
         LIMIT ?3",
    )?;
    stmt.query_map(params![namespace, node_uuid, limit as i64], |row| {
        let details = row.get::<_, String>(4)?;
        Ok(AuditEntryContract {
            id: row.get(0)?,
            action: row.get(1)?,
            uri: row.get(2)?,
            node_uuid: row.get(3)?,
            details: serde_json::from_str::<Value>(&details).unwrap_or(Value::String(details)),
            created_at: row.get(5)?,
        })
    })?
    .collect::<rusqlite::Result<Vec<_>>>()
    .map_err(Into::into)
}

fn evaluate_snapshot_governance(
    node_snapshot: &snapshot::NodeSnapshot,
) -> Vec<ContentGovernanceResultContract> {
    let uris = std::iter::once(node_snapshot.primary_uri.as_str())
        .chain(node_snapshot.aliases.iter().map(|alias| alias.uri.as_str()));
    governance::evaluate_uri_strings(uris, &node_snapshot.content)
}

fn summarize_governance_status(results: &[ContentGovernanceResultContract]) -> String {
    if results.iter().any(|result| result.status == "conflict") {
        "conflict".to_string()
    } else if results.iter().any(|result| result.status == "normalized") {
        "normalized".to_string()
    } else if results.iter().any(|result| result.status == "accepted") {
        "accepted".to_string()
    } else {
        "notApplicable".to_string()
    }
}

fn alias_review_priority(
    alias_count: i64,
    missing_triggers: bool,
    content_governance_status: &str,
) -> (&'static str, i64) {
    if content_governance_status == "conflict" {
        return ("high", 200 + alias_count);
    }
    if content_governance_status == "normalized" {
        return ("medium", 150 + alias_count);
    }
    if missing_triggers {
        let priority = if alias_count >= 3 { "high" } else { "medium" };
        let score = 100 + alias_count;
        (priority, score)
    } else {
        let priority = if alias_count >= 4 { "medium" } else { "low" };
        (priority, alias_count)
    }
}

fn alias_priority_reason(
    alias_count: i64,
    missing_triggers: bool,
    content_governance_status: &str,
) -> String {
    if content_governance_status == "conflict" {
        return "content governance conflict detected".to_string();
    }
    if content_governance_status == "normalized" {
        return "content governance normalization pending review".to_string();
    }
    if missing_triggers {
        format!("missing triggers across {alias_count} alias paths")
    } else {
        format!("covered by triggers across {alias_count} alias paths")
    }
}

fn infer_alias_keywords(
    primary_uri: &str,
    aliases: &[crate::service::contracts::NodeAliasContract],
) -> Vec<String> {
    let mut paths = Vec::with_capacity(aliases.len() + 1);
    if let Ok(uri) = ZmemoryUri::parse(primary_uri) {
        paths.push((uri.domain, uri.path));
    }
    paths.extend(aliases.iter().filter_map(|alias| {
        ZmemoryUri::parse(&alias.uri)
            .ok()
            .map(|uri| (uri.domain, uri.path))
    }));

    let mut keywords = paths
        .into_iter()
        .flat_map(|(_domain, path)| {
            path.split(|ch: char| !ch.is_ascii_alphanumeric())
                .filter(|segment| segment.len() >= 3)
                .map(str::to_lowercase)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    keywords.sort();
    keywords.dedup();
    keywords.truncate(3);
    keywords
}
