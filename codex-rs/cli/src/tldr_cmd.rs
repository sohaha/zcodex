use anyhow::Result;
use anyhow::bail;
use clap::Parser;
use clap::Subcommand;
use clap::ValueEnum;
use codex_native_tldr::TldrEngine;
use codex_native_tldr::api::AnalysisKind;
use codex_native_tldr::api::AnalysisRequest;
use codex_native_tldr::daemon::TldrDaemonCommand;
use codex_native_tldr::daemon::pid_path_for_project;
use codex_native_tldr::daemon::query_daemon;
use codex_native_tldr::daemon::socket_path_for_project;
use codex_native_tldr::lang_support::LanguageRegistry;
use codex_native_tldr::lang_support::SupportedLanguage;
use codex_native_tldr::lifecycle::DaemonLifecycleManager;
use codex_utils_cargo_bin::cargo_bin;
use once_cell::sync::Lazy;
use serde_json::json;
use std::future::Future;
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
            let engine = TldrEngine::builder(project_root.clone()).build();
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
    let engine = TldrEngine::builder(cmd.project.canonicalize()?).build();
    let enabled = engine.config().semantic.enabled;
    let payload = json!({
        "project": engine.config().project_root,
        "language": language.as_str(),
        "query": cmd.query,
        "enabled": enabled,
        "message": "semantic search is not enabled in this build yet",
    });

    if cmd.json {
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        println!("language: {}", language.as_str());
        println!("semantic enabled: {enabled}");
        println!("message: semantic search is not enabled in this build yet");
    }

    Ok(())
}

async fn run_daemon_command(cmd: TldrDaemonCli) -> Result<()> {
    let project_root = cmd.project.canonicalize()?;
    let command = match cmd.subcommand {
        TldrDaemonSubcommand::Ping => TldrDaemonCommand::Ping,
        TldrDaemonSubcommand::Warm => TldrDaemonCommand::Warm,
        TldrDaemonSubcommand::Snapshot => TldrDaemonCommand::Snapshot,
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
        println!("status: {}", response.status);
        println!("message: {}", response.message);
        if let Some(snapshot) = response.snapshot {
            println!("cached entries: {}", snapshot.cached_entries);
            println!("dirty files: {}", snapshot.dirty_files);
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

#[cfg(unix)]
async fn try_start_native_tldr_daemon(project_root: &Path) -> Result<bool> {
    let daemon_bin = cargo_bin("codex-native-tldr-daemon")?;
    let mut child = Command::new(daemon_bin)
        .arg("--project")
        .arg(project_root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    tokio::spawn(async move {
        let _ = child.wait().await;
    });

    let start = Instant::now();
    let timeout = Duration::from_secs(3);
    while start.elapsed() < timeout {
        if daemon_metadata_looks_alive(project_root) {
            return Ok(true);
        }
        sleep(Duration::from_millis(50)).await;
    }

    Ok(false)
}

#[cfg(not(unix))]
async fn try_start_native_tldr_daemon(_project_root: &Path) -> Result<bool> {
    Ok(false)
}

fn daemon_metadata_looks_alive(project_root: &Path) -> bool {
    let socket_path = socket_path_for_project(project_root);
    if !socket_path.exists() {
        return false;
    }

    let pid_path = pid_path_for_project(project_root);
    let Ok(pid) = std::fs::read_to_string(&pid_path)
        .map(|content| content.trim().to_string())
        .and_then(|content| content.parse::<i32>().map_err(std::io::Error::other))
    else {
        return false;
    };

    pid_is_alive(pid)
}

fn cleanup_stale_daemon_artifacts(project_root: &Path) {
    cleanup_file_if_exists(socket_path_for_project(project_root));
    cleanup_file_if_exists(pid_path_for_project(project_root));
}

fn cleanup_file_if_exists(path: PathBuf) {
    if let Err(err) = std::fs::remove_file(&path)
        && err.kind() != std::io::ErrorKind::NotFound
    {
        let _ = err;
    }
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

#[cfg(test)]
mod lifecycle_tests {
    use super::cleanup_stale_daemon_artifacts;
    use super::daemon_metadata_looks_alive;
    use super::query_daemon_with_hooks;
    use codex_native_tldr::daemon::TldrDaemonCommand;
    use codex_native_tldr::daemon::TldrDaemonResponse;
    use codex_native_tldr::daemon::pid_path_for_project;
    use codex_native_tldr::daemon::socket_path_for_project;
    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;
    use tempfile::tempdir;

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
}
