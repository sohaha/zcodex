use anyhow::Context;
use anyhow::Result;
use clap::Parser;
use clap::Subcommand;
use codex_arg0::Arg0DispatchPaths;
use codex_core::config_loader::LoaderOverrides;
use codex_core::mission::Handoff;
use codex_core::mission::MissionPhaseDefinition;
use codex_core::mission::MissionPlanner;
use codex_core::mission::MissionPlanningStep;
use codex_core::mission::MissionState;
use codex_core::mission::MissionStateStore;
use codex_core::mission::MissionStatusReport;
use codex_core::mission::ScrutinyValidator;
use codex_core::mission::UserTestingValidator;
use codex_core::mission::Validator;
use codex_core::mission::ValidatorConfig;
use codex_tui::Cli as TuiCli;
use codex_utils_cli::CliConfigOverrides;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(bin_name = "codex zmission")]
pub struct ZmissionCli {
    #[command(subcommand)]
    pub subcommand: ZmissionSubcommand,

    #[clap(flatten)]
    pub config_overrides: CliConfigOverrides,
}

#[derive(Debug, Subcommand)]
pub enum ZmissionSubcommand {
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
    /// 跳过 Git 仓库检查。
    #[arg(long = "skip-git-repo-check", default_value_t = false)]
    pub skip_git_repo_check: bool,

    /// Mission 目标；省略时直接进入 TUI 输入。
    #[arg(value_name = "目标")]
    pub goal: Option<String>,
}

#[derive(Debug, Parser)]
pub struct MissionContinueCommand {
    /// 跳过 Git 仓库检查。
    #[arg(long = "skip-git-repo-check", default_value_t = false)]
    pub skip_git_repo_check: bool,

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

pub async fn run_zmission_command(
    cli: ZmissionCli,
    arg0_paths: Arg0DispatchPaths,
    root_config_overrides: CliConfigOverrides,
) -> Result<()> {
    match cli.subcommand {
        ZmissionSubcommand::Start(command) => {
            start_mission(command, arg0_paths, root_config_overrides).await
        }
        ZmissionSubcommand::Status => print_status(),
        ZmissionSubcommand::Continue(_command) => {
            // 读取当前 Mission 状态，根据阶段决定 TUI prompt，直接进入 TUI
            let store = MissionStateStore::for_workspace(current_workspace()?);
            let state = match store.status_report()? {
                MissionStatusReport::Empty { .. } => {
                    anyhow::bail!("没有活跃的 Mission。请先运行 `codex zmission start`");
                }
                MissionStatusReport::Active { state, .. } => state,
            };
            let prompt = if state.phase.is_some() {
                // 仍在规划阶段：TUI 中显示继续按钮
                None
            } else {
                // 已进入执行阶段：注入执行 prompt
                build_execution_prompt(&planner_for_current_dir()?, &state.goal)
            };
            launch_tui_with_prompt(
                prompt,
                arg0_paths,
                root_config_overrides,
                /*mission_mode*/ true,
            )
            .await
        }
        ZmissionSubcommand::Validate(command) => validate_mission(command),
    }
}

async fn start_mission(
    command: MissionStartCommand,
    arg0_paths: Arg0DispatchPaths,
    root_config_overrides: CliConfigOverrides,
) -> Result<()> {
    let Some(goal) = command.goal else {
        // 无目标时直接启动 TUI，让用户在 TUI 中输入
        return launch_tui_with_prompt(
            None,
            arg0_paths,
            root_config_overrides,
            /*mission_mode*/ true,
        )
        .await;
    };

    let planner = planner_for_current_dir()?;
    let step = planner.start(goal)?;
    print_planning_step(&step);
    run_phases_loop(
        step,
        planner,
        arg0_paths,
        root_config_overrides,
        command.skip_git_repo_check,
    )
    .await
}

/// 在当前阶段完成后自动提示用户继续下一阶段，直到规划完成。
async fn run_phases_loop(
    mut step: MissionPlanningStep,
    planner: MissionPlanner,
    arg0_paths: Arg0DispatchPaths,
    root_config_overrides: CliConfigOverrides,
    skip_git_repo_check: bool,
) -> Result<()> {
    loop {
        launch_tui_for_phase(
            &step,
            &planner,
            arg0_paths.clone(),
            root_config_overrides.clone(),
            skip_git_repo_check,
        )
        .await?;

        let Some(definition) = step.definition else {
            return launch_tui_with_prompt(
                Some(format!("开始执行 Mission：{}", step.state.goal)),
                arg0_paths,
                root_config_overrides,
                /*mission_mode*/ true,
            )
            .await;
        };

        let next_phase = match definition.phase.next() {
            Some(next) => next,
            None => {
                return launch_tui_with_prompt(
                    Some(format!("开始执行 Mission：{}", step.state.goal)),
                    arg0_paths,
                    root_config_overrides,
                    /*mission_mode*/ true,
                )
                .await;
            }
        };

        let next_def = codex_core::mission::phase_definition(next_phase);
        println!("\n{}", "-".repeat(60));
        println!(
            "阶段 [{}] 已完成。下一阶段：{} ({})",
            definition.phase.label(),
            next_def.title,
            next_phase.label(),
        );
        println!("提示：{}", next_def.prompt);
        println!("{}", "-".repeat(60));

        if !confirm_continue()? {
            println!("已暂停。随时运行 `codex zmission continue` 继续下一阶段。");
            return Ok(());
        }

        let note = Some(format!("{} 阶段已完成，继续推进", definition.phase.label()));
        if let Err(e) = planner.ensure_phase_artifact(definition.phase, note.as_deref()) {
            eprintln!("⚠️ 产物文件创建失败: {e}");
        }
        step = planner.continue_planning(note)?;
        print_planning_step(&step);
    }
}

/// 询问用户是否继续下一阶段。
fn confirm_continue() -> std::io::Result<bool> {
    print!("\n继续下一阶段？[Y/n] ");
    std::io::stdout().flush()?;
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    let answer = input.trim();
    Ok(answer.is_empty() || answer.eq_ignore_ascii_case("y") || answer.eq_ignore_ascii_case("yes"))
}

fn print_status() -> Result<()> {
    let store = MissionStateStore::for_workspace(current_workspace()?);
    match store.status_report()? {
        MissionStatusReport::Empty { state_path } => print_empty_status(state_path),
        MissionStatusReport::Active { state_path, state } => print_active_status(state_path, state),
    }
    Ok(())
}

/// 为当前规划阶段构造 prompt，然后启动 TUI 来执行该阶段的分析。
async fn launch_tui_for_phase(
    step: &MissionPlanningStep,
    planner: &MissionPlanner,
    arg0_paths: Arg0DispatchPaths,
    root_config_overrides: CliConfigOverrides,
    _skip_git_repo_check: bool,
) -> Result<()> {
    let Some(definition) = step.definition else {
        return Ok(());
    };

    let artifact_path = planner.phase_artifact_path(definition.phase);
    let prompt = format!(
        "你正在执行一个 Mission 规划流程。\n\n\
         Mission 目标：{goal}\n\n\
         当前阶段：{title} ({phase})\n\
         阶段提示：{prompt_text}\n\
         出口条件：{exit_condition}\n\n\
         请将本阶段的分析结果写入产物文件：`{artifact_path}`
         产物文件必须存在且非空，否则无法推进到下一阶段。

         请根据上述信息完成当前阶段的分析。",
        goal = step.state.goal,
        title = definition.title,
        phase = definition.phase.label(),
        prompt_text = definition.prompt,
        exit_condition = definition.exit_condition,
        artifact_path = artifact_path.display(),
    );

    launch_tui_with_prompt(
        Some(prompt),
        arg0_paths,
        root_config_overrides,
        /*mission_mode*/ true,
    )
    .await
}

/// 使用给定 prompt 启动交互式 TUI。
async fn launch_tui_with_prompt(
    prompt: Option<String>,
    arg0_paths: Arg0DispatchPaths,
    root_config_overrides: CliConfigOverrides,
    mission_mode: bool,
) -> Result<()> {
    let mut tui_cli = TuiCli::try_parse_from(["codex"])?;
    tui_cli.mission_mode = mission_mode;
    tui_cli.prompt = prompt;
    prepend_config_flags(&mut tui_cli.config_overrides, root_config_overrides);
    codex_tui::run_main(
        tui_cli,
        arg0_paths,
        LoaderOverrides::default(),
        /*remote*/ None,
        /*remote_auth_token*/ None,
    )
    .await
    .map_err(|e| anyhow::anyhow!("TUI 退出异常: {e}"))?;
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
            let scrutiny_validator = ScrutinyValidator::new(config.clone());
            let user_testing_validator = UserTestingValidator::new(config);

            let scrutiny_report = scrutiny_validator.validate(&handoff);
            let user_testing_report = user_testing_validator.validate(&handoff);

            match command.output {
                OutputFormat::Markdown => {
                    println!(
                        "{}",
                        scrutiny_validator.report_as_markdown(&scrutiny_report)
                    );
                    println!("\n---\n\n");
                    println!(
                        "{}",
                        user_testing_validator.report_as_markdown(&user_testing_report)
                    );
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
        Handoff::load_from(path)
            .with_context(|| format!("无法加载 Handoff 文件: {}", path.display()))
    } else {
        let workspace = current_workspace()?;
        let mission_dir = workspace.join(".mission").join("handoffs");

        if !mission_dir.exists() {
            anyhow::bail!("Handoff 目录不存在: {}", mission_dir.display());
        }

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

        handoffs.sort_by_key(|path| {
            path.metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
        });

        let latest_handoff = handoffs.last().unwrap();
        Handoff::load_from(latest_handoff)
            .with_context(|| format!("无法加载 Handoff 文件: {}", latest_handoff.display()))
    }
}

fn planner_for_current_dir() -> Result<MissionPlanner> {
    Ok(MissionPlanner::for_workspace(current_workspace()?))
}

fn current_workspace() -> Result<PathBuf> {
    std::env::current_dir().context("无法读取当前工作目录")
}

fn prepend_config_flags(target: &mut CliConfigOverrides, source: CliConfigOverrides) {
    target.raw_overrides.splice(0..0, source.raw_overrides);
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

fn print_planning_step(step: &MissionPlanningStep) {
    println!("Mission 状态：{}", step.state.status.label());
    println!("目标：{}", step.state.goal);
    if let Some(definition) = step.definition {
        print_phase_details(&definition);
    } else {
        println!("规划阶段已完成，Mission 进入执行状态。");
    }
}

fn print_phase_details(definition: &MissionPhaseDefinition) {
    println!(
        "当前阶段：{} ({})",
        definition.title,
        definition.phase.label()
    );
    println!("提示：{}", definition.prompt);
    println!("出口条件：{}", definition.exit_condition);
    println!("产物文件：{}", definition.artifact_filename);
}

/// 构建执行 prompt，尝试加载 `.agents/mission/plan.md` 中的方案内容。
fn build_execution_prompt(planner: &MissionPlanner, goal: &str) -> Option<String> {
    match planner.load_execution_plan() {
        Ok(plan_content) => Some(format!(
            "开始执行 Mission：{goal}\n\n\
             ## 执行方案\n\n\
             {plan_content}\n\n\
             请严格按照上述方案中的步骤执行。"
        )),
        Err(_) => Some(format!("开始执行 Mission：{goal}")),
    }
}
