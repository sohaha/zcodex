use crate::path_resolution::ZmemoryPathSource;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PathResolutionContract {
    pub(crate) db_path: String,
    pub(crate) workspace_key: Option<String>,
    pub(crate) source: ZmemoryPathSource,
    pub(crate) reason: String,
    pub(crate) namespace: String,
    pub(crate) namespace_source: String,
    pub(crate) supports_namespace_selection: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NodeChildContract {
    pub(crate) name: String,
    pub(crate) priority: i64,
    pub(crate) disclosure: Option<String>,
    pub(crate) uri: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NodeAliasContract {
    pub(crate) uri: String,
    pub(crate) priority: i64,
    pub(crate) disclosure: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReadNodeContract {
    pub(crate) uri: String,
    pub(crate) node_uuid: String,
    pub(crate) memory_id: i64,
    pub(crate) content: String,
    pub(crate) priority: i64,
    pub(crate) disclosure: Option<String>,
    pub(crate) keywords: Vec<String>,
    pub(crate) children: Vec<NodeChildContract>,
    pub(crate) alias_count: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExportScopeContract {
    pub(crate) r#type: String,
    pub(crate) value: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExportNodeContract {
    pub(crate) uri: String,
    pub(crate) content: String,
    pub(crate) priority: i64,
    pub(crate) disclosure: Option<String>,
    pub(crate) keywords: Vec<String>,
    pub(crate) aliases: Vec<NodeAliasContract>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExportResultContract {
    pub(crate) scope: ExportScopeContract,
    pub(crate) count: usize,
    pub(crate) items: Vec<ExportNodeContract>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HistoryVersionContract {
    pub(crate) id: i64,
    pub(crate) content: String,
    pub(crate) deprecated: bool,
    pub(crate) migrated_to: Option<i64>,
    pub(crate) created_at: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChangeSetRecord {
    pub(crate) uri: String,
    pub(crate) node_uuid: String,
    pub(crate) versions: Vec<HistoryVersionContract>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReviewNodeSnapshotContract {
    pub(crate) uri: String,
    pub(crate) node_uuid: String,
    pub(crate) memory_id: i64,
    pub(crate) content: String,
    pub(crate) priority: i64,
    pub(crate) disclosure: Option<String>,
    pub(crate) keywords: Vec<String>,
    pub(crate) aliases: Vec<NodeAliasContract>,
    pub(crate) children: Vec<NodeChildContract>,
    pub(crate) alias_count: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReviewGroupContract {
    pub(crate) node_uuid: String,
    pub(crate) domain: String,
    pub(crate) path: String,
    pub(crate) alias_count: i64,
    pub(crate) trigger_count: i64,
    pub(crate) missing_triggers: bool,
    pub(crate) review_priority: String,
    pub(crate) priority_score: i64,
    pub(crate) priority_reason: String,
    pub(crate) suggested_keywords: Vec<String>,
    pub(crate) node_uri: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReviewRecommendationContract {
    pub(crate) node_uri: String,
    pub(crate) missing_triggers: bool,
    pub(crate) priority_score: i64,
    pub(crate) review_priority: String,
    pub(crate) priority_reason: String,
    pub(crate) suggested_keywords: Vec<String>,
    pub(crate) action: String,
    pub(crate) advice: String,
    pub(crate) command: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AliasReviewViewContract {
    pub(crate) view: String,
    pub(crate) entry_count: usize,
    pub(crate) alias_node_count: i64,
    pub(crate) trigger_node_count: i64,
    pub(crate) alias_nodes_missing_triggers: i64,
    pub(crate) coverage_percent: i64,
    pub(crate) recommendations: Vec<ReviewRecommendationContract>,
    pub(crate) entries: Vec<ReviewGroupContract>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReviewRollbackTargetContract {
    pub(crate) id: i64,
    pub(crate) content: String,
    pub(crate) deprecated: bool,
    pub(crate) migrated_to: Option<i64>,
    pub(crate) created_at: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReviewGroupDiffContract {
    pub(crate) group: ReviewGroupContract,
    pub(crate) snapshot: ReviewNodeSnapshotContract,
    pub(crate) changeset: ChangeSetRecord,
    pub(crate) rollback_targets: Vec<ReviewRollbackTargetContract>,
    pub(crate) recent_audit_entries: Vec<AuditEntryContract>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ContentGovernanceScopeContract {
    pub(crate) uri: String,
    pub(crate) kind: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ContentGovernanceIssueContract {
    pub(crate) code: String,
    pub(crate) severity: String,
    pub(crate) message: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ContentGovernanceRuleContract {
    pub(crate) rule_id: String,
    pub(crate) outcome: String,
    pub(crate) message: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ContentGovernanceResultContract {
    pub(crate) status: String,
    pub(crate) scope: Option<ContentGovernanceScopeContract>,
    pub(crate) changed: bool,
    pub(crate) governed_content: String,
    pub(crate) issues: Vec<ContentGovernanceIssueContract>,
    pub(crate) rules: Vec<ContentGovernanceRuleContract>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DoctorIssueContract {
    pub(crate) code: String,
    pub(crate) message: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DoctorSummaryContract {
    pub(crate) healthy: bool,
    pub(crate) orphaned_memory_count: i64,
    pub(crate) deprecated_memory_count: i64,
    pub(crate) alias_node_count: i64,
    pub(crate) trigger_node_count: i64,
    pub(crate) alias_nodes_missing_triggers: i64,
    pub(crate) paths_missing_disclosure: i64,
    pub(crate) disclosures_needing_review: i64,
    pub(crate) issues: Vec<DoctorIssueContract>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MaintenanceStatsContract {
    pub(crate) db_path: String,
    pub(crate) workspace_key: Option<String>,
    pub(crate) source: ZmemoryPathSource,
    pub(crate) reason: String,
    pub(crate) namespace: String,
    pub(crate) namespace_source: String,
    pub(crate) supports_namespace_selection: bool,
    pub(crate) path_resolution: PathResolutionContract,
    pub(crate) node_count: i64,
    pub(crate) memory_count: i64,
    pub(crate) path_count: i64,
    pub(crate) glossary_keyword_count: i64,
    pub(crate) orphaned_memory_count: i64,
    pub(crate) deprecated_memory_count: i64,
    pub(crate) alias_node_count: i64,
    pub(crate) trigger_node_count: i64,
    pub(crate) alias_nodes_missing_triggers: i64,
    pub(crate) disclosure_path_count: i64,
    pub(crate) paths_missing_disclosure: i64,
    pub(crate) disclosures_needing_review: i64,
    pub(crate) search_document_count: i64,
    pub(crate) fts_document_count: i64,
    pub(crate) audit_log_count: i64,
    pub(crate) latest_audit_at: Option<String>,
    pub(crate) audit_action_counts: BTreeMap<String, i64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MaintenanceDoctorContract {
    pub(crate) db_path: String,
    pub(crate) workspace_key: Option<String>,
    pub(crate) source: ZmemoryPathSource,
    pub(crate) reason: String,
    pub(crate) namespace: String,
    pub(crate) namespace_source: String,
    pub(crate) supports_namespace_selection: bool,
    pub(crate) healthy: bool,
    pub(crate) orphaned_memory_count: i64,
    pub(crate) deprecated_memory_count: i64,
    pub(crate) alias_node_count: i64,
    pub(crate) trigger_node_count: i64,
    pub(crate) alias_nodes_missing_triggers: i64,
    pub(crate) paths_missing_disclosure: i64,
    pub(crate) disclosures_needing_review: i64,
    pub(crate) issues: Vec<DoctorIssueContract>,
    pub(crate) stats: MaintenanceStatsContract,
    pub(crate) path_resolution: PathResolutionContract,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AuditEntryContract {
    pub(crate) id: i64,
    pub(crate) action: String,
    pub(crate) uri: Option<String>,
    pub(crate) node_uuid: Option<String>,
    pub(crate) details: serde_json::Value,
    pub(crate) created_at: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AuditResultContract {
    pub(crate) count: usize,
    pub(crate) limit: usize,
    pub(crate) audit_action: Option<String>,
    pub(crate) uri: Option<String>,
    pub(crate) entries: Vec<AuditEntryContract>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RebuildSearchResultContract {
    pub(crate) document_count: i64,
    pub(crate) fts_document_count: i64,
}
