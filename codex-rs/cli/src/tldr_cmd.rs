use anyhow::Result;
use anyhow::bail;
use clap::Parser;
use clap::Subcommand;
use clap::ValueEnum;
use codex_native_tldr::TldrEngine;
use codex_native_tldr::api::AnalysisKind;
use codex_native_tldr::api::AnalysisRequest;
use codex_native_tldr::daemon::TldrDaemonCommand;
use codex_native_tldr::daemon::daemon_health;
use codex_native_tldr::daemon::daemon_lock_is_held;
use codex_native_tldr::daemon::lock_path_for_project;
use codex_native_tldr::daemon::pid_path_for_project;
use codex_native_tldr::daemon::query_daemon;
use codex_native_tldr::daemon::socket_path_for_project;
use codex_native_tldr::lang_support::LanguageRegistry;
use codex_native_tldr::lang_support::SupportedLanguage;
use codex_native_tldr::lifecycle::DaemonLifecycleManager;
use codex_native_tldr::load_tldr_config;
use codex_native_tldr::semantic::SemanticSearchRequest;
use codex_utils_cargo_bin::cargo_bin;
use once_cell::sync::Lazy;
use serde_json::json;
use std::fs::File;
use std::fs::OpenOptions;
use std::future::Future;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::pin::Pin;
use std::process::Stdio;
use std::time::Duration;
use std::time::Instant;
use tokio::process::Command;
use tokio::time::sleep;

#[derive(Debug, Parser)]
pub struct TldrCli {
    #[command(subcommand)]
    pub subcommand: TldrSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum TldrSubcommand {
    /// 列出当前内建支持的语言。
    Languages,

    /// 获取结构化概览。
    Structure(TldrAnalyzeCommand),

    /// 获取上下文概览。
    Context(TldrAnalyzeCommand),

    /// 触发 semantic 占位入口。
    Semantic(TldrSemanticCommand),

    /// 与 native-tldr daemon 直接交互。
    Daemon(TldrDaemonCli),
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum CliLanguage {
    Rust,
    Typescript,
    Javascript,
    Python,
    Go,
    Php,
    Zig,
}

impl From<CliLanguage> for SupportedLanguage {
    fn from(value: CliLanguage) -> Self {
        match value {
            CliLanguage::Rust => SupportedLanguage::Rust,
            CliLanguage::Typescript => SupportedLanguage::TypeScript,
            CliLanguage::Javascript => SupportedLanguage::JavaScript,
            CliLanguage::Python => SupportedLanguage::Python,
            CliLanguage::Go => SupportedLanguage::Go,
            CliLanguage::Php => SupportedLanguage::Php,
            CliLanguage::Zig => SupportedLanguage::Zig,
        }
    }
}

#[derive(Debug, Parser)]
pub struct TldrAnalyzeCommand {
    /// 项目根目录。
    #[arg(long, default_value = ".")]
    pub project: PathBuf,

    /// 目标语言。
    #[arg(long, value_enum)]
    pub lang: CliLanguage,

    /// 目标符号名。
    #[arg(value_name = "SYMBOL")]
    pub symbol: Option<String>,

    /// 以 JSON 输出。
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

#[derive(Debug, Parser)]
pub struct TldrSemanticCommand {
    /// 项目根目录。
    #[arg(long, default_value = ".")]
    pub project: PathBuf,

    /// 目标语言。
    #[arg(long, value_enum)]
    pub lang: CliLanguage,

    /// 自然语言查询。
    #[arg(value_name = "QUERY")]
    pub query: String,

    /// 以 JSON 输出。
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

#[derive(Debug, Parser)]
pub struct TldrDaemonCli {
    /// 项目根目录。
    #[arg(long, default_value = ".")]
    pub project: PathBuf,

    /// 以 JSON 输出。
    #[arg(long, default_value_t = false)]
    pub json: bool,

    #[command(subcommand)]
    pub subcommand: TldrDaemonSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum TldrDaemonSubcommand {
    /// 检查 daemon 是否在线。
    Ping,

    /// 清空 dirty 文件集合并返回 session 快照。
    Warm,

    /// 返回当前 session 快照。
    Snapshot,

    /// 返回 daemon 健康状态与配置摘要。
    Status,
}

pub async fn run_tldr_command(cli: TldrCli) -> Result<()> {
    match cli.subcommand {
        TldrSubcommand::Languages => {
            for language in [
                SupportedLanguage::Rust,
                SupportedLanguage::TypeScript,
                SupportedLanguage::JavaScript,
                SupportedLanguage::Python,
                SupportedLanguage::Go,
                SupportedLanguage::Php,
                SupportedLanguage::Zig,
            ] {
                println!("{}", language.as_str());
            }
        }
        TldrSubcommand::Structure(cmd) => {
            run_analysis_command(cmd, AnalysisKind::Ast).await?;
        }
        TldrSubcommand::Context(cmd) => {
            run_analysis_command(cmd, AnalysisKind::CallGraph).await?;
        }
        TldrSubcommand::Semantic(cmd) => {
            run_semantic_command(cmd)?;
        }
        TldrSubcommand::Daemon(cmd) => {
            run_daemon_command(cmd).await?;
        }
    }

    Ok(())
}

async fn run_analysis_command(cmd: TldrAnalyzeCommand, kind: AnalysisKind) -> Result<()> {
    let language: SupportedLanguage = cmd.lang.into();
    let project_root = cmd.project.canonicalize()?;
    let config = load_tldr_config(&project_root)?;
    let request = AnalysisRequest {
        kind,
        symbol: cmd.symbol.clone(),
    };
    let daemon_response = query_daemon_with_autostart(
        &project_root,
        &TldrDaemonCommand::Analyze {
            key: analysis_cache_key(kind, language, cmd.symbol.as_deref()),
            request: request.clone(),
        },
    )
    .await?;
    let (source, daemon_message, summary, engine_project_root) =
        if let Some(response) = daemon_response {
            let analysis = response
                .analysis
                .ok_or_else(|| anyhow::anyhow!("daemon response missing analysis payload"))?;
            (
                "daemon",
                Some(response.message),
                analysis.summary,
                project_root.clone(),
            )
        } else {
            let engine = TldrEngine::builder(project_root.clone())
                .with_config(config.clone())
                .build();
            let response = engine.analyze(request)?;
            (
                "local",
                Some("daemon unavailable; used local engine".to_string()),
                response.summary,
                engine.config().project_root.clone(),
            )
        };

    let support = LanguageRegistry::support_for(language);

    if cmd.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "project": engine_project_root,
                "language": language.as_str(),
                "source": source,
                "message": daemon_message,
                "supportLevel": format!("{:?}", support.support_level),
                "fallbackStrategy": support.fallback_strategy,
                "summary": summary,
                "symbol": cmd.symbol,
            }))?
        );
    } else {
        println!("language: {}", language.as_str());
        println!("source: {source}");
        println!("support: {:?}", support.support_level);
        println!("fallback: {}", support.fallback_strategy);
        if let Some(message) = daemon_message {
            println!("message: {message}");
        }
        println!("summary: {summary}");
    }

    Ok(())
}

fn run_semantic_command(cmd: TldrSemanticCommand) -> Result<()> {
    let language: SupportedLanguage = cmd.lang.into();
    let project_root = cmd.project.canonicalize()?;
    let config = load_tldr_config(&project_root)?;
    let engine = TldrEngine::builder(project_root)
        .with_config(config)
        .build();
    let response = engine.semantic_search(SemanticSearchRequest {
        language,
        query: cmd.query,
    })?;
    let payload = json!({
        "project": engine.config().project_root,
        "language": language.as_str(),
        "query": response.query,
        "enabled": response.enabled,
        "indexedFiles": response.indexed_files,
        "truncated": response.truncated,
        "matches": response.matches,
        "message": response.message,
    });

    if cmd.json {
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        println!("language: {}", language.as_str());
        println!("semantic enabled: {}", response.enabled);
        println!("message: {}", response.message);
        println!("matches: {}", response.matches.len());
    }

    Ok(())
}

async fn run_daemon_command(cmd: TldrDaemonCli) -> Result<()> {
    let project_root = cmd.project.canonicalize()?;
    let command = match cmd.subcommand {
        TldrDaemonSubcommand::Ping => TldrDaemonCommand::Ping,
        TldrDaemonSubcommand::Warm => TldrDaemonCommand::Warm,
        TldrDaemonSubcommand::Snapshot => TldrDaemonCommand::Snapshot,
        TldrDaemonSubcommand::Status => TldrDaemonCommand::Status,
    };

    let Some(response) = query_daemon_with_autostart(&project_root, &command).await? else {
        bail!(
            "native-tldr daemon is unavailable for {}",
            project_root.display()
        );
    };

    if cmd.json {
        println!("{}", serde_json::to_string_pretty(&response)?);
    } else {
        let daemon_status = response.daemon_status;
        let reindex_report = response.reindex_report;
        let snapshot = response.snapshot;
        println!("status: {}", response.status);
        println!("message: {}", response.message);
        if let Some(daemon_status) = daemon_status {
            println!("project: {}", daemon_status.project_root.display());
            println!("socket: {}", daemon_status.socket_path.display());
            println!("socket exists: {}", daemon_status.socket_exists);
            println!("pid live: {}", daemon_status.pid_is_live);
            println!("lock held: {}", daemon_status.lock_is_held);
            println!("healthy: {}", daemon_status.healthy);
            println!("stale socket: {}", daemon_status.stale_socket);
            println!("stale pid: {}", daemon_status.stale_pid);
            if let Some(reason) = daemon_status.health_reason.as_deref() {
                println!("health reason: {reason}");
            }
            if let Some(hint) = daemon_status.recovery_hint.as_deref() {
                println!("recovery hint: {hint}");
            }
            println!(
                "session reindex pending: {}",
                snapshot
                    .as_ref()
                    .map(|snapshot| snapshot.reindex_pending)
                    .unwrap_or(false)
            );
            println!(
                "semantic reindex pending: {}",
                daemon_status.semantic_reindex_pending
            );
            if let Some(last_query_at) = daemon_status.last_query_at {
                println!("last query at: {last_query_at:?}");
            }
            println!("auto start: {}", daemon_status.config.auto_start);
            println!("socket mode: {}", daemon_status.config.socket_mode);
            println!(
                "semantic enabled: {}",
                daemon_status.config.semantic_enabled
            );
        }
        if let Some(snapshot) = snapshot {
            println!("cached entries: {}", snapshot.cached_entries);
            println!("dirty files: {}", snapshot.dirty_files);
            println!("dirty threshold: {}", snapshot.dirty_file_threshold);
            println!("reindex pending: {}", snapshot.reindex_pending);
        }
        if let Some(reindex_report) = reindex_report {
            println!("reindex status: {:?}", reindex_report.status);
            println!("reindex files: {}", reindex_report.indexed_files);
            println!("reindex units: {}", reindex_report.indexed_units);
            println!("reindex message: {}", reindex_report.message);
        }
    }

    Ok(())
}

async fn query_daemon_with_autostart(
    project_root: &Path,
    command: &TldrDaemonCommand,
) -> Result<Option<codex_native_tldr::daemon::TldrDaemonResponse>> {
    query_daemon_with_hooks(
        project_root,
        command,
        |project_root, command| Box::pin(query_daemon(project_root, command)),
        |project_root| Box::pin(ensure_daemon_running(project_root)),
    )
    .await
}

type QueryDaemonFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<Option<codex_native_tldr::daemon::TldrDaemonResponse>>>
            + Send
            + 'a,
    >,
>;
type EnsureDaemonFuture<'a> = Pin<Box<dyn Future<Output = Result<bool>> + Send + 'a>>;

static DAEMON_LIFECYCLE_MANAGER: Lazy<DaemonLifecycleManager> =
    Lazy::new(DaemonLifecycleManager::default);

async fn query_daemon_with_hooks<Q, E>(
    project_root: &Path,
    command: &TldrDaemonCommand,
    query: Q,
    ensure_running: E,
) -> Result<Option<codex_native_tldr::daemon::TldrDaemonResponse>>
where
    Q: for<'a> Fn(&'a Path, &'a TldrDaemonCommand) -> QueryDaemonFuture<'a>,
    E: for<'a> Fn(&'a Path) -> EnsureDaemonFuture<'a>,
{
    DAEMON_LIFECYCLE_MANAGER
        .query_or_spawn_with_hooks(project_root, command, query, ensure_running)
        .await
}

fn analysis_cache_key(
    kind: AnalysisKind,
    language: SupportedLanguage,
    symbol: Option<&str>,
) -> String {
    let symbol = symbol.unwrap_or("*");
    format!("{}:{kind:?}:{symbol}", language.as_str())
}

async fn ensure_daemon_running(project_root: &Path) -> Result<bool> {
    DAEMON_LIFECYCLE_MANAGER
        .ensure_running(
            project_root,
            daemon_metadata_looks_alive,
            cleanup_stale_daemon_artifacts,
            |project_root| Box::pin(try_start_native_tldr_daemon(project_root)),
        )
        .await
}

const CODEX_TLDR_TEST_DAEMON_BIN_ENV: &str = "CODEX_TLDR_TEST_DAEMON_BIN";
const CODEX_TLDR_TEST_LAUNCH_COUNTER_ENV: &str = "CODEX_TLDR_TEST_LAUNCH_COUNTER";

fn daemon_launcher_bin_for_tests() -> Result<PathBuf> {
    #[cfg(test)]
    if let Some(path) = std::env::var_os(CODEX_TLDR_TEST_DAEMON_BIN_ENV) {
        return Ok(PathBuf::from(path));
    }
    Ok(cargo_bin("codex-native-tldr-daemon")?)
}

fn record_test_daemon_spawn(project_root: &Path) {
    #[cfg(test)]
    if let Some(path) = std::env::var_os(CODEX_TLDR_TEST_LAUNCH_COUNTER_ENV)
        && let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(PathBuf::from(path))
        {
            let _ = writeln!(file, "{} {}", project_root.display(), std::process::id());
        }
}

#[cfg(unix)]
async fn try_start_native_tldr_daemon(project_root: &Path) -> Result<bool> {
    if daemon_lock_is_held(project_root)? {
        return wait_for_daemon_startup(project_root).await;
    }

    let Some(launcher_lock) = try_open_launcher_lock(project_root)? else {
        return wait_for_daemon_startup(project_root).await;
    };

    if daemon_metadata_looks_alive(project_root) {
        return Ok(true);
    }
    if daemon_lock_is_held(project_root)? {
        return wait_for_daemon_startup_during_launch(project_root).await;
    }

    cleanup_stale_daemon_artifacts(project_root);

    let daemon_bin = daemon_launcher_bin_for_tests()?;
    let mut child = Command::new(daemon_bin)
        .arg("--project")
        .arg(project_root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    record_test_daemon_spawn(project_root);

    tokio::spawn(async move {
        let _ = child.wait().await;
    });

    let started = wait_for_daemon_startup_during_launch(project_root).await;
    drop(launcher_lock);
    started
}

#[cfg(not(unix))]
async fn try_start_native_tldr_daemon(_project_root: &Path) -> Result<bool> {
    Ok(false)
}

async fn wait_for_daemon_startup(project_root: &Path) -> Result<bool> {
    wait_for_daemon_startup_with_launcher_lock(project_root, false).await
}

async fn wait_for_daemon_startup_during_launch(project_root: &Path) -> Result<bool> {
    wait_for_daemon_startup_with_launcher_lock(project_root, true).await
}

async fn wait_for_daemon_startup_with_launcher_lock(
    project_root: &Path,
    ignore_launcher_lock: bool,
) -> Result<bool> {
    let start = Instant::now();
    let timeout = Duration::from_secs(3);
    while start.elapsed() < timeout {
        if daemon_metadata_looks_alive_with_launcher_lock(project_root, ignore_launcher_lock) {
            return Ok(true);
        }
        sleep(Duration::from_millis(50)).await;
    }

    Ok(false)
}

fn daemon_metadata_looks_alive(project_root: &Path) -> bool {
    daemon_metadata_looks_alive_with_launcher_lock(project_root, false)
}

fn daemon_metadata_looks_alive_with_launcher_lock(
    project_root: &Path,
    ignore_launcher_lock: bool,
) -> bool {
    match daemon_health(project_root) {
        Ok(health) => {
            if health.healthy {
                return true;
            }
            if !ignore_launcher_lock && launcher_lock_is_held(project_root).unwrap_or(false) {
                return false;
            }
            if health.should_cleanup_artifacts() {
                cleanup_stale_daemon_artifacts(project_root);
            }
            false
        }
        Err(_) => false,
    }
}

fn cleanup_stale_daemon_artifacts(project_root: &Path) {
    if launcher_lock_is_held(project_root).unwrap_or(false) {
        return;
    }

    let Ok(health) = daemon_health(project_root) else {
        return;
    };
    if !health.should_cleanup_artifacts() {
        return;
    }
    cleanup_file_if_exists(socket_path_for_project(project_root));
    cleanup_file_if_exists(pid_path_for_project(project_root));
}

fn launcher_lock_path_for_project(project_root: &Path) -> PathBuf {
    lock_path_for_project(project_root).with_extension("launch.lock")
}

fn try_open_launcher_lock(project_root: &Path) -> Result<Option<File>> {
    let lock_path = launcher_lock_path_for_project(project_root);
    let lock_file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)?;

    match lock_file.try_lock() {
        Ok(()) => Ok(Some(lock_file)),
        Err(std::fs::TryLockError::WouldBlock) => Ok(None),
        Err(err) => Err(err.into()),
    }
}

fn launcher_lock_is_held(project_root: &Path) -> Result<bool> {
    Ok(try_open_launcher_lock(project_root)?.is_none())
}

fn cleanup_file_if_exists(path: PathBuf) {
    if let Err(err) = std::fs::remove_file(&path)
        && err.kind() != std::io::ErrorKind::NotFound
    {
        let _ = err;
    }
}

#[cfg(test)]
mod lifecycle_tests {
    use super::cleanup_stale_daemon_artifacts;
    use super::daemon_metadata_looks_alive;
    use super::launcher_lock_path_for_project;
    use super::query_daemon_with_autostart;
    use super::query_daemon_with_hooks;
    use crate::tldr_cmd::CODEX_TLDR_TEST_DAEMON_BIN_ENV;
    use anyhow::Result;
    use codex_native_tldr::daemon::TldrDaemonCommand;
    use codex_native_tldr::daemon::TldrDaemonResponse;
    use codex_native_tldr::daemon::pid_path_for_project;
    use codex_native_tldr::daemon::socket_path_for_project;
    use std::path::Path;
    use std::path::PathBuf;
    use std::process::Stdio;
    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;
    use std::thread::sleep;
    use std::time::Duration;
    use std::time::Instant;
    use tempfile::tempdir;
    #[cfg(unix)]
    use tokio::io::AsyncBufReadExt;
    #[cfg(unix)]
    use tokio::io::AsyncWriteExt;
    #[cfg(unix)]
    use tokio::io::BufReader;
    #[cfg(unix)]
    use tokio::net::UnixListener;

    const CODEX_TLDR_TEST_PROJECT_ROOT_ENV: &str = "CODEX_TLDR_TEST_PROJECT_ROOT";
    const CODEX_TLDR_TEST_START_SIGNAL_ENV: &str = "CODEX_TLDR_TEST_START_SIGNAL";
    const CODEX_TLDR_TEST_DONE_SIGNAL_ENV: &str = "CODEX_TLDR_TEST_DONE_SIGNAL";
    const CODEX_TLDR_TEST_LAUNCH_COUNTER_ENV: &str = "CODEX_TLDR_TEST_LAUNCH_COUNTER";
    const CODEX_TLDR_TEST_FAKE_DAEMON_PROJECT_ROOT_ENV: &str =
        "CODEX_TLDR_TEST_FAKE_DAEMON_PROJECT_ROOT";
    const CODEX_TLDR_TEST_FAKE_DAEMON_RELEASE_ENV: &str = "CODEX_TLDR_TEST_FAKE_DAEMON_RELEASE";

    #[tokio::test]
    async fn query_daemon_with_hooks_retries_after_autostart() {
        let tempdir = tempdir().unwrap();
        let command = TldrDaemonCommand::Ping;
        let query_calls = Arc::new(AtomicUsize::new(0));
        let ensure_calls = Arc::new(AtomicUsize::new(0));

        let query_response = TldrDaemonResponse {
            status: "ok".to_string(),
            message: "pong".to_string(),
            analysis: None,
            snapshot: None,
            daemon_status: None,
            reindex_report: None,
        };

        let response = query_daemon_with_hooks(
            tempdir.path(),
            &command,
            {
                let query_calls = Arc::clone(&query_calls);
                let query_response = query_response.clone();
                move |_project_root, _command| {
                    let query_calls = Arc::clone(&query_calls);
                    let query_response = query_response.clone();
                    Box::pin(async move {
                        let call_index = query_calls.fetch_add(1, Ordering::SeqCst);
                        Ok(if call_index == 0 {
                            None
                        } else {
                            Some(query_response)
                        })
                    })
                }
            },
            {
                let ensure_calls = Arc::clone(&ensure_calls);
                move |_project_root| {
                    let ensure_calls = Arc::clone(&ensure_calls);
                    Box::pin(async move {
                        ensure_calls.fetch_add(1, Ordering::SeqCst);
                        Ok(true)
                    })
                }
            },
        )
        .await
        .unwrap();

        assert_eq!(response, Some(query_response));
        assert_eq!(query_calls.load(Ordering::SeqCst), 2);
        assert_eq!(ensure_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn query_daemon_with_hooks_skips_retry_when_autostart_fails() {
        let tempdir = tempdir().unwrap();
        let command = TldrDaemonCommand::Ping;
        let query_calls = Arc::new(AtomicUsize::new(0));
        let ensure_calls = Arc::new(AtomicUsize::new(0));

        let response = query_daemon_with_hooks(
            tempdir.path(),
            &command,
            {
                let query_calls = Arc::clone(&query_calls);
                move |_project_root, _command| {
                    let query_calls = Arc::clone(&query_calls);
                    Box::pin(async move {
                        query_calls.fetch_add(1, Ordering::SeqCst);
                        Ok(None)
                    })
                }
            },
            {
                let ensure_calls = Arc::clone(&ensure_calls);
                move |_project_root| {
                    let ensure_calls = Arc::clone(&ensure_calls);
                    Box::pin(async move {
                        ensure_calls.fetch_add(1, Ordering::SeqCst);
                        Ok(false)
                    })
                }
            },
        )
        .await
        .unwrap();

        assert_eq!(response, None);
        assert_eq!(query_calls.load(Ordering::SeqCst), 1);
        assert_eq!(ensure_calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn daemon_metadata_requires_live_pid_and_socket() {
        let tempdir = tempdir().unwrap();
        let project_root = tempdir.path();
        let socket_path = socket_path_for_project(project_root);
        let pid_path = pid_path_for_project(project_root);

        std::fs::write(&socket_path, "").unwrap();
        std::fs::write(&pid_path, std::process::id().to_string()).unwrap();

        assert!(daemon_metadata_looks_alive(project_root));

        std::fs::write(&pid_path, "999999").unwrap();
        assert!(!daemon_metadata_looks_alive(project_root));
    }

    #[test]
    fn daemon_metadata_cleans_stale_socket_and_pid() {
        let tempdir = tempdir().unwrap();
        let project_root = tempdir.path().join("stale-clean-project");
        std::fs::create_dir(&project_root).unwrap();
        let socket_path = socket_path_for_project(&project_root);
        let pid_path = pid_path_for_project(&project_root);

        std::fs::write(&socket_path, "").unwrap();
        std::fs::write(&pid_path, "999999").unwrap();

        assert!(!daemon_metadata_looks_alive(&project_root));
        assert!(!socket_path.exists());
        assert!(!pid_path.exists());
    }

    #[test]
    fn cleanup_stale_daemon_artifacts_removes_socket_and_pid() {
        let tempdir = tempdir().unwrap();
        let project_root = tempdir.path();
        let socket_path = socket_path_for_project(project_root);
        let pid_path = pid_path_for_project(project_root);

        std::fs::write(&socket_path, "").unwrap();
        std::fs::write(&pid_path, "123").unwrap();

        cleanup_stale_daemon_artifacts(project_root);

        assert!(!socket_path.exists());
        assert!(!pid_path.exists());
    }

    #[test]
    fn cleanup_stale_daemon_artifacts_keeps_files_while_lock_is_held() {
        let tempdir = tempdir().unwrap();
        let project_root = tempdir.path();
        let socket_path = socket_path_for_project(project_root);
        let pid_path = pid_path_for_project(project_root);
        let lock_path = codex_native_tldr::daemon::lock_path_for_project(project_root);
        let lock_file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)
            .unwrap();

        lock_file.try_lock().unwrap();
        std::fs::write(&socket_path, "").unwrap();
        std::fs::write(&pid_path, "999999").unwrap();

        cleanup_stale_daemon_artifacts(project_root);

        assert!(socket_path.exists());
        assert!(pid_path.exists());
    }

    #[test]
    fn cleanup_stale_daemon_artifacts_keeps_files_while_launcher_lock_is_held() {
        let tempdir = tempdir().unwrap();
        let project_root = tempdir.path();
        let socket_path = socket_path_for_project(project_root);
        let pid_path = pid_path_for_project(project_root);
        let lock_path = launcher_lock_path_for_project(project_root);
        let lock_file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)
            .unwrap();

        lock_file.try_lock().unwrap();
        std::fs::write(&socket_path, "").unwrap();
        std::fs::write(&pid_path, "999999").unwrap();

        cleanup_stale_daemon_artifacts(project_root);

        assert!(socket_path.exists());
        assert!(pid_path.exists());
    }

    #[test]
    fn daemon_metadata_keeps_stale_files_while_launcher_lock_is_held() {
        let tempdir = tempdir().unwrap();
        let project_root = tempdir.path();
        let socket_path = socket_path_for_project(project_root);
        let pid_path = pid_path_for_project(project_root);
        let lock_path = launcher_lock_path_for_project(project_root);
        let lock_file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)
            .unwrap();

        lock_file.try_lock().unwrap();
        std::fs::write(&socket_path, "").unwrap();
        std::fs::write(&pid_path, "999999").unwrap();

        assert!(!daemon_metadata_looks_alive(project_root));
        assert!(socket_path.exists());
        assert!(pid_path.exists());
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ensure_daemon_running_only_spawns_once_across_processes() -> Result<()> {
        if std::env::var_os(CODEX_TLDR_TEST_PROJECT_ROOT_ENV).is_some() {
            return Ok(());
        }

        let project = tempdir()?;
        let canonical_project = project.path().canonicalize()?;
        let start_signal = project.path().join("start_signal");
        let counter_path = project.path().join("launch_counter.log");
        let fake_daemon_release = project.path().join("fake_daemon.release");
        let fake_daemon_bin = project.path().join("fake_daemon.sh");
        let done_paths = [
            project.path().join("child0.done"),
            project.path().join("child1.done"),
        ];
        std::fs::remove_file(&start_signal).ok();
        std::fs::remove_file(&counter_path).ok();
        std::fs::remove_file(&fake_daemon_release).ok();
        for done in &done_paths {
            std::fs::remove_file(done).ok();
        }
        write_fake_daemon_wrapper(
            std::env::current_exe()?.as_path(),
            &fake_daemon_bin,
            &fake_daemon_release,
        )?;

        let mut children = Vec::new();
        for done in &done_paths {
            let child = std::process::Command::new(std::env::current_exe()?)
                .arg("--exact")
                .arg("tldr_cmd::lifecycle_tests::cross_process_launcher_contender")
                .env(CODEX_TLDR_TEST_PROJECT_ROOT_ENV, &canonical_project)
                .env(CODEX_TLDR_TEST_START_SIGNAL_ENV, &start_signal)
                .env(CODEX_TLDR_TEST_DONE_SIGNAL_ENV, done)
                .env(CODEX_TLDR_TEST_LAUNCH_COUNTER_ENV, &counter_path)
                .env(CODEX_TLDR_TEST_DAEMON_BIN_ENV, &fake_daemon_bin)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;
            children.push((child, done.clone()));
        }

        std::fs::write(&start_signal, "go")?;
        for (_, done) in &children {
            wait_for_signal(done);
        }

        for (mut child, _) in children {
            let status = child.wait()?;
            assert!(status.success());
        }

        let spawn_count = std::fs::read_to_string(&counter_path)?.lines().count();
        assert_eq!(spawn_count, 1, "only one daemon spawn is allowed");

        assert!(socket_path_for_project(&canonical_project).exists());
        assert!(pid_path_for_project(&canonical_project).exists());

        std::fs::write(&fake_daemon_release, "release")?;
        cleanup_stale_daemon_artifacts(&canonical_project);
        Ok(())
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn cross_process_launcher_contender() -> Result<()> {
        let project_root = match std::env::var_os(CODEX_TLDR_TEST_PROJECT_ROOT_ENV) {
            Some(path) => PathBuf::from(path),
            None => return Ok(()),
        };
        let start_signal = PathBuf::from(
            std::env::var(CODEX_TLDR_TEST_START_SIGNAL_ENV).expect("start signal env should exist"),
        );
        let done_signal = PathBuf::from(
            std::env::var(CODEX_TLDR_TEST_DONE_SIGNAL_ENV).expect("done signal env should exist"),
        );

        wait_for_signal(&start_signal);

        let response = query_daemon_with_autostart(&project_root, &TldrDaemonCommand::Ping)
            .await?
            .expect("daemon should return response");

        assert_eq!(response.message, "pong");

        std::fs::write(&done_signal, "done")?;
        Ok(())
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "current_thread")]
    async fn fake_daemon_process() -> Result<()> {
        let Some(project_root) = std::env::var_os(CODEX_TLDR_TEST_FAKE_DAEMON_PROJECT_ROOT_ENV)
        else {
            return Ok(());
        };
        let project_root = PathBuf::from(project_root);
        let release_signal = PathBuf::from(
            std::env::var(CODEX_TLDR_TEST_FAKE_DAEMON_RELEASE_ENV)
                .expect("fake daemon release env should exist"),
        );
        let socket_path = socket_path_for_project(&project_root);
        let pid_path = pid_path_for_project(&project_root);
        std::fs::remove_file(&socket_path).ok();
        std::fs::remove_file(&pid_path).ok();

        let listener = UnixListener::bind(&socket_path)?;
        std::fs::write(&pid_path, std::process::id().to_string())?;

        loop {
            if release_signal.exists() {
                break;
            }

            if let Ok(accept_result) =
                tokio::time::timeout(Duration::from_millis(50), listener.accept()).await
            {
                let (stream, _) = accept_result?;
                let (reader, mut writer) = tokio::io::split(stream);
                let mut lines = BufReader::new(reader).lines();
                if let Some(line) = lines.next_line().await? {
                    let command: TldrDaemonCommand = serde_json::from_str(&line)?;
                    let message = match command {
                        TldrDaemonCommand::Ping => "pong",
                        TldrDaemonCommand::Warm => "warm",
                        TldrDaemonCommand::Snapshot => "snapshot",
                        TldrDaemonCommand::Status => "status",
                        TldrDaemonCommand::Analyze { .. } => "analyze",
                        TldrDaemonCommand::Notify { .. } => "notify",
                    };
                    let response = TldrDaemonResponse {
                        status: "ok".to_string(),
                        message: message.to_string(),
                        analysis: None,
                        snapshot: None,
                        daemon_status: None,
                        reindex_report: None,
                    };
                    writer
                        .write_all(format!("{}\n", serde_json::to_string(&response)?).as_bytes())
                        .await?;
                }
            }
        }

        std::fs::remove_file(&socket_path).ok();
        std::fs::remove_file(&pid_path).ok();
        Ok(())
    }

    #[cfg(unix)]
    fn write_fake_daemon_wrapper(
        current_exe: &Path,
        script_path: &Path,
        release_signal: &Path,
    ) -> Result<()> {
        let script = format!(
            "#!/bin/sh\nexport {project_env}=\"$2\"\nexport {release_env}=\"{release}\"\nexec \"{exe}\" --exact tldr_cmd::lifecycle_tests::fake_daemon_process --nocapture\n",
            project_env = CODEX_TLDR_TEST_FAKE_DAEMON_PROJECT_ROOT_ENV,
            release_env = CODEX_TLDR_TEST_FAKE_DAEMON_RELEASE_ENV,
            release = release_signal.display(),
            exe = current_exe.display(),
        );
        std::fs::write(script_path, script)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = std::fs::metadata(script_path)?.permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(script_path, permissions)?;
        }
        Ok(())
    }

    fn wait_for_signal(path: &Path) {
        let deadline = Instant::now() + Duration::from_secs(15);
        while !path.exists() && Instant::now() < deadline {
            sleep(Duration::from_millis(10));
        }
        assert!(path.exists(), "timed out waiting for {}", path.display());
    }
}
