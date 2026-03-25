use crate::TldrEngine;
use crate::api::AnalysisRequest;
use crate::api::AnalysisResponse;
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
    Notify {
        path: PathBuf,
    },
    Snapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TldrDaemonResponse {
    pub status: String,
    pub message: String,
    pub analysis: Option<AnalysisResponse>,
    pub snapshot: Option<crate::session::SessionSnapshot>,
}

impl TldrDaemonResponse {
    fn ok(message: impl Into<String>) -> Self {
        Self {
            status: "ok".to_string(),
            message: message.into(),
            analysis: None,
            snapshot: None,
        }
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
        let engine = TldrEngine::builder(project_root.clone()).build();
        let session = Session::new(SessionConfig::default());
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
                session.clear_dirty_files();
                Ok(TldrDaemonResponse {
                    snapshot: Some(session.snapshot()),
                    ..TldrDaemonResponse::ok("warmed")
                })
            }
            TldrDaemonCommand::Analyze { key, request } => {
                let mut session = self.session.lock().await;
                if let Some(cached) = session.cached_analysis(&key).cloned() {
                    return Ok(TldrDaemonResponse {
                        status: "ok".to_string(),
                        message: "cache hit".to_string(),
                        analysis: Some(cached),
                        snapshot: Some(session.snapshot()),
                    });
                }

                let analysis = self.engine.analyze(request)?;
                session.store_analysis(key, analysis.clone());
                Ok(TldrDaemonResponse {
                    status: "ok".to_string(),
                    message: "computed".to_string(),
                    analysis: Some(analysis),
                    snapshot: Some(session.snapshot()),
                })
            }
            TldrDaemonCommand::Notify { path } => {
                let mut session = self.session.lock().await;
                session.mark_dirty(path);
                let message = if session.should_reindex() {
                    "dirty threshold reached"
                } else {
                    "marked dirty"
                };
                Ok(TldrDaemonResponse {
                    status: "ok".to_string(),
                    message: message.to_string(),
                    analysis: None,
                    snapshot: Some(session.snapshot()),
                })
            }
            TldrDaemonCommand::Snapshot => {
                let session = self.session.lock().await;
                Ok(TldrDaemonResponse {
                    snapshot: Some(session.snapshot()),
                    ..TldrDaemonResponse::ok("snapshot")
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
                    let engine = TldrEngine::builder(self.project_root.clone()).build();
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
                    let engine = TldrEngine::builder(self.project_root.clone()).build();
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
    session: &Arc<Mutex<Session>>,
    engine: &TldrEngine,
    command: TldrDaemonCommand,
) -> Result<TldrDaemonResponse> {
    match command {
        TldrDaemonCommand::Ping => Ok(TldrDaemonResponse::ok("pong")),
        TldrDaemonCommand::Warm => {
            let mut guard = session.lock().await;
            guard.clear_dirty_files();
            Ok(TldrDaemonResponse {
                snapshot: Some(guard.snapshot()),
                ..TldrDaemonResponse::ok("warmed")
            })
        }
        TldrDaemonCommand::Analyze { key, request } => {
            let mut guard = session.lock().await;
            if let Some(cached) = guard.cached_analysis(&key).cloned() {
                return Ok(TldrDaemonResponse {
                    status: "ok".to_string(),
                    message: "cache hit".to_string(),
                    analysis: Some(cached),
                    snapshot: Some(guard.snapshot()),
                });
            }
            let analysis = engine.analyze(request)?;
            guard.store_analysis(key, analysis.clone());
            Ok(TldrDaemonResponse {
                status: "ok".to_string(),
                message: "computed".to_string(),
                analysis: Some(analysis),
                snapshot: Some(guard.snapshot()),
            })
        }
        TldrDaemonCommand::Notify { path } => {
            let mut guard = session.lock().await;
            guard.mark_dirty(path);
            Ok(TldrDaemonResponse {
                status: "ok".to_string(),
                message: if guard.should_reindex() {
                    "dirty threshold reached".to_string()
                } else {
                    "marked dirty".to_string()
                },
                analysis: None,
                snapshot: Some(guard.snapshot()),
            })
        }
        TldrDaemonCommand::Snapshot => {
            let guard = session.lock().await;
            Ok(TldrDaemonResponse {
                snapshot: Some(guard.snapshot()),
                ..TldrDaemonResponse::ok("snapshot")
            })
        }
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
        let response = handle_with_session(&session, &engine, command).await?;
        writer
            .write_all(format!("{}\n", serde_json::to_string(&response)?).as_bytes())
            .await?;
    }

    Ok(())
}

pub fn socket_path_for_project(project_root: &Path) -> PathBuf {
    let hash = daemon_project_hash(project_root);
    std::env::temp_dir().join(format!("codex-native-tldr-{hash}.sock"))
}

pub fn pid_path_for_project(project_root: &Path) -> PathBuf {
    let hash = daemon_project_hash(project_root);
    std::env::temp_dir().join(format!("codex-native-tldr-{hash}.pid"))
}

pub fn lock_path_for_project(project_root: &Path) -> PathBuf {
    let hash = daemon_project_hash(project_root);
    std::env::temp_dir().join(format!("codex-native-tldr-{hash}.lock"))
}

fn daemon_project_hash(project_root: &Path) -> String {
    let hash = format!(
        "{:x}",
        md5_compute(project_root.to_string_lossy().as_bytes())
    );
    hash[..8].to_string()
}

fn write_pid_file(pid_path: &Path) -> Result<()> {
    std::fs::write(pid_path, std::process::id().to_string())
        .with_context(|| format!("write pid file {}", pid_path.display()))
}

fn acquire_daemon_lock(project_root: &Path) -> Result<Option<File>> {
    try_open_daemon_lock(project_root)
}

pub fn daemon_lock_is_held(project_root: &Path) -> Result<bool> {
    Ok(try_open_daemon_lock(project_root)?.is_none())
}

fn try_open_daemon_lock(project_root: &Path) -> Result<Option<File>> {
    let lock_path = lock_path_for_project(project_root);
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
    use super::TldrDaemonCommand;
    use super::TldrDaemonResponse;
    use super::daemon_lock_is_held;
    use super::lock_path_for_project;
    use super::query_daemon;
    use pretty_assertions::assert_eq;
    use tempfile::tempdir;
    use tokio::io::AsyncBufReadExt;
    use tokio::io::AsyncWriteExt;
    use tokio::io::BufReader;

    use super::pid_path_for_project;
    #[cfg(unix)]
    use super::socket_path_for_project;
    #[cfg(unix)]
    use tokio::net::UnixListener;

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
                snapshot: None,
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
                snapshot: None,
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
    fn daemon_lock_reports_when_project_lock_is_held() {
        let tempdir = tempdir().expect("tempdir should exist");
        let project_root = tempdir.path().join("lock-project");
        std::fs::create_dir(&project_root).expect("project root should be created");
        let lock_path = lock_path_for_project(&project_root);
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
}
