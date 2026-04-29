use anyhow::Context;
use anyhow::Result;
use clap::Parser;
use clap::Subcommand;
use codex_core::mission::Handoff;
use codex_core::mission::MissionPlanner;
use codex_core::mission::MissionPlanningStep;
use codex_core::mission::MissionState;
use codex_core::mission::MissionStateStore;
use codex_core::mission::MissionStatusReport;
use codex_core::mission::ScrutinyValidator;
use codex_core::mission::UserTestingValidator;
use codex_core::mission::Validator;
use codex_core::mission::ValidatorConfig;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(bin_name = "codex mission")]
pub struct MissionCli {
    #[command(subcommand)]
    pub subcommand: MissionSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum MissionSubcommand {
    /// 启动新的 Mission 并进入 7 阶段规划流程。
    Start(MissionStartCommand),
    /// 显示当前工作区的 Mission 状态。
    Status,
    /// 推进当前 Mission 的规划阶段。
    Continue(MissionContinueCommand),
    /// 运行验证器并报告结果。
    Validate(MissionValidateCommand),
}

#[derive(Debug, Parser)]
pub struct MissionStartCommand {
    /// Mission 目标。
    #[arg(value_name = "目标")]
    pub goal: String,
}

#[derive(Debug, Parser)]
pub struct MissionContinueCommand {
    /// 记录当前阶段的确认说明；省略时使用默认确认文本。
    #[arg(long, value_name = "说明")]
    pub note: Option<String>,
}

#[derive(Debug, Parser)]
pub struct MissionValidateCommand {
    /// Handoff 文件路径；省略时验证最新的 Handoff。
    #[arg(long, value_name = "路径")]
    pub handoff: Option<PathBuf>,

    /// 验证器类型。
    #[arg(long, value_name = "类型", default_value = "all")]
    pub validator: ValidatorType,

    /// 严格模式（任何问题都导致失败）。
    #[arg(long)]
    pub strict: bool,

    /// 输出格式。
    #[arg(long, value_name = "格式", default_value = "markdown")]
    pub output: OutputFormat,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidatorType {
    All,
    Scrutiny,
    UserTesting,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputFormat {
    Markdown,
    Json,
}

impl std::str::FromStr for ValidatorType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "all" => Ok(Self::All),
            "scrutiny" => Ok(Self::Scrutiny),
            "user-testing" => Ok(Self::UserTesting),
            _ => anyhow::bail!("未知的验证器类型: {}", s),
        }
    }
}

impl std::str::FromStr for OutputFormat {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "markdown" => Ok(Self::Markdown),
            "json" => Ok(Self::Json),
            _ => anyhow::bail!("未知的输出格式: {}", s),
        }
    }
}

pub async fn run_mission_command(cli: MissionCli) -> Result<()> {
    match cli.subcommand {
        MissionSubcommand::Start(command) => start_mission(command),
        MissionSubcommand::Status => print_status(),
        MissionSubcommand::Continue(command) => continue_mission(command),
        MissionSubcommand::Validate(command) => validate_mission(command),
    }
}

fn start_mission(command: MissionStartCommand) -> Result<()> {
    let planner = planner_for_current_dir()?;
    let step = planner.start(command.goal)?;
    print_planning_step(step);
    Ok(())
}

fn print_status() -> Result<()> {
    let store = MissionStateStore::for_workspace(current_workspace()?);
    match store.status_report()? {
        MissionStatusReport::Empty { state_path } => print_empty_status(state_path),
        MissionStatusReport::Active { state_path, state } => print_active_status(state_path, state),
    }
    Ok(())
}

fn continue_mission(command: MissionContinueCommand) -> Result<()> {
    let planner = planner_for_current_dir()?;
    let step = planner.continue_planning(command.note)?;
    print_planning_step(step);
    Ok(())
}

fn validate_mission(command: MissionValidateCommand) -> Result<()> {
    let handoff = load_handoff(command.handoff.as_deref())?;
    let config = ValidatorConfig {
        strict: command.strict,
        verbose: true,
    };

    match command.validator {
        ValidatorType::All => {
            // 运行所有验证器
            let scrutiny_validator = ScrutinyValidator::new(config.clone());
            let user_testing_validator = UserTestingValidator::new(config);

            let scrutiny_report = scrutiny_validator.validate(&handoff);
            let user_testing_report = user_testing_validator.validate(&handoff);

            // 输出报告
            match command.output {
                OutputFormat::Markdown => {
                    println!("{}", scrutiny_validator.report_as_markdown(&scrutiny_report));
                    println!("\n---\n\n");
                    println!("{}", user_testing_validator.report_as_markdown(&user_testing_report));
                }
                OutputFormat::Json => {
                    let output = serde_json::json!({
                        "scrutiny": scrutiny_report,
                        "user_testing": user_testing_report,
                    });
                    println!("{}", serde_json::to_string_pretty(&output)?);
                }
            }
        }
        ValidatorType::Scrutiny => {
            let validator = ScrutinyValidator::new(config);
            let report = validator.validate(&handoff);

            match command.output {
                OutputFormat::Markdown => {
                    println!("{}", validator.report_as_markdown(&report));
                }
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&report)?);
                }
            }
        }
        ValidatorType::UserTesting => {
            let validator = UserTestingValidator::new(config);
            let report = validator.validate(&handoff);

            match command.output {
                OutputFormat::Markdown => {
                    println!("{}", validator.report_as_markdown(&report));
                }
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&report)?);
                }
            }
        }
    }

    Ok(())
}

fn load_handoff(handoff_path: Option<&Path>) -> Result<Handoff> {
    if let Some(path) = handoff_path {
        // 从指定路径加载
        Handoff::load_from(path).with_context(|| format!("无法加载 Handoff 文件: {}", path.display()))
    } else {
        // 从 Mission 目录加载最新的 Handoff
        let workspace = current_workspace()?;
        let mission_dir = workspace.join(".mission").join("handoffs");

        if !mission_dir.exists() {
            anyhow::bail!("Handoff 目录不存在: {}", mission_dir.display());
        }

        // 查找最新的 Handoff 文件
        let mut handoffs: Vec<_> = fs::read_dir(&mission_dir)
            .with_context(|| format!("无法读取 Handoff 目录: {}", mission_dir.display()))?
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("json") {
                    Some(path)
                } else {
                    None
                }
            })
            .collect();

        if handoffs.is_empty() {
            anyhow::bail!("未找到任何 Handoff 文件: {}", mission_dir.display());
        }

        // 按修改时间排序，取最新的
        handoffs.sort_by_key(|path| {
            path.metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
        });

        let latest_handoff = handoffs.last().unwrap();
        Handoff::load_from(latest_handoff).with_context(|| {
            format!("无法加载 Handoff 文件: {}", latest_handoff.display())
        })
    }
}

fn planner_for_current_dir() -> Result<MissionPlanner> {
    Ok(MissionPlanner::for_workspace(current_workspace()?))
}

fn current_workspace() -> Result<PathBuf> {
    std::env::current_dir().context("无法读取当前工作目录")
}

fn print_empty_status(state_path: PathBuf) {
    println!("Mission 状态：未启动");
    println!("状态文件：{}", state_path.display());
}

fn print_active_status(state_path: PathBuf, state: MissionState) {
    println!("Mission 状态：{}", state.status.label());
    println!("目标：{}", state.goal);
    if let Some(phase) = state.phase {
        println!("阶段：{}", phase.label());
    }
    println!("状态文件：{}", state_path.display());
}

fn print_planning_step(step: MissionPlanningStep) {
    println!("Mission 状态：{}", step.state.status.label());
    println!("目标：{}", step.state.goal);
    if let Some(definition) = step.definition {
        println!(
            "当前阶段：{} ({})",
            definition.title,
            definition.phase.label()
        );
        println!("提示：{}", definition.prompt);
        println!("出口条件：{}", definition.exit_condition);
        return;
    }
    println!("规划阶段已完成，Mission 进入执行状态。");
}
