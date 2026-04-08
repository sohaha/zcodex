use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ErrorDetailResponse {
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DomainSummary {
    pub domain: String,
    pub root_count: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BreadcrumbItem {
    pub path: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GlossaryMatchNodeResponse {
    pub node_uuid: String,
    pub uri: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GlossaryMatchResponse {
    pub keyword: String,
    pub nodes: Vec<GlossaryMatchNodeResponse>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BrowseNodeResponse {
    pub path: String,
    pub domain: String,
    pub uri: String,
    pub name: String,
    pub content: String,
    pub priority: i64,
    pub disclosure: Option<String>,
    pub created_at: Option<String>,
    pub is_virtual: bool,
    pub aliases: Vec<String>,
    pub node_uuid: String,
    pub glossary_keywords: Vec<String>,
    pub glossary_matches: Vec<GlossaryMatchResponse>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BrowseChildResponse {
    pub domain: String,
    pub path: String,
    pub uri: String,
    pub name: String,
    pub priority: i64,
    pub disclosure: Option<String>,
    pub content_snippet: String,
    pub approx_children_count: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BrowseNodePayload {
    pub node: BrowseNodeResponse,
    pub children: Vec<BrowseChildResponse>,
    pub breadcrumbs: Vec<BreadcrumbItem>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct UpdateNodeResponse {
    pub success: bool,
    pub memory_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GlossaryNodeResponse {
    pub node_uuid: String,
    pub uri: String,
    pub content_snippet: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GlossaryEntryResponse {
    pub keyword: String,
    pub nodes: Vec<GlossaryNodeResponse>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GlossaryListResponse {
    pub glossary: Vec<GlossaryEntryResponse>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SuccessMessageResponse {
    pub message: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RebuildSearchResponse {
    pub status: String,
    pub rebuilt_nodes: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReviewGroupItemResponse {
    pub node_uuid: String,
    pub display_uri: String,
    pub top_level_table: String,
    pub action: String,
    pub row_count: i64,
    pub namespaces: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct StateMetaResponse {
    pub priority: Option<i64>,
    pub disclosure: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PathChangeResponse {
    pub action: String,
    pub uri: String,
    pub namespace: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GlossaryChangeResponse {
    pub action: String,
    pub keyword: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReviewDiffResponse {
    pub uri: String,
    pub change_type: String,
    pub action: String,
    pub before_content: Option<String>,
    pub current_content: Option<String>,
    pub before_meta: StateMetaResponse,
    pub current_meta: StateMetaResponse,
    pub path_changes: Vec<PathChangeResponse>,
    pub glossary_changes: Vec<GlossaryChangeResponse>,
    pub active_paths: Vec<String>,
    pub has_changes: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReviewDeprecatedItemResponse {
    pub id: i64,
    pub content_snippet: String,
    pub migrated_to: Option<i64>,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReviewDeprecatedResponse {
    pub count: usize,
    pub memories: Vec<ReviewDeprecatedItemResponse>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AdminStatsResponse {
    pub generated_at: String,
    pub active_paths: i64,
    pub unique_nodes: i64,
    pub glossary_keywords: i64,
    pub orphaned_memories: i64,
    pub deprecated_memories: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AdminDoctorResponse {
    pub generated_at: String,
    pub status: String,
    pub checks: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct OrphanMigrationTargetSnippetResponse {
    pub id: i64,
    pub paths: Vec<String>,
    pub content_snippet: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct OrphanMigrationTargetDetailResponse {
    pub id: i64,
    pub paths: Vec<String>,
    pub content: String,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct OrphanListItemResponse {
    pub id: i64,
    pub content_snippet: String,
    pub created_at: Option<String>,
    pub deprecated: bool,
    pub migrated_to: Option<i64>,
    pub category: String,
    pub migration_target: Option<OrphanMigrationTargetSnippetResponse>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct OrphanDetailResponse {
    pub id: i64,
    pub content: String,
    pub created_at: Option<String>,
    pub deprecated: bool,
    pub migrated_to: Option<i64>,
    pub category: String,
    pub migration_target: Option<OrphanMigrationTargetDetailResponse>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DeleteOrphanResponse {
    pub deleted_memory_id: i64,
    pub chain_repaired_to: Option<i64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct HealthResponse {
    pub status: String,
    pub database: String,
}
