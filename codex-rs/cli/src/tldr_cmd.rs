use anyhow::Result;
use clap::Parser;
use clap::Subcommand;
use clap::ValueEnum;
use codex_native_tldr::TldrEngine;
use codex_native_tldr::api::AnalysisKind;
use codex_native_tldr::api::AnalysisRequest;
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
            run_analysis_command(cmd, AnalysisKind::Ast)?;
        }
        TldrSubcommand::Context(cmd) => {
            run_analysis_command(cmd, AnalysisKind::CallGraph)?;
        }
        TldrSubcommand::Semantic(cmd) => {
            run_semantic_command(cmd)?;
        }
    }

    Ok(())
}

fn run_analysis_command(cmd: TldrAnalyzeCommand, kind: AnalysisKind) -> Result<()> {
    let language: SupportedLanguage = cmd.lang.into();
    let engine = TldrEngine::builder(cmd.project.canonicalize()?).build();
    let support = LanguageRegistry::support_for(language);
    let response = engine.analyze(AnalysisRequest {
        kind,
        symbol: cmd.symbol.clone(),
    })?;

    if cmd.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "project": engine.config().project_root,
                "language": language.as_str(),
                "supportLevel": format!("{:?}", support.support_level),
                "fallbackStrategy": support.fallback_strategy,
                "summary": response.summary,
                "symbol": cmd.symbol,
            }))?
        );
    } else {
        println!("language: {}", language.as_str());
        println!("support: {:?}", support.support_level);
        println!("fallback: {}", support.fallback_strategy);
        println!("summary: {}", response.summary);
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
