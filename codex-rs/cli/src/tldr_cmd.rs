use anyhow::Result;
use anyhow::bail;
use clap::Parser;
use clap::Subcommand;
use clap::ValueEnum;
use codex_native_tldr::TldrEngine;
use codex_native_tldr::api::AnalysisKind;
use codex_native_tldr::api::AnalysisRequest;
use codex_native_tldr::daemon::TldrDaemonCommand;
use codex_native_tldr::daemon::query_daemon;
use codex_native_tldr::lang_support::LanguageRegistry;
use codex_native_tldr::lang_support::SupportedLanguage;
use serde_json::json;
use std::path::PathBuf;

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
    let daemon_response = query_daemon(
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

    let Some(response) = query_daemon(&project_root, &command).await? else {
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

fn analysis_cache_key(
    kind: AnalysisKind,
    language: SupportedLanguage,
    symbol: Option<&str>,
) -> String {
    let symbol = symbol.unwrap_or("*");
    format!("{}:{kind:?}:{symbol}", language.as_str())
}
