use crate::zoffsec_config::ZoffsecTemplate;
use crate::zoffsec_config::render_zoffsec_base_instructions;
use anyhow::Context;
use clap::ArgGroup;
use clap::Args;
use clap::Parser;
use clap::Subcommand;
use codex_core::INTERACTIVE_SESSION_SOURCES;
use codex_core::RolloutRecorder;
use codex_core::SortDirection;
use codex_core::ThreadSortKey;
use codex_core::config::Config;
use codex_core::find_thread_path_by_id_str;
use codex_rollout::ZOFFSEC_CLEAN_DEFAULT_REPLACEMENT;
use codex_rollout::ZoffsecCleanOptions;
use codex_rollout::clean_zoffsec_rollout;
use codex_tui::Cli as TuiCli;
use codex_utils_cli::CliConfigOverrides;
use std::path::PathBuf;

#[derive(Debug, Parser)]
pub struct ZoffsecCommand {
    #[command(flatten)]
    pub start: ZoffsecStartCommand,

    #[command(subcommand)]
    pub subcommand: Option<ZoffsecSubcommand>,
}

#[derive(Debug, Args)]
pub struct ZoffsecStartCommand {
    /// 要使用的内置 zoffsec 模板。
    #[arg(
        long = "template",
        default_value = "default",
        value_enum,
        value_name = "模板",
        help = "要使用的内置 zoffsec 模板。可选值：default、web、reverse。"
    )]
    pub template: ZoffsecTemplate,

    #[clap(flatten)]
    pub interactive: TuiCli,
}

#[derive(Debug, Subcommand)]
pub enum ZoffsecSubcommand {
    /// 清理指定 zoffsec rollout 中的拒绝消息与 reasoning 记录。
    Clean(ZoffsecCleanCommand),

    /// 恢复 zoffsec 会话，并在恢复前显式执行 clean。
    #[clap(visible_alias = "r")]
    Resume(Box<ZoffsecResumeCommand>),
}

#[derive(Debug, Args)]
#[command(group(
    ArgGroup::new("target")
        .args(["last", "id", "path"])
        .multiple(false)
))]
pub struct ZoffsecCleanCommand {
    /// 直接清理最近一次交互会话；若未指定目标，默认等价于 `--last`。
    #[arg(long = "last", default_value_t = false)]
    pub last: bool,

    /// 按会话 ID（UUID）定位 rollout。
    #[arg(long = "id", value_name = "会话ID")]
    pub id: Option<String>,

    /// 直接指定 rollout 文件路径。
    #[arg(long = "path", value_name = "路径")]
    pub path: Option<PathBuf>,

    /// 仅输出预览摘要，不写回文件。
    #[arg(long = "dry-run", default_value_t = false)]
    pub dry_run: bool,

    /// 跳过默认备份。
    #[arg(long = "no-backup", default_value_t = false)]
    pub no_backup: bool,

    /// 用于替换拒绝消息的文本。
    #[arg(
        long = "replacement",
        value_name = "文本",
        default_value = ZOFFSEC_CLEAN_DEFAULT_REPLACEMENT
    )]
    pub replacement: String,
}

#[derive(Debug, Args)]
pub struct ZoffsecResumeCommand {
    /// 会话 ID（UUID）或线程名。省略时默认打开选择器；使用 --last 可继续最近一次会话。
    #[arg(value_name = "会话ID")]
    pub session_id: Option<String>,

    /// 不显示选择器，直接继续最近一次会话。
    #[arg(long = "last", default_value_t = false)]
    pub last: bool,

    /// 显示所有会话（关闭 cwd 过滤并显示 CWD 列）。
    #[arg(long = "all", default_value_t = false)]
    pub all: bool,

    #[clap(flatten)]
    pub interactive: TuiCli,
}

pub fn apply_zoffsec_overrides(
    config_overrides: &mut CliConfigOverrides,
    template: ZoffsecTemplate,
) {
    config_overrides
        .raw_overrides
        .push(format_zoffsec_override(template));
}

pub async fn run_zoffsec_clean_command(
    command: ZoffsecCleanCommand,
    root_config_overrides: CliConfigOverrides,
) -> anyhow::Result<()> {
    let cli_overrides = root_config_overrides
        .parse_overrides()
        .map_err(anyhow::Error::msg)?;
    let config = Config::load_with_cli_overrides(cli_overrides)
        .await
        .context("加载 zoffsec clean 配置失败")?;

    let target_path = resolve_target_path(&config, &command).await?;
    let summary = clean_zoffsec_rollout(
        target_path.as_path(),
        &ZoffsecCleanOptions {
            replacement: command.replacement,
            dry_run: command.dry_run,
            create_backup: !command.no_backup,
        },
    )
    .await?;

    let mode = if command.dry_run { "预览" } else { "执行" };
    println!("{mode}: {}", summary.path.display());
    println!("模板: {}", summary.template);
    match summary.backup_path.as_ref() {
        Some(backup_path) => println!("备份: {}", backup_path.display()),
        None if command.dry_run => println!("备份: dry-run 未生成"),
        None if command.no_backup => println!("备份: 已禁用"),
        None => println!("备份: 未生成"),
    }
    println!(
        "assistant 拒绝替换: {}",
        summary.assistant_messages_replaced
    );
    println!("event_msg 同步替换: {}", summary.event_messages_replaced);
    println!("reasoning 删除: {}", summary.reasoning_items_removed);
    println!(
        "发生写回: {}",
        if summary.changed && !command.dry_run {
            "yes"
        } else {
            "no"
        }
    );

    Ok(())
}

fn format_zoffsec_override(template: ZoffsecTemplate) -> String {
    let base_instructions = toml::Value::String(render_zoffsec_base_instructions(template));
    format!("base_instructions={base_instructions}")
}

async fn resolve_target_path(
    config: &Config,
    command: &ZoffsecCleanCommand,
) -> anyhow::Result<PathBuf> {
    if let Some(path) = command.path.as_ref() {
        return Ok(path.clone());
    }

    if let Some(id) = command.id.as_ref() {
        return find_thread_path_by_id_str(config.codex_home.as_path(), id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("未找到会话 `{id}` 对应的 rollout 文件"));
    }

    let page = RolloutRecorder::list_threads(
        config,
        1,
        None,
        ThreadSortKey::UpdatedAt,
        SortDirection::Desc,
        INTERACTIVE_SESSION_SOURCES.as_slice(),
        None,
        config.model_provider_id.as_str(),
        None,
    )
    .await?;
    let thread = page
        .items
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("未找到可清理的交互会话"))?;
    Ok(thread.path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zoffsec_config::ZOFFSEC_SESSION_MARKER;
    use pretty_assertions::assert_eq;

    #[test]
    fn zoffsec_override_uses_base_instructions_string() {
        let override_value = format_zoffsec_override(ZoffsecTemplate::Web);

        assert!(override_value.starts_with("base_instructions="));
        assert!(override_value.contains(ZOFFSEC_SESSION_MARKER));
        assert!(override_value.contains("template=web"));
    }

    #[test]
    fn zoffsec_apply_overrides_appends_highest_priority_override() {
        let mut config_overrides = CliConfigOverrides {
            raw_overrides: vec!["model=\"o3\"".to_string()],
        };

        apply_zoffsec_overrides(&mut config_overrides, ZoffsecTemplate::Reverse);

        assert_eq!(
            config_overrides.raw_overrides,
            vec![
                "model=\"o3\"".to_string(),
                format_zoffsec_override(ZoffsecTemplate::Reverse),
            ]
        );
    }

    #[test]
    fn zoffsec_command_defaults_to_start_template() {
        let command = ZoffsecCommand::parse_from(["zoffsec", "find the flag"]);

        assert_eq!(command.start.template, ZoffsecTemplate::Default);
        assert_eq!(
            command.start.interactive.prompt.as_deref(),
            Some("find the flag")
        );
        assert!(command.subcommand.is_none());
    }

    #[test]
    fn zoffsec_command_parses_clean_subcommand() {
        let command =
            ZoffsecCommand::parse_from(["zoffsec", "clean", "--id", "session-123", "--dry-run"]);

        match command.subcommand {
            Some(ZoffsecSubcommand::Clean(clean)) => {
                assert_eq!(clean.id.as_deref(), Some("session-123"));
                assert!(clean.dry_run);
                assert!(!clean.last);
            }
            _ => panic!("expected clean subcommand"),
        }
    }

    #[test]
    fn zoffsec_command_parses_resume_subcommand() {
        let command =
            ZoffsecCommand::parse_from(["zoffsec", "resume", "--last", "-m", "gpt-5.1-test"]);

        match command.subcommand {
            Some(ZoffsecSubcommand::Resume(resume)) => {
                assert!(resume.last);
                assert_eq!(resume.session_id, None);
                assert_eq!(resume.interactive.model.as_deref(), Some("gpt-5.1-test"));
            }
            _ => panic!("expected resume subcommand"),
        }
    }

    #[test]
    fn zoffsec_command_parses_short_resume_alias() {
        let command = ZoffsecCommand::parse_from(["zoffsec", "r", "session-123"]);

        match command.subcommand {
            Some(ZoffsecSubcommand::Resume(resume)) => {
                assert_eq!(resume.session_id.as_deref(), Some("session-123"));
                assert!(!resume.last);
            }
            _ => panic!("expected short resume alias"),
        }
    }
}
