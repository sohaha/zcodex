use anyhow::Result;
use anyhow::bail;
use clap::Parser;
use clap::Subcommand;
use clap::ValueEnum;
use codex_native_tldr::TldrConfig;
use codex_native_tldr::TldrEngine;
use codex_native_tldr::api::AnalysisKind;
use codex_native_tldr::api::AnalysisRequest;
use codex_native_tldr::api::AnalysisResponse;
use codex_native_tldr::daemon::TldrDaemon;
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
use codex_native_tldr::semantic::SemanticSearchResponse;
use codex_native_tldr::wire::daemon_response_payload;
use codex_native_tldr::wire::semantic_payload;
use once_cell::sync::Lazy;
use serde_json::json;
use std::ffi::OsString;
use std::fs::File;
use std::fs::OpenOptions;
use std::future::Future;
#[cfg(test)]
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

    /// 运行语义检索。
    Semantic(TldrSemanticCommand),

    /// 与 native-tldr daemon 直接交互。
    Daemon(TldrDaemonCli),

    /// 内部：运行 native-tldr daemon 服务。
    #[command(hide = true, name = "internal-daemon")]
    InternalDaemon(TldrInternalDaemonCli),
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

#[derive(Debug, Parser)]
pub struct TldrInternalDaemonCli {
    /// 项目根目录。
    #[arg(long, default_value = ".")]
    pub project: PathBuf,
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
            run_semantic_command(cmd).await?;
        }
        TldrSubcommand::Daemon(cmd) => {
            run_daemon_command(cmd).await?;
        }
        TldrSubcommand::InternalDaemon(cmd) => {
            run_internal_daemon_command(cmd).await?;
        }
    }

    Ok(())
}

async fn run_internal_daemon_command(cmd: TldrInternalDaemonCli) -> Result<()> {
    let project_root = cmd.project.canonicalize()?;
    let daemon = TldrDaemon::from_config(load_tldr_config(&project_root)?);
    daemon.run_until_shutdown().await
}

async fn run_analysis_command(cmd: TldrAnalyzeCommand, kind: AnalysisKind) -> Result<()> {
    let language: SupportedLanguage = cmd.lang.into();
    let project_root = cmd.project.canonicalize()?;
    let config = load_tldr_config(&project_root)?;
    let request = AnalysisRequest {
        kind,
        language,
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
    let (source, daemon_message, analysis, engine_project_root) =
        if let Some(response) = daemon_response {
            let analysis = response
                .analysis
                .ok_or_else(|| anyhow::anyhow!("daemon response missing analysis payload"))?;
            (
                "daemon",
                Some(response.message),
                analysis,
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
                response,
                engine.config().project_root.clone(),
            )
        };

    let support = LanguageRegistry::support_for(language);
    let payload = analysis_payload(
        &engine_project_root,
        language,
        source,
        daemon_message.as_deref(),
        support,
        cmd.symbol.as_deref(),
        &analysis,
    );

    if cmd.json {
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        println!("language: {}", language.as_str());
        println!("source: {source}");
        println!("support: {:?}", support.support_level);
        println!("fallback: {}", support.fallback_strategy);
        if let Some(message) = daemon_message {
            println!("message: {message}");
        }
        println!("summary: {}", analysis.summary);
    }

    Ok(())
}

async fn run_semantic_command(cmd: TldrSemanticCommand) -> Result<()> {
    let language: SupportedLanguage = cmd.lang.into();
    let project_root = cmd.project.canonicalize()?;
    let config = load_tldr_config(&project_root)?;
    let request = SemanticSearchRequest {
        language,
        query: cmd.query.clone(),
    };
    let daemon_response = query_daemon_with_autostart(
        &project_root,
        &TldrDaemonCommand::Semantic {
            request: request.clone(),
        },
    )
    .await?;
    let (source, response, engine_project_root) = if let Some(response) = daemon_response {
        if let Some(semantic) = response.semantic {
            ("daemon", semantic, project_root.clone())
        } else {
            let (local_response, local_root) =
                run_local_semantic_search(&project_root, config.clone(), request.clone())?;
            ("local", local_response, local_root)
        }
    } else {
        let (local_response, local_root) =
            run_local_semantic_search(&project_root, config, request.clone())?;
        ("local", local_response, local_root)
    };
    let payload = cli_semantic_payload(&engine_project_root, language, source, &response);

    if cmd.json {
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        for line in render_semantic_response_text(language, source, &response) {
            println!("{line}");
        }
    }

    Ok(())
}

fn run_local_semantic_search(
    project_root: &Path,
    config: TldrConfig,
    request: SemanticSearchRequest,
) -> Result<(SemanticSearchResponse, PathBuf)> {
    let engine = TldrEngine::builder(project_root.to_path_buf())
        .with_config(config)
        .build();
    let response = engine.semantic_search(request)?;
    Ok((response, engine.config().project_root.clone()))
}

fn analysis_payload(
    project_root: &Path,
    language: SupportedLanguage,
    source: &str,
    message: Option<&str>,
    support: &codex_native_tldr::lang_support::LanguageSupport,
    symbol: Option<&str>,
    response: &AnalysisResponse,
) -> serde_json::Value {
    json!({
        "project": project_root,
        "language": language.as_str(),
        "source": source,
        "message": message,
        "supportLevel": format!("{:?}", support.support_level),
        "fallbackStrategy": support.fallback_strategy,
        "summary": response.summary.clone(),
        "symbol": symbol,
        "analysis": response,
    })
}

fn cli_semantic_payload(
    project_root: &Path,
    language: SupportedLanguage,
    source: &str,
    response: &SemanticSearchResponse,
) -> serde_json::Value {
    let mut payload = semantic_payload(None, project_root, language, source, response);
    let semantic = semantic_result_payload(&payload);
    if let Some(object) = payload.as_object_mut() {
        object.insert("semantic".to_string(), semantic);
    }
    payload
}

fn semantic_result_payload(payload: &serde_json::Value) -> serde_json::Value {
    let mut semantic = payload.clone();
    if let Some(object) = semantic.as_object_mut() {
        for key in ["action", "project", "language", "source"] {
            object.remove(key);
        }
    }
    semantic
}

fn render_semantic_response_text(
    language: SupportedLanguage,
    source: &str,
    response: &SemanticSearchResponse,
) -> Vec<String> {
    let mut lines = vec![
        format!("language: {}", language.as_str()),
        format!("source: {source}"),
        format!("query: {}", response.query),
        format!("semantic enabled: {}", response.enabled),
        format!("indexed files: {}", response.indexed_files),
        format!("truncated: {}", response.truncated),
        format!("message: {}", response.message),
        format!("embedding used: {}", response.embedding_used),
        format!("matches: {}", response.matches.len()),
    ];
    for (index, semantic_match) in response.matches.iter().enumerate() {
        let score = semantic_match
            .embedding_score
            .map(|value| format!("{value:.3}"))
            .unwrap_or_else(|| "n/a".to_string());
        lines.push(format!(
            "match {index}: {}:{} (embedding score: {score})",
            semantic_match.path.display(),
            semantic_match.line
        ));
        lines.push(format!(
            "  snippet: {}",
            semantic_match.snippet.replace('\n', "\\n")
        ));
    }
    lines
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
        if matches!(cmd.subcommand, TldrDaemonSubcommand::Status) {
            println!(
                "{}",
                serde_json::to_string_pretty(&daemon_response_payload(
                    "status",
                    &project_root,
                    &response,
                ))?
            );
        } else {
            println!("{}", serde_json::to_string_pretty(&response)?);
        }
    } else {
        for line in render_daemon_response_text(&response) {
            println!("{line}");
        }
    }

    Ok(())
}

fn render_daemon_response_text(
    response: &codex_native_tldr::daemon::TldrDaemonResponse,
) -> Vec<String> {
    let daemon_status = response.daemon_status.as_ref();
    let reindex_report = response.reindex_report.as_ref();
    let snapshot = response.snapshot.as_ref();
    let mut lines = vec![
        format!("status: {}", response.status),
        format!("message: {}", response.message),
    ];

    if let Some(daemon_status) = daemon_status {
        lines.push(format!("project: {}", daemon_status.project_root.display()));
        lines.push(format!("socket: {}", daemon_status.socket_path.display()));
        lines.push(format!("socket exists: {}", daemon_status.socket_exists));
        lines.push(format!("pid live: {}", daemon_status.pid_is_live));
        lines.push(format!("lock held: {}", daemon_status.lock_is_held));
        lines.push(format!("healthy: {}", daemon_status.healthy));
        lines.push(format!("stale socket: {}", daemon_status.stale_socket));
        lines.push(format!("stale pid: {}", daemon_status.stale_pid));
        if let Some(reason) = daemon_status.health_reason.as_deref() {
            lines.push(format!("health reason: {reason}"));
        }
        if let Some(hint) = daemon_status.recovery_hint.as_deref() {
            lines.push(format!("recovery hint: {hint}"));
        }
        lines.push(format!(
            "session reindex pending: {}",
            snapshot
                .map(|snapshot| snapshot.reindex_pending)
                .unwrap_or(false)
        ));
        lines.push(format!(
            "semantic reindex pending: {}",
            daemon_status.semantic_reindex_pending
        ));
        if let Some(last_query_at) = daemon_status.last_query_at {
            lines.push(format!("last query at: {last_query_at:?}"));
        }
        lines.push(format!("auto start: {}", daemon_status.config.auto_start));
        lines.push(format!("socket mode: {}", daemon_status.config.socket_mode));
        lines.push(format!(
            "semantic enabled: {}",
            daemon_status.config.semantic_enabled
        ));
    }
    if let Some(snapshot) = snapshot {
        lines.push(format!("cached entries: {}", snapshot.cached_entries));
        lines.push(format!("dirty files: {}", snapshot.dirty_files));
        lines.push(format!(
            "dirty threshold: {}",
            snapshot.dirty_file_threshold
        ));
        lines.push(format!("reindex pending: {}", snapshot.reindex_pending));
        if let Some(last_reindex) = snapshot.last_reindex.as_ref() {
            lines.push(format!("last completed reindex: {:?}", last_reindex.status));
        }
        if let Some(last_reindex_attempt) = snapshot.last_reindex_attempt.as_ref() {
            lines.push(format!(
                "last reindex attempt: {:?}",
                last_reindex_attempt.status
            ));
        }
    }
    if let Some(reindex_report) = reindex_report {
        lines.push(format!("reindex status: {:?}", reindex_report.status));
        lines.push(format!("reindex files: {}", reindex_report.indexed_files));
        lines.push(format!("reindex units: {}", reindex_report.indexed_units));
        lines.push(format!("reindex message: {}", reindex_report.message));
    }

    lines
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

#[cfg(test)]
const CODEX_TLDR_TEST_DAEMON_BIN_ENV: &str = "CODEX_TLDR_TEST_DAEMON_BIN";
#[cfg(test)]
const CODEX_TLDR_TEST_LAUNCH_COUNTER_ENV: &str = "CODEX_TLDR_TEST_LAUNCH_COUNTER";
#[cfg(test)]
const CODEX_TLDR_TEST_LAUNCHER_WAIT_COUNTER_ENV: &str = "CODEX_TLDR_TEST_LAUNCHER_WAIT_COUNTER";

fn daemon_launcher_command(project_root: &Path) -> Result<Command> {
    #[cfg(test)]
    if let Some(path) = std::env::var_os(CODEX_TLDR_TEST_DAEMON_BIN_ENV) {
        let mut command = Command::new(PathBuf::from(path));
        command.arg("--project").arg(project_root);
        return Ok(command);
    }

    let current_exe = std::env::current_exe()?;
    let mut command = Command::new(current_exe);
    command.args(daemon_launcher_args(project_root));
    Ok(command)
}

fn daemon_launcher_args(project_root: &Path) -> [OsString; 4] {
    [
        OsString::from("tldr"),
        OsString::from("internal-daemon"),
        OsString::from("--project"),
        project_root.as_os_str().to_os_string(),
    ]
}

fn record_test_daemon_spawn(_project_root: &Path) {
    #[cfg(test)]
    if let Some(path) = std::env::var_os(CODEX_TLDR_TEST_LAUNCH_COUNTER_ENV)
        && let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(PathBuf::from(path))
    {
        let _ = writeln!(file, "{} {}", _project_root.display(), std::process::id());
    }
}

fn record_test_launcher_wait(_project_root: &Path) {
    #[cfg(test)]
    if let Some(path) = std::env::var_os(CODEX_TLDR_TEST_LAUNCHER_WAIT_COUNTER_ENV)
        && let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(PathBuf::from(path))
    {
        let _ = writeln!(file, "{} {}", _project_root.display(), std::process::id());
    }
}

#[cfg(unix)]
async fn try_start_native_tldr_daemon(project_root: &Path) -> Result<bool> {
    if daemon_lock_is_held(project_root)? {
        return wait_for_daemon_startup(project_root).await;
    }

    let Some(launcher_lock) = try_open_launcher_lock(project_root)? else {
        record_test_launcher_wait(project_root);
        return wait_for_daemon_startup(project_root).await;
    };

    if daemon_metadata_looks_alive(project_root) {
        return Ok(true);
    }
    if daemon_lock_is_held(project_root)? {
        return wait_for_daemon_startup_during_launch(project_root).await;
    }

    cleanup_stale_daemon_artifacts(project_root);

    let mut child = daemon_launcher_command(project_root)?
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
    if let Some(parent) = lock_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
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
mod output_tests {
    use super::analysis_payload;
    use super::cli_semantic_payload;
    use super::render_semantic_response_text;
    use codex_native_tldr::api::AnalysisDetail;
    use codex_native_tldr::api::AnalysisEdgeDetail;
    use codex_native_tldr::api::AnalysisFileDetail;
    use codex_native_tldr::api::AnalysisKind;
    use codex_native_tldr::api::AnalysisNodeDetail;
    use codex_native_tldr::api::AnalysisOverviewDetail;
    use codex_native_tldr::api::AnalysisResponse;
    use codex_native_tldr::api::AnalysisSymbolIndexEntry;
    use codex_native_tldr::api::AnalysisUnitDetail;
    use codex_native_tldr::lang_support::LanguageRegistry;
    use codex_native_tldr::lang_support::SupportedLanguage;
    use codex_native_tldr::semantic::EmbeddingUnit;
    use codex_native_tldr::semantic::SemanticMatch;
    use codex_native_tldr::semantic::SemanticSearchResponse;
    use pretty_assertions::assert_eq;
    use std::path::Path;
    use std::path::PathBuf;

    #[test]
    fn analysis_payload_includes_nested_native_response() {
        let payload = analysis_payload(
            Path::new("/tmp/project"),
            SupportedLanguage::Rust,
            "daemon",
            Some("daemon summary ready"),
            LanguageRegistry::support_for(SupportedLanguage::Rust),
            Some("main"),
            &AnalysisResponse {
                kind: AnalysisKind::CallGraph,
                summary: "context summary".to_string(),
                details: Some(AnalysisDetail {
                    indexed_files: 1,
                    total_symbols: 1,
                    symbol_query: Some("main".to_string()),
                    truncated: false,
                    overview: AnalysisOverviewDetail {
                        kinds: vec![codex_native_tldr::api::AnalysisCountDetail {
                            name: "function".to_string(),
                            count: 1,
                        }],
                        outgoing_edges: 1,
                        incoming_edges: 0,
                        reference_count: 0,
                        import_count: 0,
                    },
                    files: vec![AnalysisFileDetail {
                        path: "src/main.rs".to_string(),
                        symbol_count: 1,
                        kinds: vec![codex_native_tldr::api::AnalysisCountDetail {
                            name: "function".to_string(),
                            count: 1,
                        }],
                    }],
                    nodes: vec![AnalysisNodeDetail {
                        id: "main".to_string(),
                        label: "main".to_string(),
                        kind: "function".to_string(),
                        path: Some("src/main.rs".to_string()),
                        line: Some(1),
                        signature: Some("fn main()".to_string()),
                    }],
                    edges: vec![AnalysisEdgeDetail {
                        from: "src/main.rs".to_string(),
                        to: "main".to_string(),
                        kind: "contains".to_string(),
                    }],
                    symbol_index: vec![AnalysisSymbolIndexEntry {
                        symbol: "main".to_string(),
                        node_ids: vec!["main".to_string()],
                    }],
                    units: vec![AnalysisUnitDetail {
                        path: "src/main.rs".to_string(),
                        line: 1,
                        span_end_line: 3,
                        symbol: Some("main".to_string()),
                        qualified_symbol: Some("crate::main".to_string()),
                        kind: "function".to_string(),
                        module_path: vec!["crate".to_string()],
                        visibility: None,
                        signature: Some("fn main()".to_string()),
                        calls: vec!["validate".to_string()],
                        called_by: Vec::new(),
                        references: Vec::new(),
                        imports: Vec::new(),
                        dependencies: Vec::new(),
                        cfg_summary: "cfg".to_string(),
                        dfg_summary: "dfg".to_string(),
                    }],
                }),
            },
        );

        assert_eq!(
            payload,
            serde_json::json!({
                "project": "/tmp/project",
                "language": "rust",
                "source": "daemon",
                "message": "daemon summary ready",
                "supportLevel": "DataFlow",
                "fallbackStrategy": "structure + search",
                "summary": "context summary",
                "symbol": "main",
                "analysis": {
                    "kind": "call_graph",
                    "summary": "context summary",
                    "details": {
                        "indexed_files": 1,
                        "total_symbols": 1,
                        "symbol_query": "main",
                        "truncated": false,
                        "overview": {
                            "kinds": [{"name": "function", "count": 1}],
                            "outgoing_edges": 1,
                            "incoming_edges": 0,
                            "reference_count": 0,
                            "import_count": 0
                        },
                        "files": [{
                            "path": "src/main.rs",
                            "symbol_count": 1,
                            "kinds": [{"name": "function", "count": 1}]
                        }],
                        "nodes": [{
                            "id": "main",
                            "label": "main",
                            "kind": "function",
                            "path": "src/main.rs",
                            "line": 1,
                            "signature": "fn main()"
                        }],
                        "edges": [{
                            "from": "src/main.rs",
                            "to": "main",
                            "kind": "contains"
                        }],
                        "symbol_index": [{
                            "symbol": "main",
                            "node_ids": ["main"]
                        }],
                        "units": [{
                            "path": "src/main.rs",
                            "line": 1,
                            "span_end_line": 3,
                            "symbol": "main",
                            "qualified_symbol": "crate::main",
                            "kind": "function",
                            "module_path": ["crate"],
                            "visibility": null,
                            "signature": "fn main()",
                            "calls": ["validate"],
                            "called_by": [],
                            "references": [],
                            "imports": [],
                            "dependencies": [],
                            "cfg_summary": "cfg",
                            "dfg_summary": "dfg"
                        }]
                    }
                }
            })
        );
    }

    #[test]
    fn cli_semantic_payload_wraps_public_semantic_result() {
        let payload = cli_semantic_payload(
            Path::new("/tmp/project"),
            SupportedLanguage::Rust,
            "local",
            &SemanticSearchResponse {
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
                        span_end_line: 1,
                        module_path: vec!["auth".to_string()],
                        visibility: Some("pub".to_string()),
                        signature: Some("pub fn verify_token()".to_string()),
                        docs: vec!["Validates auth token".to_string()],
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
            },
        );

        assert_eq!(payload["semantic"]["query"], "auth token");
        assert_eq!(payload["semantic"]["embeddingUsed"], true);
        assert_eq!(payload["semantic"]["matches"][0]["path"], "src/auth.rs");
        assert!(payload["semantic"]["matches"][0].get("unit").is_none());
        assert!(
            payload["semantic"]["matches"][0]
                .get("embedding_text")
                .is_none()
        );
    }

    #[test]
    fn render_semantic_response_text_lists_match_details() {
        let lines = render_semantic_response_text(
            SupportedLanguage::Rust,
            "daemon",
            &SemanticSearchResponse {
                enabled: true,
                query: "auth token".to_string(),
                indexed_files: 1,
                truncated: false,
                matches: vec![SemanticMatch {
                    score: 7,
                    path: PathBuf::from("src/auth.rs"),
                    line: 2,
                    snippet: "let auth_token = true;\nverify();".to_string(),
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
                        span_end_line: 1,
                        module_path: vec!["auth".to_string()],
                        visibility: Some("pub".to_string()),
                        signature: Some("pub fn verify_token()".to_string()),
                        docs: vec!["Validates auth token".to_string()],
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
            },
        );

        assert!(lines.contains(&"query: auth token".to_string()));
        assert!(lines.contains(&"indexed files: 1".to_string()));
        assert!(lines.contains(&"match 0: src/auth.rs:2 (embedding score: 0.750)".to_string()));
        assert!(lines.contains(&"  snippet: let auth_token = true;\\nverify();".to_string()));
    }
}

#[cfg(test)]
mod lifecycle_tests {
    use super::cleanup_stale_daemon_artifacts;
    use super::daemon_launcher_args;
    use super::daemon_metadata_looks_alive;
    use super::ensure_daemon_running;
    use super::launcher_lock_path_for_project;
    use super::query_daemon_with_autostart;
    use super::query_daemon_with_hooks;
    use super::render_daemon_response_text;
    use super::try_start_native_tldr_daemon;
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
    use std::sync::Mutex;
    use std::sync::MutexGuard;
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

    fn create_artifact_parent(path: &Path) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("artifact parent should be created");
        }
    }

    #[test]
    fn daemon_launcher_args_target_internal_daemon_mode() -> Result<()> {
        let temp = tempdir()?;
        let project_root = temp.path().join("project");
        let args = daemon_launcher_args(&project_root);

        assert_eq!(
            args,
            [
                "tldr".into(),
                "internal-daemon".into(),
                "--project".into(),
                project_root.as_os_str().to_os_string(),
            ]
        );
        Ok(())
    }

    const CODEX_TLDR_TEST_PROJECT_ROOT_ENV: &str = "CODEX_TLDR_TEST_PROJECT_ROOT";
    const CODEX_TLDR_TEST_START_SIGNAL_ENV: &str = "CODEX_TLDR_TEST_START_SIGNAL";
    const CODEX_TLDR_TEST_DONE_SIGNAL_ENV: &str = "CODEX_TLDR_TEST_DONE_SIGNAL";
    const CODEX_TLDR_TEST_LAUNCH_COUNTER_ENV: &str = "CODEX_TLDR_TEST_LAUNCH_COUNTER";
    const CODEX_TLDR_TEST_LAUNCHER_WAIT_COUNTER_ENV: &str = "CODEX_TLDR_TEST_LAUNCHER_WAIT_COUNTER";
    const CODEX_TLDR_TEST_FAKE_DAEMON_PROJECT_ROOT_ENV: &str =
        "CODEX_TLDR_TEST_FAKE_DAEMON_PROJECT_ROOT";
    const CODEX_TLDR_TEST_FAKE_DAEMON_RELEASE_ENV: &str = "CODEX_TLDR_TEST_FAKE_DAEMON_RELEASE";
    const CODEX_TLDR_TEST_FAKE_DAEMON_BOOT_RELEASE_ENV: &str =
        "CODEX_TLDR_TEST_FAKE_DAEMON_BOOT_RELEASE";
    const CODEX_TLDR_TEST_FAKE_DAEMON_SPAWNED_SIGNAL_ENV: &str =
        "CODEX_TLDR_TEST_FAKE_DAEMON_SPAWNED_SIGNAL";
    const CODEX_TLDR_TEST_CONTENDER_ENTERED_SIGNAL_ENV: &str =
        "CODEX_TLDR_TEST_CONTENDER_ENTERED_SIGNAL";
    const CODEX_TLDR_TEST_EXTERNAL_DAEMON_LOCKED_SIGNAL_ENV: &str =
        "CODEX_TLDR_TEST_EXTERNAL_DAEMON_LOCKED_SIGNAL";

    static LIFECYCLE_TEST_LOCK: Mutex<()> = Mutex::new(());

    struct LifecycleTestGuard {
        _local: MutexGuard<'static, ()>,
        _cross_process: std::fs::File,
    }

    fn lifecycle_test_guard() -> LifecycleTestGuard {
        let local = match LIFECYCLE_TEST_LOCK.lock() {
            Ok(guard) => guard,
            Err(err) => panic!("lifecycle test lock should not be poisoned: {err}"),
        };
        let lock_path = std::env::temp_dir().join("codex-native-tldr-artifact-tests.lock");
        let cross_process = match std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
        {
            Ok(file) => file,
            Err(err) => panic!("cross-process lifecycle test lock should be opened: {err}"),
        };
        if let Err(err) = cross_process.lock() {
            panic!("cross-process lifecycle test lock should be acquired: {err}");
        }
        LifecycleTestGuard {
            _local: local,
            _cross_process: cross_process,
        }
    }

    #[test]
    fn render_daemon_response_text_surfaces_last_reindex_attempts() {
        let started_at = std::time::SystemTime::UNIX_EPOCH;
        let finished_at = started_at + Duration::from_secs(1);
        let response = TldrDaemonResponse {
            status: "ok".to_string(),
            message: "status".to_string(),
            analysis: None,
            semantic: None,
            snapshot: Some(codex_native_tldr::session::SessionSnapshot {
                cached_entries: 3,
                dirty_files: 1,
                dirty_file_threshold: 20,
                reindex_pending: true,
                last_query_at: None,
                last_reindex: Some(codex_native_tldr::semantic::SemanticReindexReport {
                    status: codex_native_tldr::semantic::SemanticReindexStatus::Completed,
                    languages: vec![codex_native_tldr::lang_support::SupportedLanguage::Rust],
                    indexed_files: 2,
                    indexed_units: 4,
                    truncated: false,
                    started_at,
                    finished_at,
                    message: "done".to_string(),
                    embedding_enabled: false,
                    embedding_dimensions: 0,
                }),
                last_reindex_attempt: Some(codex_native_tldr::semantic::SemanticReindexReport {
                    status: codex_native_tldr::semantic::SemanticReindexStatus::Failed,
                    languages: vec![codex_native_tldr::lang_support::SupportedLanguage::Rust],
                    indexed_files: 2,
                    indexed_units: 4,
                    truncated: false,
                    started_at,
                    finished_at,
                    message: "boom".to_string(),
                    embedding_enabled: false,
                    embedding_dimensions: 0,
                }),
            }),
            daemon_status: None,
            reindex_report: None,
        };

        let output = render_daemon_response_text(&response).join("\n");

        assert!(output.contains("last completed reindex: Completed"));
        assert!(output.contains("last reindex attempt: Failed"));
    }

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
            semantic: None,
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
        create_artifact_parent(&socket_path);
        create_artifact_parent(&pid_path);

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
        create_artifact_parent(&socket_path);
        create_artifact_parent(&pid_path);

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
        create_artifact_parent(&socket_path);
        create_artifact_parent(&pid_path);

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
        create_artifact_parent(&socket_path);
        create_artifact_parent(&pid_path);
        create_artifact_parent(&lock_path);
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
        create_artifact_parent(&socket_path);
        create_artifact_parent(&pid_path);
        create_artifact_parent(&lock_path);
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
    fn try_open_launcher_lock_creates_lock_file_in_shared_scope_dir() {
        let _guard = lifecycle_test_guard();
        let tempdir = tempdir().unwrap();
        let project_root = tempdir.path().join("nested-launcher-lock-project");
        std::fs::create_dir(&project_root).unwrap();
        let lock_path = launcher_lock_path_for_project(&project_root);
        let parent = lock_path.parent().unwrap().to_path_buf();
        std::fs::create_dir_all(&parent).unwrap();
        std::fs::remove_file(&lock_path).ok();
        assert!(parent.exists());
        assert!(!lock_path.exists());

        let lock = super::try_open_launcher_lock(&project_root).unwrap();

        assert!(lock.is_some());
        assert!(parent.exists());
        assert!(lock_path.exists());
    }

    #[test]
    fn try_open_launcher_lock_recovers_after_lock_file_is_deleted() {
        let tempdir = tempdir().unwrap();
        let project_root = tempdir.path().join("deleted-launcher-lock-project");
        std::fs::create_dir(&project_root).unwrap();
        let lock_path = launcher_lock_path_for_project(&project_root);
        create_artifact_parent(&lock_path);
        std::fs::write(&lock_path, "").unwrap();
        std::fs::remove_file(&lock_path).unwrap();
        assert!(!lock_path.exists());

        let lock = super::try_open_launcher_lock(&project_root).unwrap();

        assert!(lock.is_some());
        assert!(lock_path.exists());
    }

    #[test]
    fn launcher_lock_is_held_recovers_after_lock_file_is_deleted() {
        let tempdir = tempdir().unwrap();
        let project_root = tempdir.path().join("deleted-launcher-lock-state-project");
        std::fs::create_dir(&project_root).unwrap();
        let lock_path = launcher_lock_path_for_project(&project_root);
        create_artifact_parent(&lock_path);
        std::fs::write(&lock_path, "").unwrap();
        std::fs::remove_file(&lock_path).unwrap();
        assert!(!lock_path.exists());

        let lock_is_held = super::launcher_lock_is_held(&project_root).unwrap();

        assert!(!lock_is_held);
        assert!(lock_path.exists());
    }

    #[test]
    fn daemon_metadata_keeps_stale_files_while_launcher_lock_is_held() {
        let tempdir = tempdir().unwrap();
        let project_root = tempdir.path();
        let socket_path = socket_path_for_project(project_root);
        let pid_path = pid_path_for_project(project_root);
        let lock_path = launcher_lock_path_for_project(project_root);
        create_artifact_parent(&socket_path);
        create_artifact_parent(&pid_path);
        create_artifact_parent(&lock_path);
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
        let _guard = lifecycle_test_guard();
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
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ensure_daemon_running_only_spawns_once_with_two_launcher_contenders() -> Result<()> {
        let _guard = lifecycle_test_guard();
        if std::env::var_os(CODEX_TLDR_TEST_PROJECT_ROOT_ENV).is_some() {
            return Ok(());
        }

        let project = tempdir()?;
        let canonical_project = project.path().canonicalize()?;
        let start_signal = project.path().join("start_signal_launcher");
        let counter_path = project.path().join("launch_counter_launcher.log");
        let fake_daemon_release = project.path().join("fake_daemon_launcher.release");
        let fake_daemon_bin = project.path().join("fake_daemon_launcher.sh");
        let done_paths = [
            project.path().join("launcher_child0.done"),
            project.path().join("launcher_child1.done"),
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
                .arg("tldr_cmd::lifecycle_tests::cross_process_ensure_running_contender")
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
        assert_eq!(
            spawn_count, 1,
            "only one launcher contender should spawn the daemon"
        );

        assert!(socket_path_for_project(&canonical_project).exists());
        assert!(pid_path_for_project(&canonical_project).exists());

        std::fs::write(&fake_daemon_release, "release")?;
        cleanup_stale_daemon_artifacts(&canonical_project);
        Ok(())
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn ensure_running_records_launcher_wait_in_two_process_race() -> Result<()> {
        let _guard = lifecycle_test_guard();
        if std::env::var_os(CODEX_TLDR_TEST_PROJECT_ROOT_ENV).is_some() {
            return Ok(());
        }

        let project = tempdir()?;
        let canonical_project = project.path().canonicalize()?;
        let start_signal = project.path().join("start_signal_direct");
        let counter_path = project.path().join("launch_counter_direct.log");
        let launcher_wait_counter = project.path().join("launcher_wait_counter_direct.log");
        let fake_daemon_release = project.path().join("fake_daemon_direct.release");
        let fake_daemon_boot_release = project.path().join("fake_daemon_direct.boot_release");
        let fake_daemon_spawned = project.path().join("fake_daemon_direct.spawned");
        let fake_daemon_bin = project.path().join("fake_daemon_direct.sh");
        let done_paths = [
            project.path().join("direct_child0.done"),
            project.path().join("direct_child1.done"),
        ];
        let entered_paths = [
            project.path().join("direct_child0.entered"),
            project.path().join("direct_child1.entered"),
        ];
        std::fs::remove_file(&start_signal).ok();
        std::fs::remove_file(&counter_path).ok();
        std::fs::remove_file(&launcher_wait_counter).ok();
        std::fs::remove_file(&fake_daemon_release).ok();
        std::fs::remove_file(&fake_daemon_boot_release).ok();
        std::fs::remove_file(&fake_daemon_spawned).ok();
        for path in done_paths.iter().chain(entered_paths.iter()) {
            std::fs::remove_file(path).ok();
        }
        write_fake_daemon_wrapper(
            std::env::current_exe()?.as_path(),
            &fake_daemon_bin,
            &fake_daemon_release,
        )?;

        let mut children = Vec::new();
        for (done, entered) in done_paths.iter().zip(entered_paths.iter()) {
            let child = std::process::Command::new(std::env::current_exe()?)
                .arg("--exact")
                .arg("tldr_cmd::lifecycle_tests::cross_process_ensure_running_contender")
                .env(CODEX_TLDR_TEST_PROJECT_ROOT_ENV, &canonical_project)
                .env(CODEX_TLDR_TEST_START_SIGNAL_ENV, &start_signal)
                .env(CODEX_TLDR_TEST_DONE_SIGNAL_ENV, done)
                .env(CODEX_TLDR_TEST_CONTENDER_ENTERED_SIGNAL_ENV, entered)
                .env(CODEX_TLDR_TEST_LAUNCH_COUNTER_ENV, &counter_path)
                .env(
                    CODEX_TLDR_TEST_LAUNCHER_WAIT_COUNTER_ENV,
                    &launcher_wait_counter,
                )
                .env(CODEX_TLDR_TEST_DAEMON_BIN_ENV, &fake_daemon_bin)
                .env(
                    CODEX_TLDR_TEST_FAKE_DAEMON_BOOT_RELEASE_ENV,
                    &fake_daemon_boot_release,
                )
                .env(
                    CODEX_TLDR_TEST_FAKE_DAEMON_SPAWNED_SIGNAL_ENV,
                    &fake_daemon_spawned,
                )
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;
            children.push(child);
        }

        std::fs::write(&start_signal, "go")?;
        for entered in &entered_paths {
            wait_for_signal(entered);
        }
        wait_for_signal(&fake_daemon_spawned);
        std::fs::write(&fake_daemon_boot_release, "boot")?;
        for done in &done_paths {
            wait_for_signal(done);
        }

        for mut child in children {
            let status = child.wait()?;
            assert!(status.success());
        }

        let spawn_count = std::fs::read_to_string(&counter_path)?.lines().count();
        assert_eq!(spawn_count, 1, "only one daemon spawn is allowed");
        let launcher_wait_count = std::fs::read_to_string(&launcher_wait_counter)?
            .lines()
            .count();
        assert_eq!(
            launcher_wait_count, 1,
            "exactly one contender should wait on the launcher lock"
        );
        assert!(socket_path_for_project(&canonical_project).exists());
        assert!(pid_path_for_project(&canonical_project).exists());

        std::fs::write(&fake_daemon_release, "release")?;
        cleanup_stale_daemon_artifacts(&canonical_project);
        Ok(())
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ensure_running_never_spawns_when_external_daemon_lock_owner_finishes_boot()
    -> Result<()> {
        let _guard = lifecycle_test_guard();
        if std::env::var_os(CODEX_TLDR_TEST_PROJECT_ROOT_ENV).is_some() {
            return Ok(());
        }

        let project = tempdir()?;
        let canonical_project = project.path().canonicalize()?;
        let start_signal = project.path().join("start_signal_external_lock");
        let done_signal = project.path().join("child_external_lock.done");
        let counter_path = project.path().join("launch_counter_external_lock.log");
        let launcher_wait_counter = project.path().join("launcher_wait_external_lock.log");
        let external_release = project.path().join("external_daemon.release");
        let external_boot_release = project.path().join("external_daemon.boot_release");
        let external_locked_signal = project.path().join("external_daemon.locked");
        for path in [
            &start_signal,
            &done_signal,
            &counter_path,
            &launcher_wait_counter,
            &external_release,
            &external_boot_release,
            &external_locked_signal,
        ] {
            std::fs::remove_file(path).ok();
        }

        let mut external_daemon = std::process::Command::new(std::env::current_exe()?)
            .arg("--exact")
            .arg("tldr_cmd::lifecycle_tests::external_daemon_lock_owner_process")
            .env(CODEX_TLDR_TEST_PROJECT_ROOT_ENV, &canonical_project)
            .env(CODEX_TLDR_TEST_FAKE_DAEMON_RELEASE_ENV, &external_release)
            .env(
                CODEX_TLDR_TEST_FAKE_DAEMON_BOOT_RELEASE_ENV,
                &external_boot_release,
            )
            .env(
                CODEX_TLDR_TEST_EXTERNAL_DAEMON_LOCKED_SIGNAL_ENV,
                &external_locked_signal,
            )
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;

        wait_for_signal(&external_locked_signal);

        let mut contender = std::process::Command::new(std::env::current_exe()?)
            .arg("--exact")
            .arg("tldr_cmd::lifecycle_tests::cross_process_launcher_contender")
            .env(CODEX_TLDR_TEST_PROJECT_ROOT_ENV, &canonical_project)
            .env(CODEX_TLDR_TEST_START_SIGNAL_ENV, &start_signal)
            .env(CODEX_TLDR_TEST_DONE_SIGNAL_ENV, &done_signal)
            .env(CODEX_TLDR_TEST_LAUNCH_COUNTER_ENV, &counter_path)
            .env(
                CODEX_TLDR_TEST_LAUNCHER_WAIT_COUNTER_ENV,
                &launcher_wait_counter,
            )
            .env(CODEX_TLDR_TEST_DAEMON_BIN_ENV, canonical_project.as_path())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;

        std::fs::write(&start_signal, "go")?;
        std::thread::sleep(Duration::from_millis(200));
        assert!(
            !counter_path.exists(),
            "no daemon spawn should happen while external daemon lock owner is booting"
        );
        assert!(
            !launcher_wait_counter.exists(),
            "daemon-lock wait should not be counted as launcher-lock wait"
        );

        std::fs::write(&external_boot_release, "boot")?;
        wait_for_signal(&done_signal);

        let contender_status = contender.wait()?;
        assert!(contender_status.success());
        assert!(
            !counter_path.exists(),
            "external daemon owner must keep spawn count at zero"
        );
        assert!(
            !launcher_wait_counter.exists(),
            "external daemon owner path should bypass launcher wait tracking"
        );
        std::fs::write(&external_release, "release")?;
        let external_status = tokio::time::timeout(
            Duration::from_secs(5),
            tokio::task::spawn_blocking(move || external_daemon.wait()),
        )
        .await???;
        assert!(external_status.success());

        cleanup_stale_daemon_artifacts(&canonical_project);
        Ok(())
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ensure_running_recovers_when_external_lock_owner_loses_artifact_dir_mid_boot()
    -> Result<()> {
        let _guard = lifecycle_test_guard();
        if std::env::var_os(CODEX_TLDR_TEST_PROJECT_ROOT_ENV).is_some() {
            return Ok(());
        }

        let project = tempdir()?;
        let canonical_project = project.path().canonicalize()?;
        let start_signal = project.path().join("start_signal_external_lock_recovery");
        let done_signal = project.path().join("child_external_lock_recovery.done");
        let counter_path = project
            .path()
            .join("launch_counter_external_lock_recovery.log");
        let launcher_wait_counter = project
            .path()
            .join("launcher_wait_external_lock_recovery.log");
        let external_release = project.path().join("external_daemon_recovery.release");
        let external_boot_release = project.path().join("external_daemon_recovery.boot_release");
        let external_locked_signal = project.path().join("external_daemon_recovery.locked");
        for path in [
            &start_signal,
            &done_signal,
            &counter_path,
            &launcher_wait_counter,
            &external_release,
            &external_boot_release,
            &external_locked_signal,
        ] {
            std::fs::remove_file(path).ok();
        }

        let mut external_daemon = std::process::Command::new(std::env::current_exe()?)
            .arg("--exact")
            .arg("tldr_cmd::lifecycle_tests::external_daemon_lock_owner_process")
            .env(CODEX_TLDR_TEST_PROJECT_ROOT_ENV, &canonical_project)
            .env(CODEX_TLDR_TEST_FAKE_DAEMON_RELEASE_ENV, &external_release)
            .env(
                CODEX_TLDR_TEST_FAKE_DAEMON_BOOT_RELEASE_ENV,
                &external_boot_release,
            )
            .env(
                CODEX_TLDR_TEST_EXTERNAL_DAEMON_LOCKED_SIGNAL_ENV,
                &external_locked_signal,
            )
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;

        wait_for_signal(&external_locked_signal);
        let artifact_dir = socket_path_for_project(&canonical_project)
            .parent()
            .expect("socket path should have parent")
            .to_path_buf();
        assert!(
            artifact_dir.exists(),
            "lock owner should create artifact dir"
        );

        let mut contender = std::process::Command::new(std::env::current_exe()?)
            .arg("--exact")
            .arg("tldr_cmd::lifecycle_tests::cross_process_launcher_contender")
            .env(CODEX_TLDR_TEST_PROJECT_ROOT_ENV, &canonical_project)
            .env(CODEX_TLDR_TEST_START_SIGNAL_ENV, &start_signal)
            .env(CODEX_TLDR_TEST_DONE_SIGNAL_ENV, &done_signal)
            .env(CODEX_TLDR_TEST_LAUNCH_COUNTER_ENV, &counter_path)
            .env(
                CODEX_TLDR_TEST_LAUNCHER_WAIT_COUNTER_ENV,
                &launcher_wait_counter,
            )
            .env(CODEX_TLDR_TEST_DAEMON_BIN_ENV, canonical_project.as_path())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;

        std::fs::write(&start_signal, "go")?;
        std::fs::remove_dir_all(&artifact_dir)?;
        assert!(
            !artifact_dir.exists(),
            "artifact dir should be removed before boot resumes"
        );

        std::fs::write(&external_boot_release, "boot")?;
        wait_for_signal(&done_signal);

        let contender_status = contender.wait()?;
        assert!(contender_status.success());
        assert!(
            !counter_path.exists(),
            "external daemon owner should still avoid any extra spawn"
        );
        assert!(
            !launcher_wait_counter.exists(),
            "daemon-lock wait should still bypass launcher wait tracking"
        );
        assert!(socket_path_for_project(&canonical_project).exists());
        assert!(pid_path_for_project(&canonical_project).exists());

        std::fs::write(&external_release, "release")?;
        let external_status = tokio::time::timeout(
            Duration::from_secs(5),
            tokio::task::spawn_blocking(move || external_daemon.wait()),
        )
        .await???;
        assert!(external_status.success());

        cleanup_stale_daemon_artifacts(&canonical_project);
        Ok(())
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn ensure_daemon_running_only_spawns_once_even_with_three_processes() -> Result<()> {
        let _guard = lifecycle_test_guard();
        if std::env::var_os(CODEX_TLDR_TEST_PROJECT_ROOT_ENV).is_some() {
            return Ok(());
        }

        let project = tempdir()?;
        let canonical_project = project.path().canonicalize()?;
        let start_signal = project.path().join("start_signal_three");
        let counter_path = project.path().join("launch_counter_three.log");
        let fake_daemon_release = project.path().join("fake_daemon_three.release");
        let fake_daemon_bin = project.path().join("fake_daemon_three.sh");
        let done_paths = [
            project.path().join("child0.done"),
            project.path().join("child1.done"),
            project.path().join("child2.done"),
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
        for (index, done) in done_paths.iter().enumerate() {
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
            children.push((child, done.clone(), index));
        }

        std::fs::write(&start_signal, "go")?;
        for (_, done, _) in &children {
            wait_for_signal(done);
        }

        for (mut child, _, _) in children {
            let status = child.wait()?;
            assert!(status.success());
        }

        let spawn_count = std::fs::read_to_string(&counter_path)?.lines().count();
        assert_eq!(
            spawn_count, 1,
            "only one daemon spawn is allowed even with three contenders"
        );

        assert!(socket_path_for_project(&canonical_project).exists());
        assert!(pid_path_for_project(&canonical_project).exists());

        std::fs::write(&fake_daemon_release, "release")?;
        cleanup_stale_daemon_artifacts(&canonical_project);
        Ok(())
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn ensure_running_waits_when_launcher_lock_and_daemon_alive() -> Result<()> {
        let _guard = lifecycle_test_guard();
        let project = tempdir()?;
        let canonical_project = project.path().canonicalize()?;
        let socket_path = socket_path_for_project(&canonical_project);
        let pid_path = pid_path_for_project(&canonical_project);
        create_artifact_parent(&socket_path);
        create_artifact_parent(&pid_path);
        std::fs::write(&socket_path, "").unwrap();
        std::fs::write(&pid_path, std::process::id().to_string()).unwrap();

        let lock_path = launcher_lock_path_for_project(&canonical_project);
        create_artifact_parent(&lock_path);
        let lock_file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)?;
        lock_file.try_lock().unwrap();

        let counter_path = project.path().join("launcher_lock_counter.log");
        unsafe { std::env::set_var(CODEX_TLDR_TEST_DAEMON_BIN_ENV, canonical_project.as_path()) };
        unsafe { std::env::set_var(CODEX_TLDR_TEST_LAUNCH_COUNTER_ENV, &counter_path) };

        let started = ensure_daemon_running(&canonical_project).await?;
        assert!(started);
        assert!(!counter_path.exists(), "launcher lock should prevent spawn");

        unsafe { std::env::remove_var(CODEX_TLDR_TEST_DAEMON_BIN_ENV) };
        unsafe { std::env::remove_var(CODEX_TLDR_TEST_LAUNCH_COUNTER_ENV) };
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
    #[tokio::test]
    async fn cross_process_ensure_running_contender() -> Result<()> {
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
        let entered_signal =
            std::env::var_os(CODEX_TLDR_TEST_CONTENDER_ENTERED_SIGNAL_ENV).map(PathBuf::from);

        wait_for_signal(&start_signal);
        if let Some(entered_signal) = entered_signal {
            std::fs::write(entered_signal, "entered")?;
        }

        let started = ensure_daemon_running(&project_root).await?;
        assert!(started, "launcher contender should observe a live daemon");
        assert!(daemon_metadata_looks_alive(&project_root));

        std::fs::write(&done_signal, "done")?;
        Ok(())
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn try_start_daemon_does_not_spawn_while_daemon_lock_is_held() -> Result<()> {
        let project = tempdir()?;
        let canonical_project = project.path().canonicalize()?;
        let counter_path = project.path().join("launch_counter.log");
        create_artifact_parent(&codex_native_tldr::daemon::lock_path_for_project(
            &canonical_project,
        ));
        let daemon_lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(codex_native_tldr::daemon::lock_path_for_project(
                &canonical_project,
            ))?;
        daemon_lock.try_lock()?;

        let started = tokio::time::timeout(
            Duration::from_secs(4),
            try_start_native_tldr_daemon(&canonical_project),
        )
        .await??;

        assert!(!started);
        assert!(!counter_path.exists(), "daemon should not be spawned");
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
        let boot_release =
            std::env::var_os(CODEX_TLDR_TEST_FAKE_DAEMON_BOOT_RELEASE_ENV).map(PathBuf::from);
        let spawned_signal =
            std::env::var_os(CODEX_TLDR_TEST_FAKE_DAEMON_SPAWNED_SIGNAL_ENV).map(PathBuf::from);
        let socket_path = socket_path_for_project(&project_root);
        let pid_path = pid_path_for_project(&project_root);
        create_artifact_parent(&socket_path);
        create_artifact_parent(&pid_path);
        std::fs::remove_file(&socket_path).ok();
        std::fs::remove_file(&pid_path).ok();

        if let Some(path) = &spawned_signal {
            std::fs::write(path, std::process::id().to_string())?;
        }
        if let Some(boot_release) = boot_release {
            while !boot_release.exists() && !release_signal.exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            if release_signal.exists() {
                return Ok(());
            }
        }

        create_artifact_parent(&socket_path);
        let listener = UnixListener::bind(&socket_path)?;
        create_artifact_parent(&pid_path);
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
                        TldrDaemonCommand::Semantic { .. } => "semantic",
                        TldrDaemonCommand::Notify { .. } => "notify",
                    };
                    let response = TldrDaemonResponse {
                        status: "ok".to_string(),
                        message: message.to_string(),
                        analysis: None,
                        semantic: None,
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
    #[tokio::test(flavor = "current_thread")]
    async fn external_daemon_lock_owner_process() -> Result<()> {
        let Some(project_root) = std::env::var_os(CODEX_TLDR_TEST_PROJECT_ROOT_ENV) else {
            return Ok(());
        };
        let project_root = PathBuf::from(project_root);
        let release_signal = PathBuf::from(
            std::env::var(CODEX_TLDR_TEST_FAKE_DAEMON_RELEASE_ENV)
                .expect("external daemon release env should exist"),
        );
        let boot_release = PathBuf::from(
            std::env::var(CODEX_TLDR_TEST_FAKE_DAEMON_BOOT_RELEASE_ENV)
                .expect("external daemon boot release env should exist"),
        );
        let locked_signal = PathBuf::from(
            std::env::var(CODEX_TLDR_TEST_EXTERNAL_DAEMON_LOCKED_SIGNAL_ENV)
                .expect("external daemon locked signal env should exist"),
        );
        let socket_path = socket_path_for_project(&project_root);
        let pid_path = pid_path_for_project(&project_root);
        create_artifact_parent(&socket_path);
        create_artifact_parent(&pid_path);
        std::fs::remove_file(&socket_path).ok();
        std::fs::remove_file(&pid_path).ok();

        create_artifact_parent(&codex_native_tldr::daemon::lock_path_for_project(
            &project_root,
        ));
        let daemon_lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(codex_native_tldr::daemon::lock_path_for_project(
                &project_root,
            ))?;
        daemon_lock.try_lock()?;
        std::fs::write(&locked_signal, "locked")?;

        while !boot_release.exists() && !release_signal.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        if release_signal.exists() {
            return Ok(());
        }

        create_artifact_parent(&socket_path);
        let listener = UnixListener::bind(&socket_path)?;
        create_artifact_parent(&pid_path);
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
                        TldrDaemonCommand::Semantic { .. } => "semantic",
                        TldrDaemonCommand::Notify { .. } => "notify",
                    };
                    let response = TldrDaemonResponse {
                        status: "ok".to_string(),
                        message: message.to_string(),
                        analysis: None,
                        semantic: None,
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
