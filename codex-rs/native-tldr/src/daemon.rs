use crate::TldrConfig;
use crate::TldrEngine;
use crate::api::AnalysisRequest;
use crate::api::AnalysisResponse;
use crate::api::DiagnosticsRequest;
use crate::api::DiagnosticsResponse;
use crate::api::ImportersRequest;
use crate::api::ImportersResponse;
use crate::api::ImportsRequest;
use crate::api::ImportsResponse;
use crate::api::SearchRequest;
use crate::api::SearchResponse;
use crate::lang_support::SupportedLanguage;
use crate::semantic::SemanticReindexReport;
use crate::semantic::SemanticSearchRequest;
use crate::semantic::SemanticSearchResponse;
use crate::session::Session;
use crate::session::SessionConfig;
use crate::session::WarmReport;
use crate::session::WarmStatus;
use anyhow::Context;
use anyhow::Result;
use md5::compute as md5_compute;
use serde::Deserialize;
use serde::Serialize;
use std::fs::File;
use std::fs::OpenOptions;
use std::net::SocketAddr;
use std::net::TcpStream as StdTcpStream;
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::time::Instant;
use std::time::SystemTime;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::sync::Mutex;
use tokio::time::timeout;

#[cfg(not(unix))]
use tokio::net::TcpListener;
#[cfg(not(unix))]
use tokio::net::TcpStream;
#[cfg(unix)]
use tokio::net::UnixListener;
#[cfg(unix)]
use tokio::net::UnixStream;

const DAEMON_CONNECT_TIMEOUT: Duration = Duration::from_millis(250);
const DAEMON_IO_TIMEOUT: Duration = Duration::from_secs(1);
const DAEMON_HEAVY_IO_TIMEOUT: Duration = Duration::from_secs(180);
#[cfg(unix)]
const UNIX_SOCKET_PATH_MAX_BYTES: usize = 103;
#[cfg(unix)]
const UNIX_TEMP_ARTIFACT_ROOT: &str = "/tmp";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DaemonConfig {
    pub auto_start: bool,
    pub socket_mode: String,
    pub session: SessionConfig,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            auto_start: true,
            socket_mode: "auto".to_string(),
            session: SessionConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum TldrDaemonCommand {
    Ping,
    Warm,
    Analyze {
        key: String,
        request: AnalysisRequest,
    },
    Imports {
        request: ImportsRequest,
    },
    Importers {
        request: ImportersRequest,
    },
    Search {
        request: SearchRequest,
    },
    Diagnostics {
        request: DiagnosticsRequest,
    },
    Semantic {
        request: SemanticSearchRequest,
    },
    Notify {
        path: PathBuf,
    },
    Snapshot,
    Status,
    Shutdown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TldrDaemonResponse {
    pub status: String,
    pub message: String,
    pub analysis: Option<AnalysisResponse>,
    pub imports: Option<ImportsResponse>,
    pub importers: Option<ImportersResponse>,
    pub search: Option<SearchResponse>,
    pub diagnostics: Option<DiagnosticsResponse>,
    pub semantic: Option<SemanticSearchResponse>,
    pub snapshot: Option<crate::session::SessionSnapshot>,
    pub daemon_status: Option<TldrDaemonStatus>,
    pub reindex_report: Option<SemanticReindexReport>,
}

impl TldrDaemonResponse {
    fn ok(message: impl Into<String>) -> Self {
        Self {
            status: "ok".to_string(),
            message: message.into(),
            analysis: None,
            imports: None,
            importers: None,
            search: None,
            diagnostics: None,
            semantic: None,
            snapshot: None,
            daemon_status: None,
            reindex_report: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TldrDaemonConfigSummary {
    pub auto_start: bool,
    pub socket_mode: String,
    pub semantic_enabled: bool,
    pub semantic_auto_reindex_threshold: usize,
    pub session_dirty_file_threshold: usize,
    pub session_idle_timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StructuredFailureKind {
    DaemonUnavailable,
    DaemonStarting,
    StaleSocket,
    StalePid,
    DaemonUnhealthy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StructuredFailure {
    pub kind: StructuredFailureKind,
    pub reason: String,
    pub retryable: bool,
    pub retry_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DegradedModeKind {
    DiagnosticOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DegradedMode {
    pub kind: DegradedModeKind,
    pub fallback_path: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TldrDaemonStatus {
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
    pub structured_failure: Option<StructuredFailure>,
    pub degraded_mode: Option<DegradedMode>,
    pub semantic_reindex_pending: bool,
    pub semantic_reindex_in_progress: bool,
    pub last_query_at: Option<std::time::SystemTime>,
    pub config: TldrDaemonConfigSummary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonHealth {
    pub socket_exists: bool,
    pub pid_is_live: bool,
    pub lock_is_held: bool,
    pub launch_lock_is_held: bool,
    pub healthy: bool,
    pub stale_socket: bool,
    pub stale_pid: bool,
    pub health_reason: Option<String>,
    pub recovery_hint: Option<String>,
    pub structured_failure: Option<StructuredFailure>,
    pub degraded_mode: Option<DegradedMode>,
}

impl DaemonHealth {
    pub fn should_cleanup_artifacts(&self) -> bool {
        !self.lock_is_held && !self.launch_lock_is_held && (self.stale_socket || self.stale_pid)
    }
}

#[derive(Debug)]
pub struct TldrDaemon {
    project_root: PathBuf,
    engine: TldrEngine,
    session: Arc<Mutex<Session>>,
}

impl TldrDaemon {
    pub fn new(project_root: PathBuf) -> Self {
        Self::from_config(TldrConfig::for_project(project_root))
    }

    pub fn from_config(config: TldrConfig) -> Self {
        let project_root = config.project_root.clone();
        let engine = TldrEngine::builder(project_root.clone())
            .with_config(config.clone())
            .build();
        let session = Session::new(config.session);
        Self {
            project_root,
            engine,
            session: Arc::new(Mutex::new(session)),
        }
    }

    pub fn socket_path(&self) -> PathBuf {
        socket_path_for_project(&self.project_root)
    }

    pub async fn handle_command(&self, command: TldrDaemonCommand) -> Result<TldrDaemonResponse> {
        match command {
            TldrDaemonCommand::Ping => Ok(TldrDaemonResponse::ok("pong")),
            TldrDaemonCommand::Warm => {
                let mut session = self.session.lock().await;
                let outcome = warm_with_reindex(&mut session, &self.engine);
                let base_snapshot = session.snapshot();
                let daemon_status =
                    build_daemon_status(&self.project_root, self.engine.config(), &base_snapshot)?;
                session.record_runtime_signals(
                    daemon_status.structured_failure.clone(),
                    daemon_status.degraded_mode.is_some(),
                );
                let snapshot = session.snapshot();
                Ok(TldrDaemonResponse {
                    status: "ok".to_string(),
                    message: outcome.message,
                    analysis: None,
                    imports: None,
                    importers: None,
                    search: None,
                    diagnostics: None,
                    semantic: None,
                    snapshot: Some(snapshot),
                    daemon_status: Some(daemon_status),
                    reindex_report: outcome.reindex_report,
                })
            }
            TldrDaemonCommand::Analyze { key, request } => {
                let mut session = self.session.lock().await;
                analyze_with_session(&mut session, &self.engine, key, request)
            }
            TldrDaemonCommand::Imports { request } => {
                let response = self.engine.imports(request)?;
                Ok(TldrDaemonResponse {
                    status: "ok".to_string(),
                    message: format!("imports ready: {}", response.path),
                    analysis: None,
                    imports: Some(response),
                    importers: None,
                    search: None,
                    diagnostics: None,
                    semantic: None,
                    snapshot: None,
                    daemon_status: None,
                    reindex_report: None,
                })
            }
            TldrDaemonCommand::Importers { request } => {
                let response = self.engine.importers(request)?;
                Ok(TldrDaemonResponse {
                    status: "ok".to_string(),
                    message: format!("importers ready: {}", response.module),
                    analysis: None,
                    imports: None,
                    importers: Some(response),
                    search: None,
                    diagnostics: None,
                    semantic: None,
                    snapshot: None,
                    daemon_status: None,
                    reindex_report: None,
                })
            }
            TldrDaemonCommand::Search { request } => {
                let response = self.engine.search(request)?;
                Ok(TldrDaemonResponse {
                    status: "ok".to_string(),
                    message: format!("search returned {} matches", response.matches.len()),
                    analysis: None,
                    imports: None,
                    importers: None,
                    search: Some(response),
                    diagnostics: None,
                    semantic: None,
                    snapshot: None,
                    daemon_status: None,
                    reindex_report: None,
                })
            }
            TldrDaemonCommand::Diagnostics { request } => {
                let response = self.engine.diagnostics(request)?;
                Ok(TldrDaemonResponse {
                    status: "ok".to_string(),
                    message: response.message.clone(),
                    analysis: None,
                    imports: None,
                    importers: None,
                    search: None,
                    diagnostics: Some(response),
                    semantic: None,
                    snapshot: None,
                    daemon_status: None,
                    reindex_report: None,
                })
            }
            TldrDaemonCommand::Notify { path } => {
                let mut session = self.session.lock().await;
                let outcome = notify_session_message(
                    &mut session,
                    path,
                    self.engine.config().semantic.auto_reindex_threshold,
                );
                let snapshot = session.snapshot();
                let reindex_report = session.last_reindex_report();
                drop(session);
                if let Some(languages) = outcome.background_triggered {
                    let session = Arc::clone(&self.session);
                    let engine = self.engine.clone();
                    spawn_background_reindex(session, engine, languages);
                }
                Ok(TldrDaemonResponse {
                    status: "ok".to_string(),
                    message: outcome.message,
                    analysis: None,
                    imports: None,
                    importers: None,
                    search: None,
                    diagnostics: None,
                    semantic: None,
                    snapshot: Some(snapshot),
                    daemon_status: None,
                    reindex_report,
                })
            }
            TldrDaemonCommand::Snapshot => {
                let mut session = self.session.lock().await;
                let health = daemon_health(&self.project_root)?;
                session.record_runtime_signals(
                    health.structured_failure,
                    health.degraded_mode.is_some(),
                );
                Ok(TldrDaemonResponse {
                    analysis: None,
                    imports: None,
                    importers: None,
                    search: None,
                    diagnostics: None,
                    snapshot: Some(session.snapshot()),
                    ..TldrDaemonResponse::ok("snapshot")
                })
            }
            TldrDaemonCommand::Status => {
                let mut session = self.session.lock().await;
                let base_snapshot = session.snapshot();
                let daemon_status =
                    build_daemon_status(&self.project_root, self.engine.config(), &base_snapshot)?;
                session.record_runtime_signals(
                    daemon_status.structured_failure.clone(),
                    daemon_status.degraded_mode.is_some(),
                );
                let snapshot = session.snapshot();
                Ok(TldrDaemonResponse {
                    analysis: None,
                    imports: None,
                    importers: None,
                    search: None,
                    diagnostics: None,
                    snapshot: Some(snapshot),
                    reindex_report: session.last_reindex_attempt_report(),
                    daemon_status: Some(daemon_status),
                    ..TldrDaemonResponse::ok("status")
                })
            }
            TldrDaemonCommand::Shutdown => Ok(TldrDaemonResponse::ok("shutdown requested")),
            TldrDaemonCommand::Semantic { request } => {
                let response = self.engine.semantic_search(request)?;
                Ok(TldrDaemonResponse {
                    status: "ok".to_string(),
                    message: response.message.clone(),
                    analysis: None,
                    imports: None,
                    importers: None,
                    search: None,
                    diagnostics: None,
                    semantic: Some(response),
                    snapshot: None,
                    daemon_status: None,
                    reindex_report: None,
                })
            }
        }
    }

    pub async fn run_until_shutdown(&self) -> Result<()> {
        #[cfg(unix)]
        {
            self.run_unix().await
        }

        #[cfg(not(unix))]
        {
            self.run_tcp().await
        }
    }

    #[cfg(unix)]
    async fn run_unix(&self) -> Result<()> {
        let socket_path = self.socket_path();
        let pid_path = pid_path_for_project(&self.project_root);
        let idle_timeout = self.engine.config().session.idle_timeout;
        let last_activity = Arc::new(Mutex::new(Instant::now()));
        let active_connections = Arc::new(AtomicUsize::new(0));
        let shutdown_requested = Arc::new(AtomicBool::new(false));
        let Some(_daemon_lock) = acquire_daemon_lock(&self.project_root)? else {
            return Ok(());
        };
        ensure_daemon_artifact_parent(&socket_path)?;
        if socket_path.exists() {
            std::fs::remove_file(&socket_path)
                .with_context(|| format!("remove stale socket {}", socket_path.display()))?;
        }
        if pid_path.exists() {
            std::fs::remove_file(&pid_path)
                .with_context(|| format!("remove stale pid file {}", pid_path.display()))?;
        }
        let listener = UnixListener::bind(&socket_path)
            .with_context(|| format!("bind socket {}", socket_path.display()))?;
        write_pid_file(&pid_path)?;

        loop {
            tokio::select! {
                accept_result = listener.accept() => {
                    let (stream, _) = accept_result?;
                    let session = Arc::clone(&self.session);
                    let engine = self.engine.clone();
                    let last_activity = Arc::clone(&last_activity);
                    let active_connections = Arc::clone(&active_connections);
                    let shutdown_requested = Arc::clone(&shutdown_requested);
                    tokio::spawn(async move {
                        let _ = serve_connection(
                            stream,
                            session,
                            engine,
                            last_activity,
                            active_connections,
                            shutdown_requested,
                        ).await;
                    });
                }
                _ = tokio::time::sleep(idle_poll_interval(idle_timeout)) => {
                    if shutdown_requested.load(Ordering::SeqCst) {
                        break;
                    }
                    if should_shutdown_for_idle_timeout(
                        &self.session,
                        &last_activity,
                        &active_connections,
                        idle_timeout,
                    ).await {
                        break;
                    }
                }
                signal = tokio::signal::ctrl_c() => {
                    signal?;
                    break;
                }
            }
        }

        if socket_path.exists() {
            std::fs::remove_file(&socket_path)
                .with_context(|| format!("cleanup socket {}", socket_path.display()))?;
        }
        if pid_path.exists() {
            std::fs::remove_file(&pid_path)
                .with_context(|| format!("cleanup pid file {}", pid_path.display()))?;
        }

        Ok(())
    }

    #[cfg(not(unix))]
    async fn run_tcp(&self) -> Result<()> {
        let socket_path = self.socket_path();
        let pid_path = pid_path_for_project(&self.project_root);
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let idle_timeout = self.engine.config().session.idle_timeout;
        let last_activity = Arc::new(Mutex::new(Instant::now()));
        let active_connections = Arc::new(AtomicUsize::new(0));
        let shutdown_requested = Arc::new(AtomicBool::new(false));
        let Some(_daemon_lock) = acquire_daemon_lock(&self.project_root)? else {
            return Ok(());
        };
        cleanup_stale_socket(&socket_path);
        cleanup_stale_pid_file(&pid_path);
        write_tcp_endpoint_file(&socket_path, listener.local_addr()?)?;
        write_pid_file(&pid_path)?;
        loop {
            tokio::select! {
                accept_result = listener.accept() => {
                    let (stream, _) = accept_result?;
                    let session = Arc::clone(&self.session);
                    let engine = self.engine.clone();
                    let last_activity = Arc::clone(&last_activity);
                    let active_connections = Arc::clone(&active_connections);
                    let shutdown_requested = Arc::clone(&shutdown_requested);
                    tokio::spawn(async move {
                        let _ = serve_connection(
                            stream,
                            session,
                            engine,
                            last_activity,
                            active_connections,
                            shutdown_requested,
                        ).await;
                    });
                }
                _ = tokio::time::sleep(idle_poll_interval(idle_timeout)) => {
                    if shutdown_requested.load(Ordering::SeqCst) {
                        break;
                    }
                    if should_shutdown_for_idle_timeout(
                        &self.session,
                        &last_activity,
                        &active_connections,
                        idle_timeout,
                    ).await {
                        break;
                    }
                }
                signal = tokio::signal::ctrl_c() => {
                    signal?;
                    break;
                }
            }
        }
        cleanup_stale_socket(&socket_path);
        cleanup_stale_pid_file(&pid_path);
        Ok(())
    }
}

fn idle_poll_interval(idle_timeout: Duration) -> Duration {
    if idle_timeout.is_zero() {
        return Duration::from_millis(10);
    }
    idle_timeout.min(Duration::from_secs(1))
}

async fn should_shutdown_for_idle_timeout(
    session: &Arc<Mutex<Session>>,
    last_activity: &Arc<Mutex<Instant>>,
    active_connections: &Arc<AtomicUsize>,
    idle_timeout: Duration,
) -> bool {
    if active_connections.load(Ordering::SeqCst) > 0 {
        return false;
    }
    if session.lock().await.background_reindex_in_progress() {
        return false;
    }
    last_activity.lock().await.elapsed() >= idle_timeout
}

async fn handle_with_session(
    project_root: &Path,
    session: &Arc<Mutex<Session>>,
    engine: &TldrEngine,
    command: TldrDaemonCommand,
) -> Result<TldrDaemonResponse> {
    match command {
        TldrDaemonCommand::Ping => Ok(TldrDaemonResponse::ok("pong")),
        TldrDaemonCommand::Warm => {
            let mut guard = session.lock().await;
            let outcome = warm_with_reindex(&mut guard, engine);
            let snapshot = guard.snapshot();
            Ok(TldrDaemonResponse {
                status: "ok".to_string(),
                message: outcome.message,
                analysis: None,
                imports: None,
                importers: None,
                search: None,
                diagnostics: None,
                semantic: None,
                snapshot: Some(snapshot.clone()),
                daemon_status: Some(build_daemon_status(
                    project_root,
                    engine.config(),
                    &snapshot,
                )?),
                reindex_report: outcome.reindex_report,
            })
        }
        TldrDaemonCommand::Analyze { key, request } => {
            let mut guard = session.lock().await;
            analyze_with_session(&mut guard, engine, key, request)
        }
        TldrDaemonCommand::Imports { request } => {
            let response = engine.imports(request)?;
            Ok(TldrDaemonResponse {
                status: "ok".to_string(),
                message: format!("imports ready: {}", response.path),
                analysis: None,
                imports: Some(response),
                importers: None,
                search: None,
                diagnostics: None,
                semantic: None,
                snapshot: None,
                daemon_status: None,
                reindex_report: None,
            })
        }
        TldrDaemonCommand::Importers { request } => {
            let response = engine.importers(request)?;
            Ok(TldrDaemonResponse {
                status: "ok".to_string(),
                message: format!("importers ready: {}", response.module),
                analysis: None,
                imports: None,
                importers: Some(response),
                search: None,
                diagnostics: None,
                semantic: None,
                snapshot: None,
                daemon_status: None,
                reindex_report: None,
            })
        }
        TldrDaemonCommand::Search { request } => {
            let response = engine.search(request)?;
            Ok(TldrDaemonResponse {
                status: "ok".to_string(),
                message: format!("search returned {} matches", response.matches.len()),
                analysis: None,
                imports: None,
                importers: None,
                search: Some(response),
                diagnostics: None,
                semantic: None,
                snapshot: None,
                daemon_status: None,
                reindex_report: None,
            })
        }
        TldrDaemonCommand::Diagnostics { request } => {
            let response = engine.diagnostics(request)?;
            Ok(TldrDaemonResponse {
                status: "ok".to_string(),
                message: response.message.clone(),
                analysis: None,
                imports: None,
                importers: None,
                search: None,
                diagnostics: Some(response),
                semantic: None,
                snapshot: None,
                daemon_status: None,
                reindex_report: None,
            })
        }
        TldrDaemonCommand::Notify { path } => {
            let mut guard = session.lock().await;
            let outcome = notify_session_message(
                &mut guard,
                path,
                engine.config().semantic.auto_reindex_threshold,
            );
            let snapshot = guard.snapshot();
            let reindex_report = guard.last_reindex_report();
            drop(guard);
            if let Some(languages) = outcome.background_triggered {
                let session = Arc::clone(session);
                let engine = engine.clone();
                spawn_background_reindex(session, engine, languages);
            }
            Ok(TldrDaemonResponse {
                status: "ok".to_string(),
                message: outcome.message,
                analysis: None,
                imports: None,
                importers: None,
                search: None,
                diagnostics: None,
                semantic: None,
                snapshot: Some(snapshot),
                daemon_status: None,
                reindex_report,
            })
        }
        TldrDaemonCommand::Snapshot => {
            let guard = session.lock().await;
            Ok(TldrDaemonResponse {
                analysis: None,
                imports: None,
                importers: None,
                search: None,
                diagnostics: None,
                snapshot: Some(guard.snapshot()),
                ..TldrDaemonResponse::ok("snapshot")
            })
        }
        TldrDaemonCommand::Status => {
            let guard = session.lock().await;
            let snapshot = guard.snapshot();
            Ok(TldrDaemonResponse {
                analysis: None,
                imports: None,
                importers: None,
                search: None,
                diagnostics: None,
                snapshot: Some(snapshot.clone()),
                reindex_report: guard.last_reindex_attempt_report(),
                daemon_status: Some(build_daemon_status(
                    project_root,
                    engine.config(),
                    &snapshot,
                )?),
                ..TldrDaemonResponse::ok("status")
            })
        }
        TldrDaemonCommand::Shutdown => Ok(TldrDaemonResponse::ok("shutdown requested")),
        TldrDaemonCommand::Semantic { request } => {
            let response = engine.semantic_search(request)?;
            Ok(TldrDaemonResponse {
                status: "ok".to_string(),
                message: response.message.clone(),
                analysis: None,
                imports: None,
                importers: None,
                search: None,
                diagnostics: None,
                semantic: Some(response),
                snapshot: None,
                daemon_status: None,
                reindex_report: None,
            })
        }
    }
}

fn analyze_with_session(
    session: &mut Session,
    engine: &TldrEngine,
    key: String,
    request: AnalysisRequest,
) -> Result<TldrDaemonResponse> {
    if !session.reindex_pending()
        && let Some(cached) = session.cached_analysis(&key).cloned()
    {
        return Ok(TldrDaemonResponse {
            status: "ok".to_string(),
            message: "cache hit".to_string(),
            analysis: Some(cached),
            imports: None,
            importers: None,
            search: None,
            diagnostics: None,
            semantic: None,
            snapshot: Some(session.snapshot()),
            daemon_status: None,
            reindex_report: session.last_reindex_report(),
        });
    }

    let analysis = engine.analyze(request)?;
    let message = if session.reindex_pending() {
        "computed (cache bypassed: reindex pending)"
    } else {
        session.store_analysis(key, analysis.clone());
        "computed"
    };
    Ok(TldrDaemonResponse {
        status: "ok".to_string(),
        message: message.to_string(),
        analysis: Some(analysis),
        imports: None,
        importers: None,
        search: None,
        diagnostics: None,
        semantic: None,
        snapshot: Some(session.snapshot()),
        daemon_status: None,
        reindex_report: session.last_reindex_report(),
    })
}

struct NotifyOutcome {
    message: String,
    background_triggered: Option<Vec<SupportedLanguage>>,
}

struct WarmOutcome {
    message: String,
    reindex_report: Option<SemanticReindexReport>,
}

fn notify_session_message(
    session: &mut Session,
    path: PathBuf,
    auto_reindex_threshold: usize,
) -> NotifyOutcome {
    let dirty_state = session.mark_dirty(path);
    let background_triggered = if dirty_state.dirty_files >= auto_reindex_threshold.max(1) {
        session.claim_background_reindex()
    } else {
        None
    };
    let mut message = if dirty_state.cache_invalidated {
        format!(
            "dirty threshold reached; invalidated {} cached analyses; reindex pending",
            dirty_state.invalidated_entries
        )
    } else if dirty_state.reindex_pending {
        format!(
            "marked dirty ({}); phase-2 reindex pending",
            dirty_state.dirty_files
        )
    } else {
        format!("marked dirty ({})", dirty_state.dirty_files)
    };
    if background_triggered.is_some() {
        message.push_str("; background reindex scheduled");
    }
    NotifyOutcome {
        message,
        background_triggered,
    }
}

fn warm_with_reindex(session: &mut Session, engine: &TldrEngine) -> WarmOutcome {
    let started_at = SystemTime::now();
    if session.background_reindex_in_progress() {
        let message = "background semantic reindex already in progress".to_string();
        session.record_warm(WarmReport {
            status: WarmStatus::Busy,
            languages: Vec::new(),
            started_at,
            finished_at: SystemTime::now(),
            message: message.clone(),
        });
        return WarmOutcome {
            message,
            reindex_report: session.last_reindex_attempt_report(),
        };
    }

    if session.reindex_pending() {
        let dirty_languages = session.dirty_languages();
        if dirty_languages.is_empty() {
            let report = if session.has_unmapped_dirty_paths() {
                SemanticReindexReport::skipped(
                    Vec::new(),
                    "dirty paths did not map to supported source languages",
                    engine.config().semantic.embedding_enabled,
                    engine.config().semantic.embedding.dimensions,
                )
            } else {
                SemanticReindexReport::skipped(
                    Vec::new(),
                    "no semantic sources were marked dirty",
                    engine.config().semantic.embedding_enabled,
                    engine.config().semantic.embedding.dimensions,
                )
            };
            session.clear_dirty_files();
            session.record_reindex_attempt(report.clone());
            session.record_warm(WarmReport {
                status: WarmStatus::Skipped,
                languages: report.languages.clone(),
                started_at,
                finished_at: SystemTime::now(),
                message: report.message.clone(),
            });
            return WarmOutcome {
                message: report.message.clone(),
                reindex_report: Some(report),
            };
        }
        match engine.semantic_reindex_languages(&dirty_languages) {
            Ok(report) => {
                session.record_reindex_attempt(report.clone());
                if report.is_completed() {
                    session.complete_reindex(report.clone());
                }
                let status = match report.status {
                    crate::semantic::SemanticReindexStatus::Completed => WarmStatus::Reindexed,
                    crate::semantic::SemanticReindexStatus::Failed => WarmStatus::Failed,
                    crate::semantic::SemanticReindexStatus::Skipped => WarmStatus::Skipped,
                };
                session.record_warm(WarmReport {
                    status,
                    languages: report.languages.clone(),
                    started_at,
                    finished_at: SystemTime::now(),
                    message: report.message.clone(),
                });
                WarmOutcome {
                    message: report.message.clone(),
                    reindex_report: Some(report),
                }
            }
            Err(err) => {
                let failure = SemanticReindexReport::failed(
                    dirty_languages,
                    err.to_string(),
                    engine.config().semantic.embedding_enabled,
                    engine.config().semantic.embedding.dimensions,
                );
                session.record_reindex_attempt(failure.clone());
                session.record_warm(WarmReport {
                    status: WarmStatus::Failed,
                    languages: failure.languages.clone(),
                    started_at,
                    finished_at: SystemTime::now(),
                    message: failure.message.clone(),
                });
                WarmOutcome {
                    message: failure.message.clone(),
                    reindex_report: Some(failure),
                }
            }
        }
    } else {
        let project_languages = match engine.project_languages() {
            Ok(languages) => languages,
            Err(err) => {
                let message = format!("warm failed: {err}");
                session.record_warm(WarmReport {
                    status: WarmStatus::Failed,
                    languages: Vec::new(),
                    started_at,
                    finished_at: SystemTime::now(),
                    message: message.clone(),
                });
                return WarmOutcome {
                    message,
                    reindex_report: session.last_reindex_attempt_report(),
                };
            }
        };
        if project_languages.is_empty() {
            let message = "warm skipped: no supported source languages found".to_string();
            session.record_warm(WarmReport {
                status: WarmStatus::Skipped,
                languages: Vec::new(),
                started_at,
                finished_at: SystemTime::now(),
                message: message.clone(),
            });
            return WarmOutcome {
                message,
                reindex_report: session.last_reindex_attempt_report(),
            };
        }
        match engine.warm_language_indexes(&project_languages) {
            Ok(warmed_languages) => {
                let message = format!(
                    "warm loaded {} language indexes into daemon cache",
                    warmed_languages.len()
                );
                session.record_warm(WarmReport {
                    status: WarmStatus::Loaded,
                    languages: warmed_languages,
                    started_at,
                    finished_at: SystemTime::now(),
                    message: message.clone(),
                });
                WarmOutcome {
                    message,
                    reindex_report: session.last_reindex_attempt_report(),
                }
            }
            Err(err) => {
                let message = format!("warm failed: {err}");
                session.record_warm(WarmReport {
                    status: WarmStatus::Failed,
                    languages: project_languages,
                    started_at,
                    finished_at: SystemTime::now(),
                    message: message.clone(),
                });
                WarmOutcome {
                    message,
                    reindex_report: session.last_reindex_attempt_report(),
                }
            }
        }
    }
}

fn spawn_background_reindex(
    session: Arc<Mutex<Session>>,
    engine: TldrEngine,
    languages: Vec<SupportedLanguage>,
) {
    let embedding_enabled = engine.config().semantic.embedding_enabled;
    let embedding_dimensions = engine.config().semantic.embedding.dimensions;
    tokio::spawn(async move {
        let result = engine.semantic_reindex_languages(&languages);
        let mut guard = session.lock().await;
        match result {
            Ok(report) => {
                guard.record_reindex_attempt(report.clone());
                if report.is_completed() {
                    guard.complete_reindex(report);
                }
            }
            Err(err) => {
                let failure = SemanticReindexReport::failed(
                    languages.clone(),
                    err.to_string(),
                    embedding_enabled,
                    embedding_dimensions,
                );
                guard.record_reindex_attempt(failure);
            }
        }
    });
}

async fn serve_connection<T>(
    stream: T,
    session: Arc<Mutex<Session>>,
    engine: TldrEngine,
    last_activity: Arc<Mutex<Instant>>,
    active_connections: Arc<AtomicUsize>,
    shutdown_requested: Arc<AtomicBool>,
) -> Result<()>
where
    T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let _connection_guard = ActiveConnectionGuard::new(active_connections);
    let (reader, mut writer) = tokio::io::split(stream);
    let mut lines = BufReader::new(reader).lines();

    while let Some(line) = lines.next_line().await? {
        let command: TldrDaemonCommand = serde_json::from_str(&line)?;
        let should_shutdown = matches!(command, TldrDaemonCommand::Shutdown);
        let response = handle_with_session(
            engine.config().project_root.as_path(),
            &session,
            &engine,
            command,
        )
        .await?;
        writer
            .write_all(format!("{}\n", serde_json::to_string(&response)?).as_bytes())
            .await?;
        *last_activity.lock().await = Instant::now();
        if should_shutdown {
            shutdown_requested.store(true, Ordering::SeqCst);
            break;
        }
    }

    Ok(())
}

struct ActiveConnectionGuard {
    active_connections: Arc<AtomicUsize>,
}

impl ActiveConnectionGuard {
    fn new(active_connections: Arc<AtomicUsize>) -> Self {
        active_connections.fetch_add(1, Ordering::SeqCst);
        Self { active_connections }
    }
}

impl Drop for ActiveConnectionGuard {
    fn drop(&mut self) {
        self.active_connections.fetch_sub(1, Ordering::SeqCst);
    }
}

pub fn socket_path_for_project(project_root: &Path) -> PathBuf {
    let project_hash = daemon_project_hash(project_root);
    daemon_artifact_dir_for_project_hash(project_root, &project_hash)
        .join(daemon_socket_file_name(&project_hash))
}

pub fn pid_path_for_project(project_root: &Path) -> PathBuf {
    let project_hash = daemon_project_hash(project_root);
    daemon_artifact_dir_for_project_hash(project_root, &project_hash)
        .join(daemon_pid_file_name(&project_hash))
}

pub fn lock_path_for_project(project_root: &Path) -> PathBuf {
    let project_hash = daemon_project_hash(project_root);
    daemon_artifact_scope_dir(project_root, &project_hash)
        .join(daemon_lock_file_name(&project_hash))
}

pub fn launch_lock_path_for_project(project_root: &Path) -> PathBuf {
    let project_hash = daemon_project_hash(project_root);
    daemon_artifact_scope_dir(project_root, &project_hash)
        .join(daemon_launch_lock_file_name(&project_hash))
}

pub(crate) fn daemon_project_hash(project_root: &Path) -> String {
    let hash = format!(
        "{:x}",
        md5_compute(project_root.to_string_lossy().as_bytes())
    );
    hash[..8].to_string()
}

pub(crate) fn daemon_artifact_dir_for_project(project_root: &Path) -> PathBuf {
    let project_hash = daemon_project_hash(project_root);
    daemon_artifact_dir_for_project_hash(project_root, &project_hash)
}

pub(crate) fn temp_artifact_dir_for_project(project_root: &Path) -> PathBuf {
    daemon_temp_artifact_scope_dir().join(daemon_project_hash(project_root))
}

fn daemon_socket_file_name(project_hash: &str) -> String {
    format!("codex-native-tldr-{project_hash}.sock")
}

fn daemon_pid_file_name(project_hash: &str) -> String {
    format!("codex-native-tldr-{project_hash}.pid")
}

fn daemon_lock_file_name(project_hash: &str) -> String {
    format!("codex-native-tldr-{project_hash}.lock")
}

fn daemon_launch_lock_file_name(project_hash: &str) -> String {
    format!("codex-native-tldr-{project_hash}.launch.lock")
}

fn daemon_artifact_dir_for_project_hash(project_root: &Path, project_hash: &str) -> PathBuf {
    daemon_artifact_scope_dir(project_root, project_hash).join(project_hash)
}

fn daemon_artifact_scope_dir(_project_root: &Path, project_hash: &str) -> PathBuf {
    #[cfg(unix)]
    {
        let uid = unsafe { libc::geteuid() };
        let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from);
        daemon_artifact_scope_dir_for_project_hash(runtime_dir.as_deref(), uid, project_hash)
    }

    #[cfg(not(unix))]
    {
        daemon_artifact_scope_dir_for_runtime_dir(None)
    }
}

fn daemon_temp_artifact_scope_dir() -> PathBuf {
    #[cfg(unix)]
    {
        let uid = unsafe { libc::geteuid() };
        PathBuf::from(UNIX_TEMP_ARTIFACT_ROOT)
            .join("codex-native-tldr")
            .join(uid.to_string())
    }

    #[cfg(not(unix))]
    {
        std::env::temp_dir().join("codex-native-tldr")
    }
}

#[cfg(unix)]
fn daemon_artifact_scope_dir_for_project_hash(
    runtime_dir: Option<&Path>,
    uid: libc::uid_t,
    project_hash: &str,
) -> PathBuf {
    let preferred_scope_dir = daemon_artifact_scope_dir_for_runtime_dir(runtime_dir, uid);
    if unix_socket_path_fits(&preferred_scope_dir, project_hash) {
        preferred_scope_dir
    } else {
        daemon_temp_artifact_scope_dir()
    }
}

#[cfg(unix)]
fn daemon_artifact_scope_dir_for_runtime_dir(
    runtime_dir: Option<&Path>,
    uid: libc::uid_t,
) -> PathBuf {
    if let Some(runtime_dir) = runtime_dir
        && runtime_dir.is_absolute()
    {
        return runtime_dir.join("codex-native-tldr").join(uid.to_string());
    }
    daemon_temp_artifact_scope_dir()
}

#[cfg(not(unix))]
fn daemon_artifact_scope_dir_for_runtime_dir(_runtime_dir: Option<&Path>) -> PathBuf {
    daemon_temp_artifact_scope_dir()
}

#[cfg(unix)]
fn unix_socket_path_fits(scope_dir: &Path, project_hash: &str) -> bool {
    let socket_path = scope_dir
        .join(project_hash)
        .join(daemon_socket_file_name(project_hash));
    socket_path.as_os_str().as_bytes().len() < UNIX_SOCKET_PATH_MAX_BYTES
}

fn ensure_daemon_artifact_parent(path: &Path) -> Result<()> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    std::fs::create_dir_all(parent)
        .with_context(|| format!("create daemon artifact dir {}", parent.display()))
}

fn write_pid_file(pid_path: &Path) -> Result<()> {
    ensure_daemon_artifact_parent(pid_path)?;
    std::fs::write(pid_path, std::process::id().to_string())
        .with_context(|| format!("write pid file {}", pid_path.display()))
}

fn write_tcp_endpoint_file(endpoint_path: &Path, address: SocketAddr) -> Result<()> {
    ensure_daemon_artifact_parent(endpoint_path)?;
    std::fs::write(endpoint_path, address.to_string())
        .with_context(|| format!("write daemon endpoint {}", endpoint_path.display()))
}

fn read_tcp_endpoint(endpoint_path: &Path) -> Option<SocketAddr> {
    std::fs::read_to_string(endpoint_path)
        .ok()?
        .trim()
        .parse::<SocketAddr>()
        .ok()
}

fn acquire_daemon_lock(project_root: &Path) -> Result<Option<File>> {
    try_open_daemon_lock(project_root)
}

pub fn daemon_lock_is_held(project_root: &Path) -> Result<bool> {
    Ok(try_open_daemon_lock(project_root)?.is_none())
}

pub fn launch_lock_is_held(project_root: &Path) -> Result<bool> {
    Ok(try_open_launch_lock(project_root)?.is_none())
}

pub fn daemon_health(project_root: &Path) -> Result<DaemonHealth> {
    let socket_path = socket_path_for_project(project_root);
    let pid_path = pid_path_for_project(project_root);
    let socket_exists = socket_path.exists();
    #[cfg(unix)]
    let pid_is_live = read_live_pid(&pid_path).unwrap_or(false);
    #[cfg(not(unix))]
    let pid_is_live = pid_path.exists() && tcp_endpoint_is_alive(&socket_path);
    let lock_is_held = daemon_lock_is_held(project_root)?;
    let launch_lock_is_held = launch_lock_is_held(project_root)?;
    let healthy = socket_exists && pid_is_live;
    let (structured_failure, degraded_mode) = if healthy {
        (None, None)
    } else {
        health_diagnostics(
            socket_exists,
            pid_is_live,
            lock_is_held,
            launch_lock_is_held,
        )
    };
    let health_reason = structured_failure
        .as_ref()
        .map(|failure| failure.reason.clone());
    let recovery_hint = structured_failure
        .as_ref()
        .and_then(|failure| failure.retry_hint.clone());
    Ok(DaemonHealth {
        socket_exists,
        pid_is_live,
        lock_is_held,
        launch_lock_is_held,
        healthy,
        stale_socket: socket_exists && !pid_is_live,
        stale_pid: !socket_exists && pid_is_live,
        health_reason,
        recovery_hint,
        structured_failure,
        degraded_mode,
    })
}

fn health_diagnostics(
    socket_exists: bool,
    pid_is_live: bool,
    lock_is_held: bool,
    launch_lock_is_held: bool,
) -> (Option<StructuredFailure>, Option<DegradedMode>) {
    if socket_exists && !pid_is_live {
        return (
            Some(StructuredFailure {
                kind: StructuredFailureKind::StaleSocket,
                reason: "stale socket without live daemon".to_string(),
                retryable: true,
                retry_hint: Some(
                    "remove stale socket/pid files and restart the daemon".to_string(),
                ),
            }),
            Some(DegradedMode {
                kind: DegradedModeKind::DiagnosticOnly,
                fallback_path: "status_only".to_string(),
                reason: Some("daemon metadata is stale".to_string()),
            }),
        );
    }
    if !socket_exists && pid_is_live {
        return (
            Some(StructuredFailure {
                kind: StructuredFailureKind::StalePid,
                reason: "pid file exists but socket is missing".to_string(),
                retryable: true,
                retry_hint: Some(
                    "cleanup pid/socket files before restarting the daemon".to_string(),
                ),
            }),
            Some(DegradedMode {
                kind: DegradedModeKind::DiagnosticOnly,
                fallback_path: "status_only".to_string(),
                reason: Some("daemon metadata is stale".to_string()),
            }),
        );
    }
    if launch_lock_is_held {
        return (
            Some(StructuredFailure {
                kind: StructuredFailureKind::DaemonStarting,
                reason: "daemon launch lock held; another process is starting it".to_string(),
                retryable: true,
                retry_hint: Some(
                    "wait for the launcher to finish before cleaning up artifacts".to_string(),
                ),
            }),
            Some(DegradedMode {
                kind: DegradedModeKind::DiagnosticOnly,
                fallback_path: "status_only".to_string(),
                reason: Some("daemon startup is in progress".to_string()),
            }),
        );
    }
    if lock_is_held {
        return (
            Some(StructuredFailure {
                kind: StructuredFailureKind::DaemonStarting,
                reason: "daemon lock held; another process may be starting it".to_string(),
                retryable: true,
                retry_hint: Some(
                    "wait for the existing daemon or release the lock manually".to_string(),
                ),
            }),
            Some(DegradedMode {
                kind: DegradedModeKind::DiagnosticOnly,
                fallback_path: "status_only".to_string(),
                reason: Some("daemon startup is in progress".to_string()),
            }),
        );
    }
    (
        Some(StructuredFailure {
            kind: StructuredFailureKind::DaemonUnavailable,
            reason: "daemon unavailable (missing socket and pid)".to_string(),
            retryable: true,
            retry_hint: Some(
                "run `codex ztldr ...` to auto-start the internal daemon or inspect logs"
                    .to_string(),
            ),
        }),
        Some(DegradedMode {
            kind: DegradedModeKind::DiagnosticOnly,
            fallback_path: "status_only".to_string(),
            reason: Some("daemon-only actions cannot proceed without a live daemon".to_string()),
        }),
    )
}

pub fn read_live_pid(pid_path: &Path) -> Option<bool> {
    let pid = std::fs::read_to_string(pid_path)
        .ok()?
        .trim()
        .parse::<i32>()
        .ok()?;
    Some(pid_is_alive(pid))
}

#[cfg(not(unix))]
fn pid_is_alive(pid: i32) -> bool {
    let _ = pid;
    false
}

fn tcp_endpoint_is_alive(endpoint_path: &Path) -> bool {
    let Some(address) = read_tcp_endpoint(endpoint_path) else {
        return false;
    };
    StdTcpStream::connect_timeout(&address, DAEMON_CONNECT_TIMEOUT).is_ok()
}

#[cfg(unix)]
fn pid_is_alive(pid: i32) -> bool {
    if pid <= 0 {
        return false;
    }
    let result = unsafe { libc::kill(pid as libc::pid_t, 0) };
    if result == 0 {
        true
    } else {
        matches!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::EPERM)
        )
    }
}

fn build_daemon_status(
    project_root: &Path,
    config: &TldrConfig,
    snapshot: &crate::session::SessionSnapshot,
) -> Result<TldrDaemonStatus> {
    let socket_path = socket_path_for_project(project_root);
    let pid_path = pid_path_for_project(project_root);
    let health = daemon_health(project_root)?;

    let background_reindex_in_progress = snapshot.background_reindex_in_progress;
    let semantic_reindex_pending = snapshot.reindex_pending || background_reindex_in_progress;
    Ok(TldrDaemonStatus {
        project_root: project_root.to_path_buf(),
        socket_path,
        pid_path,
        lock_path: lock_path_for_project(project_root),
        socket_exists: health.socket_exists,
        pid_is_live: health.pid_is_live,
        lock_is_held: health.lock_is_held,
        healthy: health.healthy,
        stale_socket: health.stale_socket,
        stale_pid: health.stale_pid,
        health_reason: health.health_reason.clone(),
        recovery_hint: health.recovery_hint,
        structured_failure: health.structured_failure,
        degraded_mode: health.degraded_mode,
        semantic_reindex_pending,
        semantic_reindex_in_progress: background_reindex_in_progress,
        last_query_at: snapshot.last_query_at,
        config: config.daemon_config_summary(),
    })
}

fn try_open_daemon_lock(project_root: &Path) -> Result<Option<File>> {
    let lock_path = lock_path_for_project(project_root);
    ensure_daemon_artifact_parent(&lock_path)?;
    let lock_file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .with_context(|| format!("open daemon lock {}", lock_path.display()))?;

    match lock_file.try_lock() {
        Ok(()) => Ok(Some(lock_file)),
        Err(std::fs::TryLockError::WouldBlock) => Ok(None),
        Err(err) => Err(err.into()),
    }
}

fn try_open_launch_lock(project_root: &Path) -> Result<Option<File>> {
    let lock_path = launch_lock_path_for_project(project_root);
    ensure_daemon_artifact_parent(&lock_path)?;
    let lock_file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .with_context(|| format!("open daemon launch lock {}", lock_path.display()))?;

    match lock_file.try_lock() {
        Ok(()) => Ok(Some(lock_file)),
        Err(std::fs::TryLockError::WouldBlock) => Ok(None),
        Err(err) => Err(err.into()),
    }
}

#[cfg(unix)]
pub async fn query_daemon(
    project_root: &Path,
    command: &TldrDaemonCommand,
) -> Result<Option<TldrDaemonResponse>> {
    query_daemon_with_timeout(
        project_root,
        command,
        DAEMON_CONNECT_TIMEOUT,
        io_timeout_for_command(command),
    )
    .await
}

fn io_timeout_for_command(command: &TldrDaemonCommand) -> Duration {
    match command {
        TldrDaemonCommand::Ping
        | TldrDaemonCommand::Notify { .. }
        | TldrDaemonCommand::Snapshot
        | TldrDaemonCommand::Status
        | TldrDaemonCommand::Shutdown => DAEMON_IO_TIMEOUT,
        TldrDaemonCommand::Warm
        | TldrDaemonCommand::Analyze { .. }
        | TldrDaemonCommand::Imports { .. }
        | TldrDaemonCommand::Importers { .. }
        | TldrDaemonCommand::Search { .. }
        | TldrDaemonCommand::Diagnostics { .. }
        | TldrDaemonCommand::Semantic { .. } => DAEMON_HEAVY_IO_TIMEOUT,
    }
}

#[cfg(unix)]
async fn query_daemon_with_timeout(
    project_root: &Path,
    command: &TldrDaemonCommand,
    connect_timeout: Duration,
    io_timeout: Duration,
) -> Result<Option<TldrDaemonResponse>> {
    let socket_path = socket_path_for_project(project_root);
    let pid_path = pid_path_for_project(project_root);
    if !socket_path.exists() {
        return Ok(None);
    }

    let stream = match timeout(connect_timeout, UnixStream::connect(&socket_path)).await {
        Ok(Ok(stream)) => stream,
        Ok(Err(err)) if daemon_unavailable(&err) => {
            maybe_cleanup_unavailable_daemon(project_root, &socket_path, &pid_path);
            return Ok(None);
        }
        Ok(Err(err)) => {
            return Err(err).with_context(|| format!("connect socket {}", socket_path.display()));
        }
        Err(_) => anyhow::bail!("timed out connecting to daemon {}", socket_path.display()),
    };

    let (reader, mut writer) = tokio::io::split(stream);
    timeout(
        io_timeout,
        writer.write_all(format!("{}\n", serde_json::to_string(command)?).as_bytes()),
    )
    .await
    .with_context(|| format!("write timeout for {}", socket_path.display()))?
    .with_context(|| format!("write daemon command to {}", socket_path.display()))?;

    let mut lines = BufReader::new(reader).lines();
    let Some(line) = timeout(io_timeout, lines.next_line())
        .await
        .with_context(|| format!("read timeout for {}", socket_path.display()))?
        .with_context(|| format!("read daemon response from {}", socket_path.display()))?
    else {
        maybe_cleanup_unavailable_daemon(project_root, &socket_path, &pid_path);
        return Ok(None);
    };

    let response = serde_json::from_str(&line)
        .with_context(|| format!("decode daemon response from {}", socket_path.display()))?;
    Ok(Some(response))
}

fn maybe_cleanup_unavailable_daemon(project_root: &Path, socket_path: &Path, pid_path: &Path) {
    if daemon_health(project_root)
        .map(|health| health.should_cleanup_artifacts())
        .unwrap_or(false)
    {
        cleanup_stale_socket(socket_path);
        cleanup_stale_pid_file(pid_path);
    }
}

fn cleanup_stale_socket(socket_path: &Path) {
    if let Err(err) = std::fs::remove_file(socket_path)
        && err.kind() != std::io::ErrorKind::NotFound
    {
        let _ = err;
    }
}

fn cleanup_stale_pid_file(pid_path: &Path) {
    if let Err(err) = std::fs::remove_file(pid_path)
        && err.kind() != std::io::ErrorKind::NotFound
    {
        let _ = err;
    }
}

fn daemon_unavailable(err: &std::io::Error) -> bool {
    matches!(
        err.kind(),
        std::io::ErrorKind::NotFound
            | std::io::ErrorKind::ConnectionRefused
            | std::io::ErrorKind::ConnectionReset
            | std::io::ErrorKind::BrokenPipe
    )
}

#[cfg(not(unix))]
pub async fn query_daemon(
    project_root: &Path,
    command: &TldrDaemonCommand,
) -> Result<Option<TldrDaemonResponse>> {
    let socket_path = socket_path_for_project(project_root);
    let pid_path = pid_path_for_project(project_root);
    let Some(address) = read_tcp_endpoint(&socket_path) else {
        return Ok(None);
    };

    let stream = match timeout(DAEMON_CONNECT_TIMEOUT, TcpStream::connect(address)).await {
        Ok(Ok(stream)) => stream,
        Ok(Err(err)) if daemon_unavailable(&err) => {
            maybe_cleanup_unavailable_daemon(project_root, &socket_path, &pid_path);
            return Ok(None);
        }
        Ok(Err(err)) => {
            return Err(err).with_context(|| format!("connect daemon {}", socket_path.display()));
        }
        Err(_) => anyhow::bail!("timed out connecting to daemon {}", socket_path.display()),
    };

    let (reader, mut writer) = tokio::io::split(stream);
    timeout(
        io_timeout_for_command(command),
        writer.write_all(format!("{}\n", serde_json::to_string(command)?).as_bytes()),
    )
    .await
    .with_context(|| format!("write timeout for {}", socket_path.display()))?
    .with_context(|| format!("write daemon command to {}", socket_path.display()))?;

    let mut lines = BufReader::new(reader).lines();
    let Some(line) = timeout(io_timeout_for_command(command), lines.next_line())
        .await
        .with_context(|| format!("read timeout for {}", socket_path.display()))?
        .with_context(|| format!("read daemon response from {}", socket_path.display()))?
    else {
        maybe_cleanup_unavailable_daemon(project_root, &socket_path, &pid_path);
        return Ok(None);
    };

    let response = serde_json::from_str(&line)
        .with_context(|| format!("decode daemon response from {}", socket_path.display()))?;
    Ok(Some(response))
}

#[cfg(test)]
mod tests {
    use super::DAEMON_HEAVY_IO_TIMEOUT;
    use super::DAEMON_IO_TIMEOUT;
    use super::TldrDaemon;
    use super::TldrDaemonCommand;
    use super::TldrDaemonResponse;
    #[cfg(unix)]
    use super::daemon_artifact_scope_dir_for_project_hash;
    use super::daemon_artifact_scope_dir_for_runtime_dir;
    use super::daemon_health;
    use super::daemon_lock_is_held;
    use super::daemon_project_hash;
    #[cfg(unix)]
    use super::daemon_temp_artifact_scope_dir;
    use super::ensure_daemon_artifact_parent;
    use super::io_timeout_for_command;
    use super::launch_lock_is_held;
    use super::launch_lock_path_for_project;
    use super::lock_path_for_project;
    use super::pid_is_alive;
    use super::query_daemon;
    use super::read_tcp_endpoint;
    use super::tcp_endpoint_is_alive;
    #[cfg(unix)]
    use super::unix_socket_path_fits;
    use super::write_pid_file;
    use super::write_tcp_endpoint_file;
    use pretty_assertions::assert_eq;
    use serial_test::serial;
    use std::fs::OpenOptions;
    use std::net::SocketAddr;
    use std::path::PathBuf;
    use std::time::Duration;
    use tempfile::tempdir;
    use tokio::io::AsyncBufReadExt;
    use tokio::io::AsyncWriteExt;
    use tokio::io::BufReader;
    #[cfg(unix)]
    use tokio::time::sleep;

    use super::pid_path_for_project;
    #[cfg(unix)]
    use super::query_daemon_with_timeout;
    #[cfg(unix)]
    use super::socket_path_for_project;
    use crate::semantic::SemanticSearchRequest;
    #[cfg(unix)]
    use tokio::net::UnixListener;

    fn create_artifact_parent(path: &std::path::Path) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("artifact parent should be created");
        }
    }

    #[tokio::test]
    async fn query_daemon_returns_none_when_socket_is_missing() {
        let tempdir = tempdir().expect("tempdir should exist");
        let project_root = tempdir.path().join("missing-daemon-project");
        std::fs::create_dir(&project_root).expect("project root should be created");

        let response = query_daemon(&project_root, &TldrDaemonCommand::Ping)
            .await
            .expect("missing socket should not error");

        assert_eq!(response, None);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn query_daemon_round_trips_response_over_socket() {
        let tempdir = tempdir().expect("tempdir should exist");
        let project_root = tempdir.path().join("daemon-project");
        std::fs::create_dir(&project_root).expect("project root should be created");
        let socket_path = socket_path_for_project(&project_root);
        create_artifact_parent(&socket_path);
        if socket_path.exists() {
            std::fs::remove_file(&socket_path).expect("stale socket should be removed");
        }

        let listener = UnixListener::bind(&socket_path).expect("socket should bind");
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("client should connect");
            let (reader, mut writer) = tokio::io::split(stream);
            let mut lines = BufReader::new(reader).lines();
            let line = lines
                .next_line()
                .await
                .expect("request should be readable")
                .expect("request line should exist");
            let command: TldrDaemonCommand =
                serde_json::from_str(&line).expect("request should decode");
            assert_eq!(command, TldrDaemonCommand::Ping);

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
            writer
                .write_all(
                    format!("{}\n", serde_json::to_string(&response).expect("encode")).as_bytes(),
                )
                .await
                .expect("response should write");
        });

        let response = query_daemon(&project_root, &TldrDaemonCommand::Ping)
            .await
            .expect("daemon query should succeed")
            .expect("daemon should respond");
        server.await.expect("server should complete");

        assert_eq!(
            response,
            TldrDaemonResponse {
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
            }
        );

        std::fs::remove_file(&socket_path).expect("socket should be cleaned up");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn query_daemon_removes_stale_socket_when_daemon_is_unavailable() {
        let tempdir = tempdir().expect("tempdir should exist");
        let project_root = tempdir.path().join("stale-daemon-project");
        std::fs::create_dir(&project_root).expect("project root should be created");
        let socket_path = socket_path_for_project(&project_root);
        create_artifact_parent(&socket_path);
        if socket_path.exists() {
            std::fs::remove_file(&socket_path).expect("stale socket should be removed");
        }

        let listener = UnixListener::bind(&socket_path).expect("socket should bind");
        drop(listener);
        assert!(
            socket_path.exists(),
            "socket path should remain after listener drop"
        );

        let response = query_daemon(&project_root, &TldrDaemonCommand::Ping)
            .await
            .expect("stale socket should not error");

        assert_eq!(response, None);
        assert!(!socket_path.exists(), "stale socket should be removed");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn query_daemon_removes_stale_pid_alongside_unavailable_socket() {
        let tempdir = tempdir().expect("tempdir should exist");
        let project_root = tempdir.path().join("stale-daemon-with-pid-project");
        std::fs::create_dir(&project_root).expect("project root should be created");
        let socket_path = socket_path_for_project(&project_root);
        let pid_path = pid_path_for_project(&project_root);
        create_artifact_parent(&socket_path);
        create_artifact_parent(&pid_path);
        if socket_path.exists() {
            std::fs::remove_file(&socket_path).expect("stale socket should be removed");
        }

        let listener = UnixListener::bind(&socket_path).expect("socket should bind");
        drop(listener);
        std::fs::write(&pid_path, "999999").expect("stale pid should be writable");
        assert!(
            socket_path.exists(),
            "socket path should remain after listener drop"
        );
        assert!(pid_path.exists(), "pid path should exist before cleanup");

        let response = query_daemon(&project_root, &TldrDaemonCommand::Ping)
            .await
            .expect("stale socket should not error");

        assert_eq!(response, None);
        assert!(!socket_path.exists(), "stale socket should be removed");
        assert!(
            !pid_path.exists(),
            "stale pid should be removed with stale socket"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn query_daemon_keeps_stale_artifacts_while_launch_lock_is_held() {
        let tempdir = tempdir().expect("tempdir should exist");
        let project_root = tempdir.path().join("launch-locked-stale-daemon-project");
        std::fs::create_dir(&project_root).expect("project root should be created");
        let socket_path = socket_path_for_project(&project_root);
        let pid_path = pid_path_for_project(&project_root);
        let launch_lock_path = launch_lock_path_for_project(&project_root);
        create_artifact_parent(&socket_path);
        create_artifact_parent(&pid_path);
        create_artifact_parent(&launch_lock_path);
        if socket_path.exists() {
            std::fs::remove_file(&socket_path).expect("stale socket should be removed");
        }

        let listener = UnixListener::bind(&socket_path).expect("socket should bind");
        drop(listener);
        std::fs::write(&pid_path, "999999").expect("stale pid should be writable");
        let launch_lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&launch_lock_path)
            .expect("launch lock file should open");
        launch_lock
            .try_lock()
            .expect("launch lock should be acquired");

        let response = query_daemon(&project_root, &TldrDaemonCommand::Ping)
            .await
            .expect("stale socket should not error");

        assert_eq!(response, None);
        assert!(
            socket_path.exists(),
            "socket should remain while launch lock is held"
        );
        assert!(
            pid_path.exists(),
            "pid should remain while launch lock is held"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn query_daemon_errors_when_response_json_is_invalid() {
        let tempdir = tempdir().expect("tempdir should exist");
        let project_root = tempdir.path().join("invalid-json-daemon-project");
        std::fs::create_dir(&project_root).expect("project root should be created");
        let socket_path = socket_path_for_project(&project_root);
        create_artifact_parent(&socket_path);
        if socket_path.exists() {
            std::fs::remove_file(&socket_path).expect("stale socket should be removed");
        }

        let listener = UnixListener::bind(&socket_path).expect("socket should bind");
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("client should connect");
            let (_reader, mut writer) = tokio::io::split(stream);
            writer
                .write_all(b"{invalid json}\n")
                .await
                .expect("response should write");
        });

        let error = query_daemon(&project_root, &TldrDaemonCommand::Ping)
            .await
            .expect_err("invalid json should error");
        server.await.expect("server should complete");

        assert!(
            error.to_string().contains("decode daemon response from"),
            "unexpected error: {error}"
        );

        std::fs::remove_file(&socket_path).expect("socket should be cleaned up");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn query_daemon_errors_when_peer_disconnects_before_reply() {
        let tempdir = tempdir().expect("tempdir should exist");
        let project_root = tempdir.path().join("disconnect-daemon-project");
        std::fs::create_dir(&project_root).expect("project root should be created");
        let socket_path = socket_path_for_project(&project_root);
        create_artifact_parent(&socket_path);
        if socket_path.exists() {
            std::fs::remove_file(&socket_path).expect("stale socket should be removed");
        }

        let listener = UnixListener::bind(&socket_path).expect("socket should bind");
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("client should connect");
            drop(stream);
        });

        let error = query_daemon(&project_root, &TldrDaemonCommand::Ping)
            .await
            .expect_err("disconnect before reply should error");
        server.await.expect("server should complete");

        assert!(
            error.to_string().contains("read daemon response from"),
            "unexpected error: {error}"
        );

        std::fs::remove_file(&socket_path).expect("socket should be cleaned up");
    }

    #[cfg(unix)]
    #[tokio::test]
    #[serial]
    async fn daemon_shuts_down_after_idle_timeout() {
        let project = tempfile::tempdir().expect("tempdir should exist");
        let project_root = project.path().to_path_buf();
        let socket_path = socket_path_for_project(&project_root);
        let pid_path = pid_path_for_project(&project_root);
        let mut config = crate::TldrConfig::for_project(project_root.clone());
        config.session = crate::session::SessionConfig {
            idle_timeout: Duration::from_millis(150),
            dirty_file_threshold: 20,
        };
        config.semantic = crate::semantic::SemanticConfig::default().with_enabled(false);
        let daemon = TldrDaemon::from_config(config);

        let daemon_task = tokio::spawn(async move { daemon.run_until_shutdown().await });

        tokio::time::timeout(Duration::from_secs(2), async {
            while !socket_path.exists() || !pid_path.exists() {
                sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("daemon should create socket and pid files");

        let response = query_daemon(&project_root, &TldrDaemonCommand::Ping)
            .await
            .expect("ping should succeed")
            .expect("daemon should respond");
        assert_eq!(response.message, "pong");

        tokio::time::timeout(Duration::from_secs(2), async {
            while socket_path.exists() || pid_path.exists() {
                sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .expect("daemon should exit after idle timeout");

        daemon_task
            .await
            .expect("daemon task should join")
            .expect("daemon should exit cleanly");

        assert_eq!(
            query_daemon(&project_root, &TldrDaemonCommand::Ping)
                .await
                .expect("post-idle query should not error"),
            None
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    #[serial]
    async fn daemon_shutdown_command_stops_server_and_cleans_artifacts() {
        let project = tempfile::tempdir().expect("tempdir should exist");
        let project_root = project.path().to_path_buf();
        let socket_path = socket_path_for_project(&project_root);
        let pid_path = pid_path_for_project(&project_root);
        let mut config = crate::TldrConfig::for_project(project_root.clone());
        config.session = crate::session::SessionConfig {
            idle_timeout: Duration::from_secs(30),
            dirty_file_threshold: 20,
        };
        config.semantic = crate::semantic::SemanticConfig::default().with_enabled(false);
        let daemon = TldrDaemon::from_config(config);

        let daemon_task = tokio::spawn(async move { daemon.run_until_shutdown().await });

        tokio::time::timeout(Duration::from_secs(2), async {
            while !socket_path.exists() || !pid_path.exists() {
                sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("daemon should create socket and pid files");

        let response = query_daemon(&project_root, &TldrDaemonCommand::Shutdown)
            .await
            .expect("shutdown should succeed")
            .expect("daemon should respond");
        assert_eq!(response, TldrDaemonResponse::ok("shutdown requested"));

        tokio::time::timeout(Duration::from_secs(2), async {
            while socket_path.exists() || pid_path.exists() {
                sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .expect("shutdown should clean daemon artifacts");

        daemon_task
            .await
            .expect("daemon task should join")
            .expect("daemon should exit cleanly");

        assert_eq!(
            query_daemon(&project_root, &TldrDaemonCommand::Ping)
                .await
                .expect("post-shutdown query should not error"),
            None
        );
    }

    #[tokio::test]
    async fn notify_invalidates_cached_analyses_when_threshold_is_reached() {
        let project = tempfile::tempdir().expect("tempdir should exist");
        std::fs::create_dir_all(project.path().join("src")).expect("src dir should exist");
        std::fs::write(project.path().join("src/main.rs"), "fn main() {}\n")
            .expect("fixture should exist");
        let mut config = crate::TldrConfig::for_project(project.path().to_path_buf());
        config.session = crate::session::SessionConfig {
            idle_timeout: std::time::Duration::from_secs(60),
            dirty_file_threshold: 1,
        };
        config.semantic = crate::semantic::SemanticConfig::default().with_enabled(false);
        let daemon = TldrDaemon::from_config(config);

        let analyze = daemon
            .handle_command(TldrDaemonCommand::Analyze {
                key: "rust:main".to_string(),
                request: crate::api::AnalysisRequest {
                    kind: crate::api::AnalysisKind::Ast,
                    language: crate::lang_support::SupportedLanguage::Rust,
                    symbol: Some("main".to_string()),
                    path: None,
                    paths: Vec::new(),

                    line: None,
                },
            })
            .await
            .expect("analyze should succeed");
        assert_eq!(analyze.message, "computed");

        let notify = daemon
            .handle_command(TldrDaemonCommand::Notify {
                path: PathBuf::from("src/main.rs"),
            })
            .await
            .expect("notify should succeed");

        assert_eq!(
            notify.message,
            "dirty threshold reached; invalidated 1 cached analyses; reindex pending"
        );
        assert_eq!(
            notify.snapshot,
            Some(crate::session::SessionSnapshot {
                cached_entries: 0,
                dirty_files: 1,
                dirty_file_threshold: 1,
                reindex_pending: true,
                background_reindex_in_progress: false,
                last_query_at: analyze
                    .snapshot
                    .expect("analyze snapshot should exist")
                    .last_query_at,
                last_reindex: None,
                last_reindex_attempt: None,
                last_warm: None,
                last_structured_failure: None,
                degraded_mode_active: false,
            })
        );
    }

    #[tokio::test]
    #[serial]
    async fn warm_clears_reindex_pending_state() {
        let project = tempfile::tempdir().expect("tempdir should exist");
        let src_dir = project.path().join("src");
        std::fs::create_dir_all(&src_dir).expect("src dir should exist");
        std::fs::write(src_dir.join("main.rs"), "fn main() {}\n").expect("fixture should exist");

        let mut config = crate::TldrConfig::for_project(project.path().to_path_buf());
        config.session = crate::session::SessionConfig {
            idle_timeout: std::time::Duration::from_secs(60),
            dirty_file_threshold: 1,
        };
        config.semantic = crate::semantic::SemanticConfig::default().with_enabled(true);
        let daemon = TldrDaemon::from_config(config);

        daemon
            .handle_command(TldrDaemonCommand::Notify {
                path: PathBuf::from("src/main.rs"),
            })
            .await
            .expect("notify should succeed");
        let warm = daemon
            .handle_command(TldrDaemonCommand::Warm)
            .await
            .expect("warm should succeed");

        assert!(warm.reindex_report.is_some());
        let snapshot = warm.snapshot.expect("warm snapshot should exist");
        assert_eq!(snapshot.cached_entries, 0);
        assert_eq!(snapshot.dirty_files, 0);
        assert_eq!(snapshot.dirty_file_threshold, 1);
        assert_eq!(snapshot.reindex_pending, false);
        assert!(snapshot.last_query_at.is_some());
        assert_eq!(snapshot.last_reindex, warm.reindex_report);
        assert_eq!(snapshot.last_reindex_attempt, warm.reindex_report);
    }

    #[tokio::test]
    #[serial]
    async fn warm_reindexes_only_dirty_languages() {
        let project = tempfile::tempdir().expect("tempdir should exist");
        let src_dir = project.path().join("src");
        let scripts_dir = project.path().join("scripts");
        std::fs::create_dir_all(&src_dir).expect("src dir should exist");
        std::fs::create_dir_all(&scripts_dir).expect("scripts dir should exist");
        std::fs::write(src_dir.join("main.rs"), "fn main() {}\n")
            .expect("rust fixture should exist");
        std::fs::write(scripts_dir.join("tool.py"), "def run():\n    return True\n")
            .expect("python fixture should exist");

        let mut config = crate::TldrConfig::for_project(project.path().to_path_buf());
        config.session = crate::session::SessionConfig {
            idle_timeout: std::time::Duration::from_secs(60),
            dirty_file_threshold: 2,
        };
        config.semantic = crate::semantic::SemanticConfig::default().with_enabled(true);
        let daemon = TldrDaemon::from_config(config);

        daemon
            .handle_command(TldrDaemonCommand::Notify {
                path: PathBuf::from("src/main.rs"),
            })
            .await
            .expect("rust notify should succeed");
        daemon
            .handle_command(TldrDaemonCommand::Notify {
                path: PathBuf::from("scripts/tool.py"),
            })
            .await
            .expect("python notify should succeed");

        let warm = daemon
            .handle_command(TldrDaemonCommand::Warm)
            .await
            .expect("warm should succeed");
        let languages = warm
            .reindex_report
            .as_ref()
            .map(|report| report.languages.clone())
            .expect("warm report should exist");

        assert_eq!(
            languages,
            vec![
                crate::lang_support::SupportedLanguage::Python,
                crate::lang_support::SupportedLanguage::Rust
            ]
        );
    }

    #[tokio::test]
    #[serial]
    async fn warm_skips_reindex_for_unmapped_dirty_paths() {
        let project = tempfile::tempdir().expect("tempdir should exist");
        let mut config = crate::TldrConfig::for_project(project.path().to_path_buf());
        config.session = crate::session::SessionConfig {
            idle_timeout: std::time::Duration::from_secs(60),
            dirty_file_threshold: 1,
        };
        config.semantic = crate::semantic::SemanticConfig::default().with_enabled(true);
        let daemon = TldrDaemon::from_config(config);

        daemon
            .handle_command(TldrDaemonCommand::Notify {
                path: PathBuf::from("README.md"),
            })
            .await
            .expect("notify should succeed");

        let warm = daemon
            .handle_command(TldrDaemonCommand::Warm)
            .await
            .expect("warm should succeed");
        let report = warm.reindex_report.expect("warm report should exist");
        let snapshot = warm.snapshot.expect("warm snapshot should exist");

        assert_eq!(
            report.status,
            crate::semantic::SemanticReindexStatus::Skipped
        );
        assert_eq!(
            report.languages,
            Vec::<crate::lang_support::SupportedLanguage>::new()
        );
        assert_eq!(snapshot.reindex_pending, false);
        assert_eq!(snapshot.dirty_files, 0);
    }

    #[tokio::test]
    #[serial]
    async fn warm_without_dirty_files_records_loaded_languages() {
        let project = tempfile::tempdir().expect("tempdir should exist");
        let src_dir = project.path().join("src");
        std::fs::create_dir_all(&src_dir).expect("src dir should exist");
        std::fs::write(src_dir.join("main.rs"), "fn main() {}\n").expect("fixture should exist");

        let mut config = crate::TldrConfig::for_project(project.path().to_path_buf());
        config.semantic = crate::semantic::SemanticConfig::default().with_enabled(true);
        let daemon = TldrDaemon::from_config(config);

        let warm = daemon
            .handle_command(TldrDaemonCommand::Warm)
            .await
            .expect("warm should succeed");
        let snapshot = warm.snapshot.expect("warm snapshot should exist");
        let last_warm = snapshot.last_warm.expect("warm report should be recorded");

        assert_eq!(warm.reindex_report, None);
        assert_eq!(last_warm.status, crate::session::WarmStatus::Loaded);
        assert_eq!(
            last_warm.languages,
            vec![crate::lang_support::SupportedLanguage::Rust]
        );
        assert!(warm.message.contains("warm loaded 1 language indexes"));
    }

    #[tokio::test]
    #[serial]
    async fn status_surfaces_last_completed_reindex_after_warm() {
        let project = tempfile::tempdir().expect("tempdir should exist");
        let src_dir = project.path().join("src");
        std::fs::create_dir_all(&src_dir).expect("src dir should exist");
        std::fs::write(src_dir.join("main.rs"), "fn main() {}\n").expect("fixture should exist");

        let mut config = crate::TldrConfig::for_project(project.path().to_path_buf());
        config.session = crate::session::SessionConfig {
            idle_timeout: std::time::Duration::from_secs(60),
            dirty_file_threshold: 1,
        };
        config.semantic = crate::semantic::SemanticConfig::default().with_enabled(true);
        let daemon = TldrDaemon::from_config(config);

        daemon
            .handle_command(TldrDaemonCommand::Notify {
                path: PathBuf::from("src/main.rs"),
            })
            .await
            .expect("notify should succeed");
        let warm = daemon
            .handle_command(TldrDaemonCommand::Warm)
            .await
            .expect("warm should succeed");
        let status = daemon
            .handle_command(TldrDaemonCommand::Status)
            .await
            .expect("status should succeed");

        assert_eq!(status.reindex_report, warm.reindex_report);
        assert_eq!(
            status
                .snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.last_reindex.clone()),
            warm.reindex_report
        );
        assert_eq!(
            status
                .snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.last_reindex_attempt.clone()),
            warm.reindex_report
        );
        assert_eq!(
            status
                .snapshot
                .as_ref()
                .map(|snapshot| snapshot.reindex_pending),
            Some(false)
        );
    }

    #[tokio::test]
    #[serial]
    async fn status_surfaces_last_failed_reindex_attempt_after_warm_failure() {
        let project = tempfile::tempdir().expect("tempdir should exist");
        let mut config = crate::TldrConfig::for_project(project.path().to_path_buf());
        config.session = crate::session::SessionConfig {
            idle_timeout: std::time::Duration::from_secs(60),
            dirty_file_threshold: 1,
        };
        config.semantic = crate::semantic::SemanticConfig::default().with_enabled(false);
        let daemon = TldrDaemon::from_config(config);

        daemon
            .handle_command(TldrDaemonCommand::Notify {
                path: PathBuf::from("src/main.rs"),
            })
            .await
            .expect("notify should succeed");
        let warm = daemon
            .handle_command(TldrDaemonCommand::Warm)
            .await
            .expect("warm should return a failed report");
        let status = daemon
            .handle_command(TldrDaemonCommand::Status)
            .await
            .expect("status should succeed");

        assert_eq!(
            warm.reindex_report.as_ref().map(|report| &report.status),
            Some(&crate::semantic::SemanticReindexStatus::Failed)
        );
        assert_eq!(
            warm.reindex_report
                .as_ref()
                .map(|report| report.languages.clone()),
            Some(vec![crate::lang_support::SupportedLanguage::Rust])
        );
        assert_eq!(
            status.reindex_report.as_ref().map(|report| &report.status),
            Some(&crate::semantic::SemanticReindexStatus::Failed)
        );
        assert_eq!(
            status
                .snapshot
                .as_ref()
                .map(|snapshot| snapshot.reindex_pending),
            Some(true)
        );
        assert_eq!(
            status
                .snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.last_reindex.clone()),
            None
        );
        assert_eq!(
            status
                .snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.last_reindex_attempt.clone()),
            warm.reindex_report
        );
        assert_eq!(
            status
                .daemon_status
                .as_ref()
                .map(|daemon_status| daemon_status.semantic_reindex_pending),
            Some(true)
        );
    }

    #[tokio::test]
    #[serial]
    async fn warm_keeps_reindex_pending_when_reindex_fails() {
        let project = tempfile::tempdir().expect("tempdir should exist");
        let mut config = crate::TldrConfig::for_project(project.path().to_path_buf());
        config.session = crate::session::SessionConfig {
            idle_timeout: std::time::Duration::from_secs(60),
            dirty_file_threshold: 1,
        };
        config.semantic = crate::semantic::SemanticConfig::default().with_enabled(false);
        let daemon = TldrDaemon::from_config(config);

        daemon
            .handle_command(TldrDaemonCommand::Notify {
                path: PathBuf::from("src/main.rs"),
            })
            .await
            .expect("notify should succeed");
        let warm = daemon
            .handle_command(TldrDaemonCommand::Warm)
            .await
            .expect("warm should return a report");

        assert_eq!(
            warm.snapshot.as_ref().map(|snapshot| snapshot.dirty_files),
            Some(1)
        );
        assert_eq!(
            warm.snapshot
                .as_ref()
                .map(|snapshot| snapshot.reindex_pending),
            Some(true)
        );
        assert_eq!(
            warm.snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.last_reindex_attempt.clone()),
            warm.reindex_report
        );
        assert_eq!(
            warm.reindex_report.as_ref().map(|report| &report.status),
            Some(&crate::semantic::SemanticReindexStatus::Failed)
        );
    }

    #[tokio::test]
    async fn analyze_bypasses_cache_while_reindex_is_pending() {
        let project = tempfile::tempdir().expect("tempdir should exist");
        std::fs::create_dir_all(project.path().join("src")).expect("src dir should exist");
        std::fs::write(project.path().join("src/main.rs"), "fn main() {}\n")
            .expect("fixture should exist");
        let mut config = crate::TldrConfig::for_project(project.path().to_path_buf());
        config.session = crate::session::SessionConfig {
            idle_timeout: std::time::Duration::from_secs(60),
            dirty_file_threshold: 1,
        };
        let daemon = TldrDaemon::from_config(config);

        let first = daemon
            .handle_command(TldrDaemonCommand::Analyze {
                key: "rust:main".to_string(),
                request: crate::api::AnalysisRequest {
                    kind: crate::api::AnalysisKind::Ast,
                    language: crate::lang_support::SupportedLanguage::Rust,
                    symbol: Some("main".to_string()),
                    path: None,
                    paths: Vec::new(),

                    line: None,
                },
            })
            .await
            .expect("first analyze should succeed");
        assert_eq!(first.message, "computed");

        daemon
            .handle_command(TldrDaemonCommand::Notify {
                path: PathBuf::from("src/main.rs"),
            })
            .await
            .expect("notify should succeed");

        let second = daemon
            .handle_command(TldrDaemonCommand::Analyze {
                key: "rust:main".to_string(),
                request: crate::api::AnalysisRequest {
                    kind: crate::api::AnalysisKind::Ast,
                    language: crate::lang_support::SupportedLanguage::Rust,
                    symbol: Some("main".to_string()),
                    path: None,
                    paths: Vec::new(),

                    line: None,
                },
            })
            .await
            .expect("second analyze should succeed");
        let third = daemon
            .handle_command(TldrDaemonCommand::Analyze {
                key: "rust:main".to_string(),
                request: crate::api::AnalysisRequest {
                    kind: crate::api::AnalysisKind::Ast,
                    language: crate::lang_support::SupportedLanguage::Rust,
                    symbol: Some("main".to_string()),
                    path: None,
                    paths: Vec::new(),

                    line: None,
                },
            })
            .await
            .expect("third analyze should succeed");

        assert_eq!(second.message, "computed (cache bypassed: reindex pending)");
        assert_eq!(third.message, "computed (cache bypassed: reindex pending)");
        let snapshot = third.snapshot.expect("third snapshot should exist");
        assert_eq!(snapshot.cached_entries, 0);
        assert_eq!(snapshot.dirty_files, 1);
        assert_eq!(snapshot.dirty_file_threshold, 1);
        assert_eq!(snapshot.reindex_pending, true);
        assert_eq!(snapshot.last_query_at.is_some(), true);
    }

    #[tokio::test]
    #[serial]
    async fn semantic_command_returns_semantic_payload() {
        let project = tempfile::tempdir().expect("tempdir should exist");
        let src_dir = project.path().join("src");
        std::fs::create_dir_all(&src_dir).expect("src dir should exist");
        std::fs::write(
            src_dir.join("auth.rs"),
            "fn verify_token() {\n    let auth_token = true;\n}\n",
        )
        .expect("fixture should exist");

        let mut config = crate::TldrConfig::for_project(project.path().to_path_buf());
        config.semantic = crate::semantic::SemanticConfig::default().with_enabled(true);
        let daemon = TldrDaemon::from_config(config);

        let response = daemon
            .handle_command(TldrDaemonCommand::Semantic {
                request: SemanticSearchRequest {
                    language: crate::lang_support::SupportedLanguage::Rust,
                    query: "auth token".to_string(),
                },
            })
            .await
            .expect("semantic should succeed");

        assert_eq!(response.analysis, None);
        assert!(response.snapshot.is_none());
        let semantic = response.semantic.expect("semantic payload should exist");
        assert!(semantic.enabled);
        assert_eq!(semantic.indexed_files, 1);
        assert_eq!(semantic.matches[0].path, PathBuf::from("src/auth.rs"));
    }

    #[tokio::test]
    #[serial]
    async fn semantic_command_reuses_cached_index_across_requests() {
        let project = tempfile::tempdir().expect("tempdir should exist");
        let src_dir = project.path().join("src");
        std::fs::create_dir_all(&src_dir).expect("src dir should exist");
        std::fs::write(
            src_dir.join("auth.rs"),
            "fn verify_token() {\n    let auth_token = true;\n}\n",
        )
        .expect("fixture should exist");

        let mut config = crate::TldrConfig::for_project(project.path().to_path_buf());
        config.semantic = crate::semantic::SemanticConfig::default().with_enabled(true);
        let daemon = TldrDaemon::from_config(config);

        for query in ["auth token", "verify_token"] {
            let response = daemon
                .handle_command(TldrDaemonCommand::Semantic {
                    request: SemanticSearchRequest {
                        language: crate::lang_support::SupportedLanguage::Rust,
                        query: query.to_string(),
                    },
                })
                .await
                .expect("semantic should succeed");
            assert!(
                response
                    .semantic
                    .expect("semantic payload should exist")
                    .enabled
            );
        }

        let cached_languages = daemon
            .engine
            .semantic_indexes
            .read()
            .expect("semantic index cache lock should not be poisoned")
            .len();
        assert_eq!(cached_languages, 1);
    }

    #[test]
    fn pid_path_uses_same_project_hash_scheme() {
        let tempdir = tempdir().expect("tempdir should exist");
        let project_root = tempdir.path().join("hash-project");

        let socket_path = socket_path_for_project(&project_root);
        let pid_path = pid_path_for_project(&project_root);
        let lock_path = lock_path_for_project(&project_root);

        let socket_stem = socket_path.file_stem().and_then(|value| value.to_str());
        let pid_stem = pid_path.file_stem().and_then(|value| value.to_str());
        let lock_stem = lock_path.file_stem().and_then(|value| value.to_str());

        assert_eq!(socket_stem, pid_stem);
        assert_eq!(socket_stem, lock_stem);
        assert_eq!(
            socket_path.extension().and_then(|value| value.to_str()),
            Some("sock")
        );
        assert_eq!(
            pid_path.extension().and_then(|value| value.to_str()),
            Some("pid")
        );
        assert_eq!(
            lock_path.extension().and_then(|value| value.to_str()),
            Some("lock")
        );
    }

    #[test]
    fn daemon_artifact_paths_are_scoped_under_runtime_root() {
        let tempdir = tempdir().expect("tempdir should exist");
        let project_root = tempdir.path().join("scoped-artifact-project");
        let socket_path = socket_path_for_project(&project_root);
        let pid_path = pid_path_for_project(&project_root);
        let lock_path = lock_path_for_project(&project_root);

        let artifact_dir = socket_path
            .parent()
            .expect("socket path should have artifact dir");
        assert_eq!(
            artifact_dir,
            pid_path.parent().expect("pid path should have parent")
        );
        assert_eq!(
            artifact_dir.file_name().and_then(|value| value.to_str()),
            Some(daemon_project_hash(&project_root).as_str())
        );
        let scope_dir = artifact_dir
            .parent()
            .expect("artifact dir should have scope dir");
        assert_eq!(
            scope_dir,
            lock_path.parent().expect("lock path should have scope dir")
        );

        #[cfg(unix)]
        {
            let uid = unsafe { libc::geteuid() }.to_string();
            assert_eq!(
                scope_dir.file_name().and_then(|value| value.to_str()),
                Some(uid.as_str())
            );
            assert_eq!(
                scope_dir
                    .parent()
                    .and_then(|value| value.file_name())
                    .and_then(|value| value.to_str()),
                Some("codex-native-tldr")
            );
        }

        #[cfg(not(unix))]
        {
            assert_eq!(
                scope_dir.parent().and_then(|value| value.file_name()),
                Some(std::ffi::OsStr::new("codex-native-tldr"))
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn daemon_artifact_paths_prefer_absolute_xdg_runtime_dir() {
        let tempdir = tempdir().expect("tempdir should exist");
        let runtime_dir = tempdir.path().join("runtime");
        std::fs::create_dir_all(&runtime_dir).expect("runtime dir should be created");
        let scope_dir = daemon_artifact_scope_dir_for_runtime_dir(Some(&runtime_dir), unsafe {
            libc::geteuid()
        });

        assert!(scope_dir.starts_with(&runtime_dir));
    }

    #[cfg(unix)]
    #[test]
    fn daemon_artifact_paths_ignore_relative_xdg_runtime_dir() {
        let tempdir = tempdir().expect("tempdir should exist");
        let relative_runtime_dir = tempdir.path().join("relative-runtime-dir");
        let scope_dir = daemon_artifact_scope_dir_for_runtime_dir(
            Some(
                relative_runtime_dir
                    .strip_prefix("/")
                    .unwrap_or(&relative_runtime_dir),
            ),
            unsafe { libc::geteuid() },
        );

        assert_eq!(scope_dir, daemon_temp_artifact_scope_dir());
    }

    #[cfg(unix)]
    #[test]
    fn daemon_artifact_paths_fall_back_when_runtime_socket_path_is_too_long() {
        let tempdir = tempdir().expect("tempdir should exist");
        let project_root = tempdir.path().join("long-runtime-socket-project");
        let project_hash = daemon_project_hash(&project_root);
        let runtime_dir = tempdir.path().join("runtime").join("x".repeat(128));
        std::fs::create_dir_all(&runtime_dir).expect("runtime dir should be created");

        let scope_dir = daemon_artifact_scope_dir_for_project_hash(
            Some(&runtime_dir),
            unsafe { libc::geteuid() },
            &project_hash,
        );

        assert_eq!(scope_dir, daemon_temp_artifact_scope_dir());
        assert!(unix_socket_path_fits(&scope_dir, &project_hash));
    }

    #[test]
    fn daemon_lock_reports_when_project_lock_is_held() {
        let tempdir = tempdir().expect("tempdir should exist");
        let project_root = tempdir.path().join("lock-project");
        std::fs::create_dir(&project_root).expect("project root should be created");
        let lock_path = lock_path_for_project(&project_root);
        create_artifact_parent(&lock_path);
        let lock_file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)
            .expect("lock file should open");

        lock_file.try_lock().expect("lock should be acquired");
        assert!(daemon_lock_is_held(&project_root).expect("lock query should succeed"));
    }

    #[test]
    fn daemon_lock_query_recovers_after_lock_file_is_deleted() {
        let tempdir = tempdir().expect("tempdir should exist");
        let project_root = tempdir.path().join("deleted-daemon-lock-project");
        std::fs::create_dir(&project_root).expect("project root should be created");
        let lock_path = lock_path_for_project(&project_root);
        create_artifact_parent(&lock_path);
        std::fs::write(&lock_path, "").expect("lock file should exist before deletion");
        std::fs::remove_file(&lock_path).expect("lock file should be removed");
        assert!(!lock_path.exists());

        let lock_is_held = daemon_lock_is_held(&project_root).expect("lock query should succeed");

        assert!(!lock_is_held);
        assert!(lock_path.exists());
    }

    #[test]
    fn launch_lock_query_recovers_after_lock_file_is_deleted() {
        let tempdir = tempdir().expect("tempdir should exist");
        let project_root = tempdir.path().join("deleted-launch-lock-project");
        std::fs::create_dir(&project_root).expect("project root should be created");
        let lock_path = launch_lock_path_for_project(&project_root);
        create_artifact_parent(&lock_path);
        std::fs::write(&lock_path, "").expect("launch lock file should exist before deletion");
        std::fs::remove_file(&lock_path).expect("launch lock file should be removed");
        assert!(!lock_path.exists());

        let lock_is_held =
            launch_lock_is_held(&project_root).expect("launch lock query should succeed");

        assert!(!lock_is_held);
        assert!(lock_path.exists());
    }

    #[test]
    fn daemon_health_recovers_after_lock_file_is_deleted() {
        let tempdir = tempdir().expect("tempdir should exist");
        let project_root = tempdir.path().join("deleted-daemon-lock-health-project");
        std::fs::create_dir(&project_root).expect("project root should be created");
        let lock_path = lock_path_for_project(&project_root);
        create_artifact_parent(&lock_path);
        std::fs::write(&lock_path, "").expect("lock file should exist before deletion");
        std::fs::remove_file(&lock_path).expect("lock file should be removed");
        assert!(!lock_path.exists());

        let health = daemon_health(&project_root).expect("health query should succeed");

        assert!(!health.lock_is_held);
        assert!(lock_path.exists());
        assert_eq!(
            health.health_reason.as_deref(),
            Some("daemon unavailable (missing socket and pid)")
        );
    }

    #[cfg(unix)]
    #[test]
    fn daemon_artifact_scope_dir_allows_isolated_runtime_root() {
        let tempdir = tempdir().expect("tempdir should exist");
        let runtime_dir = tempdir.path().join("runtime");
        std::fs::create_dir_all(&runtime_dir).expect("runtime dir should be created");
        let project_root = tempdir.path().join("lock-parent-recovery-project");
        std::fs::create_dir(&project_root).expect("project root should be created");
        let scope_dir = daemon_artifact_scope_dir_for_runtime_dir(Some(&runtime_dir), unsafe {
            libc::geteuid()
        });
        let lock_file_name = format!(
            "codex-native-tldr-{}.lock",
            daemon_project_hash(&project_root)
        );
        let lock_path = scope_dir.join(&lock_file_name);

        create_artifact_parent(&lock_path);

        assert!(scope_dir.starts_with(&runtime_dir));
        assert!(lock_path.starts_with(&scope_dir));
        assert_eq!(
            lock_path
                .file_name()
                .map(|value| value.to_string_lossy().into_owned()),
            Some(lock_file_name)
        );
    }

    #[test]
    fn write_pid_file_recreates_missing_artifact_parent() {
        let tempdir = tempdir().expect("tempdir should exist");
        let project_root = tempdir.path().join("pid-parent-recovery-project");
        let pid_path = pid_path_for_project(&project_root);
        let parent = pid_path.parent().expect("pid path should have parent");
        std::fs::remove_dir_all(parent).ok();
        assert!(!parent.exists());

        write_pid_file(&pid_path).expect("pid file should be written");

        assert!(parent.exists());
        assert_eq!(
            std::fs::read_to_string(&pid_path).expect("pid file should be readable"),
            std::process::id().to_string()
        );
    }

    #[test]
    fn tcp_endpoint_metadata_round_trips() {
        let tempdir = tempdir().expect("tempdir should exist");
        let endpoint_path = tempdir.path().join("daemon.sock");
        let address: SocketAddr = "127.0.0.1:43123".parse().expect("address should parse");

        write_tcp_endpoint_file(&endpoint_path, address).expect("metadata should write");

        assert_eq!(read_tcp_endpoint(&endpoint_path), Some(address));
    }

    #[test]
    fn tcp_endpoint_metadata_rejects_invalid_contents() {
        let tempdir = tempdir().expect("tempdir should exist");
        let endpoint_path = tempdir.path().join("daemon.sock");
        std::fs::write(&endpoint_path, "not-an-endpoint").expect("metadata should write");

        assert_eq!(read_tcp_endpoint(&endpoint_path), None);
        assert!(!tcp_endpoint_is_alive(&endpoint_path));
    }

    #[test]
    fn tcp_endpoint_liveness_tracks_listener_state() {
        let tempdir = tempdir().expect("tempdir should exist");
        let endpoint_path = tempdir.path().join("daemon.sock");
        let listener =
            std::net::TcpListener::bind("127.0.0.1:0").expect("tcp listener should bind");
        let address = listener.local_addr().expect("local addr should resolve");

        write_tcp_endpoint_file(&endpoint_path, address).expect("metadata should write");
        assert!(tcp_endpoint_is_alive(&endpoint_path));

        drop(listener);
        assert!(!tcp_endpoint_is_alive(&endpoint_path));
    }

    #[test]
    fn ensure_daemon_artifact_parent_errors_when_parent_path_is_blocked_by_file() {
        let tempdir = tempdir().expect("tempdir should exist");
        let blocked_parent = tempdir.path().join("blocked-parent");
        std::fs::write(&blocked_parent, "not a directory").expect("blocked parent should exist");
        let artifact_path = blocked_parent.join("daemon.sock");

        let error = ensure_daemon_artifact_parent(&artifact_path)
            .expect_err("file in artifact parent chain should error");

        assert!(
            error.to_string().contains("create daemon artifact dir"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn write_pid_file_errors_when_parent_path_is_blocked_by_file() {
        let tempdir = tempdir().expect("tempdir should exist");
        let blocked_parent = tempdir.path().join("blocked-pid-parent");
        std::fs::write(&blocked_parent, "not a directory").expect("blocked parent should exist");
        let pid_path = blocked_parent.join("daemon.pid");

        let error =
            write_pid_file(&pid_path).expect_err("file in pid parent chain should block writes");

        assert!(
            error.to_string().contains("create daemon artifact dir"),
            "unexpected error: {error}"
        );
    }

    #[cfg(not(unix))]
    #[test]
    fn pid_is_alive_returns_false_off_unix() {
        assert!(!pid_is_alive(123));
    }

    #[cfg(unix)]
    #[test]
    fn pid_is_alive_rejects_non_positive_pid() {
        assert!(!pid_is_alive(0));
    }

    #[test]
    fn daemon_health_marks_stale_socket_without_live_pid() {
        let tempdir = tempdir().expect("tempdir should exist");
        let project_root = tempdir.path().join("stale-health-project");
        std::fs::create_dir(&project_root).expect("project root should be created");
        let socket_path = socket_path_for_project(&project_root);
        let pid_path = pid_path_for_project(&project_root);
        create_artifact_parent(&socket_path);
        create_artifact_parent(&pid_path);

        std::fs::write(&socket_path, "").expect("socket path should be writable");
        std::fs::write(&pid_path, "999999").expect("pid path should be writable");

        let health = daemon_health(&project_root).expect("health should load");
        assert!(!health.healthy);
        assert!(health.stale_socket);
        assert!(!health.stale_pid);
        assert!(health.should_cleanup_artifacts());
    }

    #[test]
    fn daemon_health_reports_reason_for_stale_socket() {
        let tempdir = tempdir().expect("tempdir should exist");
        let project_root = tempdir.path().join("stale-reason-project");
        std::fs::create_dir(&project_root).expect("project root should be created");
        let socket_path = socket_path_for_project(&project_root);
        let pid_path = pid_path_for_project(&project_root);
        create_artifact_parent(&socket_path);
        create_artifact_parent(&pid_path);

        std::fs::write(&socket_path, "").expect("socket path should be writable");
        std::fs::write(&pid_path, "999999").expect("pid path should be writable");

        let health = daemon_health(&project_root).expect("health should load");
        assert_eq!(
            health.health_reason.as_deref(),
            Some("stale socket without live daemon")
        );
        assert_eq!(
            health.recovery_hint.as_deref(),
            Some("remove stale socket/pid files and restart the daemon")
        );
    }

    #[test]
    fn daemon_health_reports_reason_for_stale_pid() {
        let tempdir = tempdir().expect("tempdir should exist");
        let project_root = tempdir.path().join("stale-pid-reason-project");
        std::fs::create_dir(&project_root).expect("project root should be created");
        let pid_path = pid_path_for_project(&project_root);
        create_artifact_parent(&pid_path);

        std::fs::write(&pid_path, std::process::id().to_string())
            .expect("pid path should be writable");

        let health = daemon_health(&project_root).expect("health should load");
        assert!(!health.healthy);
        assert!(!health.stale_socket);
        assert!(health.stale_pid);
        assert!(health.should_cleanup_artifacts());
        assert_eq!(
            health.health_reason.as_deref(),
            Some("pid file exists but socket is missing")
        );
        assert_eq!(
            health.recovery_hint.as_deref(),
            Some("cleanup pid/socket files before restarting the daemon")
        );
    }

    #[test]
    fn daemon_health_reports_lock_hint_when_lock_is_held() {
        let tempdir = tempdir().expect("tempdir should exist");
        let project_root = tempdir.path().join("lock-reason-project");
        std::fs::create_dir(&project_root).expect("project root should be created");
        let lock_path = lock_path_for_project(&project_root);
        create_artifact_parent(&lock_path);
        let _lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)
            .expect("lock file should open");

        _lock_file.try_lock().expect("lock should be acquired");
        let health = daemon_health(&project_root).expect("health should load");
        assert_eq!(
            health.health_reason.as_deref(),
            Some("daemon lock held; another process may be starting it")
        );
        assert_eq!(
            health.recovery_hint.as_deref(),
            Some("wait for the existing daemon or release the lock manually")
        );
        assert!(!health.should_cleanup_artifacts());
    }

    #[test]
    fn daemon_health_reports_launch_lock_hint_when_startup_is_in_progress() {
        let tempdir = tempdir().expect("tempdir should exist");
        let project_root = tempdir.path().join("launch-lock-reason-project");
        std::fs::create_dir(&project_root).expect("project root should be created");
        let lock_path = launch_lock_path_for_project(&project_root);
        create_artifact_parent(&lock_path);
        let launch_lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)
            .expect("launch lock file should open");

        launch_lock
            .try_lock()
            .expect("launch lock should be acquired");
        let health = daemon_health(&project_root).expect("health should load");

        assert!(health.launch_lock_is_held);
        assert_eq!(
            health.health_reason.as_deref(),
            Some("daemon launch lock held; another process is starting it")
        );
        assert_eq!(
            health.recovery_hint.as_deref(),
            Some("wait for the launcher to finish before cleaning up artifacts")
        );
        assert!(!health.should_cleanup_artifacts());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn query_daemon_times_out_when_response_never_arrives() {
        let tempdir = tempdir().expect("tempdir should exist");
        let project_root = tempdir.path().join("hung-daemon-project");
        std::fs::create_dir(&project_root).expect("project root should be created");
        let socket_path = socket_path_for_project(&project_root);
        create_artifact_parent(&socket_path);
        let listener = UnixListener::bind(&socket_path).expect("listener should bind");
        let accept_task = tokio::spawn(async move {
            let (_stream, _) = listener.accept().await.expect("listener should accept");
            sleep(Duration::from_millis(200)).await;
        });

        let error = query_daemon_with_timeout(
            &project_root,
            &TldrDaemonCommand::Ping,
            Duration::from_millis(50),
            Duration::from_millis(50),
        )
        .await
        .expect_err("hung daemon should time out");

        assert!(error.to_string().contains("read timeout"));
        let _ = accept_task.await;
    }

    #[cfg(unix)]
    #[test]
    fn io_timeout_for_command_uses_extended_budget_for_heavy_actions() {
        assert_eq!(
            io_timeout_for_command(&TldrDaemonCommand::Ping),
            DAEMON_IO_TIMEOUT
        );
        assert_eq!(
            io_timeout_for_command(&TldrDaemonCommand::Status),
            DAEMON_IO_TIMEOUT
        );
        assert_eq!(
            io_timeout_for_command(&TldrDaemonCommand::Warm),
            DAEMON_HEAVY_IO_TIMEOUT
        );
        assert_eq!(
            io_timeout_for_command(&TldrDaemonCommand::Analyze {
                key: "rust:Ast:*:*:*".to_string(),
                request: crate::api::AnalysisRequest {
                    kind: crate::api::AnalysisKind::Ast,
                    language: crate::lang_support::SupportedLanguage::Rust,
                    symbol: None,
                    path: None,
                    line: None,
                    paths: Vec::new(),
                },
            }),
            DAEMON_HEAVY_IO_TIMEOUT
        );
    }
}
