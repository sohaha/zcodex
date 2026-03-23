use clap::Args;
use clap::CommandFactory;
use clap::FromArgMatches;
use clap::Parser;
use clap_complete::Shell;
use clap_complete::generate;
use codex_arg0::Arg0DispatchPaths;
use codex_arg0::arg0_dispatch_or_else;
use codex_chatgpt::apply_command::ApplyCommand;
use codex_chatgpt::apply_command::run_apply_command;
use codex_cli::LandlockCommand;
use codex_cli::SeatbeltCommand;
use codex_cli::WindowsCommand;
use codex_cli::login::read_api_key_from_stdin;
use codex_cli::login::run_login_status;
use codex_cli::login::run_login_with_api_key;
use codex_cli::login::run_login_with_chatgpt;
use codex_cli::login::run_login_with_device_code;
use codex_cli::login::run_logout;
use codex_cloud_tasks::Cli as CloudTasksCli;
use codex_exec::Cli as ExecCli;
use codex_exec::Command as ExecCommand;
use codex_exec::ReviewArgs;
use codex_execpolicy::ExecPolicyCheckCommand;
use codex_responses_api_proxy::Args as ResponsesApiProxyArgs;
use codex_rtk::alias_name as rtk_alias_name;
use codex_rtk::is_alias_invocation as is_rtk_alias_invocation;
use codex_state::StateRuntime;
use codex_state::state_db_path;
use codex_tui::AppExitInfo;
use codex_tui::Cli as TuiCli;
use codex_tui::ExitReason;
use codex_tui::update_action::UpdateAction;
use codex_utils_cli::CliConfigOverrides;
use owo_colors::OwoColorize;
use std::io::IsTerminal;
use std::io::Write;
use std::path::PathBuf;
use supports_color::Stream;

#[cfg(target_os = "macos")]
mod app_cmd;
#[cfg(target_os = "macos")]
mod desktop_app;
mod mcp_cmd;
#[cfg(not(windows))]
mod wsl_paths;

use crate::mcp_cmd::McpCli;

use codex_core::config::Config;
use codex_core::config::ConfigOverrides;
use codex_core::config::edit::ConfigEditsBuilder;
use codex_core::config::find_codex_home;
use codex_features::FEATURES;
use codex_features::Stage;
use codex_features::is_known_feature_key;
use codex_terminal_detection::TerminalName;

/// Codex 命令行工具
///
/// 若未指定子命令，选项会转发到交互式 CLI。
#[derive(Debug, Parser)]
#[clap(
    author,
    version,
    // If a sub‑command is given, ignore requirements of the default args.
    subcommand_negates_reqs = true,
    // The executable is sometimes invoked via a platform‑specific name like
    // `codex-x86_64-unknown-linux-musl`, but the help output should always use
    // the generic `codex` command name that users run.
    bin_name = "codex",
    override_usage = "codex [OPTIONS] [PROMPT]\n       codex [OPTIONS] <COMMAND> [ARGS]"
)]
struct MultitoolCli {
    #[clap(flatten)]
    pub config_overrides: CliConfigOverrides,

    #[clap(flatten)]
    pub feature_toggles: FeatureToggles,

    #[clap(flatten)]
    remote: InteractiveRemoteOptions,

    #[clap(flatten)]
    interactive: TuiCli,

    #[clap(subcommand)]
    subcommand: Option<Subcommand>,
}

#[derive(Debug, clap::Subcommand)]
enum Subcommand {
    /// 以非交互模式运行 Codex。
    #[clap(visible_alias = "e")]
    Exec(ExecCli),

    /// 以非交互模式执行代码评审。
    Review(ReviewArgs),

    /// 管理登录。
    Login(LoginCommand),

    /// 删除已保存的认证凭据。
    Logout(LogoutCommand),

    /// 管理 Codex 的外部 MCP 服务器。
    Mcp(McpCli),

    /// 运行 Token 优化的命令包装器。
    Rtk(RtkArgs),

    /// 以 MCP 服务器（stdio）模式启动 Codex。
    McpServer,

    /// [实验性] 运行 app server 或相关工具。
    AppServer(AppServerCommand),

    /// 启动 Codex 桌面应用（若缺失会下载 macOS 安装包）。
    #[cfg(target_os = "macos")]
    App(app_cmd::AppCommand),

    /// 生成 shell 补全脚本。
    Completion(CompletionCommand),

    /// 在 Codex 提供的沙箱中运行命令。
    Sandbox(SandboxArgs),

    /// 调试工具。
    Debug(DebugCommand),

    /// Execpolicy 工具。
    #[clap(hide = true)]
    Execpolicy(ExecpolicyCommand),

    /// 将 Codex agent 生成的最新 diff 以 `git apply` 方式应用到本地工作区。
    #[clap(visible_alias = "a")]
    Apply(ApplyCommand),

    /// 恢复之前的交互会话（默认打开选择器；使用 --last 可继续最近一次）。
    Resume(ResumeCommand),

    /// 从之前的交互会话分叉（默认打开选择器；使用 --last 可分叉最近一次）。
    Fork(ForkCommand),

    /// [实验性] 浏览 Codex Cloud 任务并将变更应用到本地。
    #[clap(name = "cloud", alias = "cloud-tasks")]
    Cloud(CloudTasksCli),

    /// 内部：运行 responses API 代理。
    #[clap(hide = true)]
    ResponsesApiProxy(ResponsesApiProxyArgs),

    /// 内部：将 stdio 转发到 Unix 域套接字。
    #[clap(hide = true, name = "stdio-to-uds")]
    StdioToUds(StdioToUdsCommand),

    /// 查看功能开关。
    Features(FeaturesCli),
}

#[derive(Debug, Parser)]
struct CompletionCommand {
    /// 生成补全脚本的目标 shell
    #[clap(value_enum, default_value_t = Shell::Bash)]
    shell: Shell,
}

#[derive(Debug, Args)]
#[command(disable_help_flag = true, disable_version_flag = true)]
struct RtkArgs {
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<std::ffi::OsString>,
}

#[derive(Debug, Parser)]
struct DebugCommand {
    #[command(subcommand)]
    subcommand: DebugSubcommand,
}

#[derive(Debug, clap::Subcommand)]
enum DebugSubcommand {
    /// 工具：用于调试 app server。
    AppServer(DebugAppServerCommand),

    /// 内部：重置本地记忆状态以重新开始。
    #[clap(hide = true)]
    ClearMemories,
}

#[derive(Debug, Parser)]
struct DebugAppServerCommand {
    #[command(subcommand)]
    subcommand: DebugAppServerSubcommand,
}

#[derive(Debug, clap::Subcommand)]
enum DebugAppServerSubcommand {
    /// 向 app server V2 发送消息。
    SendMessageV2(DebugAppServerSendMessageV2Command),
}

#[derive(Debug, Parser)]
struct DebugAppServerSendMessageV2Command {
    /// 要发送的用户消息。
    #[arg(value_name = "USER_MESSAGE", required = true)]
    user_message: String,
}

#[derive(Debug, Parser)]
struct ResumeCommand {
    /// 会话 ID（UUID）或线程名。若能解析为 UUID，则优先按 UUID 处理。
    /// 省略时可用 --last 选择最近一次记录的会话。
    #[arg(value_name = "SESSION_ID")]
    session_id: Option<String>,

    /// 不显示选择器，直接继续最近一次会话。
    #[arg(long = "last", default_value_t = false)]
    last: bool,

    /// 显示所有会话（关闭 cwd 过滤并显示 CWD 列）。
    #[arg(long = "all", default_value_t = false)]
    all: bool,

    #[clap(flatten)]
    remote: InteractiveRemoteOptions,

    #[clap(flatten)]
    config_overrides: TuiCli,
}

#[derive(Debug, Parser)]
struct ForkCommand {
    /// 会话 ID（UUID）。提供后会分叉该会话。
    /// 省略时可用 --last 选择最近一次记录的会话。
    #[arg(value_name = "SESSION_ID")]
    session_id: Option<String>,

    /// 不显示选择器，直接分叉最近一次会话。
    #[arg(long = "last", default_value_t = false, conflicts_with = "session_id")]
    last: bool,

    /// 显示所有会话（关闭 cwd 过滤并显示 CWD 列）。
    #[arg(long = "all", default_value_t = false)]
    all: bool,

    #[clap(flatten)]
    remote: InteractiveRemoteOptions,

    #[clap(flatten)]
    config_overrides: TuiCli,
}

#[derive(Debug, Parser)]
struct SandboxArgs {
    #[command(subcommand)]
    cmd: SandboxCommand,
}

#[derive(Debug, clap::Subcommand)]
enum SandboxCommand {
    /// 在 Seatbelt 下运行命令（仅 macOS）。
    #[clap(visible_alias = "seatbelt")]
    Macos(SeatbeltCommand),

    /// 在 Linux 沙箱下运行命令（默认使用 bubblewrap）。
    #[clap(visible_alias = "landlock")]
    Linux(LandlockCommand),

    /// 在 Windows 受限令牌下运行命令（仅 Windows）。
    Windows(WindowsCommand),
}

#[derive(Debug, Parser)]
struct ExecpolicyCommand {
    #[command(subcommand)]
    sub: ExecpolicySubcommand,
}

#[derive(Debug, clap::Subcommand)]
enum ExecpolicySubcommand {
    /// 使用命令检查 execpolicy 文件。
    #[clap(name = "check")]
    Check(ExecPolicyCheckCommand),
}

#[derive(Debug, Parser)]
struct LoginCommand {
    #[clap(skip)]
    config_overrides: CliConfigOverrides,

    #[arg(
        long = "with-api-key",
        help = "从 stdin 读取 API 密钥（例如：`printenv OPENAI_API_KEY | codex login --with-api-key`）"
    )]
    with_api_key: bool,

    #[arg(
        long = "api-key",
        value_name = "API_KEY",
        help = "（已弃用）此前可直接传 API 密钥；现在会退出并提示改用 --with-api-key",
        hide = true
    )]
    api_key: Option<String>,

    #[arg(long = "device-auth", help = "使用设备码流程登录")]
    use_device_code: bool,

    /// 实验性：使用自定义 OAuth issuer 基础 URL（高级）
    /// 覆盖 OAuth issuer 基础 URL（高级）
    #[arg(long = "experimental_issuer", value_name = "URL", hide = true)]
    issuer_base_url: Option<String>,

    /// 实验性：使用自定义 OAuth client ID（高级）
    #[arg(long = "experimental_client-id", value_name = "CLIENT_ID", hide = true)]
    client_id: Option<String>,

    #[command(subcommand)]
    action: Option<LoginSubcommand>,
}

#[derive(Debug, clap::Subcommand)]
enum LoginSubcommand {
    /// 显示登录状态。
    Status,
}

#[derive(Debug, Parser)]
struct LogoutCommand {
    #[clap(skip)]
    config_overrides: CliConfigOverrides,
}

#[derive(Debug, Parser)]
struct AppServerCommand {
    /// 省略时直接运行 app server；指定子命令可执行工具能力。
    #[command(subcommand)]
    subcommand: Option<AppServerSubcommand>,

    /// 传输端点 URL。支持：`stdio://`（默认）、
    /// `ws://IP:PORT`。
    #[arg(
        long = "listen",
        value_name = "URL",
        default_value = codex_app_server::AppServerTransport::DEFAULT_LISTEN_URL
    )]
    listen: codex_app_server::AppServerTransport,

    /// 控制是否默认启用分析上报。
    ///
    /// app-server 默认关闭分析上报。用户需要在 config.toml 的 `analytics`
    /// 配置段中显式选择启用。
    ///
    /// 但对于 VSCode IDE 扩展这类第一方用例，我们可通过此标志让分析上报默认启用。
    /// 用户仍可在 config.toml 中这样设置来选择退出：
    ///
    /// ```toml
    /// [analytics]
    /// enabled = false
    /// ```
    ///
    /// 详情见 https://developers.openai.com/codex/config-advanced/#metrics 。
    #[arg(long = "analytics-default-enabled")]
    analytics_default_enabled: bool,
}

#[derive(Debug, clap::Subcommand)]
#[allow(clippy::enum_variant_names)]
enum AppServerSubcommand {
    /// [实验性] 为 app server 协议生成 TypeScript 绑定。
    GenerateTs(GenerateTsCommand),

    /// [实验性] 为 app server 协议生成 JSON Schema。
    GenerateJsonSchema(GenerateJsonSchemaCommand),

    /// [内部] 为 Codex 工具链生成内部 JSON Schema 产物。
    #[clap(hide = true)]
    GenerateInternalJsonSchema(GenerateInternalJsonSchemaCommand),
}

#[derive(Debug, Args)]
struct GenerateTsCommand {
    /// 输出目录（写入 .ts 文件）
    #[arg(short = 'o', long = "out", value_name = "DIR")]
    out_dir: PathBuf,

    /// 可选：用于格式化生成文件的 Prettier 程序路径
    #[arg(short = 'p', long = "prettier", value_name = "PRETTIER_BIN")]
    prettier: Option<PathBuf>,

    /// 在输出中包含实验性方法和字段
    #[arg(long = "experimental", default_value_t = false)]
    experimental: bool,
}

#[derive(Debug, Args)]
struct GenerateJsonSchemaCommand {
    /// 输出目录（写入 schema bundle）
    #[arg(short = 'o', long = "out", value_name = "DIR")]
    out_dir: PathBuf,

    /// 在输出中包含实验性方法和字段
    #[arg(long = "experimental", default_value_t = false)]
    experimental: bool,
}

#[derive(Debug, Args)]
struct GenerateInternalJsonSchemaCommand {
    /// 输出目录（写入内部 JSON Schema 产物）
    #[arg(short = 'o', long = "out", value_name = "DIR")]
    out_dir: PathBuf,
}

#[derive(Debug, Parser)]
struct StdioToUdsCommand {
    /// 要连接的 Unix 域套接字路径。
    #[arg(value_name = "SOCKET_PATH")]
    socket_path: PathBuf,
}

fn format_exit_messages(exit_info: AppExitInfo, color_enabled: bool) -> Vec<String> {
    let AppExitInfo {
        token_usage,
        thread_id: conversation_id,
        thread_name,
        ..
    } = exit_info;

    if token_usage.is_zero() {
        return Vec::new();
    }

    let mut lines = vec![format!(
        "{}",
        codex_protocol::protocol::FinalOutput::from(token_usage)
    )];

    if let Some(resume_cmd) =
        codex_core::util::resume_command(thread_name.as_deref(), conversation_id)
    {
        let command = if color_enabled {
            resume_cmd.cyan().to_string()
        } else {
            resume_cmd
        };
        lines.push(format!("若要继续此会话，请运行 {command}"));
    }

    lines
}

/// 处理应用退出并打印结果。可选执行更新动作。
fn handle_app_exit(exit_info: AppExitInfo) -> anyhow::Result<()> {
    match exit_info.exit_reason {
        ExitReason::Fatal(message) => {
            eprintln!("错误：{message}");
            std::process::exit(1);
        }
        ExitReason::UserRequested => { /* normal exit */ }
    }

    let update_action = exit_info.update_action;
    let color_enabled = supports_color::on(Stream::Stdout).is_some();
    for line in format_exit_messages(exit_info, color_enabled) {
        println!("{line}");
    }
    if let Some(action) = update_action {
        run_update_action(action)?;
    }
    Ok(())
}

/// 执行更新动作并输出结果。
fn run_update_action(action: UpdateAction) -> anyhow::Result<()> {
    println!();
    let cmd_str = action.command_str();
    println!("正在通过 `{cmd_str}` 更新 Codex...");

    let status = {
        #[cfg(windows)]
        {
            // On Windows, run via cmd.exe so .CMD/.BAT are correctly resolved (PATHEXT semantics).
            std::process::Command::new("cmd")
                .args(["/C", &cmd_str])
                .status()?
        }
        #[cfg(not(windows))]
        {
            let (cmd, args) = action.command_args();
            let command_path = crate::wsl_paths::normalize_for_wsl(cmd);
            let normalized_args: Vec<String> = args
                .iter()
                .map(crate::wsl_paths::normalize_for_wsl)
                .collect();
            std::process::Command::new(&command_path)
                .args(&normalized_args)
                .status()?
        }
    };
    if !status.success() {
        anyhow::bail!("`{cmd_str}` 执行失败，状态码：{status}");
    }
    println!("\n🎉 更新成功！请重启 Codex。");
    Ok(())
}

fn run_execpolicycheck(cmd: ExecPolicyCheckCommand) -> anyhow::Result<()> {
    cmd.run()
}

async fn run_debug_app_server_command(cmd: DebugAppServerCommand) -> anyhow::Result<()> {
    match cmd.subcommand {
        DebugAppServerSubcommand::SendMessageV2(cmd) => {
            let codex_bin = std::env::current_exe()?;
            codex_app_server_test_client::send_message_v2(&codex_bin, &[], cmd.user_message, &None)
                .await
        }
    }
}

#[derive(Debug, Default, Parser, Clone)]
struct FeatureToggles {
    /// 启用功能（可重复）。等价于 `-c features.<name>=true`。
    #[arg(long = "enable", value_name = "FEATURE", action = clap::ArgAction::Append, global = true)]
    enable: Vec<String>,

    /// 禁用功能（可重复）。等价于 `-c features.<name>=false`。
    #[arg(long = "disable", value_name = "FEATURE", action = clap::ArgAction::Append, global = true)]
    disable: Vec<String>,
}

#[derive(Debug, Default, Parser, Clone)]
struct InteractiveRemoteOptions {
    /// 将基于 app-server 的 TUI 连接到远程 app server websocket 端点。
    ///
    /// 支持格式：`ws://host:port` 或 `wss://host:port`。
    #[arg(long = "remote", value_name = "ADDR")]
    remote: Option<String>,
}

impl FeatureToggles {
    fn to_overrides(&self) -> anyhow::Result<Vec<String>> {
        let mut v = Vec::new();
        for feature in &self.enable {
            Self::validate_feature(feature)?;
            v.push(format!("features.{feature}=true"));
        }
        for feature in &self.disable {
            Self::validate_feature(feature)?;
            v.push(format!("features.{feature}=false"));
        }
        Ok(v)
    }

    fn validate_feature(feature: &str) -> anyhow::Result<()> {
        if is_known_feature_key(feature) {
            Ok(())
        } else {
            anyhow::bail!("Unknown feature flag: {feature}")
        }
    }
}

#[derive(Debug, Parser)]
struct FeaturesCli {
    #[command(subcommand)]
    sub: FeaturesSubcommand,
}

#[derive(Debug, Parser)]
enum FeaturesSubcommand {
    /// 列出已知功能及其阶段和生效状态。
    List,
    /// 在 config.toml 中启用功能。
    Enable(FeatureSetArgs),
    /// 在 config.toml 中禁用功能。
    Disable(FeatureSetArgs),
}

#[derive(Debug, Parser)]
struct FeatureSetArgs {
    /// 要更新的功能键（例如：unified_exec）。
    feature: String,
}

fn stage_str(stage: Stage) -> &'static str {
    match stage {
        Stage::UnderDevelopment => "开发中",
        Stage::Experimental { .. } => "实验性",
        Stage::Stable => "稳定",
        Stage::Deprecated => "已弃用",
        Stage::Removed => "已移除",
    }
}

fn main() -> anyhow::Result<()> {
    arg0_dispatch_or_else(|arg0_paths: Arg0DispatchPaths| async move {
        cli_main(arg0_paths).await?;
        Ok(())
    })
}

async fn cli_main(arg0_paths: Arg0DispatchPaths) -> anyhow::Result<()> {
    let MultitoolCli {
        config_overrides: mut root_config_overrides,
        feature_toggles,
        remote,
        mut interactive,
        subcommand,
    } = parse_multitool_cli_from_env();

    // Fold --enable/--disable into config overrides so they flow to all subcommands.
    let toggle_overrides = feature_toggles.to_overrides()?;
    root_config_overrides.raw_overrides.extend(toggle_overrides);
    let root_remote = remote.remote;

    match subcommand {
        None => {
            prepend_config_flags(
                &mut interactive.config_overrides,
                root_config_overrides.clone(),
            );
            let exit_info =
                run_interactive_tui(interactive, root_remote.clone(), arg0_paths.clone()).await?;
            handle_app_exit(exit_info)?;
        }
        Some(Subcommand::Exec(mut exec_cli)) => {
            reject_remote_mode_for_subcommand(root_remote.as_deref(), "exec")?;
            prepend_config_flags(
                &mut exec_cli.config_overrides,
                root_config_overrides.clone(),
            );
            codex_exec::run_main(exec_cli, arg0_paths.clone()).await?;
        }
        Some(Subcommand::Review(review_args)) => {
            reject_remote_mode_for_subcommand(root_remote.as_deref(), "review")?;
            let mut exec_cli = ExecCli::try_parse_from(["codex", "exec"])?;
            exec_cli.command = Some(ExecCommand::Review(review_args));
            prepend_config_flags(
                &mut exec_cli.config_overrides,
                root_config_overrides.clone(),
            );
            codex_exec::run_main(exec_cli, arg0_paths.clone()).await?;
        }
        Some(Subcommand::McpServer) => {
            reject_remote_mode_for_subcommand(root_remote.as_deref(), "mcp-server")?;
            codex_mcp_server::run_main(arg0_paths.clone(), root_config_overrides).await?;
        }
        Some(Subcommand::Mcp(mut mcp_cli)) => {
            reject_remote_mode_for_subcommand(root_remote.as_deref(), "mcp")?;
            // Propagate any root-level config overrides (e.g. `-c key=value`).
            prepend_config_flags(&mut mcp_cli.config_overrides, root_config_overrides.clone());
            mcp_cli.run().await?;
        }
        Some(Subcommand::Rtk(rtk_cli)) => {
            codex_rtk::run_from_os_args(rtk_cli.args)?;
        }
        Some(Subcommand::AppServer(app_server_cli)) => match app_server_cli.subcommand {
            None => {
                reject_remote_mode_for_subcommand(root_remote.as_deref(), "app-server")?;
                let transport = app_server_cli.listen;
                codex_app_server::run_main_with_transport(
                    arg0_paths.clone(),
                    root_config_overrides,
                    codex_core::config_loader::LoaderOverrides::default(),
                    app_server_cli.analytics_default_enabled,
                    transport,
                    codex_protocol::protocol::SessionSource::VSCode,
                )
                .await?;
            }
            Some(AppServerSubcommand::GenerateTs(gen_cli)) => {
                reject_remote_mode_for_subcommand(
                    root_remote.as_deref(),
                    "app-server generate-ts",
                )?;
                let options = codex_app_server_protocol::GenerateTsOptions {
                    experimental_api: gen_cli.experimental,
                    ..Default::default()
                };
                codex_app_server_protocol::generate_ts_with_options(
                    &gen_cli.out_dir,
                    gen_cli.prettier.as_deref(),
                    options,
                )?;
            }
            Some(AppServerSubcommand::GenerateJsonSchema(gen_cli)) => {
                reject_remote_mode_for_subcommand(
                    root_remote.as_deref(),
                    "app-server generate-json-schema",
                )?;
                codex_app_server_protocol::generate_json_with_experimental(
                    &gen_cli.out_dir,
                    gen_cli.experimental,
                )?;
            }
            Some(AppServerSubcommand::GenerateInternalJsonSchema(gen_cli)) => {
                codex_app_server_protocol::generate_internal_json_schema(&gen_cli.out_dir)?;
            }
        },
        #[cfg(target_os = "macos")]
        Some(Subcommand::App(app_cli)) => {
            reject_remote_mode_for_subcommand(root_remote.as_deref(), "app")?;
            app_cmd::run_app(app_cli).await?;
        }
        Some(Subcommand::Resume(ResumeCommand {
            session_id,
            last,
            all,
            remote,
            config_overrides,
        })) => {
            interactive = finalize_resume_interactive(
                interactive,
                root_config_overrides.clone(),
                session_id,
                last,
                all,
                config_overrides,
            );
            let exit_info = run_interactive_tui(
                interactive,
                remote.remote.or(root_remote.clone()),
                arg0_paths.clone(),
            )
            .await?;
            handle_app_exit(exit_info)?;
        }
        Some(Subcommand::Fork(ForkCommand {
            session_id,
            last,
            all,
            remote,
            config_overrides,
        })) => {
            interactive = finalize_fork_interactive(
                interactive,
                root_config_overrides.clone(),
                session_id,
                last,
                all,
                config_overrides,
            );
            let exit_info = run_interactive_tui(
                interactive,
                remote.remote.or(root_remote.clone()),
                arg0_paths.clone(),
            )
            .await?;
            handle_app_exit(exit_info)?;
        }
        Some(Subcommand::Login(mut login_cli)) => {
            reject_remote_mode_for_subcommand(root_remote.as_deref(), "login")?;
            prepend_config_flags(
                &mut login_cli.config_overrides,
                root_config_overrides.clone(),
            );
            match login_cli.action {
                Some(LoginSubcommand::Status) => {
                    run_login_status(login_cli.config_overrides).await;
                }
                None => {
                    if login_cli.use_device_code {
                        run_login_with_device_code(
                            login_cli.config_overrides,
                            login_cli.issuer_base_url,
                            login_cli.client_id,
                        )
                        .await;
                    } else if login_cli.api_key.is_some() {
                        eprintln!(
                            "--api-key 参数已不再支持。请改为通过管道传入密钥，例如：`printenv OPENAI_API_KEY | codex login --with-api-key`。"
                        );
                        std::process::exit(1);
                    } else if login_cli.with_api_key {
                        let api_key = read_api_key_from_stdin();
                        run_login_with_api_key(login_cli.config_overrides, api_key).await;
                    } else {
                        run_login_with_chatgpt(login_cli.config_overrides).await;
                    }
                }
            }
        }
        Some(Subcommand::Logout(mut logout_cli)) => {
            reject_remote_mode_for_subcommand(root_remote.as_deref(), "logout")?;
            prepend_config_flags(
                &mut logout_cli.config_overrides,
                root_config_overrides.clone(),
            );
            run_logout(logout_cli.config_overrides).await;
        }
        Some(Subcommand::Completion(completion_cli)) => {
            reject_remote_mode_for_subcommand(root_remote.as_deref(), "completion")?;
            print_completion(completion_cli);
        }
        Some(Subcommand::Cloud(mut cloud_cli)) => {
            reject_remote_mode_for_subcommand(root_remote.as_deref(), "cloud")?;
            prepend_config_flags(
                &mut cloud_cli.config_overrides,
                root_config_overrides.clone(),
            );
            codex_cloud_tasks::run_main(cloud_cli, arg0_paths.codex_linux_sandbox_exe.clone())
                .await?;
        }
        Some(Subcommand::Sandbox(sandbox_args)) => match sandbox_args.cmd {
            SandboxCommand::Macos(mut seatbelt_cli) => {
                reject_remote_mode_for_subcommand(root_remote.as_deref(), "sandbox macos")?;
                prepend_config_flags(
                    &mut seatbelt_cli.config_overrides,
                    root_config_overrides.clone(),
                );
                codex_cli::debug_sandbox::run_command_under_seatbelt(
                    seatbelt_cli,
                    arg0_paths.codex_linux_sandbox_exe.clone(),
                )
                .await?;
            }
            SandboxCommand::Linux(mut landlock_cli) => {
                reject_remote_mode_for_subcommand(root_remote.as_deref(), "sandbox linux")?;
                prepend_config_flags(
                    &mut landlock_cli.config_overrides,
                    root_config_overrides.clone(),
                );
                codex_cli::debug_sandbox::run_command_under_landlock(
                    landlock_cli,
                    arg0_paths.codex_linux_sandbox_exe.clone(),
                )
                .await?;
            }
            SandboxCommand::Windows(mut windows_cli) => {
                reject_remote_mode_for_subcommand(root_remote.as_deref(), "sandbox windows")?;
                prepend_config_flags(
                    &mut windows_cli.config_overrides,
                    root_config_overrides.clone(),
                );
                codex_cli::debug_sandbox::run_command_under_windows(
                    windows_cli,
                    arg0_paths.codex_linux_sandbox_exe.clone(),
                )
                .await?;
            }
        },
        Some(Subcommand::Debug(DebugCommand { subcommand })) => match subcommand {
            DebugSubcommand::AppServer(cmd) => {
                reject_remote_mode_for_subcommand(root_remote.as_deref(), "debug app-server")?;
                run_debug_app_server_command(cmd).await?;
            }
            DebugSubcommand::ClearMemories => {
                reject_remote_mode_for_subcommand(root_remote.as_deref(), "debug clear-memories")?;
                run_debug_clear_memories_command(&root_config_overrides, &interactive).await?;
            }
        },
        Some(Subcommand::Execpolicy(ExecpolicyCommand { sub })) => match sub {
            ExecpolicySubcommand::Check(cmd) => {
                reject_remote_mode_for_subcommand(root_remote.as_deref(), "execpolicy check")?;
                run_execpolicycheck(cmd)?
            }
        },
        Some(Subcommand::Apply(mut apply_cli)) => {
            reject_remote_mode_for_subcommand(root_remote.as_deref(), "apply")?;
            prepend_config_flags(
                &mut apply_cli.config_overrides,
                root_config_overrides.clone(),
            );
            run_apply_command(apply_cli, /*cwd*/ None).await?;
        }
        Some(Subcommand::ResponsesApiProxy(args)) => {
            reject_remote_mode_for_subcommand(root_remote.as_deref(), "responses-api-proxy")?;
            tokio::task::spawn_blocking(move || codex_responses_api_proxy::run_main(args))
                .await??;
        }
        Some(Subcommand::StdioToUds(cmd)) => {
            reject_remote_mode_for_subcommand(root_remote.as_deref(), "stdio-to-uds")?;
            let socket_path = cmd.socket_path;
            tokio::task::spawn_blocking(move || codex_stdio_to_uds::run(socket_path.as_path()))
                .await??;
        }
        Some(Subcommand::Features(FeaturesCli { sub })) => match sub {
            FeaturesSubcommand::List => {
                reject_remote_mode_for_subcommand(root_remote.as_deref(), "features list")?;
                // Respect root-level `-c` overrides plus top-level flags like `--profile`.
                let mut cli_kv_overrides = root_config_overrides
                    .parse_overrides()
                    .map_err(anyhow::Error::msg)?;

                // Honor `--search` via the canonical web_search mode.
                if interactive.web_search {
                    cli_kv_overrides.push((
                        "web_search".to_string(),
                        toml::Value::String("live".to_string()),
                    ));
                }

                // Thread through relevant top-level flags (at minimum, `--profile`).
                let overrides = ConfigOverrides {
                    config_profile: interactive.config_profile.clone(),
                    ..Default::default()
                };

                let config = Config::load_with_cli_overrides_and_harness_overrides(
                    cli_kv_overrides,
                    overrides,
                )
                .await?;
                let mut rows = Vec::with_capacity(FEATURES.len());
                let mut name_width = 0;
                let mut stage_width = 0;
                for def in FEATURES {
                    let name = def.key;
                    let stage = stage_str(def.stage);
                    let enabled = config.features.enabled(def.id);
                    name_width = name_width.max(name.len());
                    stage_width = stage_width.max(stage.len());
                    rows.push((name, stage, enabled));
                }
                rows.sort_unstable_by_key(|(name, _, _)| *name);

                for (name, stage, enabled) in rows {
                    println!("{name:<name_width$}  {stage:<stage_width$}  {enabled}");
                }
            }
            FeaturesSubcommand::Enable(FeatureSetArgs { feature }) => {
                reject_remote_mode_for_subcommand(root_remote.as_deref(), "features enable")?;
                enable_feature_in_config(&interactive, &feature).await?;
            }
            FeaturesSubcommand::Disable(FeatureSetArgs { feature }) => {
                reject_remote_mode_for_subcommand(root_remote.as_deref(), "features disable")?;
                disable_feature_in_config(&interactive, &feature).await?;
            }
        },
    }

    Ok(())
}

fn parse_multitool_cli_from_env() -> MultitoolCli {
    let raw_args = std::env::args_os().collect::<Vec<_>>();
    if raw_args.is_empty() {
        return MultitoolCli::parse();
    }

    if is_rtk_alias_invocation(&raw_args[0]) {
        let mut injected_args = Vec::with_capacity(raw_args.len() + 1);
        injected_args.push(raw_args[0].clone());
        injected_args.push(rtk_alias_name().into());
        injected_args.extend(raw_args.into_iter().skip(1));
        parse_multitool_cli(injected_args)
    } else {
        parse_multitool_cli(raw_args)
    }
}

fn parse_multitool_cli<I, T>(args: I) -> MultitoolCli
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let command = localized_multitool_command();
    match command.try_get_matches_from(args) {
        Ok(matches) => MultitoolCli::from_arg_matches(&matches).unwrap_or_else(|err| err.exit()),
        Err(err) => {
            let rendered = localize_help_output(err.to_string());
            let _ = std::io::stderr().write_all(rendered.as_bytes());
            std::process::exit(err.exit_code());
        }
    }
}

fn localized_multitool_command() -> clap::Command {
    localize_clap_command(MultitoolCli::command())
}

fn localize_clap_command(cmd: clap::Command) -> clap::Command {
    cmd
}

fn localize_help_output(output: String) -> String {
    output
        .replace("Usage:", "用法：")
        .replace("Commands:", "命令：")
        .replace("Arguments:", "参数：")
        .replace("Options:", "选项：")
        .replace("[aliases:", "[别名：")
        .replace("[default:", "[默认：")
        .replace(
            "Print this message or the help of the given subcommand(s)",
            "显示此消息或指定子命令的帮助",
        )
        .replace("Print help for the subcommand(s)", "显示指定子命令的帮助")
        .replace(
            "Print help (see a summary with '-h')",
            "显示帮助（使用 '-h' 查看摘要）",
        )
        .replace(
            "Print help (see more with '--help')",
            "显示帮助（使用 '--help' 查看更多）",
        )
        .replace("Print help", "显示帮助")
        .replace("Print version", "显示版本")
        .replace("Possible values:", "可选值：")
        .replace("[possible values:", "[可选值：")
}

async fn enable_feature_in_config(interactive: &TuiCli, feature: &str) -> anyhow::Result<()> {
    FeatureToggles::validate_feature(feature)?;
    let codex_home = find_codex_home()?;
    ConfigEditsBuilder::new(&codex_home)
        .with_profile(interactive.config_profile.as_deref())
        .set_feature_enabled(feature, /*enabled*/ true)
        .apply()
        .await?;
    println!("Enabled feature `{feature}` in config.toml.");
    maybe_print_under_development_feature_warning(&codex_home, interactive, feature);
    Ok(())
}

async fn disable_feature_in_config(interactive: &TuiCli, feature: &str) -> anyhow::Result<()> {
    FeatureToggles::validate_feature(feature)?;
    let codex_home = find_codex_home()?;
    ConfigEditsBuilder::new(&codex_home)
        .with_profile(interactive.config_profile.as_deref())
        .set_feature_enabled(feature, /*enabled*/ false)
        .apply()
        .await?;
    println!("Disabled feature `{feature}` in config.toml.");
    Ok(())
}

fn maybe_print_under_development_feature_warning(
    codex_home: &std::path::Path,
    interactive: &TuiCli,
    feature: &str,
) {
    if interactive.config_profile.is_some() {
        return;
    }

    let Some(spec) = FEATURES.iter().find(|spec| spec.key == feature) else {
        return;
    };
    if !matches!(spec.stage, Stage::UnderDevelopment) {
        return;
    }

    let config_path = codex_home.join(codex_config::CONFIG_TOML_FILE);
    eprintln!(
        "Under-development features enabled: {feature}. Under-development features are incomplete and may behave unpredictably. To suppress this warning, set `suppress_unstable_features_warning = true` in {}.",
        config_path.display()
    );
}

async fn run_debug_clear_memories_command(
    root_config_overrides: &CliConfigOverrides,
    interactive: &TuiCli,
) -> anyhow::Result<()> {
    let cli_kv_overrides = root_config_overrides
        .parse_overrides()
        .map_err(anyhow::Error::msg)?;
    let overrides = ConfigOverrides {
        config_profile: interactive.config_profile.clone(),
        ..Default::default()
    };
    let config =
        Config::load_with_cli_overrides_and_harness_overrides(cli_kv_overrides, overrides).await?;

    let state_path = state_db_path(config.sqlite_home.as_path());
    let mut cleared_state_db = false;
    if tokio::fs::try_exists(&state_path).await? {
        let state_db =
            StateRuntime::init(config.sqlite_home.clone(), config.model_provider_id.clone())
                .await?;
        state_db.reset_memory_data_for_fresh_start().await?;
        cleared_state_db = true;
    }

    let memory_root = config.codex_home.join("memories");
    let removed_memory_root = match tokio::fs::remove_dir_all(&memory_root).await {
        Ok(()) => true,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => false,
        Err(err) => return Err(err.into()),
    };

    let mut message = if cleared_state_db {
        format!("Cleared memory state from {}.", state_path.display())
    } else {
        format!("No state db found at {}.", state_path.display())
    };

    if removed_memory_root {
        message.push_str(&format!(" Removed {}.", memory_root.display()));
    } else {
        message.push_str(&format!(
            " No memory directory found at {}.",
            memory_root.display()
        ));
    }

    println!("{message}");

    Ok(())
}

/// Prepend root-level overrides so they have lower precedence than
/// CLI-specific ones specified after the subcommand (if any).
fn prepend_config_flags(
    subcommand_config_overrides: &mut CliConfigOverrides,
    cli_config_overrides: CliConfigOverrides,
) {
    subcommand_config_overrides
        .raw_overrides
        .splice(0..0, cli_config_overrides.raw_overrides);
}

fn reject_remote_mode_for_subcommand(remote: Option<&str>, subcommand: &str) -> anyhow::Result<()> {
    if let Some(remote) = remote {
        anyhow::bail!("`--remote {remote}` 仅支持交互式 TUI 命令，不支持 `codex {subcommand}`");
    }
    Ok(())
}

async fn run_interactive_tui(
    mut interactive: TuiCli,
    remote: Option<String>,
    arg0_paths: Arg0DispatchPaths,
) -> std::io::Result<AppExitInfo> {
    if let Some(prompt) = interactive.prompt.take() {
        // Normalize CRLF/CR to LF so CLI-provided text can't leak `\r` into TUI state.
        interactive.prompt = Some(prompt.replace("\r\n", "\n").replace('\r', "\n"));
    }

    let terminal_info = codex_terminal_detection::terminal_info();
    if terminal_info.name == TerminalName::Dumb {
        if !(std::io::stdin().is_terminal() && std::io::stderr().is_terminal()) {
            return Ok(AppExitInfo::fatal(
                "TERM 被设置为 \"dumb\"。由于没有可用于确认提示的终端（stdin/stderr 不是 TTY），拒绝启动交互式 TUI。请在受支持的终端中运行，或取消设置 TERM。",
            ));
        }

        eprintln!("警告：TERM 被设置为 \"dumb\"。Codex 的交互式 TUI 可能无法在此终端正常工作。");
        if !confirm("仍要继续吗？[y/N]: ")? {
            return Ok(AppExitInfo::fatal(
                "由于 TERM 被设置为 \"dumb\"，拒绝启动交互式 TUI。请在受支持的终端中运行，或取消设置 TERM。",
            ));
        }
    }

    let use_app_server_tui = codex_tui::should_use_app_server_tui(&interactive).await?;
    let normalized_remote = remote
        .as_deref()
        .map(codex_tui_app_server::normalize_remote_addr)
        .transpose()
        .map_err(std::io::Error::other)?;
    if normalized_remote.is_some() && !use_app_server_tui {
        return Ok(AppExitInfo::fatal(
            "`--remote` 需要启用 `tui_app_server` 功能开关。",
        ));
    }
    if use_app_server_tui {
        codex_tui_app_server::run_main(
            into_app_server_tui_cli(interactive),
            arg0_paths,
            codex_core::config_loader::LoaderOverrides::default(),
            normalized_remote,
        )
        .await
        .map(into_legacy_app_exit_info)
    } else {
        codex_tui::run_main(
            interactive,
            arg0_paths,
            codex_core::config_loader::LoaderOverrides::default(),
        )
        .await
    }
}

fn into_app_server_tui_cli(cli: TuiCli) -> codex_tui_app_server::Cli {
    codex_tui_app_server::Cli {
        prompt: cli.prompt,
        images: cli.images,
        resume_picker: cli.resume_picker,
        resume_last: cli.resume_last,
        resume_session_id: cli.resume_session_id,
        resume_show_all: cli.resume_show_all,
        fork_picker: cli.fork_picker,
        fork_last: cli.fork_last,
        fork_session_id: cli.fork_session_id,
        fork_show_all: cli.fork_show_all,
        model: cli.model,
        oss: cli.oss,
        oss_provider: cli.oss_provider,
        config_profile: cli.config_profile,
        sandbox_mode: cli.sandbox_mode,
        approval_policy: cli.approval_policy,
        full_auto: cli.full_auto,
        dangerously_bypass_approvals_and_sandbox: cli.dangerously_bypass_approvals_and_sandbox,
        cwd: cli.cwd,
        web_search: cli.web_search,
        add_dir: cli.add_dir,
        no_alt_screen: cli.no_alt_screen,
        config_overrides: cli.config_overrides,
    }
}

fn into_legacy_update_action(
    action: codex_tui_app_server::update_action::UpdateAction,
) -> UpdateAction {
    match action {
        codex_tui_app_server::update_action::UpdateAction::NpmGlobalLatest => {
            UpdateAction::NpmGlobalLatest
        }
        codex_tui_app_server::update_action::UpdateAction::BunGlobalLatest => {
            UpdateAction::BunGlobalLatest
        }
        codex_tui_app_server::update_action::UpdateAction::BrewUpgrade => UpdateAction::BrewUpgrade,
    }
}

fn into_legacy_exit_reason(reason: codex_tui_app_server::ExitReason) -> ExitReason {
    match reason {
        codex_tui_app_server::ExitReason::UserRequested => ExitReason::UserRequested,
        codex_tui_app_server::ExitReason::Fatal(message) => ExitReason::Fatal(message),
    }
}

fn into_legacy_app_exit_info(exit_info: codex_tui_app_server::AppExitInfo) -> AppExitInfo {
    AppExitInfo {
        token_usage: exit_info.token_usage,
        thread_id: exit_info.thread_id,
        thread_name: exit_info.thread_name,
        update_action: exit_info.update_action.map(into_legacy_update_action),
        exit_reason: into_legacy_exit_reason(exit_info.exit_reason),
    }
}

fn confirm(prompt: &str) -> std::io::Result<bool> {
    eprintln!("{prompt}");

    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    let answer = input.trim();
    Ok(answer.eq_ignore_ascii_case("y") || answer.eq_ignore_ascii_case("yes"))
}

/// Build the final `TuiCli` for a `codex resume` invocation.
fn finalize_resume_interactive(
    mut interactive: TuiCli,
    root_config_overrides: CliConfigOverrides,
    session_id: Option<String>,
    last: bool,
    show_all: bool,
    resume_cli: TuiCli,
) -> TuiCli {
    // Start with the parsed interactive CLI so resume shares the same
    // configuration surface area as `codex` without additional flags.
    let resume_session_id = session_id;
    interactive.resume_picker = resume_session_id.is_none() && !last;
    interactive.resume_last = last;
    interactive.resume_session_id = resume_session_id;
    interactive.resume_show_all = show_all;

    // Merge resume-scoped flags and overrides with highest precedence.
    merge_interactive_cli_flags(&mut interactive, resume_cli);

    // Propagate any root-level config overrides (e.g. `-c key=value`).
    prepend_config_flags(&mut interactive.config_overrides, root_config_overrides);

    interactive
}

/// Build the final `TuiCli` for a `codex fork` invocation.
fn finalize_fork_interactive(
    mut interactive: TuiCli,
    root_config_overrides: CliConfigOverrides,
    session_id: Option<String>,
    last: bool,
    show_all: bool,
    fork_cli: TuiCli,
) -> TuiCli {
    // Start with the parsed interactive CLI so fork shares the same
    // configuration surface area as `codex` without additional flags.
    let fork_session_id = session_id;
    interactive.fork_picker = fork_session_id.is_none() && !last;
    interactive.fork_last = last;
    interactive.fork_session_id = fork_session_id;
    interactive.fork_show_all = show_all;

    // Merge fork-scoped flags and overrides with highest precedence.
    merge_interactive_cli_flags(&mut interactive, fork_cli);

    // Propagate any root-level config overrides (e.g. `-c key=value`).
    prepend_config_flags(&mut interactive.config_overrides, root_config_overrides);

    interactive
}

/// Merge flags provided to `codex resume`/`codex fork` so they take precedence over any
/// root-level flags. Only overrides fields explicitly set on the subcommand-scoped
/// CLI. Also appends `-c key=value` overrides with highest precedence.
fn merge_interactive_cli_flags(interactive: &mut TuiCli, subcommand_cli: TuiCli) {
    if let Some(model) = subcommand_cli.model {
        interactive.model = Some(model);
    }
    if subcommand_cli.oss {
        interactive.oss = true;
    }
    if let Some(profile) = subcommand_cli.config_profile {
        interactive.config_profile = Some(profile);
    }
    if let Some(sandbox) = subcommand_cli.sandbox_mode {
        interactive.sandbox_mode = Some(sandbox);
    }
    if let Some(approval) = subcommand_cli.approval_policy {
        interactive.approval_policy = Some(approval);
    }
    if subcommand_cli.full_auto {
        interactive.full_auto = true;
    }
    if subcommand_cli.dangerously_bypass_approvals_and_sandbox {
        interactive.dangerously_bypass_approvals_and_sandbox = true;
    }
    if let Some(cwd) = subcommand_cli.cwd {
        interactive.cwd = Some(cwd);
    }
    if subcommand_cli.web_search {
        interactive.web_search = true;
    }
    if !subcommand_cli.images.is_empty() {
        interactive.images = subcommand_cli.images;
    }
    if !subcommand_cli.add_dir.is_empty() {
        interactive.add_dir.extend(subcommand_cli.add_dir);
    }
    if let Some(prompt) = subcommand_cli.prompt {
        // Normalize CRLF/CR to LF so CLI-provided text can't leak `\r` into TUI state.
        interactive.prompt = Some(prompt.replace("\r\n", "\n").replace('\r', "\n"));
    }

    interactive
        .config_overrides
        .raw_overrides
        .extend(subcommand_cli.config_overrides.raw_overrides);
}

fn print_completion(cmd: CompletionCommand) {
    let mut app = localized_multitool_command();
    let name = "codex";
    generate(cmd.shell, &mut app, name, &mut std::io::stdout());
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use codex_protocol::ThreadId;
    use codex_protocol::protocol::TokenUsage;
    use pretty_assertions::assert_eq;

    fn finalize_resume_from_args(args: &[&str]) -> TuiCli {
        let cli = MultitoolCli::try_parse_from(args).expect("parse");
        let MultitoolCli {
            interactive,
            config_overrides: root_overrides,
            subcommand,
            feature_toggles: _,
            remote: _,
        } = cli;

        let Subcommand::Resume(ResumeCommand {
            session_id,
            last,
            all,
            remote: _,
            config_overrides: resume_cli,
        }) = subcommand.expect("resume present")
        else {
            unreachable!()
        };

        finalize_resume_interactive(
            interactive,
            root_overrides,
            session_id,
            last,
            all,
            resume_cli,
        )
    }

    fn finalize_fork_from_args(args: &[&str]) -> TuiCli {
        let cli = MultitoolCli::try_parse_from(args).expect("parse");
        let MultitoolCli {
            interactive,
            config_overrides: root_overrides,
            subcommand,
            feature_toggles: _,
            remote: _,
        } = cli;

        let Subcommand::Fork(ForkCommand {
            session_id,
            last,
            all,
            remote: _,
            config_overrides: fork_cli,
        }) = subcommand.expect("fork present")
        else {
            unreachable!()
        };

        finalize_fork_interactive(interactive, root_overrides, session_id, last, all, fork_cli)
    }

    #[test]
    fn exec_resume_last_accepts_prompt_positional() {
        let cli =
            MultitoolCli::try_parse_from(["codex", "exec", "--json", "resume", "--last", "2+2"])
                .expect("parse should succeed");

        let Some(Subcommand::Exec(exec)) = cli.subcommand else {
            panic!("expected exec subcommand");
        };
        let Some(codex_exec::Command::Resume(args)) = exec.command else {
            panic!("expected exec resume");
        };

        assert!(args.last);
        assert_eq!(args.session_id, None);
        assert_eq!(args.prompt.as_deref(), Some("2+2"));
    }

    #[test]
    fn exec_resume_accepts_output_last_message_flag_after_subcommand() {
        let cli = MultitoolCli::try_parse_from([
            "codex",
            "exec",
            "resume",
            "session-123",
            "-o",
            "/tmp/resume-output.md",
            "re-review",
        ])
        .expect("parse should succeed");

        let Some(Subcommand::Exec(exec)) = cli.subcommand else {
            panic!("expected exec subcommand");
        };
        let Some(codex_exec::Command::Resume(args)) = exec.command else {
            panic!("expected exec resume");
        };

        assert_eq!(
            exec.last_message_file,
            Some(std::path::PathBuf::from("/tmp/resume-output.md"))
        );
        assert_eq!(args.session_id.as_deref(), Some("session-123"));
        assert_eq!(args.prompt.as_deref(), Some("re-review"));
    }

    fn app_server_from_args(args: &[&str]) -> AppServerCommand {
        let cli = MultitoolCli::try_parse_from(args).expect("parse");
        let Subcommand::AppServer(app_server) = cli.subcommand.expect("app-server present") else {
            unreachable!()
        };
        app_server
    }

    fn sample_exit_info(conversation_id: Option<&str>, thread_name: Option<&str>) -> AppExitInfo {
        let token_usage = TokenUsage {
            output_tokens: 2,
            total_tokens: 2,
            ..Default::default()
        };
        AppExitInfo {
            token_usage,
            thread_id: conversation_id
                .map(ThreadId::from_string)
                .map(Result::unwrap),
            thread_name: thread_name.map(str::to_string),
            update_action: None,
            exit_reason: ExitReason::UserRequested,
        }
    }

    #[test]
    fn format_exit_messages_skips_zero_usage() {
        let exit_info = AppExitInfo {
            token_usage: TokenUsage::default(),
            thread_id: None,
            thread_name: None,
            update_action: None,
            exit_reason: ExitReason::UserRequested,
        };
        let lines = format_exit_messages(exit_info, false);
        assert!(lines.is_empty());
    }

    #[test]
    fn format_exit_messages_includes_resume_hint_without_color() {
        let exit_info = sample_exit_info(Some("123e4567-e89b-12d3-a456-426614174000"), None);
        let lines = format_exit_messages(exit_info, false);
        assert_eq!(
            lines,
            vec![
                "Token usage: total=2 input=0 output=2".to_string(),
                "若要继续此会话，请运行 codex resume 123e4567-e89b-12d3-a456-426614174000"
                    .to_string(),
            ]
        );
    }

    #[test]
    fn format_exit_messages_applies_color_when_enabled() {
        let exit_info = sample_exit_info(Some("123e4567-e89b-12d3-a456-426614174000"), None);
        let lines = format_exit_messages(exit_info, true);
        assert_eq!(lines.len(), 2);
        assert!(lines[1].contains("\u{1b}[36m"));
    }

    #[test]
    fn format_exit_messages_prefers_thread_name() {
        let exit_info = sample_exit_info(
            Some("123e4567-e89b-12d3-a456-426614174000"),
            Some("my-thread"),
        );
        let lines = format_exit_messages(exit_info, false);
        assert_eq!(
            lines,
            vec![
                "Token usage: total=2 input=0 output=2".to_string(),
                "若要继续此会话，请运行 codex resume my-thread".to_string(),
            ]
        );
    }

    #[test]
    fn resume_model_flag_applies_when_no_root_flags() {
        let interactive =
            finalize_resume_from_args(["codex", "resume", "-m", "gpt-5.1-test"].as_ref());

        assert_eq!(interactive.model.as_deref(), Some("gpt-5.1-test"));
        assert!(interactive.resume_picker);
        assert!(!interactive.resume_last);
        assert_eq!(interactive.resume_session_id, None);
    }

    #[test]
    fn resume_picker_logic_none_and_not_last() {
        let interactive = finalize_resume_from_args(["codex", "resume"].as_ref());
        assert!(interactive.resume_picker);
        assert!(!interactive.resume_last);
        assert_eq!(interactive.resume_session_id, None);
        assert!(!interactive.resume_show_all);
    }

    #[test]
    fn resume_picker_logic_last() {
        let interactive = finalize_resume_from_args(["codex", "resume", "--last"].as_ref());
        assert!(!interactive.resume_picker);
        assert!(interactive.resume_last);
        assert_eq!(interactive.resume_session_id, None);
        assert!(!interactive.resume_show_all);
    }

    #[test]
    fn resume_picker_logic_with_session_id() {
        let interactive = finalize_resume_from_args(["codex", "resume", "1234"].as_ref());
        assert!(!interactive.resume_picker);
        assert!(!interactive.resume_last);
        assert_eq!(interactive.resume_session_id.as_deref(), Some("1234"));
        assert!(!interactive.resume_show_all);
    }

    #[test]
    fn resume_all_flag_sets_show_all() {
        let interactive = finalize_resume_from_args(["codex", "resume", "--all"].as_ref());
        assert!(interactive.resume_picker);
        assert!(interactive.resume_show_all);
    }

    #[test]
    fn resume_merges_option_flags_and_full_auto() {
        let interactive = finalize_resume_from_args(
            [
                "codex",
                "resume",
                "sid",
                "--oss",
                "--full-auto",
                "--search",
                "--sandbox",
                "workspace-write",
                "--ask-for-approval",
                "on-request",
                "-m",
                "gpt-5.1-test",
                "-p",
                "my-profile",
                "-C",
                "/tmp",
                "-i",
                "/tmp/a.png,/tmp/b.png",
            ]
            .as_ref(),
        );

        assert_eq!(interactive.model.as_deref(), Some("gpt-5.1-test"));
        assert!(interactive.oss);
        assert_eq!(interactive.config_profile.as_deref(), Some("my-profile"));
        assert_matches!(
            interactive.sandbox_mode,
            Some(codex_utils_cli::SandboxModeCliArg::WorkspaceWrite)
        );
        assert_matches!(
            interactive.approval_policy,
            Some(codex_utils_cli::ApprovalModeCliArg::OnRequest)
        );
        assert!(interactive.full_auto);
        assert_eq!(
            interactive.cwd.as_deref(),
            Some(std::path::Path::new("/tmp"))
        );
        assert!(interactive.web_search);
        let has_a = interactive
            .images
            .iter()
            .any(|p| p == std::path::Path::new("/tmp/a.png"));
        let has_b = interactive
            .images
            .iter()
            .any(|p| p == std::path::Path::new("/tmp/b.png"));
        assert!(has_a && has_b);
        assert!(!interactive.resume_picker);
        assert!(!interactive.resume_last);
        assert_eq!(interactive.resume_session_id.as_deref(), Some("sid"));
    }

    #[test]
    fn resume_merges_dangerously_bypass_flag() {
        let interactive = finalize_resume_from_args(
            [
                "codex",
                "resume",
                "--dangerously-bypass-approvals-and-sandbox",
            ]
            .as_ref(),
        );
        assert!(interactive.dangerously_bypass_approvals_and_sandbox);
        assert!(interactive.resume_picker);
        assert!(!interactive.resume_last);
        assert_eq!(interactive.resume_session_id, None);
    }

    #[test]
    fn fork_picker_logic_none_and_not_last() {
        let interactive = finalize_fork_from_args(["codex", "fork"].as_ref());
        assert!(interactive.fork_picker);
        assert!(!interactive.fork_last);
        assert_eq!(interactive.fork_session_id, None);
        assert!(!interactive.fork_show_all);
    }

    #[test]
    fn fork_picker_logic_last() {
        let interactive = finalize_fork_from_args(["codex", "fork", "--last"].as_ref());
        assert!(!interactive.fork_picker);
        assert!(interactive.fork_last);
        assert_eq!(interactive.fork_session_id, None);
        assert!(!interactive.fork_show_all);
    }

    #[test]
    fn fork_picker_logic_with_session_id() {
        let interactive = finalize_fork_from_args(["codex", "fork", "1234"].as_ref());
        assert!(!interactive.fork_picker);
        assert!(!interactive.fork_last);
        assert_eq!(interactive.fork_session_id.as_deref(), Some("1234"));
        assert!(!interactive.fork_show_all);
    }

    #[test]
    fn fork_all_flag_sets_show_all() {
        let interactive = finalize_fork_from_args(["codex", "fork", "--all"].as_ref());
        assert!(interactive.fork_picker);
        assert!(interactive.fork_show_all);
    }

    #[test]
    fn app_server_analytics_default_disabled_without_flag() {
        let app_server = app_server_from_args(["codex", "app-server"].as_ref());
        assert!(!app_server.analytics_default_enabled);
        assert_eq!(
            app_server.listen,
            codex_app_server::AppServerTransport::Stdio
        );
    }

    #[test]
    fn app_server_analytics_default_enabled_with_flag() {
        let app_server =
            app_server_from_args(["codex", "app-server", "--analytics-default-enabled"].as_ref());
        assert!(app_server.analytics_default_enabled);
    }

    #[test]
    fn remote_flag_parses_for_interactive_root() {
        let cli = MultitoolCli::try_parse_from(["codex", "--remote", "ws://127.0.0.1:4500"])
            .expect("parse");
        assert_eq!(cli.remote.remote.as_deref(), Some("ws://127.0.0.1:4500"));
    }

    #[test]
    fn remote_flag_parses_for_resume_subcommand() {
        let cli =
            MultitoolCli::try_parse_from(["codex", "resume", "--remote", "ws://127.0.0.1:4500"])
                .expect("parse");
        let Subcommand::Resume(ResumeCommand { remote, .. }) =
            cli.subcommand.expect("resume present")
        else {
            panic!("expected resume subcommand");
        };
        assert_eq!(remote.remote.as_deref(), Some("ws://127.0.0.1:4500"));
    }

    #[test]
    fn reject_remote_mode_for_non_interactive_subcommands() {
        let err = reject_remote_mode_for_subcommand(Some("127.0.0.1:4500"), "exec")
            .expect_err("non-interactive subcommands should reject --remote");
        assert!(err.to_string().contains("仅支持交互式 TUI 命令"));
    }

    #[test]
    fn app_server_listen_websocket_url_parses() {
        let app_server = app_server_from_args(
            ["codex", "app-server", "--listen", "ws://127.0.0.1:4500"].as_ref(),
        );
        assert_eq!(
            app_server.listen,
            codex_app_server::AppServerTransport::WebSocket {
                bind_address: "127.0.0.1:4500".parse().expect("valid socket address"),
            }
        );
    }

    #[test]
    fn app_server_listen_stdio_url_parses() {
        let app_server =
            app_server_from_args(["codex", "app-server", "--listen", "stdio://"].as_ref());
        assert_eq!(
            app_server.listen,
            codex_app_server::AppServerTransport::Stdio
        );
    }

    #[test]
    fn app_server_listen_invalid_url_fails_to_parse() {
        let parse_result =
            MultitoolCli::try_parse_from(["codex", "app-server", "--listen", "http://foo"]);
        assert!(parse_result.is_err());
    }

    #[test]
    fn features_enable_parses_feature_name() {
        let cli = MultitoolCli::try_parse_from(["codex", "features", "enable", "unified_exec"])
            .expect("parse should succeed");
        let Some(Subcommand::Features(FeaturesCli { sub })) = cli.subcommand else {
            panic!("expected features subcommand");
        };
        let FeaturesSubcommand::Enable(FeatureSetArgs { feature }) = sub else {
            panic!("expected features enable");
        };
        assert_eq!(feature, "unified_exec");
    }

    #[test]
    fn features_disable_parses_feature_name() {
        let cli = MultitoolCli::try_parse_from(["codex", "features", "disable", "shell_tool"])
            .expect("parse should succeed");
        let Some(Subcommand::Features(FeaturesCli { sub })) = cli.subcommand else {
            panic!("expected features subcommand");
        };
        let FeaturesSubcommand::Disable(FeatureSetArgs { feature }) = sub else {
            panic!("expected features disable");
        };
        assert_eq!(feature, "shell_tool");
    }

    #[test]
    fn feature_toggles_known_features_generate_overrides() {
        let toggles = FeatureToggles {
            enable: vec!["web_search_request".to_string()],
            disable: vec!["unified_exec".to_string()],
        };
        let overrides = toggles.to_overrides().expect("valid features");
        assert_eq!(
            overrides,
            vec![
                "features.web_search_request=true".to_string(),
                "features.unified_exec=false".to_string(),
            ]
        );
    }

    #[test]
    fn feature_toggles_accept_legacy_linux_sandbox_flag() {
        let toggles = FeatureToggles {
            enable: vec!["use_linux_sandbox_bwrap".to_string()],
            disable: Vec::new(),
        };
        let overrides = toggles.to_overrides().expect("valid features");
        assert_eq!(
            overrides,
            vec!["features.use_linux_sandbox_bwrap=true".to_string(),]
        );
    }

    #[test]
    fn feature_toggles_unknown_feature_errors() {
        let toggles = FeatureToggles {
            enable: vec!["does_not_exist".to_string()],
            disable: Vec::new(),
        };
        let err = toggles
            .to_overrides()
            .expect_err("feature should be rejected");
        assert_eq!(err.to_string(), "Unknown feature flag: does_not_exist");
    }
}
