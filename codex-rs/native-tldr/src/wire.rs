use crate::daemon::DegradedMode;
use crate::daemon::DegradedModeKind;
use crate::daemon::StructuredFailure;
use crate::daemon::StructuredFailureKind;
use crate::daemon::TldrDaemonConfigSummary;
use crate::daemon::TldrDaemonResponse;
use crate::daemon::TldrDaemonStatus;
use crate::daemon::daemon_health;
use crate::lang_support::SupportedLanguage;
use crate::lifecycle::DaemonReadyResult;
use crate::semantic::SemanticMatch;
use crate::semantic::SemanticReindexReport;
use crate::semantic::SemanticSearchResponse;
use crate::session::SessionSnapshot;
use crate::session::WarmReport;
use serde::Serialize;
use serde_json::Value;
use serde_json::json;
use std::path::Path;
use std::path::PathBuf;
use std::time::SystemTime;

const SEMANTIC_PAYLOAD_MAX_MATCHES: usize = 20;
const SEMANTIC_PAYLOAD_MAX_SNIPPET_CHARS: usize = 240;

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
        let snippet = if value.snippet.chars().count() > SEMANTIC_PAYLOAD_MAX_SNIPPET_CHARS {
            let mut snippet = value
                .snippet
                .chars()
                .take(SEMANTIC_PAYLOAD_MAX_SNIPPET_CHARS)
                .collect::<String>();
            snippet.push_str("...");
            snippet
        } else {
            value.snippet.clone()
        };
        Self {
            path: value.path.clone(),
            line: value.line,
            snippet,
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
    #[serde(rename = "degradedMode")]
    pub degraded_mode: Option<TldrDegradedModeView>,
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
        let payload_truncated = response.matches.len() > SEMANTIC_PAYLOAD_MAX_MATCHES
            || response.matches.iter().any(|semantic_match| {
                semantic_match.snippet.chars().count() > SEMANTIC_PAYLOAD_MAX_SNIPPET_CHARS
            });
        Self {
            action: action.map(str::to_string),
            project: project_root.to_path_buf(),
            language: language.as_str().to_string(),
            source: source.to_string(),
            query: response.query.clone(),
            enabled: response.enabled,
            indexed_files: response.indexed_files,
            truncated: response.truncated || payload_truncated,
            embedding_used: response.embedding_used,
            message: response.message.clone(),
            degraded_mode: degraded_mode_for_source(source),
            matches: response
                .matches
                .iter()
                .take(SEMANTIC_PAYLOAD_MAX_MATCHES)
                .map(TldrSemanticMatchView::from)
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TldrStructuredFailureView {
    pub error_type: String,
    pub reason: String,
    pub retryable: bool,
    pub retry_hint: Option<String>,
}

impl From<&StructuredFailure> for TldrStructuredFailureView {
    fn from(value: &StructuredFailure) -> Self {
        Self {
            error_type: structured_failure_kind_name(&value.kind).to_string(),
            reason: value.reason.clone(),
            retryable: value.retryable,
            retry_hint: value.retry_hint.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TldrDegradedModeView {
    pub is_degraded: bool,
    pub mode: String,
    pub fallback_path: String,
    pub reason: Option<String>,
}

impl From<&DegradedMode> for TldrDegradedModeView {
    fn from(value: &DegradedMode) -> Self {
        Self {
            is_degraded: true,
            mode: degraded_mode_kind_name(&value.kind).to_string(),
            fallback_path: value.fallback_path.clone(),
            reason: value.reason.clone(),
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
    pub session_idle_timeout_secs: u64,
}

impl From<&TldrDaemonConfigSummary> for TldrDaemonConfigSummaryView {
    fn from(value: &TldrDaemonConfigSummary) -> Self {
        Self {
            auto_start: value.auto_start,
            socket_mode: value.socket_mode.clone(),
            semantic_enabled: value.semantic_enabled,
            semantic_auto_reindex_threshold: value.semantic_auto_reindex_threshold,
            session_dirty_file_threshold: value.session_dirty_file_threshold,
            session_idle_timeout_secs: value.session_idle_timeout_secs,
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
pub struct TldrWarmReportView {
    pub status: String,
    pub languages: Vec<String>,
    pub started_at: SystemTime,
    pub finished_at: SystemTime,
    pub message: String,
}

impl From<&WarmReport> for TldrWarmReportView {
    fn from(value: &WarmReport) -> Self {
        Self {
            status: format!("{:?}", value.status),
            languages: value
                .languages
                .iter()
                .map(|language| language.as_str().to_string())
                .collect(),
            started_at: value.started_at,
            finished_at: value.finished_at,
            message: value.message.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TldrSessionSnapshotView {
    pub cached_entries: usize,
    pub dirty_files: usize,
    pub dirty_file_threshold: usize,
    pub reindex_pending: bool,
    pub background_reindex_in_progress: bool,
    pub last_query_at: Option<SystemTime>,
    pub last_reindex: Option<TldrSemanticReindexReportView>,
    pub last_reindex_attempt: Option<TldrSemanticReindexReportView>,
    pub last_warm: Option<TldrWarmReportView>,
    #[serde(rename = "lastStructuredFailure")]
    pub last_structured_failure: Option<TldrStructuredFailureView>,
    #[serde(rename = "degradedModeActive")]
    pub degraded_mode_active: bool,
}

impl From<&SessionSnapshot> for TldrSessionSnapshotView {
    fn from(value: &SessionSnapshot) -> Self {
        Self {
            cached_entries: value.cached_entries,
            dirty_files: value.dirty_files,
            dirty_file_threshold: value.dirty_file_threshold,
            reindex_pending: value.reindex_pending,
            background_reindex_in_progress: value.background_reindex_in_progress,
            last_query_at: value.last_query_at,
            last_reindex: value
                .last_reindex
                .as_ref()
                .map(TldrSemanticReindexReportView::from),
            last_reindex_attempt: value
                .last_reindex_attempt
                .as_ref()
                .map(TldrSemanticReindexReportView::from),
            last_warm: value.last_warm.as_ref().map(TldrWarmReportView::from),
            last_structured_failure: value
                .last_structured_failure
                .as_ref()
                .map(TldrStructuredFailureView::from),
            degraded_mode_active: value.degraded_mode_active,
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
    #[serde(rename = "structuredFailure")]
    pub structured_failure: Option<TldrStructuredFailureView>,
    #[serde(rename = "degradedMode")]
    pub degraded_mode: Option<TldrDegradedModeView>,
    pub semantic_reindex_pending: bool,
    pub semantic_reindex_in_progress: bool,
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
            structured_failure: value
                .structured_failure
                .as_ref()
                .map(TldrStructuredFailureView::from),
            degraded_mode: value.degraded_mode.as_ref().map(TldrDegradedModeView::from),
            semantic_reindex_pending: value.semantic_reindex_pending,
            semantic_reindex_in_progress: value.semantic_reindex_in_progress,
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
    #[serde(rename = "structuredFailure")]
    pub structured_failure: Option<TldrStructuredFailureView>,
    #[serde(rename = "degradedMode")]
    pub degraded_mode: Option<TldrDegradedModeView>,
}

impl TldrDaemonResponseView {
    pub fn from_response(action: &str, project_root: &Path, response: &TldrDaemonResponse) -> Self {
        let daemon_status = response.daemon_status.as_ref();
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
            structured_failure: daemon_status
                .and_then(|status| status.structured_failure.as_ref())
                .map(TldrStructuredFailureView::from),
            degraded_mode: daemon_status
                .and_then(|status| status.degraded_mode.as_ref())
                .map(TldrDegradedModeView::from),
        }
    }
}

fn degraded_mode_for_source(source: &str) -> Option<TldrDegradedModeView> {
    if source == "local" {
        Some(TldrDegradedModeView {
            is_degraded: true,
            mode: "local_fallback".to_string(),
            fallback_path: "local".to_string(),
            reason: Some("daemon-first path unavailable; used local engine".to_string()),
        })
    } else {
        None
    }
}

fn structured_failure_kind_name(kind: &StructuredFailureKind) -> &'static str {
    match kind {
        StructuredFailureKind::DaemonUnavailable => "daemon_unavailable",
        StructuredFailureKind::DaemonStarting => "daemon_starting",
        StructuredFailureKind::StaleSocket => "stale_socket",
        StructuredFailureKind::StalePid => "stale_pid",
        StructuredFailureKind::DaemonUnhealthy => "daemon_unhealthy",
    }
}

fn degraded_mode_kind_name(kind: &DegradedModeKind) -> &'static str {
    match kind {
        DegradedModeKind::DiagnosticOnly => "diagnostic_only",
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

pub fn daemon_failure_payload_for_project(
    project_root: &Path,
    ready_result: Option<&DaemonReadyResult>,
) -> Value {
    let health = daemon_health(project_root).ok();
    let structured_failure = ready_result
        .and_then(|value| value.structured_failure.as_ref())
        .or_else(|| {
            health
                .as_ref()
                .and_then(|value| value.structured_failure.as_ref())
        })
        .map(TldrStructuredFailureView::from);
    let degraded_mode = ready_result
        .and_then(|value| value.degraded_mode.as_ref())
        .or_else(|| {
            health
                .as_ref()
                .and_then(|value| value.degraded_mode.as_ref())
        })
        .map(TldrDegradedModeView::from);

    json!({
        "structuredFailure": structured_failure,
        "degradedMode": degraded_mode,
    })
}

#[cfg(test)]
mod tests {
    use super::SEMANTIC_PAYLOAD_MAX_SNIPPET_CHARS;
    use super::daemon_failure_payload_for_project;
    use super::daemon_response_payload;
    use super::semantic_payload;
    use crate::daemon::DegradedMode;
    use crate::daemon::DegradedModeKind;
    use crate::daemon::StructuredFailure;
    use crate::daemon::StructuredFailureKind;
    use crate::daemon::TldrDaemonResponse;
    use crate::lang_support::SupportedLanguage;
    use crate::lifecycle::DaemonReadyResult;
    use crate::semantic::EmbeddingUnit;
    use crate::semantic::SemanticMatch;
    use crate::semantic::SemanticSearchResponse;
    use serde_json::Value;
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
                    owner_symbol: None,
                    owner_kind: None,
                    implemented_trait: None,
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
        assert_eq!(payload["degradedMode"], Value::Null);
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
    fn semantic_payload_caps_match_count_and_marks_truncated() {
        let matches = (0..25)
            .map(|index| SemanticMatch {
                score: index,
                path: PathBuf::from(format!("src/auth_{index}.rs")),
                line: index + 1,
                snippet: format!("let auth_token_{index} = true;"),
                unit: EmbeddingUnit {
                    path: PathBuf::from(format!("src/auth_{index}.rs")),
                    language: SupportedLanguage::Rust,
                    symbol: Some(format!("verify_token_{index}")),
                    qualified_symbol: Some(format!("auth::verify_token_{index}")),
                    symbol_aliases: vec![format!("verify_token_{index}")],
                    kind: "function".to_string(),
                    owner_symbol: None,
                    owner_kind: None,
                    implemented_trait: None,
                    line: index + 1,
                    span_end_line: index + 2,
                    module_path: vec!["auth".to_string()],
                    visibility: Some("pub".to_string()),
                    signature: Some(format!("pub fn verify_token_{index}() -> bool")),
                    docs: Vec::new(),
                    imports: Vec::new(),
                    references: Vec::new(),
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
            })
            .collect();
        let response = SemanticSearchResponse {
            enabled: true,
            query: "auth token".to_string(),
            indexed_files: 25,
            truncated: false,
            matches,
            embedding_used: true,
            message: "semantic search returned 25 matches".to_string(),
        };

        let payload = semantic_payload(
            Some("semantic"),
            Path::new("/tmp/project"),
            SupportedLanguage::Rust,
            "daemon",
            &response,
        );

        assert_eq!(payload["matches"].as_array().map(Vec::len), Some(20));
        assert_eq!(payload["truncated"], true);
    }

    #[test]
    fn semantic_payload_marks_local_source_as_degraded_mode() {
        let response = SemanticSearchResponse {
            enabled: true,
            query: "auth token".to_string(),
            indexed_files: 1,
            truncated: false,
            matches: Vec::new(),
            embedding_used: false,
            message: "local fallback".to_string(),
        };

        let payload = semantic_payload(
            Some("semantic"),
            Path::new("/tmp/project"),
            SupportedLanguage::Rust,
            "local",
            &response,
        );

        assert_eq!(payload["degradedMode"]["is_degraded"], true);
        assert_eq!(payload["degradedMode"]["mode"], "local_fallback");
        assert_eq!(payload["degradedMode"]["fallback_path"], "local");
    }

    #[test]
    fn semantic_payload_truncates_long_snippets_and_marks_truncated() {
        let long_snippet = "a".repeat(SEMANTIC_PAYLOAD_MAX_SNIPPET_CHARS + 50);
        let response = SemanticSearchResponse {
            enabled: true,
            query: "auth token".to_string(),
            indexed_files: 1,
            truncated: false,
            matches: vec![SemanticMatch {
                score: 7,
                path: PathBuf::from("src/auth.rs"),
                line: 2,
                snippet: long_snippet,
                unit: EmbeddingUnit {
                    path: PathBuf::from("src/auth.rs"),
                    language: SupportedLanguage::Rust,
                    symbol: Some("verify_token".to_string()),
                    qualified_symbol: Some("auth::verify_token".to_string()),
                    symbol_aliases: vec!["verify_token".to_string()],
                    kind: "function".to_string(),
                    owner_symbol: None,
                    owner_kind: None,
                    implemented_trait: None,
                    line: 1,
                    span_end_line: 4,
                    module_path: vec!["auth".to_string()],
                    visibility: Some("pub".to_string()),
                    signature: Some("pub fn verify_token() -> bool".to_string()),
                    docs: Vec::new(),
                    imports: Vec::new(),
                    references: Vec::new(),
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
        let snippet = payload["matches"][0]["snippet"]
            .as_str()
            .expect("snippet should serialize");

        assert_eq!(
            snippet.chars().count(),
            SEMANTIC_PAYLOAD_MAX_SNIPPET_CHARS + 3
        );
        assert!(snippet.ends_with("..."));
        assert_eq!(payload["truncated"], true);
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
        assert_eq!(payload["structuredFailure"], Value::Null);
        assert_eq!(payload["degradedMode"], Value::Null);
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
                background_reindex_in_progress: true,
                last_query_at: None,
                last_reindex: None,
                last_reindex_attempt: None,
                last_warm: None,
                last_structured_failure: None,
                degraded_mode_active: false,
            }),
            daemon_status: None,
            reindex_report: None,
        };

        let payload = daemon_response_payload("snapshot", Path::new("/tmp/project"), &response);
        assert_eq!(payload["action"], "snapshot");
        assert_eq!(payload["snapshot"]["cached_entries"], 2);
        assert_eq!(payload["snapshot"]["dirty_files"], 1);
        assert_eq!(payload["snapshot"]["reindex_pending"], true);
        assert_eq!(payload["snapshot"]["background_reindex_in_progress"], true);
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
                background_reindex_in_progress: false,
                last_query_at: Some(std::time::SystemTime::UNIX_EPOCH),
                last_reindex: Some(report.clone()),
                last_reindex_attempt: Some(report.clone()),
                last_warm: Some(crate::session::WarmReport {
                    status: crate::session::WarmStatus::Loaded,
                    languages: vec![crate::lang_support::SupportedLanguage::Rust],
                    started_at: std::time::SystemTime::UNIX_EPOCH,
                    finished_at: std::time::SystemTime::UNIX_EPOCH,
                    message: "warm loaded 1 language indexes into daemon cache".to_string(),
                }),
                last_structured_failure: None,
                degraded_mode_active: false,
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
                structured_failure: None,
                degraded_mode: None,
                semantic_reindex_pending: false,
                semantic_reindex_in_progress: true,
                last_query_at: Some(std::time::SystemTime::UNIX_EPOCH),
                config: crate::daemon::TldrDaemonConfigSummary {
                    auto_start: true,
                    socket_mode: "unix".to_string(),
                    semantic_enabled: true,
                    semantic_auto_reindex_threshold: 20,
                    session_dirty_file_threshold: 20,
                    session_idle_timeout_secs: 1800,
                },
            }),
            reindex_report: Some(report),
        };

        let payload = daemon_response_payload("status", Path::new("/tmp/project"), &response);
        assert_eq!(payload["daemonStatus"]["healthy"], true);
        assert_eq!(payload["structuredFailure"], Value::Null);
        assert_eq!(payload["degradedMode"], Value::Null);
        assert_eq!(
            payload["daemonStatus"]["config"]["session_idle_timeout_secs"],
            1800
        );
        assert_eq!(payload["reindexReport"]["status"], "Completed");
        assert_eq!(
            payload["daemonStatus"]["semantic_reindex_in_progress"],
            true
        );
        assert_eq!(payload["snapshot"]["last_warm"]["status"], "Loaded");
        assert_eq!(payload["snapshot"]["last_reindex"]["status"], "Completed");
        assert_eq!(
            payload["snapshot"]["last_reindex_attempt"]["status"],
            "Completed"
        );
    }

    #[test]
    fn daemon_response_payload_surfaces_structured_failure_for_unhealthy_status() {
        let response = TldrDaemonResponse {
            status: "ok".to_string(),
            message: "status".to_string(),
            analysis: None,
            imports: None,
            importers: None,
            search: None,
            diagnostics: None,
            semantic: None,
            snapshot: None,
            daemon_status: Some(crate::daemon::TldrDaemonStatus {
                project_root: PathBuf::from("/tmp/project"),
                socket_path: PathBuf::from("/tmp/project.sock"),
                pid_path: PathBuf::from("/tmp/project.pid"),
                lock_path: PathBuf::from("/tmp/project.lock"),
                socket_exists: false,
                pid_is_live: false,
                lock_is_held: false,
                healthy: false,
                stale_socket: false,
                stale_pid: false,
                health_reason: Some("daemon missing".to_string()),
                recovery_hint: Some("start the daemon".to_string()),
                structured_failure: Some(crate::daemon::StructuredFailure {
                    kind: crate::daemon::StructuredFailureKind::DaemonUnavailable,
                    reason: "daemon missing".to_string(),
                    retryable: true,
                    retry_hint: Some("start the daemon".to_string()),
                }),
                degraded_mode: Some(crate::daemon::DegradedMode {
                    kind: crate::daemon::DegradedModeKind::DiagnosticOnly,
                    fallback_path: "status_only".to_string(),
                    reason: Some(
                        "daemon-only actions cannot proceed without a live daemon".to_string(),
                    ),
                }),
                semantic_reindex_pending: false,
                semantic_reindex_in_progress: false,
                last_query_at: None,
                config: crate::daemon::TldrDaemonConfigSummary {
                    auto_start: true,
                    socket_mode: "unix".to_string(),
                    semantic_enabled: true,
                    semantic_auto_reindex_threshold: 20,
                    session_dirty_file_threshold: 20,
                    session_idle_timeout_secs: 1800,
                },
            }),
            reindex_report: None,
        };

        let payload = daemon_response_payload("status", Path::new("/tmp/project"), &response);

        assert_eq!(
            payload["structuredFailure"]["error_type"],
            "daemon_unavailable"
        );
        assert_eq!(payload["structuredFailure"]["retryable"], true);
        assert_eq!(
            payload["structuredFailure"]["retry_hint"],
            "start the daemon"
        );
        assert_eq!(payload["degradedMode"]["is_degraded"], true);
        assert_eq!(payload["degradedMode"]["mode"], "diagnostic_only");
    }

    #[test]
    fn daemon_failure_payload_prefers_ready_result_metadata() {
        let payload = daemon_failure_payload_for_project(
            Path::new("/tmp/project"),
            Some(&DaemonReadyResult {
                ready: false,
                structured_failure: Some(StructuredFailure {
                    kind: StructuredFailureKind::DaemonUnavailable,
                    reason: "daemon boot timed out".to_string(),
                    retryable: true,
                    retry_hint: Some("retry once".to_string()),
                }),
                degraded_mode: Some(DegradedMode {
                    kind: DegradedModeKind::DiagnosticOnly,
                    fallback_path: "status_only".to_string(),
                    reason: Some("daemon-only action".to_string()),
                }),
            }),
        );

        assert_eq!(
            payload["structuredFailure"]["reason"],
            "daemon boot timed out"
        );
        assert_eq!(payload["structuredFailure"]["retry_hint"], "retry once");
        assert_eq!(payload["degradedMode"]["mode"], "diagnostic_only");
        assert_eq!(payload["degradedMode"]["fallback_path"], "status_only");
    }
}
