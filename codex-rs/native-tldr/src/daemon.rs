use crate::TldrConfig;
use crate::TldrEngine;
use crate::api::AnalysisRequest;
use crate::api::AnalysisResponse;
use crate::semantic::SemanticReindexReport;
use crate::semantic::SemanticSearchRequest;
use crate::semantic::SemanticSearchResponse;
use crate::session::Session;
use crate::session::SessionConfig;
use anyhow::Context;
use anyhow::Result;
use md5::compute as md5_compute;
use serde::Deserialize;
use serde::Serialize;
use std::fs::File;
use std::fs::OpenOptions;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::sync::Mutex;

#[cfg(not(unix))]
use tokio::net::TcpListener;
#[cfg(unix)]
use tokio::net::UnixListener;
#[cfg(unix)]
use tokio::net::UnixStream;

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
    Semantic {
        request: SemanticSearchRequest,
    },
    Notify {
        path: PathBuf,
    },
    Snapshot,
    Status,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TldrDaemonResponse {
    pub status: String,
    pub message: String,
    pub analysis: Option<AnalysisResponse>,
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
    pub semantic_reindex_pending: bool,
    pub last_query_at: Option<std::time::SystemTime>,
    pub config: TldrDaemonConfigSummary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonHealth {
    pub socket_exists: bool,
    pub pid_is_live: bool,
    pub lock_is_held: bool,
    pub healthy: bool,
    pub stale_socket: bool,
    pub stale_pid: bool,
    pub health_reason: Option<String>,
    pub recovery_hint: Option<String>,
}

impl DaemonHealth {
    pub fn should_cleanup_artifacts(&self) -> bool {
        !self.lock_is_held && (self.stale_socket || self.stale_pid)
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
                let (message, reindex_report) = warm_with_reindex(&mut session, &self.engine);
                let snapshot = session.snapshot();
                Ok(TldrDaemonResponse {
                    status: "ok".to_string(),
                    message,
                    analysis: None,
                    semantic: None,
                    snapshot: Some(snapshot.clone()),
                    daemon_status: Some(build_daemon_status(
                        &self.project_root,
                        self.engine.config(),
                        &snapshot,
                    )?),
                    reindex_report,
                })
            }
            TldrDaemonCommand::Analyze { key, request } => {
                let mut session = self.session.lock().await;
                analyze_with_session(&mut session, &self.engine, key, request)
            }
            TldrDaemonCommand::Notify { path } => {
                let mut session = self.session.lock().await;
                let message = notify_session_message(&mut session, path);
                Ok(TldrDaemonResponse {
                    status: "ok".to_string(),
                    message,
                    analysis: None,
                    semantic: None,
                    snapshot: Some(session.snapshot()),
                    daemon_status: None,
                    reindex_report: session.last_reindex_report(),
                })
            }
            TldrDaemonCommand::Snapshot => {
                let session = self.session.lock().await;
                Ok(TldrDaemonResponse {
                    snapshot: Some(session.snapshot()),
                    ..TldrDaemonResponse::ok("snapshot")
                })
            }
            TldrDaemonCommand::Status => {
                let session = self.session.lock().await;
                let snapshot = session.snapshot();
                Ok(TldrDaemonResponse {
                    snapshot: Some(snapshot.clone()),
                    reindex_report: session.last_reindex_attempt_report(),
                    daemon_status: Some(build_daemon_status(
                        &self.project_root,
                        self.engine.config(),
                        &snapshot,
                    )?),
                    ..TldrDaemonResponse::ok("status")
                })
            }
            TldrDaemonCommand::Semantic { request } => {
                let response = self.engine.semantic_search(request)?;
                Ok(TldrDaemonResponse {
                    status: "ok".to_string(),
                    message: response.message.clone(),
                    analysis: None,
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
                    tokio::spawn(async move {
                        let _ = serve_connection(stream, session, engine).await;
                    });
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
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        loop {
            tokio::select! {
                accept_result = listener.accept() => {
                    let (stream, _) = accept_result?;
                    let session = Arc::clone(&self.session);
                    let engine = self.engine.clone();
                    tokio::spawn(async move {
                        let _ = serve_connection(stream, session, engine).await;
                    });
                }
                signal = tokio::signal::ctrl_c() => {
                    signal?;
                    break;
                }
            }
        }
        Ok(())
    }
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
            let (message, reindex_report) = warm_with_reindex(&mut guard, engine);
            let snapshot = guard.snapshot();
            Ok(TldrDaemonResponse {
                status: "ok".to_string(),
                message,
                analysis: None,
                semantic: None,
                snapshot: Some(snapshot.clone()),
                daemon_status: Some(build_daemon_status(
                    project_root,
                    engine.config(),
                    &snapshot,
                )?),
                reindex_report,
            })
        }
        TldrDaemonCommand::Analyze { key, request } => {
            let mut guard = session.lock().await;
            analyze_with_session(&mut guard, engine, key, request)
        }
        TldrDaemonCommand::Notify { path } => {
            let mut guard = session.lock().await;
            let message = notify_session_message(&mut guard, path);
            Ok(TldrDaemonResponse {
                status: "ok".to_string(),
                message,
                analysis: None,
                semantic: None,
                snapshot: Some(guard.snapshot()),
                daemon_status: None,
                reindex_report: guard.last_reindex_report(),
            })
        }
        TldrDaemonCommand::Snapshot => {
            let guard = session.lock().await;
            Ok(TldrDaemonResponse {
                snapshot: Some(guard.snapshot()),
                ..TldrDaemonResponse::ok("snapshot")
            })
        }
        TldrDaemonCommand::Status => {
            let guard = session.lock().await;
            let snapshot = guard.snapshot();
            Ok(TldrDaemonResponse {
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
        TldrDaemonCommand::Semantic { request } => {
            let response = engine.semantic_search(request)?;
            Ok(TldrDaemonResponse {
                status: "ok".to_string(),
                message: response.message.clone(),
                analysis: None,
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
        semantic: None,
        snapshot: Some(session.snapshot()),
        daemon_status: None,
        reindex_report: session.last_reindex_report(),
    })
}

fn notify_session_message(session: &mut Session, path: PathBuf) -> String {
    let dirty_state = session.mark_dirty(path);
    if dirty_state.cache_invalidated {
        return format!(
            "dirty threshold reached; invalidated {} cached analyses; reindex pending",
            dirty_state.invalidated_entries
        );
    }
    if dirty_state.reindex_pending {
        return format!(
            "marked dirty ({})；phase-1 reindex pending",
            dirty_state.dirty_files
        );
    }
    format!("marked dirty ({})", dirty_state.dirty_files)
}

fn warm_with_reindex(
    session: &mut Session,
    engine: &TldrEngine,
) -> (String, Option<SemanticReindexReport>) {
    if session.reindex_pending() {
        match engine.semantic_reindex() {
            Ok(report) => {
                session.record_reindex_attempt(report.clone());
                if report.is_completed() {
                    session.complete_reindex(report.clone());
                }
                (report.message.clone(), Some(report))
            }
            Err(err) => {
                let failure = SemanticReindexReport::failed(
                    err.to_string(),
                    engine.config().semantic.embedding_enabled,
                    engine.config().semantic.embedding.dimensions,
                );
                session.record_reindex_attempt(failure.clone());
                (failure.message.clone(), Some(failure))
            }
        }
    } else if let Some(report) = session.last_reindex_attempt_report() {
        (report.message.clone(), Some(report))
    } else {
        ("already warm".to_string(), None)
    }
}

async fn serve_connection<T>(
    stream: T,
    session: Arc<Mutex<Session>>,
    engine: TldrEngine,
) -> Result<()>
where
    T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let (reader, mut writer) = tokio::io::split(stream);
    let mut lines = BufReader::new(reader).lines();

    while let Some(line) = lines.next_line().await? {
        let command: TldrDaemonCommand = serde_json::from_str(&line)?;
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
    }

    Ok(())
}

pub fn socket_path_for_project(project_root: &Path) -> PathBuf {
    daemon_artifact_dir_for_project(project_root).join(format!(
        "codex-native-tldr-{}.sock",
        daemon_project_hash(project_root)
    ))
}

pub fn pid_path_for_project(project_root: &Path) -> PathBuf {
    daemon_artifact_dir_for_project(project_root).join(format!(
        "codex-native-tldr-{}.pid",
        daemon_project_hash(project_root)
    ))
}

pub fn lock_path_for_project(project_root: &Path) -> PathBuf {
    daemon_artifact_dir_for_project(project_root).join(format!(
        "codex-native-tldr-{}.lock",
        daemon_project_hash(project_root)
    ))
}

fn daemon_project_hash(project_root: &Path) -> String {
    let hash = format!(
        "{:x}",
        md5_compute(project_root.to_string_lossy().as_bytes())
    );
    hash[..8].to_string()
}

fn daemon_artifact_dir_for_project(project_root: &Path) -> PathBuf {
    #[cfg(unix)]
    {
        let uid = unsafe { libc::geteuid() };
        if let Some(runtime_dir) = std::env::var_os("XDG_RUNTIME_DIR") {
            let runtime_dir = PathBuf::from(runtime_dir);
            if runtime_dir.is_absolute() {
                return runtime_dir
                    .join("codex-native-tldr")
                    .join(uid.to_string())
                    .join(daemon_project_hash(project_root));
            }
        }
        std::env::temp_dir()
            .join("codex-native-tldr")
            .join(uid.to_string())
            .join(daemon_project_hash(project_root))
    }

    #[cfg(not(unix))]
    {
        std::env::temp_dir()
            .join("codex-native-tldr")
            .join(daemon_project_hash(project_root))
    }
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

fn acquire_daemon_lock(project_root: &Path) -> Result<Option<File>> {
    try_open_daemon_lock(project_root)
}

pub fn daemon_lock_is_held(project_root: &Path) -> Result<bool> {
    Ok(try_open_daemon_lock(project_root)?.is_none())
}

pub fn daemon_health(project_root: &Path) -> Result<DaemonHealth> {
    let socket_exists = socket_path_for_project(project_root).exists();
    let pid_is_live = read_live_pid(&pid_path_for_project(project_root)).unwrap_or(false);
    let lock_is_held = daemon_lock_is_held(project_root)?;
    let healthy = socket_exists && pid_is_live;
    let (health_reason, recovery_hint) = if healthy {
        (None, None)
    } else {
        health_diagnostics(socket_exists, pid_is_live, lock_is_held)
    };
    Ok(DaemonHealth {
        socket_exists,
        pid_is_live,
        lock_is_held,
        healthy,
        stale_socket: socket_exists && !pid_is_live,
        stale_pid: !socket_exists && pid_is_live,
        health_reason,
        recovery_hint,
    })
}

fn health_diagnostics(
    socket_exists: bool,
    pid_is_live: bool,
    lock_is_held: bool,
) -> (Option<String>, Option<String>) {
    if socket_exists && !pid_is_live {
        return (
            Some("stale socket without live daemon".to_string()),
            Some("remove stale socket/pid files and restart the daemon".to_string()),
        );
    }
    if !socket_exists && pid_is_live {
        return (
            Some("pid file exists but socket is missing".to_string()),
            Some("cleanup pid/socket files before restarting the daemon".to_string()),
        );
    }
    if lock_is_held {
        return (
            Some("daemon lock held; another process may be starting it".to_string()),
            Some("wait for the existing daemon or release the lock manually".to_string()),
        );
    }
    (
        Some("daemon unavailable (missing socket and pid)".to_string()),
        Some("start codex-native-tldr-daemon or inspect logs".to_string()),
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
        semantic_reindex_pending: snapshot.reindex_pending,
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

#[cfg(unix)]
pub async fn query_daemon(
    project_root: &Path,
    command: &TldrDaemonCommand,
) -> Result<Option<TldrDaemonResponse>> {
    let socket_path = socket_path_for_project(project_root);
    let pid_path = pid_path_for_project(project_root);
    if !socket_path.exists() {
        return Ok(None);
    }

    let stream = match UnixStream::connect(&socket_path).await {
        Ok(stream) => stream,
        Err(err) if daemon_unavailable(&err) => {
            cleanup_stale_socket(&socket_path);
            cleanup_stale_pid_file(&pid_path);
            return Ok(None);
        }
        Err(err) => {
            return Err(err).with_context(|| format!("connect socket {}", socket_path.display()));
        }
    };

    let (reader, mut writer) = tokio::io::split(stream);
    writer
        .write_all(format!("{}\n", serde_json::to_string(command)?).as_bytes())
        .await
        .with_context(|| format!("write daemon command to {}", socket_path.display()))?;

    let mut lines = BufReader::new(reader).lines();
    let Some(line) = lines
        .next_line()
        .await
        .with_context(|| format!("read daemon response from {}", socket_path.display()))?
    else {
        cleanup_stale_socket(&socket_path);
        cleanup_stale_pid_file(&pid_path);
        return Ok(None);
    };

    let response = serde_json::from_str(&line)
        .with_context(|| format!("decode daemon response from {}", socket_path.display()))?;
    Ok(Some(response))
}

#[cfg(unix)]
fn cleanup_stale_socket(socket_path: &Path) {
    if let Err(err) = std::fs::remove_file(socket_path)
        && err.kind() != std::io::ErrorKind::NotFound
    {
        let _ = err;
    }
}

#[cfg(unix)]
fn cleanup_stale_pid_file(pid_path: &Path) {
    if let Err(err) = std::fs::remove_file(pid_path)
        && err.kind() != std::io::ErrorKind::NotFound
    {
        let _ = err;
    }
}

#[cfg(unix)]
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
    _project_root: &Path,
    _command: &TldrDaemonCommand,
) -> Result<Option<TldrDaemonResponse>> {
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::TldrDaemon;
    use super::TldrDaemonCommand;
    use super::TldrDaemonResponse;
    use super::daemon_health;
    use super::daemon_lock_is_held;
    use super::daemon_project_hash;
    use super::lock_path_for_project;
    use super::query_daemon;
    use pretty_assertions::assert_eq;
    use serial_test::serial;
    use std::fs::OpenOptions;
    use std::path::PathBuf;
    use tempfile::tempdir;
    use tokio::io::AsyncBufReadExt;
    use tokio::io::AsyncWriteExt;
    use tokio::io::BufReader;

    use super::pid_path_for_project;
    #[cfg(unix)]
    use super::socket_path_for_project;
    use crate::semantic::SemanticSearchRequest;
    use crate::semantic::reset_semantic_index_build_count;
    use crate::semantic::semantic_index_build_count;
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

    #[tokio::test]
    async fn notify_invalidates_cached_analyses_when_threshold_is_reached() {
        let mut config =
            crate::TldrConfig::for_project(std::env::temp_dir().join("notify-project"));
        config.session = crate::session::SessionConfig {
            idle_timeout: std::time::Duration::from_secs(60),
            dirty_file_threshold: 1,
        };
        let daemon = TldrDaemon::from_config(config);

        let analyze = daemon
            .handle_command(TldrDaemonCommand::Analyze {
                key: "rust:main".to_string(),
                request: crate::api::AnalysisRequest {
                    kind: crate::api::AnalysisKind::Ast,
                    symbol: Some("main".to_string()),
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
                last_query_at: analyze
                    .snapshot
                    .expect("analyze snapshot should exist")
                    .last_query_at,
                last_reindex: None,
                last_reindex_attempt: None,
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
        let mut config =
            crate::TldrConfig::for_project(std::env::temp_dir().join("pending-reindex-project"));
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
                    symbol: Some("main".to_string()),
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
                    symbol: Some("main".to_string()),
                },
            })
            .await
            .expect("second analyze should succeed");
        let third = daemon
            .handle_command(TldrDaemonCommand::Analyze {
                key: "rust:main".to_string(),
                request: crate::api::AnalysisRequest {
                    kind: crate::api::AnalysisKind::Ast,
                    symbol: Some("main".to_string()),
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

        reset_semantic_index_build_count();

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

        assert_eq!(semantic_index_build_count(), 1);
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
            artifact_dir,
            lock_path.parent().expect("lock path should have parent")
        );
        assert_eq!(
            artifact_dir.file_name().and_then(|value| value.to_str()),
            Some(daemon_project_hash(&project_root).as_str())
        );

        #[cfg(unix)]
        {
            let uid = unsafe { libc::geteuid() }.to_string();
            assert_eq!(
                artifact_dir
                    .parent()
                    .and_then(|value| value.file_name())
                    .and_then(|value| value.to_str()),
                Some(uid.as_str())
            );
            assert_eq!(
                artifact_dir
                    .parent()
                    .and_then(std::path::Path::parent)
                    .and_then(|value| value.file_name())
                    .and_then(|value| value.to_str()),
                Some("codex-native-tldr")
            );
        }

        #[cfg(not(unix))]
        {
            assert_eq!(
                artifact_dir.parent().and_then(|value| value.file_name()),
                Some(std::ffi::OsStr::new("codex-native-tldr"))
            );
        }
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
}
