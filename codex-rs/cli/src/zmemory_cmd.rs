use anyhow::Result;
use clap::Parser;
use clap::Subcommand;
use clap::ValueEnum;
use codex_core::config::ConfigBuilder;
use codex_core::config::find_codex_home;
use codex_zmemory::tool_api::ZmemoryToolAction;
use codex_zmemory::tool_api::ZmemoryToolCallParam;
use codex_zmemory::tool_api::run_zmemory_tool_with_context;
use std::path::PathBuf;

#[derive(Debug, Parser)]
pub struct ZmemoryCli {
    #[command(subcommand)]
    pub subcommand: ZmemorySubcommand,
}

#[derive(Debug, Subcommand)]
pub enum ZmemorySubcommand {
    /// 读取指定 URI 的记忆节点。
    Read(ZmemoryReadCommand),
    /// 执行全文搜索。
    Search(ZmemorySearchCommand),
    /// 创建新记忆节点。
    Create(ZmemoryCreateCommand),
    /// 更新现有记忆节点。
    Update(ZmemoryUpdateCommand),
    /// 删除指定路径。
    #[command(name = "delete-path")]
    DeletePath(ZmemoryDeletePathCommand),
    /// 为现有节点新增别名路径。
    #[command(name = "add-alias")]
    AddAlias(ZmemoryAddAliasCommand),
    /// 管理关键词触发器。
    #[command(name = "manage-triggers")]
    ManageTriggers(ZmemoryManageTriggersCommand),
    /// 输出统计信息。
    Stats(ZmemoryOutputCommand),
    /// 运行一致性检查。
    Doctor(ZmemoryOutputCommand),
    /// 重建搜索投影与 FTS。
    #[command(name = "rebuild-search")]
    RebuildSearch(ZmemoryOutputCommand),
    /// 导出内置系统视图。
    Export(ZmemoryExportCommand),
}

#[derive(Debug, Parser)]
pub struct ZmemoryReadCommand {
    #[arg(value_name = "URI")]
    pub uri: String,
    #[command(flatten)]
    pub output: ZmemoryOutputCommand,
}

#[derive(Debug, Parser)]
pub struct ZmemorySearchCommand {
    #[arg(value_name = "查询")]
    pub query: String,
    #[arg(long, value_name = "URI")]
    pub uri: Option<String>,
    #[arg(long, value_name = "限制")]
    pub limit: Option<usize>,
    #[command(flatten)]
    pub output: ZmemoryOutputCommand,
}

#[derive(Debug, Parser)]
pub struct ZmemoryCreateCommand {
    #[arg(value_name = "URI")]
    pub uri: Option<String>,
    #[arg(long, value_name = "父URI")]
    pub parent_uri: Option<String>,
    #[arg(long, value_name = "内容")]
    pub content: String,
    #[arg(long, value_name = "标题")]
    pub title: Option<String>,
    #[arg(long, value_name = "优先级")]
    pub priority: Option<i64>,
    #[arg(long, value_name = "披露")]
    pub disclosure: Option<String>,
    #[command(flatten)]
    pub output: ZmemoryOutputCommand,
}

#[derive(Debug, Parser)]
pub struct ZmemoryUpdateCommand {
    #[arg(value_name = "URI")]
    pub uri: String,
    #[arg(long, value_name = "内容")]
    pub content: Option<String>,
    #[arg(long, value_name = "旧文本")]
    pub old_string: Option<String>,
    #[arg(long, value_name = "新文本")]
    pub new_string: Option<String>,
    #[arg(long, value_name = "追加")]
    pub append: Option<String>,
    #[arg(long, value_name = "优先级")]
    pub priority: Option<i64>,
    #[arg(long, value_name = "披露")]
    pub disclosure: Option<String>,
    #[command(flatten)]
    pub output: ZmemoryOutputCommand,
}

#[derive(Debug, Parser)]
pub struct ZmemoryDeletePathCommand {
    #[arg(value_name = "URI")]
    pub uri: String,
    #[command(flatten)]
    pub output: ZmemoryOutputCommand,
}

#[derive(Debug, Parser)]
pub struct ZmemoryAddAliasCommand {
    #[arg(value_name = "新URI")]
    pub new_uri: String,
    #[arg(value_name = "目标URI")]
    pub target_uri: String,
    #[arg(long, value_name = "优先级")]
    pub priority: Option<i64>,
    #[arg(long, value_name = "披露")]
    pub disclosure: Option<String>,
    #[command(flatten)]
    pub output: ZmemoryOutputCommand,
}

#[derive(Debug, Parser)]
pub struct ZmemoryManageTriggersCommand {
    #[arg(value_name = "URI")]
    pub uri: String,
    #[arg(long = "add", value_name = "关键词")]
    pub add: Vec<String>,
    #[arg(long = "remove", value_name = "关键词")]
    pub remove: Vec<String>,
    #[command(flatten)]
    pub output: ZmemoryOutputCommand,
}

#[derive(Debug, Parser, Default)]
pub struct ZmemoryOutputCommand {
    #[arg(long, default_value_t = false)]
    pub json: bool,
    #[arg(long, value_name = "路径")]
    pub codex_home: Option<PathBuf>,
}

#[derive(Clone, Debug, ValueEnum)]
pub enum ZmemoryExportTarget {
    Boot,
    Defaults,
    Workspace,
    Index,
    Recent,
    Glossary,
    Alias,
}

#[derive(Debug, Parser)]
pub struct ZmemoryExportCommand {
    #[arg(value_enum)]
    pub target: ZmemoryExportTarget,
    #[arg(long, value_name = "域")]
    pub domain: Option<String>,
    #[arg(long, value_name = "限制")]
    pub limit: Option<usize>,
    #[command(flatten)]
    pub output: ZmemoryOutputCommand,
}

pub async fn run_zmemory_command(cli: ZmemoryCli) -> Result<()> {
    let (args, output) = match cli.subcommand {
        ZmemorySubcommand::Read(command) => (
            ZmemoryToolCallParam {
                action: ZmemoryToolAction::Read,
                uri: Some(command.uri),
                ..ZmemoryToolCallParam::default()
            },
            command.output,
        ),
        ZmemorySubcommand::Search(command) => (
            ZmemoryToolCallParam {
                action: ZmemoryToolAction::Search,
                uri: command.uri,
                query: Some(command.query),
                limit: command.limit,
                ..ZmemoryToolCallParam::default()
            },
            command.output,
        ),
        ZmemorySubcommand::Create(command) => (
            ZmemoryToolCallParam {
                action: ZmemoryToolAction::Create,
                uri: command.uri,
                parent_uri: command.parent_uri,
                content: Some(command.content),
                title: command.title,
                priority: command.priority,
                disclosure: command.disclosure,
                ..ZmemoryToolCallParam::default()
            },
            command.output,
        ),
        ZmemorySubcommand::Update(command) => (
            ZmemoryToolCallParam {
                action: ZmemoryToolAction::Update,
                uri: Some(command.uri),
                content: command.content,
                old_string: command.old_string,
                new_string: command.new_string,
                append: command.append,
                priority: command.priority,
                disclosure: command.disclosure,
                ..ZmemoryToolCallParam::default()
            },
            command.output,
        ),
        ZmemorySubcommand::DeletePath(command) => (
            ZmemoryToolCallParam {
                action: ZmemoryToolAction::DeletePath,
                uri: Some(command.uri),
                ..ZmemoryToolCallParam::default()
            },
            command.output,
        ),
        ZmemorySubcommand::AddAlias(command) => (
            ZmemoryToolCallParam {
                action: ZmemoryToolAction::AddAlias,
                new_uri: Some(command.new_uri),
                target_uri: Some(command.target_uri),
                priority: command.priority,
                disclosure: command.disclosure,
                ..ZmemoryToolCallParam::default()
            },
            command.output,
        ),
        ZmemorySubcommand::ManageTriggers(command) => (
            ZmemoryToolCallParam {
                action: ZmemoryToolAction::ManageTriggers,
                uri: Some(command.uri),
                add: Some(command.add),
                remove: Some(command.remove),
                ..ZmemoryToolCallParam::default()
            },
            command.output,
        ),
        ZmemorySubcommand::Stats(output) => (
            ZmemoryToolCallParam {
                action: ZmemoryToolAction::Stats,
                ..ZmemoryToolCallParam::default()
            },
            output,
        ),
        ZmemorySubcommand::Doctor(output) => (
            ZmemoryToolCallParam {
                action: ZmemoryToolAction::Doctor,
                ..ZmemoryToolCallParam::default()
            },
            output,
        ),
        ZmemorySubcommand::RebuildSearch(output) => (
            ZmemoryToolCallParam {
                action: ZmemoryToolAction::RebuildSearch,
                ..ZmemoryToolCallParam::default()
            },
            output,
        ),
        ZmemorySubcommand::Export(command) => (
            ZmemoryToolCallParam {
                action: ZmemoryToolAction::Read,
                uri: Some(export_uri(&command)),
                limit: command.limit,
                ..ZmemoryToolCallParam::default()
            },
            command.output,
        ),
    };

    let codex_home = output.codex_home.unwrap_or(find_codex_home()?);
    let cwd = std::env::current_dir()?;
    let config = ConfigBuilder::default()
        .codex_home(codex_home.clone())
        .fallback_cwd(Some(cwd.clone()))
        .build()
        .await?;
    let result = run_zmemory_tool_with_context(
        &codex_home,
        config.cwd.as_path(),
        config.zmemory.path.as_deref(),
        Some(config.zmemory.to_runtime_settings()),
        args,
    )?;
    if output.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&result.structured_content)?
        );
    } else {
        println!("{}", result.text);
    }
    Ok(())
}

fn export_uri(command: &ZmemoryExportCommand) -> String {
    match command.target {
        ZmemoryExportTarget::Boot => "system://boot".to_string(),
        ZmemoryExportTarget::Defaults => "system://defaults".to_string(),
        ZmemoryExportTarget::Workspace => "system://workspace".to_string(),
        ZmemoryExportTarget::Index => match command.domain.as_deref() {
            Some(domain) => format!("system://index/{domain}"),
            None => "system://index".to_string(),
        },
        ZmemoryExportTarget::Recent => match command.limit {
            Some(limit) => format!("system://recent/{limit}"),
            None => "system://recent".to_string(),
        },
        ZmemoryExportTarget::Glossary => "system://glossary".to_string(),
        ZmemoryExportTarget::Alias => match command.limit {
            Some(limit) => format!("system://alias/{limit}"),
            None => "system://alias".to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn export_command(target: ZmemoryExportTarget) -> ZmemoryExportCommand {
        ZmemoryExportCommand {
            target,
            domain: None,
            limit: None,
            output: ZmemoryOutputCommand::default(),
        }
    }

    #[test]
    fn export_uri_supports_defaults_and_workspace_views() {
        assert_eq!(
            export_uri(&export_command(ZmemoryExportTarget::Defaults)),
            "system://defaults"
        );
        assert_eq!(
            export_uri(&export_command(ZmemoryExportTarget::Workspace)),
            "system://workspace"
        );
    }
}
