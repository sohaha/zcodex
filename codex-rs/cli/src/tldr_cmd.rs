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
use codex_native_tldr::api::DiagnosticsRequest;
use codex_native_tldr::api::DiagnosticsResponse;
use codex_native_tldr::api::DoctorRequest;
use codex_native_tldr::api::DoctorResponse;
use codex_native_tldr::api::SearchMatchMode;
use codex_native_tldr::api::SearchRequest;
use codex_native_tldr::daemon::TldrDaemon;
use codex_native_tldr::daemon::TldrDaemonCommand;
use codex_native_tldr::daemon::daemon_health;
use codex_native_tldr::daemon::daemon_lock_is_held;
use codex_native_tldr::daemon::launch_lock_path_for_project as native_launch_lock_path_for_project;
use codex_native_tldr::daemon::pid_path_for_project;
use codex_native_tldr::daemon::query_daemon;
use codex_native_tldr::daemon::read_live_pid;
use codex_native_tldr::daemon::socket_path_for_project;
use codex_native_tldr::lang_support::LanguageRegistry;
use codex_native_tldr::lang_support::SupportedLanguage;
use codex_native_tldr::lifecycle::DaemonLifecycleManager;
use codex_native_tldr::lifecycle::DaemonReadyResult;
use codex_native_tldr::lifecycle::QueryHooksResult;
use codex_native_tldr::load_tldr_config;
use codex_native_tldr::semantic::SemanticSearchRequest;
use codex_native_tldr::semantic::SemanticSearchResponse;
use codex_native_tldr::tool_api::daemon_unavailable_error_for_project;
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

const TLDR_DAEMON_FALLBACK_MESSAGE: &str = "daemon 不可用；已回退到本地引擎";

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

    /// 提取单文件结构摘要。
    Extract(TldrExtractCommand),

    /// 列出单文件 import。
    Imports(TldrExtractCommand),

    /// 查找导入某个模块的符号/文件。
    Importers(TldrImportersCommand),

    /// 获取指定行的 backward slice。
    Slice(TldrSliceCommand),

    /// 获取上下文概览。
    Context(TldrAnalyzeCommand),

    /// 获取影响分析概览。
    Impact(TldrAnalyzeCommand),

    /// 评估变更文件的影响范围。
    ChangeImpact(TldrChangeImpactCommand),

    /// 获取控制流概览。
    Cfg(TldrAnalyzeCommand),

    /// 获取数据流概览。
    Dfg(TldrAnalyzeCommand),

    /// 运行语义检索。
    Semantic(TldrSemanticCommand),

    /// 预热最靠近 daemon 的索引缓存。
    Warm(TldrWarmCommand),

    /// 在索引中搜索匹配。
    Search(TldrSearchCommand),

    /// 列出调用图中的所有调用边。
    Calls(TldrAnalyzeCommand),

    /// 找出死代码候选项。
    Dead(TldrAnalyzeCommand),

    /// 展示调用拓扑结构统计。
    Arch(TldrAnalyzeCommand),

    /// 运行语言诊断工具集合。
    Diagnostics(TldrDiagnosticsCommand),

    /// 运行诊断工具侦测可用项。
    Doctor(TldrDoctorCommand),

    /// 与 daemon 直接交互。
    Daemon(TldrDaemonCli),

    /// 内部：运行 native-tldr daemon 服务。
    #[command(hide = true, name = "internal-daemon")]
    InternalDaemon(TldrInternalDaemonCli),
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum CliLanguage {
    C,
    Cpp,
    Csharp,
    Java,
    Rust,
    Typescript,
    Javascript,
    Lua,
    Luau,
    Python,
    Go,
    Php,
    Ruby,
    Swift,
    Zig,
}

impl From<CliLanguage> for SupportedLanguage {
    fn from(value: CliLanguage) -> Self {
        match value {
            CliLanguage::C => SupportedLanguage::C,
            CliLanguage::Cpp => SupportedLanguage::Cpp,
            CliLanguage::Csharp => SupportedLanguage::CSharp,
            CliLanguage::Java => SupportedLanguage::Java,
            CliLanguage::Rust => SupportedLanguage::Rust,
            CliLanguage::Typescript => SupportedLanguage::TypeScript,
            CliLanguage::Javascript => SupportedLanguage::JavaScript,
            CliLanguage::Lua => SupportedLanguage::Lua,
            CliLanguage::Luau => SupportedLanguage::Luau,
            CliLanguage::Python => SupportedLanguage::Python,
            CliLanguage::Go => SupportedLanguage::Go,
            CliLanguage::Php => SupportedLanguage::Php,
            CliLanguage::Ruby => SupportedLanguage::Ruby,
            CliLanguage::Swift => SupportedLanguage::Swift,
            CliLanguage::Zig => SupportedLanguage::Zig,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum CliSearchMatchMode {
    Literal,
    Regex,
}

impl From<CliSearchMatchMode> for SearchMatchMode {
    fn from(value: CliSearchMatchMode) -> Self {
        match value {
            CliSearchMatchMode::Literal => SearchMatchMode::Literal,
            CliSearchMatchMode::Regex => SearchMatchMode::Regex,
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
    #[arg(value_name = "符号", required = false)]
    pub symbol: Option<String>,

    /// 以 JSON 输出。
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

#[derive(Debug, Parser)]
pub struct TldrExtractCommand {
    /// 项目根目录。
    #[arg(long, default_value = ".")]
    pub project: PathBuf,

    /// 目标文件路径。
    #[arg(value_name = "路径")]
    pub path: PathBuf,

    /// 目标语言；未指定时按文件扩展名推断。
    #[arg(long, value_enum)]
    pub lang: Option<CliLanguage>,

    /// 以 JSON 输出。
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

#[derive(Debug, Parser)]
pub struct TldrSliceCommand {
    /// 项目根目录。
    #[arg(long, default_value = ".")]
    pub project: PathBuf,

    /// 目标文件路径。
    #[arg(value_name = "路径")]
    pub path: PathBuf,

    /// 目标符号。
    #[arg(value_name = "符号")]
    pub symbol: String,

    /// 目标行号。
    #[arg(value_name = "行号")]
    pub line: usize,

    /// 目标语言；未指定时按文件扩展名推断。
    #[arg(long, value_enum)]
    pub lang: Option<CliLanguage>,

    /// 以 JSON 输出。
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

#[derive(Debug, Parser)]
pub struct TldrChangeImpactCommand {
    /// 项目根目录。
    #[arg(long, default_value = ".")]
    pub project: PathBuf,

    /// 目标语言。
    #[arg(long, value_enum)]
    pub lang: CliLanguage,

    /// 发生变更的路径列表。
    #[arg(value_name = "路径")]
    pub paths: Vec<PathBuf>,

    /// 以 JSON 输出。
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

#[derive(Debug, Parser)]
pub struct TldrImportersCommand {
    /// 项目根目录。
    #[arg(long, default_value = ".")]
    pub project: PathBuf,

    /// 目标语言。
    #[arg(long, value_enum)]
    pub lang: CliLanguage,

    /// 模块或 import 片段。
    #[arg(value_name = "模块")]
    pub module: String,

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
    #[arg(value_name = "查询")]
    pub query: String,

    /// 以 JSON 输出。
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

#[derive(Debug, Parser)]
pub struct TldrWarmCommand {
    /// 项目根目录。
    #[arg(long, default_value = ".")]
    pub project: PathBuf,

    /// 以 JSON 输出。
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

#[derive(Debug, Parser)]
pub struct TldrSearchCommand {
    /// 项目根目录。
    #[arg(long, default_value = ".")]
    pub project: PathBuf,

    /// 目标语言。
    #[arg(long, value_enum)]
    pub lang: Option<CliLanguage>,

    /// 匹配模式。
    #[arg(value_name = "模式")]
    pub pattern: String,

    /// 匹配语义；默认按字面量匹配。
    #[arg(long, value_enum, default_value_t = CliSearchMatchMode::Literal)]
    pub match_mode: CliSearchMatchMode,

    /// 最多返回多少条结果。
    #[arg(long, default_value_t = 100)]
    pub max_results: usize,

    /// 以 JSON 输出。
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

#[derive(Debug, Parser)]
pub struct TldrDiagnosticsCommand {
    /// 项目根目录。
    #[arg(long, default_value = ".")]
    pub project: PathBuf,

    /// 目标文件路径。
    #[arg(value_name = "路径")]
    pub path: PathBuf,

    /// 目标语言；未指定时按文件扩展名推断。
    #[arg(long, value_enum)]
    pub lang: Option<CliLanguage>,

    /// 只运行这些诊断工具，可重复传入。
    #[arg(long = "only-tool", value_name = "工具")]
    pub only_tools: Vec<String>,

    /// 跳过 lint 类工具。
    #[arg(long, default_value_t = false)]
    pub no_lint: bool,

    /// 跳过 typecheck 类工具。
    #[arg(long, default_value_t = false)]
    pub no_typecheck: bool,

    /// 最多保留多少条诊断。
    #[arg(long, default_value_t = 50)]
    pub max_issues: usize,

    /// 以 JSON 输出。
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

#[derive(Debug, Parser)]
pub struct TldrDoctorCommand {
    /// 项目根目录。
    #[arg(long, default_value = ".")]
    pub project: PathBuf,

    /// 仅检查指定语言相关的工具。
    #[arg(long, value_enum)]
    pub lang: Option<CliLanguage>,

    /// 只检查这些工具，可重复传入。
    #[arg(long = "only-tool", value_name = "工具")]
    pub only_tools: Vec<String>,

    /// 不返回安装提示。
    #[arg(long, default_value_t = false)]
    pub no_install_hints: bool,

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
    /// 启动 daemon。
    Start,

    /// 停止 daemon。
    Stop,

    /// 检查 daemon 是否在线。
    Ping,

    /// 清空 dirty 文件集合并返回 session 快照。
    Warm,

    /// 返回当前 session 快照。
    Snapshot,

    /// 返回 daemon 健康状态与配置摘要。
    Status,

    /// 通知 daemon 某个路径发生变化。
    Notify {
        /// 发生变化的路径。
        path: PathBuf,
    },
}

pub async fn run_tldr_command(cli: TldrCli) -> Result<()> {
    match cli.subcommand {
        TldrSubcommand::Languages => {
            let registry = LanguageRegistry;
            for language in registry.supported_languages() {
                println!("{}", language.as_str());
            }
        }
        TldrSubcommand::Structure(cmd) => {
            run_analysis_command(cmd, AnalysisKind::Ast).await?;
        }
        TldrSubcommand::Extract(cmd) => {
            run_extract_command(cmd).await?;
        }
        TldrSubcommand::Imports(cmd) => {
            run_imports_command(cmd).await?;
        }
        TldrSubcommand::Importers(cmd) => {
            run_importers_command(cmd).await?;
        }
        TldrSubcommand::Slice(cmd) => {
            run_slice_command(cmd).await?;
        }
        TldrSubcommand::Context(cmd) => {
            run_analysis_command(cmd, AnalysisKind::CallGraph).await?;
        }
        TldrSubcommand::Impact(cmd) => {
            run_analysis_command(cmd, AnalysisKind::Impact).await?;
        }
        TldrSubcommand::ChangeImpact(cmd) => {
            run_change_impact_command(cmd).await?;
        }
        TldrSubcommand::Cfg(cmd) => {
            run_analysis_command(cmd, AnalysisKind::Cfg).await?;
        }
        TldrSubcommand::Dfg(cmd) => {
            run_analysis_command(cmd, AnalysisKind::Dfg).await?;
        }
        TldrSubcommand::Semantic(cmd) => {
            run_semantic_command(cmd).await?;
        }
        TldrSubcommand::Daemon(cmd) => {
            run_daemon_command(cmd).await?;
        }
        TldrSubcommand::Warm(cmd) => {
            run_warm_command(cmd).await?;
        }
        TldrSubcommand::Search(cmd) => {
            run_search_command(cmd).await?;
        }
        TldrSubcommand::Calls(cmd) => {
            run_analysis_command(cmd, AnalysisKind::Calls).await?;
        }
        TldrSubcommand::Dead(cmd) => {
            run_analysis_command(cmd, AnalysisKind::Dead).await?;
        }
        TldrSubcommand::Arch(cmd) => {
            run_analysis_command(cmd, AnalysisKind::Arch).await?;
        }
        TldrSubcommand::Diagnostics(cmd) => {
            run_diagnostics_command(cmd).await?;
        }
        TldrSubcommand::Doctor(cmd) => {
            run_doctor_command(cmd).await?;
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
        path: None,
        paths: Vec::new(),
        line: None,
    };
    let daemon_response = query_daemon_with_autostart(
        &project_root,
        &TldrDaemonCommand::Analyze {
            key: analysis_cache_key(kind, language, cmd.symbol.as_deref(), None, None),
            request: request.clone(),
        },
    )
    .await?;
    let (source, daemon_message, analysis, engine_project_root) =
        if let Some(response) = daemon_response {
            let analysis = response
                .analysis
                .ok_or_else(|| anyhow::anyhow!("daemon 响应缺少 analysis 数据"))?;
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
                Some(TLDR_DAEMON_FALLBACK_MESSAGE.to_string()),
                response,
                engine.config().project_root.clone(),
            )
        };

    let support = LanguageRegistry::support_for(language);
    let payload = analysis_payload(
        AnalysisRenderContext {
            action: analysis_action_name(kind),
            project_root: &engine_project_root,
            language,
            source,
            message: daemon_message.as_deref(),
            support,
            symbol: cmd.symbol.as_deref(),
            path: None,
            line: None,
        },
        &analysis,
    );

    if cmd.json {
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        for line in render_analysis_response_text(
            language,
            source,
            support,
            daemon_message.as_deref(),
            None,
            None,
            &analysis.summary,
        ) {
            println!("{line}");
        }
    }

    Ok(())
}

async fn run_extract_command(cmd: TldrExtractCommand) -> Result<()> {
    let project_root = cmd.project.canonicalize()?;
    let requested_path = cmd.path.display().to_string();
    let language = match cmd.lang {
        Some(language) => SupportedLanguage::from(language),
        None => SupportedLanguage::from_path(&cmd.path)
            .ok_or_else(|| anyhow::anyhow!("文件扩展名不受支持时必须显式传入 `--lang`"))?,
    };
    let config = load_tldr_config(&project_root)?;
    let request = AnalysisRequest {
        kind: AnalysisKind::Extract,
        language,
        symbol: None,
        path: Some(requested_path.clone()),
        paths: Vec::new(),
        line: None,
    };
    let daemon_response = query_daemon_with_autostart(
        &project_root,
        &TldrDaemonCommand::Analyze {
            key: analysis_cache_key(
                AnalysisKind::Extract,
                language,
                None,
                Some(requested_path.as_str()),
                None,
            ),
            request: request.clone(),
        },
    )
    .await?;
    let (source, daemon_message, analysis, engine_project_root) =
        if let Some(response) = daemon_response {
            let analysis = response
                .analysis
                .ok_or_else(|| anyhow::anyhow!("daemon 响应缺少 analysis 数据"))?;
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
                Some(TLDR_DAEMON_FALLBACK_MESSAGE.to_string()),
                response,
                engine.config().project_root.clone(),
            )
        };

    let support = LanguageRegistry::support_for(language);
    let payload = analysis_payload(
        AnalysisRenderContext {
            action: "extract",
            project_root: &engine_project_root,
            language,
            source,
            message: daemon_message.as_deref(),
            support,
            symbol: None,
            path: Some(requested_path.as_str()),
            line: None,
        },
        &analysis,
    );

    if cmd.json {
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        for line in render_analysis_response_text(
            language,
            source,
            support,
            daemon_message.as_deref(),
            Some(requested_path.as_str()),
            None,
            &analysis.summary,
        ) {
            println!("{line}");
        }
    }

    Ok(())
}

async fn run_imports_command(cmd: TldrExtractCommand) -> Result<()> {
    let requested_path = cmd.path.display().to_string();
    let language = cmd
        .lang
        .map(Into::into)
        .or_else(|| SupportedLanguage::from_path(&cmd.path))
        .ok_or_else(|| anyhow::anyhow!("无法从路径推断语言：{}", cmd.path.display()))?;
    let project_root = cmd.project.canonicalize()?;
    let config = load_tldr_config(&project_root)?;
    let daemon_response = query_daemon_with_autostart(
        &project_root,
        &TldrDaemonCommand::Imports {
            request: codex_native_tldr::api::ImportsRequest {
                language,
                path: requested_path.clone(),
            },
        },
    )
    .await?;
    let (source, message, response) = if let Some(response) = daemon_response {
        let imports = response
            .imports
            .ok_or_else(|| anyhow::anyhow!("daemon 响应缺少 imports 数据"))?;
        ("daemon", Some(response.message), imports)
    } else {
        let engine = TldrEngine::builder(project_root.clone())
            .with_config(config.clone())
            .build();
        let response = engine.imports(codex_native_tldr::api::ImportsRequest {
            language,
            path: requested_path.clone(),
        })?;
        (
            "local",
            Some(TLDR_DAEMON_FALLBACK_MESSAGE.to_string()),
            response,
        )
    };
    let payload = json!({
        "action": "imports",
        "project": project_root,
        "language": language.as_str(),
        "source": source,
        "message": message,
        "path": requested_path,
        "imports": {
            "path": response.path,
            "language": response.language.as_str(),
            "indexedFiles": response.indexed_files,
            "imports": response.imports,
        }
    });
    if cmd.json {
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        for line in render_imports_response_text(
            language,
            source,
            payload.get("message").and_then(serde_json::Value::as_str),
            payload["path"].as_str().unwrap_or_default(),
            payload["imports"]["imports"].as_array().map_or(0, Vec::len),
        ) {
            println!("{line}");
        }
    }
    Ok(())
}

async fn run_importers_command(cmd: TldrImportersCommand) -> Result<()> {
    let language: SupportedLanguage = cmd.lang.into();
    let project_root = cmd.project.canonicalize()?;
    let config = load_tldr_config(&project_root)?;
    let daemon_response = query_daemon_with_autostart(
        &project_root,
        &TldrDaemonCommand::Importers {
            request: codex_native_tldr::api::ImportersRequest {
                language,
                module: cmd.module.clone(),
            },
        },
    )
    .await?;
    let (source, message, response) = if let Some(response) = daemon_response {
        let importers = response
            .importers
            .ok_or_else(|| anyhow::anyhow!("daemon 响应缺少 importers 数据"))?;
        ("daemon", Some(response.message), importers)
    } else {
        let engine = TldrEngine::builder(project_root.clone())
            .with_config(config.clone())
            .build();
        let response = engine.importers(codex_native_tldr::api::ImportersRequest {
            language,
            module: cmd.module.clone(),
        })?;
        (
            "local",
            Some(TLDR_DAEMON_FALLBACK_MESSAGE.to_string()),
            response,
        )
    };
    let payload = json!({
        "action": "importers",
        "project": project_root,
        "language": language.as_str(),
        "source": source,
        "message": message,
        "module": cmd.module,
        "importers": {
            "module": response.module,
            "language": response.language.as_str(),
            "indexedFiles": response.indexed_files,
            "matches": response.matches,
        }
    });
    if cmd.json {
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        for line in render_importers_response_text(
            language,
            source,
            payload.get("message").and_then(serde_json::Value::as_str),
            payload["module"].as_str().unwrap_or_default(),
            payload["importers"]["matches"]
                .as_array()
                .map_or(0, Vec::len),
        ) {
            println!("{line}");
        }
    }
    Ok(())
}

async fn run_warm_command(cmd: TldrWarmCommand) -> Result<()> {
    let project_root = cmd.project.canonicalize()?;
    let Some(response) =
        query_daemon_with_autostart(&project_root, &TldrDaemonCommand::Warm).await?
    else {
        return Err(daemon_unavailable_error(&project_root));
    };

    if cmd.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&daemon_response_payload(
                "warm",
                &project_root,
                &response
            ))?
        );
    } else {
        for line in render_daemon_response_text("warm", &response) {
            println!("{line}");
        }
    }

    Ok(())
}

async fn run_search_command(cmd: TldrSearchCommand) -> Result<()> {
    let project_root = cmd.project.canonicalize()?;
    let config = load_tldr_config(&project_root)?;
    let language = cmd.lang.map(Into::into);
    let match_mode: SearchMatchMode = cmd.match_mode.into();
    let request = SearchRequest {
        pattern: cmd.pattern.clone(),
        match_mode,
        language,
        max_results: cmd.max_results.max(1),
    };
    let daemon_response = query_daemon_with_autostart(
        &project_root,
        &TldrDaemonCommand::Search {
            request: request.clone(),
        },
    )
    .await?;
    let (source, message, response) = if let Some(response) = daemon_response {
        let search = response
            .search
            .ok_or_else(|| anyhow::anyhow!("daemon 响应缺少 search 数据"))?;
        ("daemon", Some(response.message), search)
    } else {
        let engine = TldrEngine::builder(project_root.clone())
            .with_config(config)
            .build();
        let response = engine.search(request)?;
        (
            "local",
            Some(TLDR_DAEMON_FALLBACK_MESSAGE.to_string()),
            response,
        )
    };
    let payload = json!({
        "action": "search",
        "project": project_root,
        "language": language.map(SupportedLanguage::as_str),
        "source": source,
        "message": message,
        "pattern": cmd.pattern,
        "matchMode": response.match_mode.as_str(),
        "search": response,
    });
    if cmd.json {
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        for line in render_search_response_text(
            source,
            payload.get("message").and_then(serde_json::Value::as_str),
            payload["pattern"].as_str().unwrap_or_default(),
            payload["matchMode"].as_str().unwrap_or_default(),
            payload["search"]["indexed_files"]
                .as_u64()
                .unwrap_or_default() as usize,
            payload["search"]["matches"].as_array().map_or(0, Vec::len),
        ) {
            println!("{line}");
        }
    }
    Ok(())
}

async fn run_diagnostics_command(cmd: TldrDiagnosticsCommand) -> Result<()> {
    let project_root = cmd.project.canonicalize()?;
    let config = load_tldr_config(&project_root)?;
    let requested_path = cmd.path.display().to_string();
    let language = cmd
        .lang
        .map(Into::into)
        .or_else(|| SupportedLanguage::from_path(&cmd.path))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "{} 的文件扩展名不受支持时必须显式传入 `--lang`",
                cmd.path.display()
            )
        })?;
    let request = DiagnosticsRequest {
        language,
        path: requested_path.clone(),
        only_tools: cmd.only_tools.clone(),
        run_lint: !cmd.no_lint,
        run_typecheck: !cmd.no_typecheck,
        max_issues: cmd.max_issues.max(1),
    };
    let daemon_response = query_daemon_with_autostart(
        &project_root,
        &TldrDaemonCommand::Diagnostics {
            request: request.clone(),
        },
    )
    .await?;
    let (source, response) = if let Some(response) = daemon_response {
        let diagnostics = response
            .diagnostics
            .ok_or_else(|| anyhow::anyhow!("daemon 响应缺少 diagnostics 数据"))?;
        ("daemon", diagnostics)
    } else {
        let engine = TldrEngine::builder(project_root.clone())
            .with_config(config)
            .build();
        let response = engine.diagnostics(request)?;
        ("local", response)
    };
    let payload = json!({
        "action": "diagnostics",
        "project": project_root,
        "language": language.as_str(),
        "source": source,
        "path": requested_path,
        "diagnostics": response,
    });
    if cmd.json {
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        for line in render_diagnostics_response_text(language, source, &response) {
            println!("{line}");
        }
    }
    Ok(())
}

async fn run_doctor_command(cmd: TldrDoctorCommand) -> Result<()> {
    let project_root = cmd.project.canonicalize()?;
    let config = load_tldr_config(&project_root)?;
    let engine = TldrEngine::builder(project_root.clone())
        .with_config(config)
        .build();
    let response = engine.doctor(DoctorRequest {
        language: cmd.lang.map(Into::into),
        only_tools: cmd.only_tools.clone(),
        include_install_hints: !cmd.no_install_hints,
    });
    let payload = json!({
        "action": "doctor",
        "project": project_root,
        "doctor": response,
    });
    if cmd.json {
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        for line in render_doctor_response_text(&response) {
            println!("{line}");
        }
    }
    Ok(())
}

async fn run_slice_command(cmd: TldrSliceCommand) -> Result<()> {
    let project_root = cmd.project.canonicalize()?;
    let requested_path = cmd.path.display().to_string();
    let language = match cmd.lang {
        Some(language) => SupportedLanguage::from(language),
        None => SupportedLanguage::from_path(&cmd.path)
            .ok_or_else(|| anyhow::anyhow!("文件扩展名不受支持时必须显式传入 `--lang`"))?,
    };
    let config = load_tldr_config(&project_root)?;
    let request = AnalysisRequest {
        kind: AnalysisKind::Slice,
        language,
        symbol: Some(cmd.symbol.clone()),
        path: Some(requested_path.clone()),
        paths: Vec::new(),
        line: Some(cmd.line),
    };
    let daemon_response = query_daemon_with_autostart(
        &project_root,
        &TldrDaemonCommand::Analyze {
            key: analysis_cache_key(
                AnalysisKind::Slice,
                language,
                Some(cmd.symbol.as_str()),
                Some(requested_path.as_str()),
                Some(cmd.line),
            ),
            request: request.clone(),
        },
    )
    .await?;
    let (source, daemon_message, analysis, engine_project_root) =
        if let Some(response) = daemon_response {
            let analysis = response
                .analysis
                .ok_or_else(|| anyhow::anyhow!("daemon 响应缺少 analysis 数据"))?;
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
                Some(TLDR_DAEMON_FALLBACK_MESSAGE.to_string()),
                response,
                engine.config().project_root.clone(),
            )
        };

    let support = LanguageRegistry::support_for(language);
    let payload = analysis_payload(
        AnalysisRenderContext {
            action: "slice",
            project_root: &engine_project_root,
            language,
            source,
            message: daemon_message.as_deref(),
            support,
            symbol: Some(cmd.symbol.as_str()),
            path: Some(requested_path.as_str()),
            line: Some(cmd.line),
        },
        &analysis,
    );

    if cmd.json {
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        for line in render_analysis_response_text(
            language,
            source,
            support,
            daemon_message.as_deref(),
            Some(requested_path.as_str()),
            Some(cmd.line),
            &analysis.summary,
        ) {
            println!("{line}");
        }
    }

    Ok(())
}

async fn run_change_impact_command(cmd: TldrChangeImpactCommand) -> Result<()> {
    let language: SupportedLanguage = cmd.lang.into();
    let project_root = cmd.project.canonicalize()?;
    let config = load_tldr_config(&project_root)?;
    let requested_paths = cmd
        .paths
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>();
    if requested_paths.is_empty() {
        bail!("change-impact 至少需要提供一个路径");
    }
    let request = AnalysisRequest {
        kind: AnalysisKind::ChangeImpact,
        language,
        symbol: None,
        path: None,
        line: None,
        paths: requested_paths.clone(),
    };
    let daemon_response = query_daemon_with_autostart(
        &project_root,
        &TldrDaemonCommand::Analyze {
            key: analysis_cache_key(AnalysisKind::ChangeImpact, language, None, None, None),
            request: request.clone(),
        },
    )
    .await?;
    let (source, daemon_message, analysis, engine_project_root) =
        if let Some(response) = daemon_response {
            let analysis = response
                .analysis
                .ok_or_else(|| anyhow::anyhow!("daemon 响应缺少 analysis 数据"))?;
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
                Some(TLDR_DAEMON_FALLBACK_MESSAGE.to_string()),
                response,
                engine.config().project_root.clone(),
            )
        };

    let support = LanguageRegistry::support_for(language);
    let payload = json!({
        "action": "change-impact",
        "project": engine_project_root,
        "language": language.as_str(),
        "source": source,
        "message": daemon_message,
        "supportLevel": format!("{:?}", support.support_level),
        "fallbackStrategy": support.fallback_strategy,
        "summary": analysis.summary,
        "paths": requested_paths,
        "analysis": analysis,
    });

    if cmd.json {
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        for line in render_analysis_response_text(
            language,
            source,
            support,
            payload.get("message").and_then(serde_json::Value::as_str),
            None,
            None,
            payload["summary"].as_str().unwrap_or_default(),
        ) {
            println!("{line}");
        }
    }

    Ok(())
}

fn analysis_action_name(kind: AnalysisKind) -> &'static str {
    match kind {
        AnalysisKind::Ast => "structure",
        AnalysisKind::Extract => "extract",
        AnalysisKind::CallGraph => "context",
        AnalysisKind::Impact => "impact",
        AnalysisKind::Pdg => "pdg",
        AnalysisKind::Calls => "calls",
        AnalysisKind::Dead => "dead",
        AnalysisKind::Arch => "arch",
        AnalysisKind::ChangeImpact => "change-impact",
        AnalysisKind::Cfg => "cfg",
        AnalysisKind::Dfg => "dfg",
        AnalysisKind::Slice => "slice",
    }
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

struct AnalysisRenderContext<'a> {
    action: &'a str,
    project_root: &'a Path,
    language: SupportedLanguage,
    source: &'a str,
    message: Option<&'a str>,
    support: &'a codex_native_tldr::lang_support::LanguageSupport,
    symbol: Option<&'a str>,
    path: Option<&'a str>,
    line: Option<usize>,
}

fn analysis_payload(
    context: AnalysisRenderContext<'_>,
    response: &AnalysisResponse,
) -> serde_json::Value {
    json!({
        "action": context.action,
        "project": context.project_root,
        "language": context.language.as_str(),
        "source": context.source,
        "message": context.message,
        "supportLevel": format!("{:?}", context.support.support_level),
        "fallbackStrategy": context.support.fallback_strategy,
        "summary": response.summary.clone(),
        "symbol": context.symbol,
        "path": context.path,
        "line": context.line,
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

fn render_analysis_response_text(
    language: SupportedLanguage,
    source: &str,
    support: &codex_native_tldr::lang_support::LanguageSupport,
    message: Option<&str>,
    path: Option<&str>,
    line: Option<usize>,
    summary: &str,
) -> Vec<String> {
    let mut lines = vec![
        format!("语言：{}", language.as_str()),
        format!("来源：{source}"),
        format!("支持级别：{:?}", support.support_level),
        format!("回退策略：{}", support.fallback_strategy),
    ];
    if let Some(message) = message {
        lines.push(format!("消息：{message}"));
    }
    if let Some(path) = path {
        lines.push(format!("路径：{path}"));
    }
    if let Some(line) = line {
        lines.push(format!("行号：{line}"));
    }
    lines.push(format!("摘要：{summary}"));
    lines
}

fn render_imports_response_text(
    language: SupportedLanguage,
    source: &str,
    message: Option<&str>,
    path: &str,
    import_count: usize,
) -> Vec<String> {
    let mut lines = vec![
        format!("语言：{}", language.as_str()),
        format!("来源：{source}"),
        format!("路径：{path}"),
        format!("导入数：{import_count}"),
    ];
    if let Some(message) = message {
        lines.push(format!("消息：{message}"));
    }
    lines
}

fn render_importers_response_text(
    language: SupportedLanguage,
    source: &str,
    message: Option<&str>,
    module: &str,
    match_count: usize,
) -> Vec<String> {
    let mut lines = vec![
        format!("语言：{}", language.as_str()),
        format!("来源：{source}"),
        format!("模块：{module}"),
        format!("匹配数：{match_count}"),
    ];
    if let Some(message) = message {
        lines.push(format!("消息：{message}"));
    }
    lines
}

fn render_doctor_response_text(response: &DoctorResponse) -> Vec<String> {
    let available_tools = response.tools.iter().filter(|tool| tool.available).count();
    let mut lines = vec![
        format!("已检查工具数：{}", response.tools.len()),
        format!("可用工具数：{available_tools}"),
        format!("消息：{}", response.message),
    ];
    for tool in &response.tools {
        lines.push(format!("工具 {}：{}", tool.tool, tool.available));
    }
    lines
}

fn render_search_response_text(
    source: &str,
    message: Option<&str>,
    pattern: &str,
    match_mode: &str,
    indexed_files: usize,
    match_count: usize,
) -> Vec<String> {
    let mut lines = vec![
        format!("来源：{source}"),
        format!("模式：{pattern}"),
        format!("匹配语义：{match_mode}"),
        format!("已索引文件数：{indexed_files}"),
        format!("匹配数：{match_count}"),
    ];
    if let Some(message) = message {
        lines.push(format!("消息：{message}"));
    }
    lines
}

fn render_semantic_response_text(
    language: SupportedLanguage,
    source: &str,
    response: &SemanticSearchResponse,
) -> Vec<String> {
    let mut lines = vec![
        format!("语言：{}", language.as_str()),
        format!("来源：{source}"),
        format!("查询：{}", response.query),
        format!("语义检索启用：{}", response.enabled),
        format!("已索引文件数：{}", response.indexed_files),
        format!("已截断：{}", response.truncated),
        format!("消息：{}", response.message),
        format!("匹配数：{}", response.matches.len()),
    ];
    for (index, semantic_match) in response.matches.iter().enumerate() {
        let score = semantic_match
            .embedding_score
            .map(|value| format!("{value:.3}"))
            .unwrap_or_else(|| "n/a".to_string());
        lines.push(format!(
            "匹配 {index}：{}:{} (embedding score: {score})",
            semantic_match.path.display(),
            semantic_match.line
        ));
        lines.push(format!(
            "  片段：{}",
            semantic_match.snippet.replace('\n', "\\n")
        ));
    }
    lines
}

fn render_diagnostics_response_text(
    language: SupportedLanguage,
    source: &str,
    response: &DiagnosticsResponse,
) -> Vec<String> {
    let mut lines = vec![
        format!("语言：{}", language.as_str()),
        format!("来源：{source}"),
        format!("路径：{}", response.path),
        format!("工具数：{}", response.tools.len()),
        format!("诊断数：{}", response.diagnostics.len()),
        format!("消息：{}", response.message),
    ];
    for tool in &response.tools {
        lines.push(format!("工具 {}：{}", tool.tool, tool.available));
    }
    for diagnostic in &response.diagnostics {
        let severity = match diagnostic.severity {
            codex_native_tldr::api::DiagnosticSeverity::Error => "错误",
            codex_native_tldr::api::DiagnosticSeverity::Warning => "警告",
            codex_native_tldr::api::DiagnosticSeverity::Info => "信息",
        };
        let code = diagnostic
            .code
            .as_deref()
            .map(|value| format!(" [{value}]"))
            .unwrap_or_default();
        lines.push(format!(
            "{}:{}:{} {severity} {}{} {}",
            diagnostic.path,
            diagnostic.line,
            diagnostic.column,
            diagnostic.source,
            code,
            diagnostic.message.replace('\n', "\\n")
        ));
    }
    lines
}

async fn run_daemon_command(cmd: TldrDaemonCli) -> Result<()> {
    let project_root = cmd.project.canonicalize()?;
    match cmd.subcommand {
        TldrDaemonSubcommand::Start => run_daemon_start_command(&project_root, cmd.json).await?,
        TldrDaemonSubcommand::Stop => run_daemon_stop_command(&project_root, cmd.json).await?,
        subcommand => {
            let (action, command) = daemon_action_and_command(&subcommand);
            run_daemon_action(&project_root, cmd.json, action, command).await?;
        }
    }

    Ok(())
}

fn daemon_action_and_command(
    subcommand: &TldrDaemonSubcommand,
) -> (&'static str, TldrDaemonCommand) {
    match subcommand {
        TldrDaemonSubcommand::Start | TldrDaemonSubcommand::Stop => {
            unreachable!("start/stop are handled before daemon command mapping")
        }
        TldrDaemonSubcommand::Ping => ("ping", TldrDaemonCommand::Ping),
        TldrDaemonSubcommand::Warm => ("warm", TldrDaemonCommand::Warm),
        TldrDaemonSubcommand::Snapshot => ("snapshot", TldrDaemonCommand::Snapshot),
        TldrDaemonSubcommand::Status => ("status", TldrDaemonCommand::Status),
        TldrDaemonSubcommand::Notify { path } => {
            ("notify", TldrDaemonCommand::Notify { path: path.clone() })
        }
    }
}

fn render_daemon_response_text(
    action: &str,
    response: &codex_native_tldr::daemon::TldrDaemonResponse,
) -> Vec<String> {
    let daemon_status = response.daemon_status.as_ref();
    let reindex_report = response.reindex_report.as_ref();
    let snapshot = response.snapshot.as_ref();
    let mut lines = vec![
        format!("动作：{action}"),
        format!("状态：{}", response.status),
        format!("消息：{}", response.message),
    ];

    if let Some(daemon_status) = daemon_status {
        lines.push(format!("项目：{}", daemon_status.project_root.display()));
        lines.push(format!("套接字：{}", daemon_status.socket_path.display()));
        lines.push(format!("套接字存在：{}", daemon_status.socket_exists));
        lines.push(format!("PID 存活：{}", daemon_status.pid_is_live));
        lines.push(format!("锁已持有：{}", daemon_status.lock_is_held));
        lines.push(format!("健康：{}", daemon_status.healthy));
        lines.push(format!("过期套接字：{}", daemon_status.stale_socket));
        lines.push(format!("过期 PID：{}", daemon_status.stale_pid));
        if let Some(reason) = daemon_status.health_reason.as_deref() {
            lines.push(format!("健康原因：{reason}"));
        }
        if let Some(hint) = daemon_status.recovery_hint.as_deref() {
            lines.push(format!("恢复提示：{hint}"));
        }
        lines.push(format!(
            "会话重索引待处理：{}",
            snapshot
                .map(|snapshot| snapshot.reindex_pending)
                .unwrap_or(false)
        ));
        lines.push(format!(
            "会话重索引进行中：{}",
            snapshot
                .map(|snapshot| snapshot.background_reindex_in_progress)
                .unwrap_or(false)
        ));
        lines.push(format!(
            "语义重索引待处理：{}",
            daemon_status.semantic_reindex_pending
        ));
        lines.push(format!(
            "语义重索引进行中：{}",
            daemon_status.semantic_reindex_in_progress
        ));
        if let Some(last_query_at) = daemon_status.last_query_at {
            lines.push(format!("最近查询时间：{last_query_at:?}"));
        }
        lines.push(format!("自动启动：{}", daemon_status.config.auto_start));
        lines.push(format!("套接字模式：{}", daemon_status.config.socket_mode));
        lines.push(format!(
            "会话空闲超时秒数：{}",
            daemon_status.config.session_idle_timeout_secs
        ));
        lines.push(format!(
            "会话脏文件阈值：{}",
            daemon_status.config.session_dirty_file_threshold
        ));
        lines.push(format!(
            "语义检索启用：{}",
            daemon_status.config.semantic_enabled
        ));
        lines.push(format!(
            "语义自动重索引阈值：{}",
            daemon_status.config.semantic_auto_reindex_threshold
        ));
    }
    if let Some(snapshot) = snapshot {
        if daemon_status.is_none() {
            lines.push(format!("会话重索引待处理：{}", snapshot.reindex_pending));
            lines.push(format!(
                "会话重索引进行中：{}",
                snapshot.background_reindex_in_progress
            ));
        }
        lines.push(format!("缓存条目数：{}", snapshot.cached_entries));
        lines.push(format!("脏文件数：{}", snapshot.dirty_files));
        lines.push(format!("脏文件阈值：{}", snapshot.dirty_file_threshold));
        lines.push(format!("重索引待处理：{}", snapshot.reindex_pending));
        lines.push(format!(
            "重索引进行中：{}",
            snapshot.background_reindex_in_progress
        ));
        if let Some(last_reindex) = snapshot.last_reindex.as_ref() {
            lines.push(format!("最近完成的重索引：{:?}", last_reindex.status));
        }
        if let Some(last_reindex_attempt) = snapshot.last_reindex_attempt.as_ref() {
            lines.push(format!("最近重索引尝试：{:?}", last_reindex_attempt.status));
        }
        if let Some(last_warm) = snapshot.last_warm.as_ref() {
            lines.push(format!("最近预热状态：{:?}", last_warm.status));
            if !last_warm.languages.is_empty() {
                let languages = last_warm
                    .languages
                    .iter()
                    .copied()
                    .map(SupportedLanguage::as_str)
                    .collect::<Vec<_>>()
                    .join(",");
                lines.push(format!("最近预热语言：{languages}"));
            }
            lines.push(format!("最近预热消息：{}", last_warm.message));
            lines.push(format!("最近预热完成时间：{:?}", last_warm.finished_at));
        }
    }
    if let Some(reindex_report) = reindex_report {
        lines.push(format!("重索引状态：{:?}", reindex_report.status));
        lines.push(format!("重索引文件数：{}", reindex_report.indexed_files));
        lines.push(format!("重索引单元数：{}", reindex_report.indexed_units));
        lines.push(format!("重索引消息：{}", reindex_report.message));
    }

    lines
}

async fn run_daemon_action(
    project_root: &Path,
    json_output: bool,
    action: &str,
    command: TldrDaemonCommand,
) -> Result<()> {
    let QueryHooksResult {
        response,
        ready_result,
    } = query_daemon_with_autostart_detailed(project_root, &command).await?;
    let Some(response) = response else {
        return Err(daemon_unavailable_error_with_ready_result(
            project_root,
            ready_result.as_ref(),
        ));
    };

    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&daemon_response_payload(
                action,
                project_root,
                &response
            ))?
        );
    } else {
        for line in render_daemon_response_text(action, &response) {
            println!("{line}");
        }
    }

    Ok(())
}

fn daemon_unavailable_error(project_root: &Path) -> anyhow::Error {
    daemon_unavailable_error_with_ready_result(project_root, None)
}

fn daemon_unavailable_error_with_ready_result(
    project_root: &Path,
    ready_result: Option<&DaemonReadyResult>,
) -> anyhow::Error {
    daemon_unavailable_error_for_project(project_root, ready_result)
}

async fn run_daemon_start_command(project_root: &Path, json_output: bool) -> Result<()> {
    let started = ensure_daemon_running_detailed(project_root, true).await?;
    let status = query_daemon(project_root, &TldrDaemonCommand::Status).await?;
    let Some(response) = status else {
        return Err(daemon_unavailable_error_with_ready_result(
            project_root,
            Some(&started),
        ));
    };
    let mut payload = daemon_response_payload("start", project_root, &response);
    if let Some(object) = payload.as_object_mut() {
        object.insert("started".to_string(), json!(started.ready));
    }
    if json_output {
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        println!("已启动：{}", started.ready);
        for line in render_daemon_response_text("start", &response) {
            println!("{line}");
        }
    }
    Ok(())
}

async fn run_daemon_stop_command(project_root: &Path, json_output: bool) -> Result<()> {
    let (stopped, message) = stop_native_tldr_daemon(project_root).await?;
    let payload = json!({
        "action": "stop",
        "project": project_root,
        "stopped": stopped,
        "message": message,
    });
    if json_output {
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        println!("已停止：{stopped}");
        println!("消息：{message}");
    }
    Ok(())
}

#[cfg(unix)]
async fn stop_native_tldr_daemon(project_root: &Path) -> Result<(bool, String)> {
    let pid_path = pid_path_for_project(project_root);
    let pid = std::fs::read_to_string(&pid_path)
        .ok()
        .and_then(|value| value.trim().parse::<i32>().ok());
    let Some(pid) = pid else {
        cleanup_stale_daemon_artifacts(project_root);
        return Ok((false, "daemon 的 pid 文件缺失".to_string()));
    };
    if !read_live_pid(&pid_path).unwrap_or(false) {
        cleanup_stale_daemon_artifacts(project_root);
        return Ok((false, "已清理过期 daemon 遗留文件".to_string()));
    }

    let result = unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };
    if result != 0 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::ESRCH) {
            return Err(error.into());
        }
    }

    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(3) {
        if !read_live_pid(&pid_path).unwrap_or(false) {
            cleanup_stale_daemon_artifacts(project_root);
            return Ok((true, format!("已停止 daemon 进程 {pid}")));
        }
        sleep(Duration::from_millis(50)).await;
    }

    bail!("daemon 进程 {pid} 未在超时时间内退出");
}

#[cfg(not(unix))]
async fn stop_native_tldr_daemon(project_root: &Path) -> Result<(bool, String)> {
    let response = query_daemon(project_root, &TldrDaemonCommand::Shutdown).await?;
    let Some(response) = response else {
        cleanup_stale_daemon_artifacts(project_root);
        return Ok((false, "daemon 未运行；已清理遗留 metadata".to_string()));
    };

    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(3) {
        if !daemon_metadata_looks_alive(project_root) {
            cleanup_stale_daemon_artifacts(project_root);
            return Ok((true, response.message));
        }
        sleep(Duration::from_millis(50)).await;
    }

    bail!("daemon 未在超时时间内完成 shutdown")
}

async fn query_daemon_with_autostart(
    project_root: &Path,
    command: &TldrDaemonCommand,
) -> Result<Option<codex_native_tldr::daemon::TldrDaemonResponse>> {
    Ok(query_daemon_with_autostart_detailed(project_root, command)
        .await?
        .response)
}

async fn query_daemon_with_autostart_detailed(
    project_root: &Path,
    command: &TldrDaemonCommand,
) -> Result<QueryHooksResult> {
    let auto_start_enabled = load_tldr_config(project_root)?.daemon.auto_start;
    query_daemon_with_hooks_detailed(
        project_root,
        command,
        |project_root, command| Box::pin(query_daemon(project_root, command)),
        move |project_root| {
            Box::pin(ensure_daemon_running_detailed(
                project_root,
                auto_start_enabled,
            ))
        },
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
type EnsureDaemonDetailedFuture<'a> =
    Pin<Box<dyn Future<Output = Result<DaemonReadyResult>> + Send + 'a>>;

static DAEMON_LIFECYCLE_MANAGER: Lazy<DaemonLifecycleManager> =
    Lazy::new(DaemonLifecycleManager::default);

async fn query_daemon_with_hooks_detailed<Q, E>(
    project_root: &Path,
    command: &TldrDaemonCommand,
    query: Q,
    ensure_running: E,
) -> Result<QueryHooksResult>
where
    Q: for<'a> Fn(&'a Path, &'a TldrDaemonCommand) -> QueryDaemonFuture<'a>,
    E: for<'a> Fn(&'a Path) -> EnsureDaemonDetailedFuture<'a>,
{
    DAEMON_LIFECYCLE_MANAGER
        .query_or_spawn_with_hooks_detailed(project_root, command, query, ensure_running)
        .await
}

fn analysis_cache_key(
    kind: AnalysisKind,
    language: SupportedLanguage,
    symbol: Option<&str>,
    path: Option<&str>,
    line: Option<usize>,
) -> String {
    let symbol = symbol.unwrap_or("*");
    let path = path.unwrap_or("*");
    let line = line.map_or("*".to_string(), |value| value.to_string());
    format!("{}:{kind:?}:{symbol}:{path}:{line}", language.as_str())
}

async fn ensure_daemon_running_detailed(
    project_root: &Path,
    auto_start_enabled: bool,
) -> Result<DaemonReadyResult> {
    if !auto_start_enabled {
        return Ok(DaemonReadyResult {
            ready: false,
            structured_failure: None,
            degraded_mode: None,
        });
    }

    DAEMON_LIFECYCLE_MANAGER
        .ensure_running_with_launcher_lock_detailed(
            project_root,
            daemon_metadata_looks_alive_with_launcher_lock,
            cleanup_stale_daemon_artifacts,
            daemon_lock_is_held,
            try_open_launcher_lock,
            record_test_launcher_wait,
            |project_root| Box::pin(spawn_native_tldr_daemon(project_root)),
        )
        .await
}

#[cfg(test)]
const CODEX_TLDR_TEST_DAEMON_BIN_ENV: &str = "CODEX_TLDR_TEST_DAEMON_BIN";
#[cfg(test)]
const CODEX_TLDR_TEST_LAUNCH_COUNTER_ENV: &str = "CODEX_TLDR_TEST_LAUNCH_COUNTER";
#[cfg(test)]
const CODEX_TLDR_TEST_LAUNCHER_WAIT_COUNTER_ENV: &str = "CODEX_TLDR_TEST_LAUNCHER_WAIT_COUNTER";
#[cfg(test)]
const CODEX_TLDR_TEST_LAUNCH_RETURN_RELEASE_ENV: &str = "CODEX_TLDR_TEST_LAUNCH_RETURN_RELEASE";

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
        OsString::from("ztldr"),
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

async fn spawn_native_tldr_daemon(project_root: &Path) -> Result<bool> {
    let mut child = daemon_launcher_command(project_root)?
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    record_test_daemon_spawn(project_root);

    #[cfg(test)]
    if let Some(path) = std::env::var_os(CODEX_TLDR_TEST_LAUNCH_RETURN_RELEASE_ENV) {
        let path = PathBuf::from(path);
        while !path.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    tokio::spawn(async move {
        let _ = child.wait().await;
    });
    Ok(true)
}

#[cfg_attr(not(test), allow(dead_code))]
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
    native_launch_lock_path_for_project(project_root)
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
    use super::AnalysisRenderContext;
    use super::TldrDaemonSubcommand;
    use super::analysis_action_name;
    use super::analysis_payload;
    use super::cli_semantic_payload;
    use super::daemon_action_and_command;
    use super::render_analysis_response_text;
    use super::render_diagnostics_response_text;
    use super::render_importers_response_text;
    use super::render_imports_response_text;
    use super::render_search_response_text;
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
    use codex_native_tldr::api::DiagnosticItem;
    use codex_native_tldr::api::DiagnosticSeverity;
    use codex_native_tldr::api::DiagnosticToolStatus;
    use codex_native_tldr::api::DiagnosticsResponse;
    use codex_native_tldr::lang_support::LanguageRegistry;
    use codex_native_tldr::lang_support::SupportedLanguage;
    use codex_native_tldr::semantic::EmbeddingUnit;
    use codex_native_tldr::semantic::SemanticMatch;
    use codex_native_tldr::semantic::SemanticSearchResponse;
    use pretty_assertions::assert_eq;
    use serde_json::json;
    use std::path::Path;
    use std::path::PathBuf;

    #[test]
    fn analysis_payload_includes_nested_native_response() {
        let payload = analysis_payload(
            AnalysisRenderContext {
                action: "context",
                project_root: Path::new("/tmp/project"),
                language: SupportedLanguage::Rust,
                source: "daemon",
                message: Some("daemon summary ready"),
                support: LanguageRegistry::support_for(SupportedLanguage::Rust),
                symbol: Some("main"),
                path: None,
                line: None,
            },
            &AnalysisResponse {
                kind: AnalysisKind::CallGraph,
                summary: "context summary".to_string(),
                details: Some(AnalysisDetail {
                    indexed_files: 1,
                    total_symbols: 1,
                    symbol_query: Some("main".to_string()),
                    truncated: false,
                    change_paths: Vec::new(),
                    slice_target: None,
                    slice_lines: Vec::new(),
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
                        owner_symbol: None,
                        owner_kind: None,
                        implemented_trait: None,
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

        assert_eq!(payload["action"], "context");
        assert_eq!(payload["project"], "/tmp/project");
        assert_eq!(payload["language"], "rust");
        assert_eq!(payload["source"], "daemon");
        assert_eq!(payload["message"], "daemon summary ready");
        assert_eq!(payload["supportLevel"], "DataFlow");
        assert_eq!(payload["fallbackStrategy"], "structure + search");
        assert_eq!(payload["summary"], "context summary");
        assert_eq!(payload["symbol"], "main");
        assert_eq!(payload["analysis"]["kind"], "call_graph");
        assert_eq!(payload["analysis"]["summary"], "context summary");
        assert_eq!(payload["analysis"]["details"]["symbol_query"], "main");
        assert_eq!(
            payload["analysis"]["details"]["nodes"][0]["kind"],
            "function"
        );
        assert_eq!(
            payload["analysis"]["details"]["edges"][0]["kind"],
            "contains"
        );
        assert_eq!(
            payload["analysis"]["details"]["symbol_index"][0]["node_ids"],
            serde_json::json!(["main"])
        );
        assert_eq!(
            payload["analysis"]["details"]["units"][0]["qualified_symbol"],
            "crate::main"
        );
    }

    #[test]
    fn analysis_action_name_maps_ast_to_structure() {
        assert_eq!(analysis_action_name(AnalysisKind::Ast), "structure");
        assert_eq!(analysis_action_name(AnalysisKind::Extract), "extract");
        assert_eq!(analysis_action_name(AnalysisKind::Slice), "slice");
    }

    #[test]
    fn analysis_payload_preserves_impact_action_and_summary() {
        let payload = analysis_payload(
            AnalysisRenderContext {
                action: "impact",
                project_root: Path::new("/tmp/project"),
                language: SupportedLanguage::Rust,
                source: "daemon",
                message: Some("impact ready"),
                support: LanguageRegistry::support_for(SupportedLanguage::Rust),
                symbol: Some("AuthService"),
                path: None,
                line: None,
            },
            &AnalysisResponse {
                kind: AnalysisKind::Impact,
                summary:
                    "impact summary: 1 symbols across 1 files (1 touched paths); dependency edges=1"
                        .to_string(),
                details: Some(AnalysisDetail {
                    indexed_files: 1,
                    total_symbols: 1,
                    symbol_query: Some("AuthService".to_string()),
                    truncated: false,
                    change_paths: Vec::new(),
                    slice_target: None,
                    slice_lines: Vec::new(),
                    overview: AnalysisOverviewDetail::default(),
                    files: Vec::new(),
                    nodes: Vec::new(),
                    edges: vec![AnalysisEdgeDetail {
                        from: "AuthService".to_string(),
                        to: "auth::audit".to_string(),
                        kind: "depends_on".to_string(),
                    }],
                    symbol_index: Vec::new(),
                    units: Vec::new(),
                }),
            },
        );

        assert_eq!(payload["action"], "impact");
        assert_eq!(payload["message"], "impact ready");
        assert_eq!(
            payload["summary"],
            "impact summary: 1 symbols across 1 files (1 touched paths); dependency edges=1"
        );
        assert_eq!(payload["analysis"]["kind"], "impact");
        assert_eq!(
            payload["analysis"]["summary"],
            "impact summary: 1 symbols across 1 files (1 touched paths); dependency edges=1"
        );
        assert_eq!(
            payload["analysis"]["details"]["symbol_query"],
            "AuthService"
        );
    }

    #[test]
    fn analysis_payload_preserves_change_impact_paths_and_summary() {
        let payload = json!({
            "action": "change-impact",
            "project": "/tmp/project",
            "language": "rust",
            "source": "daemon",
            "message": "change-impact ready",
            "supportLevel": "DataFlow",
            "fallbackStrategy": "structure + search",
            "summary": "change-impact summary: 1 changed paths -> 2 impacted symbols across 1 indexed files",
            "paths": ["src/lib.rs"],
            "analysis": {
                "kind": "change_impact",
                "summary": "change-impact summary: 1 changed paths -> 2 impacted symbols across 1 indexed files",
                "details": {
                    "indexed_files": 1,
                    "total_symbols": 2,
                    "symbol_query": null,
                    "truncated": false,
                    "change_paths": ["src/lib.rs"],
                    "slice_target": null,
                    "slice_lines": [],
                    "overview": {
                        "kinds": [],
                        "outgoing_edges": 0,
                        "incoming_edges": 0,
                        "reference_count": 0,
                        "import_count": 0
                    },
                    "files": [],
                    "nodes": [],
                    "edges": [],
                    "symbol_index": [],
                    "units": []
                }
            }
        });

        assert_eq!(payload["action"], "change-impact");
        assert_eq!(payload["paths"], serde_json::json!(["src/lib.rs"]));
        assert_eq!(payload["analysis"]["kind"], "change_impact");
        assert_eq!(
            payload["analysis"]["details"]["change_paths"],
            serde_json::json!(["src/lib.rs"])
        );
    }

    #[test]
    fn analysis_payload_preserves_cfg_action_and_summary() {
        let payload = analysis_payload(
            AnalysisRenderContext {
                action: "cfg",
                project_root: Path::new("/tmp/project"),
                language: SupportedLanguage::Rust,
                source: "daemon",
                message: Some("cfg ready"),
                support: LanguageRegistry::support_for(SupportedLanguage::Rust),
                symbol: Some("AuthService"),
                path: None,
                line: None,
            },
            &AnalysisResponse {
                kind: AnalysisKind::Cfg,
                summary: "cfg summary: 1 symbols across 1 files; sample: AuthService [cfg]"
                    .to_string(),
                details: Some(AnalysisDetail {
                    indexed_files: 1,
                    total_symbols: 1,
                    symbol_query: Some("AuthService".to_string()),
                    truncated: false,
                    change_paths: Vec::new(),
                    slice_target: None,
                    slice_lines: Vec::new(),
                    overview: AnalysisOverviewDetail::default(),
                    files: Vec::new(),
                    nodes: Vec::new(),
                    edges: Vec::new(),
                    symbol_index: Vec::new(),
                    units: vec![AnalysisUnitDetail {
                        path: "src/lib.rs".to_string(),
                        line: 1,
                        span_end_line: 3,
                        symbol: Some("AuthService".to_string()),
                        qualified_symbol: Some("auth::AuthService".to_string()),
                        kind: "struct".to_string(),
                        owner_symbol: None,
                        owner_kind: None,
                        implemented_trait: None,
                        module_path: vec!["auth".to_string()],
                        visibility: Some("pub".to_string()),
                        signature: Some("pub struct AuthService".to_string()),
                        calls: Vec::new(),
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

        assert_eq!(payload["action"], "cfg");
        assert_eq!(payload["message"], "cfg ready");
        assert_eq!(
            payload["summary"],
            "cfg summary: 1 symbols across 1 files; sample: AuthService [cfg]"
        );
        assert_eq!(payload["analysis"]["kind"], "cfg");
    }

    #[test]
    fn analysis_payload_preserves_extract_action_path_and_summary() {
        let payload = analysis_payload(
            AnalysisRenderContext {
                action: "extract",
                project_root: Path::new("/tmp/project"),
                language: SupportedLanguage::Rust,
                source: "daemon",
                message: Some("extract ready"),
                support: LanguageRegistry::support_for(SupportedLanguage::Rust),
                symbol: None,
                path: Some("src/lib.rs"),
                line: None,
            },
            &AnalysisResponse {
                kind: AnalysisKind::Extract,
                summary:
                    "extract summary: src/lib.rs => 1 symbols (1 function); imports=0, references=0; sample: main:1-1"
                        .to_string(),
                details: Some(AnalysisDetail {
                    indexed_files: 1,
                    total_symbols: 1,
                    symbol_query: None,
                    truncated: false,
                                change_paths: Vec::new(),
                    slice_target: None,
                    slice_lines: Vec::new(),
                    overview: AnalysisOverviewDetail::default(),
                    files: vec![AnalysisFileDetail {
                        path: "src/lib.rs".to_string(),
                        symbol_count: 1,
                        kinds: vec![codex_native_tldr::api::AnalysisCountDetail {
                            name: "function".to_string(),
                            count: 1,
                        }],
                    }],
                    nodes: Vec::new(),
                    edges: Vec::new(),
                    symbol_index: Vec::new(),
                    units: vec![AnalysisUnitDetail {
                        path: "src/lib.rs".to_string(),
                        line: 1,
                        span_end_line: 1,
                        symbol: Some("main".to_string()),
                        qualified_symbol: None,
                        kind: "function".to_string(),
                        owner_symbol: None,
                        owner_kind: None,
                        implemented_trait: None,
                        module_path: vec!["crate".to_string()],
                        visibility: None,
                        signature: Some("fn main()".to_string()),
                        calls: Vec::new(),
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

        assert_eq!(payload["action"], "extract");
        assert_eq!(payload["path"], "src/lib.rs");
        assert_eq!(
            payload["summary"],
            "extract summary: src/lib.rs => 1 symbols (1 function); imports=0, references=0; sample: main:1-1"
        );
        assert_eq!(payload["analysis"]["kind"], "extract");
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
                        owner_symbol: None,
                        owner_kind: None,
                        implemented_trait: None,
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
    fn render_analysis_response_text_preserves_impact_summary_contract() {
        let lines = render_analysis_response_text(
            SupportedLanguage::Rust,
            "daemon",
            LanguageRegistry::support_for(SupportedLanguage::Rust),
            Some("impact ready"),
            None,
            None,
            "impact summary: 1 symbols across 1 files (1 touched paths); dependency edges=1",
        );

        assert_eq!(
            lines,
            vec![
                "语言：rust".to_string(),
                "来源：daemon".to_string(),
                "支持级别：DataFlow".to_string(),
                "回退策略：structure + search".to_string(),
                "消息：impact ready".to_string(),
                "摘要：impact summary: 1 symbols across 1 files (1 touched paths); dependency edges=1".to_string(),
            ]
        );
    }

    #[test]
    fn render_analysis_response_text_includes_extract_path() {
        let lines = render_analysis_response_text(
            SupportedLanguage::Rust,
            "daemon",
            LanguageRegistry::support_for(SupportedLanguage::Rust),
            Some("extract ready"),
            Some("src/lib.rs"),
            None,
            "extract summary: src/lib.rs => 1 symbols (1 function); imports=0, references=0; sample: main:1-1",
        );

        assert!(lines.contains(&"路径：src/lib.rs".to_string()));
        assert!(
            lines
                .iter()
                .any(|line| line.starts_with("摘要：extract summary:"))
        );
    }

    #[test]
    fn render_analysis_response_text_includes_slice_line() {
        let lines = render_analysis_response_text(
            SupportedLanguage::Rust,
            "daemon",
            LanguageRegistry::support_for(SupportedLanguage::Rust),
            Some("slice ready"),
            Some("src/lib.rs"),
            Some(4),
            "slice summary: backward slice for src/lib.rs:login:4 -> 3 lines [1, 3, 4]",
        );

        assert!(lines.contains(&"路径：src/lib.rs".to_string()));
        assert!(lines.contains(&"行号：4".to_string()));
        assert!(
            lines
                .iter()
                .any(|line| line.starts_with("摘要：slice summary:"))
        );
    }

    #[test]
    fn render_imports_response_text_preserves_contract() {
        let lines = render_imports_response_text(
            SupportedLanguage::Rust,
            "daemon",
            Some("imports ready: src/lib.rs"),
            "src/lib.rs",
            1,
        );

        assert_eq!(
            lines,
            vec![
                "语言：rust".to_string(),
                "来源：daemon".to_string(),
                "路径：src/lib.rs".to_string(),
                "导入数：1".to_string(),
                "消息：imports ready: src/lib.rs".to_string(),
            ]
        );
    }

    #[test]
    fn render_importers_response_text_preserves_contract() {
        let lines = render_importers_response_text(
            SupportedLanguage::Rust,
            "local",
            Some("importers ready: auth::token"),
            "auth::token",
            2,
        );

        assert_eq!(
            lines,
            vec![
                "语言：rust".to_string(),
                "来源：local".to_string(),
                "模块：auth::token".to_string(),
                "匹配数：2".to_string(),
                "消息：importers ready: auth::token".to_string(),
            ]
        );
    }

    #[test]
    fn render_search_response_text_includes_match_mode() {
        let lines = render_search_response_text(
            "local",
            Some("search ready"),
            "resolveProjectAvatar(",
            "literal",
            1,
            1,
        );

        assert_eq!(
            lines,
            vec![
                "来源：local".to_string(),
                "模式：resolveProjectAvatar(".to_string(),
                "匹配语义：literal".to_string(),
                "已索引文件数：1".to_string(),
                "匹配数：1".to_string(),
                "消息：search ready".to_string(),
            ]
        );
    }

    #[test]
    fn daemon_action_and_command_maps_notify() {
        let path = PathBuf::from("src/lib.rs");
        let (action, command) =
            daemon_action_and_command(&TldrDaemonSubcommand::Notify { path: path.clone() });

        assert_eq!(action, "notify");
        assert_eq!(
            command,
            codex_native_tldr::daemon::TldrDaemonCommand::Notify { path }
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
                        owner_symbol: None,
                        owner_kind: None,
                        implemented_trait: None,
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

        assert!(lines.contains(&"查询：auth token".to_string()));
        assert!(lines.contains(&"已索引文件数：1".to_string()));
        assert!(lines.contains(&"匹配 0：src/auth.rs:2 (embedding score: 0.750)".to_string()));
        assert!(lines.contains(&"  片段：let auth_token = true;\\nverify();".to_string()));
    }

    #[test]
    fn render_diagnostics_response_text_includes_severity_and_code() {
        let lines = render_diagnostics_response_text(
            SupportedLanguage::Rust,
            "local",
            &DiagnosticsResponse {
                language: SupportedLanguage::Rust,
                path: "src/main.rs".to_string(),
                tools: vec![DiagnosticToolStatus {
                    tool: "cargo-check".to_string(),
                    available: true,
                }],
                diagnostics: vec![DiagnosticItem {
                    path: "src/main.rs".to_string(),
                    line: 7,
                    column: 3,
                    severity: DiagnosticSeverity::Error,
                    message: "unknown field".to_string(),
                    code: Some("E0609".to_string()),
                    source: "cargo-check".to_string(),
                }],
                message: "diagnostics reported 1 issues".to_string(),
            },
        );

        assert!(lines.contains(&"工具 cargo-check：true".to_string()));
        assert!(
            lines.contains(&"src/main.rs:7:3 错误 cargo-check [E0609] unknown field".to_string())
        );
    }
}

#[cfg(test)]
mod parse_tests {
    use super::CliSearchMatchMode;
    use super::TldrCli;
    use super::TldrSubcommand;
    use clap::Parser;
    use pretty_assertions::assert_eq;

    #[test]
    fn diagnostics_command_parses_filter_flags() {
        let cli = TldrCli::try_parse_from([
            "codex",
            "diagnostics",
            "--lang",
            "python",
            "--only-tool",
            "pyright",
            "--no-lint",
            "--max-issues",
            "5",
            "tool.py",
        ])
        .expect("diagnostics args should parse");

        let TldrSubcommand::Diagnostics(command) = cli.subcommand else {
            panic!("expected diagnostics subcommand");
        };
        assert_eq!(command.only_tools, vec!["pyright".to_string()]);
        assert_eq!(command.no_lint, true);
        assert_eq!(command.no_typecheck, false);
        assert_eq!(command.max_issues, 5);
    }

    #[test]
    fn doctor_command_parses_language_and_tool_filters() {
        let cli = TldrCli::try_parse_from([
            "codex",
            "doctor",
            "--lang",
            "rust",
            "--only-tool",
            "cargo-clippy",
            "--no-install-hints",
        ])
        .expect("doctor args should parse");

        let TldrSubcommand::Doctor(command) = cli.subcommand else {
            panic!("expected doctor subcommand");
        };
        assert_eq!(command.only_tools, vec!["cargo-clippy".to_string()]);
        assert_eq!(command.no_install_hints, true);
        assert!(command.lang.is_some());
    }

    #[test]
    fn search_command_defaults_to_literal_match_mode() {
        let cli = TldrCli::try_parse_from(["codex", "search", "resolveProjectAvatar("])
            .expect("search args should parse");

        let TldrSubcommand::Search(command) = cli.subcommand else {
            panic!("expected search subcommand");
        };
        assert_eq!(command.match_mode, CliSearchMatchMode::Literal);
    }

    #[test]
    fn search_command_parses_regex_match_mode() {
        let cli =
            TldrCli::try_parse_from(["codex", "search", "--match-mode", "regex", "log(in|out)"])
                .expect("search args should parse");

        let TldrSubcommand::Search(command) = cli.subcommand else {
            panic!("expected search subcommand");
        };
        assert_eq!(command.match_mode, CliSearchMatchMode::Regex);
    }
}

#[cfg(test)]
mod lifecycle_tests {
    use super::cleanup_stale_daemon_artifacts;
    use super::daemon_launcher_args;
    use super::daemon_metadata_looks_alive;
    use super::ensure_daemon_running_detailed;
    use super::launcher_lock_path_for_project;
    use super::query_daemon_with_autostart;
    use super::query_daemon_with_hooks_detailed;
    use super::render_daemon_response_text;
    use crate::tldr_cmd::CODEX_TLDR_TEST_DAEMON_BIN_ENV;
    use anyhow::Result;
    use codex_native_tldr::daemon::DegradedMode;
    use codex_native_tldr::daemon::DegradedModeKind;
    use codex_native_tldr::daemon::StructuredFailure;
    use codex_native_tldr::daemon::StructuredFailureKind;
    use codex_native_tldr::daemon::TldrDaemonCommand;
    use codex_native_tldr::daemon::TldrDaemonResponse;
    use codex_native_tldr::daemon::pid_path_for_project;
    use codex_native_tldr::daemon::socket_path_for_project;
    use codex_native_tldr::lang_support::SupportedLanguage;
    use codex_native_tldr::lifecycle::DaemonReadyResult;
    use codex_native_tldr::lifecycle::QueryHooksResult;
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
                "ztldr".into(),
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
    const CODEX_TLDR_TEST_LAUNCH_RETURN_RELEASE_ENV: &str = "CODEX_TLDR_TEST_LAUNCH_RETURN_RELEASE";
    const CODEX_TLDR_TEST_FAKE_DAEMON_PROJECT_ROOT_ENV: &str =
        "CODEX_TLDR_TEST_FAKE_DAEMON_PROJECT_ROOT";
    const CODEX_TLDR_TEST_FAKE_DAEMON_RELEASE_ENV: &str = "CODEX_TLDR_TEST_FAKE_DAEMON_RELEASE";
    const CODEX_TLDR_TEST_FAKE_DAEMON_BOOT_RELEASE_ENV: &str =
        "CODEX_TLDR_TEST_FAKE_DAEMON_BOOT_RELEASE";
    const CODEX_TLDR_TEST_FAKE_DAEMON_SPAWNED_SIGNAL_ENV: &str =
        "CODEX_TLDR_TEST_FAKE_DAEMON_SPAWNED_SIGNAL";
    const CODEX_TLDR_TEST_CONTENDER_ENTERED_SIGNAL_ENV: &str =
        "CODEX_TLDR_TEST_CONTENDER_ENTERED_SIGNAL";
    const CODEX_TLDR_TEST_CONTENDER_RELEASE_ENV: &str = "CODEX_TLDR_TEST_CONTENDER_RELEASE";
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
            imports: None,
            importers: None,
            search: None,
            diagnostics: None,
            semantic: None,
            snapshot: Some(codex_native_tldr::session::SessionSnapshot {
                cached_entries: 3,
                dirty_files: 1,
                dirty_file_threshold: 20,
                reindex_pending: true,
                background_reindex_in_progress: false,
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
                last_warm: None,
                last_structured_failure: None,
                degraded_mode_active: false,
            }),
            daemon_status: None,
            reindex_report: None,
        };

        let output = render_daemon_response_text("status", &response).join("\n");

        assert!(output.contains("最近完成的重索引：Completed"));
        assert!(output.contains("最近重索引尝试：Failed"));
        assert!(output.contains("会话重索引进行中：false"));
    }

    #[test]
    fn render_daemon_response_text_for_ping_is_minimal_and_stable() {
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

        assert_eq!(
            render_daemon_response_text("ping", &response),
            vec![
                "动作：ping".to_string(),
                "状态：ok".to_string(),
                "消息：pong".to_string()
            ]
        );
    }

    #[test]
    fn render_daemon_response_text_for_notify_lists_snapshot_details() {
        let response = TldrDaemonResponse {
            status: "ok".to_string(),
            message: "marked src/lib.rs dirty".to_string(),
            analysis: None,
            imports: None,
            importers: None,
            search: None,
            diagnostics: None,
            semantic: None,
            snapshot: Some(codex_native_tldr::session::SessionSnapshot {
                cached_entries: 2,
                dirty_files: 1,
                dirty_file_threshold: 20,
                reindex_pending: true,
                background_reindex_in_progress: false,
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

        let output = render_daemon_response_text("notify", &response).join("\n");
        assert!(output.contains("动作：notify"));
        assert!(output.contains("消息：marked src/lib.rs dirty"));
        assert!(output.contains("缓存条目数：2"));
        assert!(output.contains("脏文件数：1"));
        assert!(output.contains("重索引待处理：true"));
        assert!(output.contains("重索引进行中：false"));
    }

    #[test]
    fn render_daemon_response_text_for_snapshot_lists_snapshot_details() {
        let response = TldrDaemonResponse {
            status: "ok".to_string(),
            message: "snapshot".to_string(),
            analysis: None,
            imports: None,
            importers: None,
            search: None,
            diagnostics: None,
            semantic: None,
            snapshot: Some(codex_native_tldr::session::SessionSnapshot {
                cached_entries: 3,
                dirty_files: 1,
                dirty_file_threshold: 20,
                reindex_pending: false,
                background_reindex_in_progress: false,
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

        let output = render_daemon_response_text("snapshot", &response).join("\n");
        assert!(output.contains("动作：snapshot"));
        assert!(output.contains("消息：snapshot"));
        assert!(output.contains("缓存条目数：3"));
        assert!(output.contains("脏文件数：1"));
        assert!(output.contains("重索引待处理：false"));
        assert!(output.contains("重索引进行中：false"));
    }

    #[test]
    fn render_daemon_response_text_surfaces_status_detail_fields() {
        let started_at = std::time::SystemTime::UNIX_EPOCH;
        let response = TldrDaemonResponse {
            status: "ok".to_string(),
            message: "status".to_string(),
            analysis: None,
            imports: None,
            importers: None,
            search: None,
            diagnostics: None,
            semantic: None,
            snapshot: Some(codex_native_tldr::session::SessionSnapshot {
                cached_entries: 1,
                dirty_files: 0,
                dirty_file_threshold: 20,
                reindex_pending: false,
                background_reindex_in_progress: false,
                last_query_at: Some(started_at),
                last_reindex: None,
                last_reindex_attempt: None,
                last_warm: Some(codex_native_tldr::session::WarmReport {
                    status: codex_native_tldr::session::WarmStatus::Loaded,
                    languages: vec![SupportedLanguage::Rust],
                    started_at,
                    finished_at: started_at,
                    message: "warm loaded 1 language indexes into daemon cache".to_string(),
                }),
                last_structured_failure: None,
                degraded_mode_active: false,
            }),
            daemon_status: Some(codex_native_tldr::daemon::TldrDaemonStatus {
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
                semantic_reindex_in_progress: false,
                last_query_at: Some(started_at),
                config: codex_native_tldr::daemon::TldrDaemonConfigSummary {
                    auto_start: true,
                    socket_mode: "unix".to_string(),
                    semantic_enabled: true,
                    semantic_auto_reindex_threshold: 20,
                    session_dirty_file_threshold: 20,
                    session_idle_timeout_secs: 1800,
                },
            }),
            reindex_report: Some(codex_native_tldr::semantic::SemanticReindexReport {
                status: codex_native_tldr::semantic::SemanticReindexStatus::Completed,
                languages: vec![codex_native_tldr::lang_support::SupportedLanguage::Rust],
                indexed_files: 2,
                indexed_units: 3,
                truncated: false,
                started_at,
                finished_at: started_at,
                message: "done".to_string(),
                embedding_enabled: true,
                embedding_dimensions: 256,
            }),
        };

        let output = render_daemon_response_text("status", &response).join("\n");

        assert!(output.contains("PID 存活：true"));
        assert!(output.contains("锁已持有：true"));
        assert!(output.contains("脏文件阈值：20"));
        assert!(output.contains("会话脏文件阈值：20"));
        assert!(output.contains("会话空闲超时秒数：1800"));
        assert!(output.contains("语义自动重索引阈值：20"));
        assert!(output.contains("会话重索引进行中：false"));
        assert!(output.contains("语义重索引进行中：false"));
        assert!(output.contains("最近预热状态：Loaded"));
        assert!(output.contains("最近预热语言：rust"));
        assert!(output.contains("重索引状态：Completed"));
        assert!(output.contains("重索引文件数：2"));
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
            imports: None,
            importers: None,
            search: None,
            diagnostics: None,
            semantic: None,
            snapshot: None,
            daemon_status: None,
            reindex_report: None,
        };

        let response = query_daemon_with_hooks_detailed(
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
                        Ok(DaemonReadyResult {
                            ready: true,
                            structured_failure: None,
                            degraded_mode: None,
                        })
                    })
                }
            },
        )
        .await
        .unwrap();

        assert_eq!(response.response, Some(query_response));
        assert_eq!(response.ready_result, None);
        assert_eq!(query_calls.load(Ordering::SeqCst), 2);
        assert_eq!(ensure_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn query_daemon_with_hooks_skips_retry_when_autostart_fails() {
        let tempdir = tempdir().unwrap();
        let command = TldrDaemonCommand::Ping;
        let query_calls = Arc::new(AtomicUsize::new(0));
        let ensure_calls = Arc::new(AtomicUsize::new(0));

        let response = query_daemon_with_hooks_detailed(
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
                        Ok(DaemonReadyResult {
                            ready: false,
                            structured_failure: None,
                            degraded_mode: None,
                        })
                    })
                }
            },
        )
        .await
        .unwrap();

        assert_eq!(response.response, None);
        assert_eq!(
            response.ready_result,
            Some(DaemonReadyResult {
                ready: false,
                structured_failure: None,
                degraded_mode: None,
            })
        );
        assert_eq!(query_calls.load(Ordering::SeqCst), 1);
        assert_eq!(ensure_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn query_daemon_with_hooks_detailed_returns_ready_result_when_autostart_fails() {
        let tempdir = tempdir().unwrap();
        let command = TldrDaemonCommand::Ping;
        let query_calls = Arc::new(AtomicUsize::new(0));
        let ensure_calls = Arc::new(AtomicUsize::new(0));

        let response = query_daemon_with_hooks_detailed(
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
                        Ok(DaemonReadyResult {
                            ready: false,
                            structured_failure: Some(StructuredFailure {
                                kind: StructuredFailureKind::DaemonUnavailable,
                                reason: "daemon failed to start".to_string(),
                                retryable: true,
                                retry_hint: Some("run `codex ztldr daemon start`".to_string()),
                            }),
                            degraded_mode: Some(DegradedMode {
                                kind: DegradedModeKind::DiagnosticOnly,
                                fallback_path: "none".to_string(),
                                reason: Some("daemon-only command".to_string()),
                            }),
                        })
                    })
                }
            },
        )
        .await
        .unwrap();

        assert_eq!(
            response,
            QueryHooksResult {
                response: None,
                ready_result: Some(DaemonReadyResult {
                    ready: false,
                    structured_failure: Some(StructuredFailure {
                        kind: StructuredFailureKind::DaemonUnavailable,
                        reason: "daemon failed to start".to_string(),
                        retryable: true,
                        retry_hint: Some("run `codex ztldr daemon start`".to_string()),
                    }),
                    degraded_mode: Some(DegradedMode {
                        kind: DegradedModeKind::DiagnosticOnly,
                        fallback_path: "none".to_string(),
                        reason: Some("daemon-only command".to_string()),
                    }),
                }),
            }
        );
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
    fn daemon_metadata_cleans_stale_pid_without_socket() {
        let tempdir = tempdir().unwrap();
        let project_root = tempdir.path().join("stale-pid-clean-project");
        std::fs::create_dir(&project_root).unwrap();
        let pid_path = pid_path_for_project(&project_root);
        create_artifact_parent(&pid_path);

        std::fs::write(&pid_path, std::process::id().to_string()).unwrap();

        assert!(!daemon_metadata_looks_alive(&project_root));
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
        let contender_release = project.path().join("contender_direct.release");
        let fake_daemon_boot_release = project.path().join("fake_daemon_direct.boot_release");
        let fake_daemon_spawned = project.path().join("fake_daemon_direct.spawned");
        let launch_return_release = project.path().join("launch_return_direct.release");
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
        std::fs::remove_file(&contender_release).ok();
        std::fs::remove_file(&launcher_wait_counter).ok();
        std::fs::remove_file(&fake_daemon_release).ok();
        std::fs::remove_file(&fake_daemon_boot_release).ok();
        std::fs::remove_file(&fake_daemon_spawned).ok();
        std::fs::remove_file(&launch_return_release).ok();
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
                .arg("--nocapture")
                .env(CODEX_TLDR_TEST_PROJECT_ROOT_ENV, &canonical_project)
                .env(CODEX_TLDR_TEST_START_SIGNAL_ENV, &start_signal)
                .env(CODEX_TLDR_TEST_DONE_SIGNAL_ENV, done)
                .env(CODEX_TLDR_TEST_CONTENDER_ENTERED_SIGNAL_ENV, entered)
                .env(CODEX_TLDR_TEST_CONTENDER_RELEASE_ENV, &contender_release)
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
                .env(
                    CODEX_TLDR_TEST_LAUNCH_RETURN_RELEASE_ENV,
                    &launch_return_release,
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
        std::fs::write(&contender_release, "go")?;
        wait_for_signal(&fake_daemon_spawned);
        wait_for_signal(&launcher_wait_counter);
        std::fs::write(&launch_return_release, "release")?;
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

        let started = ensure_daemon_running_detailed(&canonical_project, true).await?;
        assert!(started.ready);
        assert!(!counter_path.exists(), "launcher lock should prevent spawn");

        unsafe { std::env::remove_var(CODEX_TLDR_TEST_DAEMON_BIN_ENV) };
        unsafe { std::env::remove_var(CODEX_TLDR_TEST_LAUNCH_COUNTER_ENV) };
        Ok(())
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn query_daemon_with_autostart_respects_disabled_auto_start_config() -> Result<()> {
        let project = tempdir()?;
        let canonical_project = project.path().canonicalize()?;
        let codex_dir = canonical_project.join(".codex");
        std::fs::create_dir(&codex_dir)?;
        std::fs::write(
            codex_dir.join("tldr.toml"),
            "[daemon]\nauto_start = false\n",
        )?;

        let counter_path = canonical_project.join("launch_counter.log");
        unsafe { std::env::set_var(CODEX_TLDR_TEST_DAEMON_BIN_ENV, "/bin/true") };
        unsafe { std::env::set_var(CODEX_TLDR_TEST_LAUNCH_COUNTER_ENV, &counter_path) };

        let response =
            query_daemon_with_autostart(&canonical_project, &TldrDaemonCommand::Ping).await?;
        assert_eq!(response, None);
        assert!(
            !counter_path.exists(),
            "disabled auto_start should skip spawning"
        );

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
        let contender_release =
            std::env::var_os(CODEX_TLDR_TEST_CONTENDER_RELEASE_ENV).map(PathBuf::from);
        let entered_signal =
            std::env::var_os(CODEX_TLDR_TEST_CONTENDER_ENTERED_SIGNAL_ENV).map(PathBuf::from);

        wait_for_signal(&start_signal);
        if let Some(entered_signal) = entered_signal {
            std::fs::write(entered_signal, "entered")?;
        }
        if let Some(contender_release) = contender_release {
            wait_for_signal(&contender_release);
        }

        let started = ensure_daemon_running_detailed(&project_root, true).await?;
        assert!(
            started.ready,
            "launcher contender should observe a live daemon"
        );
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

        unsafe { std::env::set_var(CODEX_TLDR_TEST_DAEMON_BIN_ENV, "/bin/true") };
        unsafe { std::env::set_var(CODEX_TLDR_TEST_LAUNCH_COUNTER_ENV, &counter_path) };

        let started = tokio::time::timeout(
            Duration::from_secs(6),
            ensure_daemon_running_detailed(&canonical_project, true),
        )
        .await??;

        assert!(!started.ready);
        assert!(!counter_path.exists(), "daemon should not be spawned");

        unsafe { std::env::remove_var(CODEX_TLDR_TEST_DAEMON_BIN_ENV) };
        unsafe { std::env::remove_var(CODEX_TLDR_TEST_LAUNCH_COUNTER_ENV) };
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
                        TldrDaemonCommand::Imports { .. } => "imports",
                        TldrDaemonCommand::Importers { .. } => "importers",
                        TldrDaemonCommand::Search { .. } => "search",
                        TldrDaemonCommand::Diagnostics { .. } => "diagnostics",
                        TldrDaemonCommand::Shutdown => "shutdown",
                    };
                    let response = TldrDaemonResponse {
                        status: "ok".to_string(),
                        message: message.to_string(),
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
                        TldrDaemonCommand::Imports { .. } => "imports",
                        TldrDaemonCommand::Importers { .. } => "importers",
                        TldrDaemonCommand::Search { .. } => "search",
                        TldrDaemonCommand::Diagnostics { .. } => "diagnostics",
                        TldrDaemonCommand::Shutdown => "shutdown",
                    };
                    let response = TldrDaemonResponse {
                        status: "ok".to_string(),
                        message: message.to_string(),
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
