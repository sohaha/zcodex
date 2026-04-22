use crate::service::contracts::DoctorIssueContract;
use crate::service::contracts::DoctorSummaryContract;
use crate::service::stats::StatsSnapshot;
use anyhow::Result;
use rusqlite::Connection;

pub fn run_doctor(
    conn: &Connection,
    namespace: &str,
    stats: &StatsSnapshot,
) -> Result<DoctorSummaryContract> {
    let search_count = stats.search_document_count;
    let fts_count = stats.fts_document_count;

    let mut issues = Vec::new();
    if search_count != fts_count {
        issues.push(DoctorIssueContract {
            code: "fts_count_mismatch".to_string(),
            message: format!("search_documents={search_count}, search_documents_fts={fts_count}"),
            uris: Vec::new(),
        });
    }

    let active_memory_conflicts: i64 = conn.query_row(
        "SELECT COUNT(*) FROM (
            SELECT node_uuid
            FROM memories
            WHERE namespace = ?1 AND deprecated = FALSE
            GROUP BY node_uuid
            HAVING COUNT(*) > 1
        )",
        [namespace],
        |row| row.get(0),
    )?;
    if active_memory_conflicts > 0 {
        issues.push(DoctorIssueContract {
            code: "multiple_active_memories".to_string(),
            message: format!(
                "{active_memory_conflicts} nodes have more than one active memory row"
            ),
            uris: Vec::new(),
        });
    }

    let dangling_keywords: i64 = conn.query_row(
        "SELECT COUNT(*) FROM glossary_keywords g
         WHERE g.namespace = ?1
           AND NOT EXISTS (
             SELECT 1
             FROM edges e
             JOIN paths p ON p.edge_id = e.id AND p.namespace = e.namespace
             WHERE e.namespace = ?1 AND e.child_uuid = g.node_uuid
         )",
        [namespace],
        |row| row.get(0),
    )?;
    if dangling_keywords > 0 {
        issues.push(DoctorIssueContract {
            code: "dangling_keywords".to_string(),
            message: format!(
                "{dangling_keywords} glossary keyword rows point to nodes without any live path"
            ),
            uris: Vec::new(),
        });
    }

    let orphaned_memories = stats.orphaned_memory_count;
    if orphaned_memories > 0 {
        issues.push(DoctorIssueContract {
            code: "orphaned_memories".to_string(),
            message: format!("orphaned memories: {orphaned_memories}"),
            uris: Vec::new(),
        });
    }

    let deprecated_memories = stats.deprecated_memory_count;
    if deprecated_memories > 0 {
        issues.push(DoctorIssueContract {
            code: "deprecated_memories_awaiting_review".to_string(),
            message: format!("deprecated memories awaiting review: {deprecated_memories}"),
            uris: Vec::new(),
        });
    }

    let alias_nodes = stats.alias_node_count;
    let trigger_nodes = stats.trigger_node_count;
    let alias_nodes_missing = crate::service::stats::alias_nodes_missing_triggers(conn, namespace)?;
    let paths_missing_disclosure = stats.paths_missing_disclosure;
    let disclosures_needing_review = stats.disclosures_needing_review;
    if alias_nodes_missing > 0 {
        issues.push(DoctorIssueContract {
            code: "alias_nodes_missing_triggers".to_string(),
            message: format!("{alias_nodes_missing} alias nodes have no keywords"),
            uris: Vec::new(),
        });
    }
    if disclosures_needing_review > 0 {
        issues.push(DoctorIssueContract {
            code: "disclosures_need_review".to_string(),
            message: format!(
                "{disclosures_needing_review} disclosures look multi-trigger or ambiguous"
            ),
            uris: Vec::new(),
        });
    }

    let content_governance_issue_count = stats.content_governance_issue_count;
    let content_governance_conflict_count = stats.content_governance_conflict_count;
    if content_governance_issue_count > 0 {
        let affected_uris = stats
            .content_governance_results
            .iter()
            .filter(|result| result.status != "accepted")
            .filter_map(|result| result.scope.as_ref().map(|scope| scope.uri.clone()))
            .collect::<Vec<_>>();
        let code = if content_governance_conflict_count > 0 {
            "content_governance_conflicts"
        } else {
            "content_governance_normalization_needed"
        };
        let message = if content_governance_conflict_count > 0 {
            format!(
                "{content_governance_conflict_count} governed memories contain conflicting canonical facts"
            )
        } else {
            format!(
                "{content_governance_issue_count} governed memories need canonical content normalization"
            )
        };
        issues.push(DoctorIssueContract {
            code: code.to_string(),
            message,
            uris: affected_uris,
        });
    }

    Ok(DoctorSummaryContract {
        healthy: issues.is_empty(),
        orphaned_memory_count: orphaned_memories,
        deprecated_memory_count: deprecated_memories,
        alias_node_count: alias_nodes,
        trigger_node_count: trigger_nodes,
        alias_nodes_missing_triggers: alias_nodes_missing,
        paths_missing_disclosure,
        disclosures_needing_review,
        content_governance_issue_count,
        content_governance_conflict_count,
        issues,
    })
}
