use crate::daemon::TldrDaemonConfigSummary;
use crate::daemon::TldrDaemonResponse;
use crate::daemon::TldrDaemonStatus;
use crate::lang_support::SupportedLanguage;
use crate::semantic::SemanticMatch;
use crate::semantic::SemanticReindexReport;
use crate::semantic::SemanticSearchResponse;
use crate::session::SessionSnapshot;
use serde::Serialize;
use serde_json::Value;
use serde_json::json;
use std::path::Path;
use std::path::PathBuf;
use std::time::SystemTime;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TldrSemanticMatchView {
    pub path: PathBuf,
    pub line: usize,
    pub snippet: String,
    pub symbol: Option<String>,
    #[serde(rename = "qualifiedSymbol")]
    pub qualified_symbol: Option<String>,
    pub kind: String,
    pub signature: Option<String>,
    pub embedding_score: Option<f32>,
}

impl From<&SemanticMatch> for TldrSemanticMatchView {
    fn from(value: &SemanticMatch) -> Self {
        Self {
            path: value.path.clone(),
            line: value.line,
            snippet: value.snippet.clone(),
            symbol: value.unit.symbol.clone(),
            qualified_symbol: value.unit.qualified_symbol.clone(),
            kind: value.unit.kind.clone(),
            signature: value.unit.signature.clone(),
            embedding_score: value.embedding_score,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TldrSemanticResponseView {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    pub project: PathBuf,
    pub language: String,
    pub source: String,
    pub query: String,
    pub enabled: bool,
    #[serde(rename = "indexedFiles")]
    pub indexed_files: usize,
    pub truncated: bool,
    #[serde(rename = "embeddingUsed")]
    pub embedding_used: bool,
    pub message: String,
    pub matches: Vec<TldrSemanticMatchView>,
}

impl TldrSemanticResponseView {
    pub fn from_response(
        action: Option<&str>,
        project_root: &Path,
        language: SupportedLanguage,
        source: &str,
        response: &SemanticSearchResponse,
    ) -> Self {
        Self {
            action: action.map(str::to_string),
            project: project_root.to_path_buf(),
            language: language.as_str().to_string(),
            source: source.to_string(),
            query: response.query.clone(),
            enabled: response.enabled,
            indexed_files: response.indexed_files,
            truncated: response.truncated,
            embedding_used: response.embedding_used,
            message: response.message.clone(),
            matches: response
                .matches
                .iter()
                .map(TldrSemanticMatchView::from)
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TldrDaemonConfigSummaryView {
    pub auto_start: bool,
    pub socket_mode: String,
    pub semantic_enabled: bool,
    pub semantic_auto_reindex_threshold: usize,
    pub session_dirty_file_threshold: usize,
}

impl From<&TldrDaemonConfigSummary> for TldrDaemonConfigSummaryView {
    fn from(value: &TldrDaemonConfigSummary) -> Self {
        Self {
            auto_start: value.auto_start,
            socket_mode: value.socket_mode.clone(),
            semantic_enabled: value.semantic_enabled,
            semantic_auto_reindex_threshold: value.semantic_auto_reindex_threshold,
            session_dirty_file_threshold: value.session_dirty_file_threshold,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TldrSemanticReindexReportView {
    pub status: String,
    pub languages: Vec<String>,
    pub indexed_files: usize,
    pub indexed_units: usize,
    pub truncated: bool,
    pub started_at: SystemTime,
    pub finished_at: SystemTime,
    pub message: String,
    pub embedding_enabled: bool,
    pub embedding_dimensions: usize,
}

impl From<&SemanticReindexReport> for TldrSemanticReindexReportView {
    fn from(value: &SemanticReindexReport) -> Self {
        Self {
            status: format!("{:?}", value.status),
            languages: value
                .languages
                .iter()
                .map(|language| language.as_str().to_string())
                .collect(),
            indexed_files: value.indexed_files,
            indexed_units: value.indexed_units,
            truncated: value.truncated,
            started_at: value.started_at,
            finished_at: value.finished_at,
            message: value.message.clone(),
            embedding_enabled: value.embedding_enabled,
            embedding_dimensions: value.embedding_dimensions,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TldrSessionSnapshotView {
    pub cached_entries: usize,
    pub dirty_files: usize,
    pub dirty_file_threshold: usize,
    pub reindex_pending: bool,
    pub last_query_at: Option<SystemTime>,
    pub last_reindex: Option<TldrSemanticReindexReportView>,
    pub last_reindex_attempt: Option<TldrSemanticReindexReportView>,
}

impl From<&SessionSnapshot> for TldrSessionSnapshotView {
    fn from(value: &SessionSnapshot) -> Self {
        Self {
            cached_entries: value.cached_entries,
            dirty_files: value.dirty_files,
            dirty_file_threshold: value.dirty_file_threshold,
            reindex_pending: value.reindex_pending,
            last_query_at: value.last_query_at,
            last_reindex: value
                .last_reindex
                .as_ref()
                .map(TldrSemanticReindexReportView::from),
            last_reindex_attempt: value
                .last_reindex_attempt
                .as_ref()
                .map(TldrSemanticReindexReportView::from),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TldrDaemonStatusView {
    pub project_root: PathBuf,
    pub socket_path: PathBuf,
    pub pid_path: PathBuf,
    pub lock_path: PathBuf,
    pub socket_exists: bool,
    pub pid_is_live: bool,
    pub lock_is_held: bool,
    pub healthy: bool,
    pub stale_socket: bool,
    pub stale_pid: bool,
    pub health_reason: Option<String>,
    pub recovery_hint: Option<String>,
    pub semantic_reindex_pending: bool,
    pub last_query_at: Option<SystemTime>,
    pub config: TldrDaemonConfigSummaryView,
}

impl From<&TldrDaemonStatus> for TldrDaemonStatusView {
    fn from(value: &TldrDaemonStatus) -> Self {
        Self {
            project_root: value.project_root.clone(),
            socket_path: value.socket_path.clone(),
            pid_path: value.pid_path.clone(),
            lock_path: value.lock_path.clone(),
            socket_exists: value.socket_exists,
            pid_is_live: value.pid_is_live,
            lock_is_held: value.lock_is_held,
            healthy: value.healthy,
            stale_socket: value.stale_socket,
            stale_pid: value.stale_pid,
            health_reason: value.health_reason.clone(),
            recovery_hint: value.recovery_hint.clone(),
            semantic_reindex_pending: value.semantic_reindex_pending,
            last_query_at: value.last_query_at,
            config: TldrDaemonConfigSummaryView::from(&value.config),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TldrDaemonResponseView {
    pub action: String,
    pub project: PathBuf,
    pub status: String,
    pub message: String,
    pub snapshot: Option<TldrSessionSnapshotView>,
    #[serde(rename = "daemonStatus")]
    pub daemon_status: Option<TldrDaemonStatusView>,
    #[serde(rename = "reindexReport")]
    pub reindex_report: Option<TldrSemanticReindexReportView>,
}

impl TldrDaemonResponseView {
    pub fn from_response(action: &str, project_root: &Path, response: &TldrDaemonResponse) -> Self {
        Self {
            action: action.to_string(),
            project: project_root.to_path_buf(),
            status: response.status.clone(),
            message: response.message.clone(),
            snapshot: response
                .snapshot
                .as_ref()
                .map(TldrSessionSnapshotView::from),
            daemon_status: response
                .daemon_status
                .as_ref()
                .map(TldrDaemonStatusView::from),
            reindex_report: response
                .reindex_report
                .as_ref()
                .map(TldrSemanticReindexReportView::from),
        }
    }
}

pub fn semantic_payload(
    action: Option<&str>,
    project_root: &Path,
    language: SupportedLanguage,
    source: &str,
    response: &SemanticSearchResponse,
) -> Value {
    json!(TldrSemanticResponseView::from_response(
        action,
        project_root,
        language,
        source,
        response,
    ))
}

pub fn daemon_response_payload(
    action: &str,
    project_root: &Path,
    response: &TldrDaemonResponse,
) -> Value {
    json!(TldrDaemonResponseView::from_response(
        action,
        project_root,
        response,
    ))
}

#[cfg(test)]
mod tests {
    use super::daemon_response_payload;
    use super::semantic_payload;
    use crate::daemon::TldrDaemonResponse;
    use crate::lang_support::SupportedLanguage;
    use crate::semantic::EmbeddingUnit;
    use crate::semantic::SemanticMatch;
    use crate::semantic::SemanticSearchResponse;
    use std::path::Path;
    use std::path::PathBuf;

    #[test]
    fn semantic_payload_omits_internal_fields() {
        let response = SemanticSearchResponse {
            enabled: true,
            query: "auth token".to_string(),
            indexed_files: 1,
            truncated: false,
            matches: vec![SemanticMatch {
                score: 7,
                path: PathBuf::from("src/auth.rs"),
                line: 2,
                snippet: "let auth_token = true;".to_string(),
                unit: EmbeddingUnit {
                    path: PathBuf::from("src/auth.rs"),
                    language: SupportedLanguage::Rust,
                    symbol: Some("verify_token".to_string()),
                    qualified_symbol: Some("auth::verify_token".to_string()),
                    symbol_aliases: vec![
                        "verify_token".to_string(),
                        "auth::verify_token".to_string(),
                    ],
                    kind: "function".to_string(),
                    line: 1,
                    span_end_line: 4,
                    module_path: vec!["auth".to_string()],
                    visibility: Some("pub".to_string()),
                    signature: Some("pub fn verify_token() -> bool".to_string()),
                    docs: vec!["Checks token".to_string()],
                    imports: vec!["use crate::auth::token;".to_string()],
                    references: vec!["Token".to_string()],
                    code_preview: "fn verify_token() {}".to_string(),
                    calls: Vec::new(),
                    called_by: Vec::new(),
                    dependencies: Vec::new(),
                    cfg_summary: "cfg".to_string(),
                    dfg_summary: "dfg".to_string(),
                    embedding_vector: None,
                },
                embedding_text: "internal".to_string(),
                embedding_score: Some(0.75),
            }],
            embedding_used: true,
            message: "semantic search returned 1 matches".to_string(),
        };

        let payload = semantic_payload(
            Some("semantic"),
            Path::new("/tmp/project"),
            SupportedLanguage::Rust,
            "daemon",
            &response,
        );

        assert_eq!(payload["embeddingUsed"], true);
        assert_eq!(payload["matches"][0]["path"], "src/auth.rs");
        assert_eq!(payload["matches"][0]["symbol"], "verify_token");
        assert_eq!(
            payload["matches"][0]["qualifiedSymbol"],
            "auth::verify_token"
        );
        assert_eq!(payload["matches"][0]["kind"], "function");
        assert_eq!(
            payload["matches"][0]["signature"],
            "pub fn verify_token() -> bool"
        );
        assert!(payload["matches"][0].get("unit").is_none());
        assert!(payload["matches"][0].get("embedding_text").is_none());
    }

    #[test]
    fn daemon_response_payload_keeps_stable_status_fields() {
        let response = TldrDaemonResponse {
            status: "ok".to_string(),
            message: "pong".to_string(),
            analysis: None,
            imports: None,
            importers: None,
            search: None,
            diagnostics: None,
            semantic: None,
            snapshot: None,
            daemon_status: None,
            reindex_report: None,
        };

        let payload = daemon_response_payload("status", Path::new("/tmp/project"), &response);
        assert_eq!(payload["action"], "status");
        assert_eq!(payload["project"], "/tmp/project");
        assert_eq!(payload["status"], "ok");
        assert_eq!(payload["message"], "pong");
        assert!(payload.get("analysis").is_none());
        assert!(payload.get("semantic").is_none());
    }

    #[test]
    fn daemon_response_payload_keeps_ping_action_contract() {
        let response = TldrDaemonResponse {
            status: "ok".to_string(),
            message: "pong".to_string(),
            analysis: None,
            imports: None,
            importers: None,
            search: None,
            diagnostics: None,
            semantic: None,
            snapshot: None,
            daemon_status: None,
            reindex_report: None,
        };

        let payload = daemon_response_payload("ping", Path::new("/tmp/project"), &response);
        assert_eq!(payload["action"], "ping");
        assert_eq!(payload["project"], "/tmp/project");
        assert_eq!(payload["status"], "ok");
        assert_eq!(payload["message"], "pong");
    }

    #[test]
    fn daemon_response_payload_keeps_snapshot_fields() {
        let response = TldrDaemonResponse {
            status: "ok".to_string(),
            message: "snapshot".to_string(),
            analysis: None,
            imports: None,
            importers: None,
            search: None,
            diagnostics: None,
            semantic: None,
            snapshot: Some(crate::session::SessionSnapshot {
                cached_entries: 2,
                dirty_files: 1,
                dirty_file_threshold: 20,
                reindex_pending: true,
                last_query_at: None,
                last_reindex: None,
                last_reindex_attempt: None,
            }),
            daemon_status: None,
            reindex_report: None,
        };

        let payload = daemon_response_payload("snapshot", Path::new("/tmp/project"), &response);
        assert_eq!(payload["action"], "snapshot");
        assert_eq!(payload["snapshot"]["cached_entries"], 2);
        assert_eq!(payload["snapshot"]["dirty_files"], 1);
        assert_eq!(payload["snapshot"]["reindex_pending"], true);
    }

    #[test]
    fn daemon_response_payload_keeps_status_detail_fields() {
        let report = crate::semantic::SemanticReindexReport {
            status: crate::semantic::SemanticReindexStatus::Completed,
            languages: vec![crate::lang_support::SupportedLanguage::Rust],
            indexed_files: 2,
            indexed_units: 3,
            truncated: false,
            started_at: std::time::SystemTime::UNIX_EPOCH,
            finished_at: std::time::SystemTime::UNIX_EPOCH,
            message: "done".to_string(),
            embedding_enabled: true,
            embedding_dimensions: 256,
        };
        let response = TldrDaemonResponse {
            status: "ok".to_string(),
            message: "status".to_string(),
            analysis: None,
            imports: None,
            importers: None,
            search: None,
            diagnostics: None,
            semantic: None,
            snapshot: Some(crate::session::SessionSnapshot {
                cached_entries: 1,
                dirty_files: 0,
                dirty_file_threshold: 20,
                reindex_pending: false,
                last_query_at: Some(std::time::SystemTime::UNIX_EPOCH),
                last_reindex: Some(report.clone()),
                last_reindex_attempt: Some(report.clone()),
            }),
            daemon_status: Some(crate::daemon::TldrDaemonStatus {
                project_root: PathBuf::from("/tmp/project"),
                socket_path: PathBuf::from("/tmp/project.sock"),
                pid_path: PathBuf::from("/tmp/project.pid"),
                lock_path: PathBuf::from("/tmp/project.lock"),
                socket_exists: true,
                pid_is_live: true,
                lock_is_held: true,
                healthy: true,
                stale_socket: false,
                stale_pid: false,
                health_reason: None,
                recovery_hint: None,
                semantic_reindex_pending: false,
                last_query_at: Some(std::time::SystemTime::UNIX_EPOCH),
                config: crate::daemon::TldrDaemonConfigSummary {
                    auto_start: true,
                    socket_mode: "unix".to_string(),
                    semantic_enabled: true,
                    semantic_auto_reindex_threshold: 20,
                    session_dirty_file_threshold: 20,
                },
            }),
            reindex_report: Some(report),
        };

        let payload = daemon_response_payload("status", Path::new("/tmp/project"), &response);
        assert_eq!(payload["daemonStatus"]["healthy"], true);
        assert_eq!(payload["reindexReport"]["status"], "Completed");
        assert_eq!(payload["snapshot"]["last_reindex"]["status"], "Completed");
        assert_eq!(
            payload["snapshot"]["last_reindex_attempt"]["status"],
            "Completed"
        );
    }
}
