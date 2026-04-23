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
use codex_cli::read_api_key_from_stdin;
use codex_cli::run_login_status;
use codex_cli::run_login_with_api_key;
use codex_cli::run_login_with_chatgpt;
use codex_cli::run_login_with_device_code;
use codex_cli::run_logout;
use codex_cli::zoffsec_cmd::ZoffsecCommand;
use codex_cli::zoffsec_cmd::ZoffsecSubcommand;
use codex_cli::zoffsec_cmd::apply_zoffsec_overrides;
use codex_cli::zoffsec_cmd::run_zoffsec_clean_command;
use codex_cloud_tasks::Cli as CloudTasksCli;
use codex_exec::Cli as ExecCli;
use codex_exec::Command as ExecCommand;
use codex_exec::ReviewArgs;
use codex_execpolicy::ExecPolicyCheckCommand;
use codex_responses_api_proxy::Args as ResponsesApiProxyArgs;
use codex_state::StateRuntime;
use codex_state::state_db_path;
use codex_tui::AppExitInfo;
use codex_tui::Cli as TuiCli;
use codex_tui::ExitReason;
use codex_tui::UpdateAction;
use codex_utils_cli::CODEX_SELF_EXE_ENV_VAR;
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
mod federation_cmd;
mod marketplace_cmd;
mod mcp_cmd;
mod responses_cmd;
mod tldr_cmd;
#[cfg(not(windows))]
mod wsl_paths;
mod zmemory_cmd;
mod zmemory_compat_server;

use crate::federation_cmd::FederationCli;
use crate::marketplace_cmd::MarketplaceCli;
use crate::mcp_cmd::McpCli;
use crate::responses_cmd::ResponsesCommand;
use crate::responses_cmd::run_responses_command;
use crate::tldr_cmd::TldrCli;
use crate::zmemory_cmd::ZmemoryCli;
use crate::zmemory_cmd::run_zmemory_command;

use codex_core::clear_memory_roots_contents;
use codex_core::config::Config;
use codex_core::config::ConfigOverrides;
use codex_core::config::edit::ConfigEditsBuilder;
use codex_core::config::find_codex_home;
use codex_core::exec_env::CODEX_THREAD_ID_ENV_VAR;
use codex_features::FEATURES;
use codex_features::Stage;
use codex_features::is_known_feature_key;
use codex_protocol::protocol::AskForApproval;
use codex_protocol::user_input::UserInput;
use codex_terminal_detection::TerminalName;

/// Codex 命令行工具
///
/// 若未指定子命令，选项会转发到交互式命令行界面。
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
    override_usage = "codex [选项] [提示]\n       codex [选项] <命令> [参数]",
    long_about = "Codex 命令行工具\n\n若未指定子命令，选项会转发到交互式命令行界面。"
)]
struct MultitoolCli {
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
    Ztok(ZtokArgs),

    /// 管理本地 federation daemon 与多实例消息投递。
    Federation(FederationCli),

    /// 管理 Codex 插件。
    Plugin(PluginCli),

    /// 运行原生 TLDR 代码上下文分析命令。
    #[clap(name = "ztldr")]
    Tldr(TldrCli),

    /// 管理本地 zmemory 长期记忆。
    Zmemory(ZmemoryCli),

    /// 以 MCP 服务器（标准输入/输出）模式启动 Codex。
    McpServer,

    /// [实验性] 运行应用服务器或相关工具。
    AppServer(AppServerCommand),

    /// 启动 Codex 桌面应用（若缺失会下载 macOS 安装包）。
    #[cfg(target_os = "macos")]
    App(app_cmd::AppCommand),

    /// 生成命令行补全脚本。
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
    #[clap(visible_alias = "r")]
    Resume(ResumeCommand),

    /// 从之前的交互会话分叉（默认打开选择器；使用 --last 可分叉最近一次）。
    Fork(ForkCommand),

    /// 启动或管理 offensive security 工作流会话。
    Zoffsec(ZoffsecCommand),

    /// [实验性] 浏览 Codex Cloud 任务并将变更应用到本地。
    #[clap(name = "cloud", alias = "cloud-tasks")]
    Cloud(CloudTasksCli),

    /// Internal: run the responses API proxy.
    #[clap(hide = true)]
    ResponsesApiProxy(ResponsesApiProxyArgs),

    /// Internal: send one raw Responses API payload through Codex auth.
    #[clap(hide = true)]
    Responses(ResponsesCommand),

    /// Internal: relay stdio to a Unix domain socket.
    #[clap(hide = true, name = "stdio-to-uds")]
    StdioToUds(StdioToUdsCommand),

    /// [实验性] 运行独立 exec-server 服务。
    ExecServer(ExecServerCommand),

    /// 查看功能开关。
    Features(FeaturesCli),
}

#[derive(Debug, Args)]
#[command(disable_help_flag = true, disable_version_flag = true)]
struct ZtokArgs {
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<std::ffi::OsString>,
}

#[derive(Debug, Parser)]
struct PluginCli {
    #[clap(flatten)]
    pub config_overrides: CliConfigOverrides,

    #[command(subcommand)]
    subcommand: PluginSubcommand,
}

#[derive(Debug, clap::Subcommand)]
enum PluginSubcommand {
    /// 管理 Codex 的插件市场。
    Marketplace(MarketplaceCli),
}

#[derive(Debug, Parser)]
struct CompletionCommand {
    /// 要生成补全脚本的 shell 类型。
    #[clap(value_enum, default_value_t = Shell::Bash)]
    shell: Shell,
}

#[derive(Debug, Parser)]
struct DebugCommand {
    #[command(subcommand)]
    subcommand: DebugSubcommand,
}

#[derive(Debug, clap::Subcommand)]
enum DebugSubcommand {
    /// 用于调试应用服务器的工具。
    AppServer(DebugAppServerCommand),

    /// 将模型可见的提示输入列表渲染为 JSON。
    PromptInput(DebugPromptInputCommand),

    /// Internal: reset local memory state for a fresh start.
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
    /// 向应用服务器的 v2 接口发送消息。
    SendMessageV2(DebugAppServerSendMessageV2Command),
}

#[derive(Debug, Parser)]
struct DebugAppServerSendMessageV2Command {
    #[arg(value_name = "用户消息", required = true)]
    user_message: String,
}

#[derive(Debug, Parser)]
struct DebugPromptInputCommand {
    /// 可选的用户提示，会附加在会话上下文之后。
    #[arg(value_name = "提示")]
    prompt: Option<String>,

    /// 可选的图片，会附加到用户提示中。
    #[arg(long = "image", short = 'i', value_name = "文件", value_delimiter = ',', num_args = 1..)]
    images: Vec<PathBuf>,
}

#[derive(Debug, Parser)]
struct ResumeCommand {
    /// 会话 ID（UUID）或线程名。若能解析为 UUID，则优先按 UUID 处理。
    /// 省略时可用 --last 选择最近一次记录的会话。
    #[arg(value_name = "会话ID")]
    session_id: Option<String>,

    /// 不显示选择器，直接继续最近一次会话。
    #[arg(long = "last", default_value_t = false)]
    last: bool,

    /// 显示所有会话（关闭 cwd 过滤并显示 CWD 列）。
    #[arg(long = "all", default_value_t = false)]
    all: bool,

    /// 在恢复选择器和 `--last` 选择中包含非交互式会话。
    #[arg(long = "include-non-interactive", default_value_t = false)]
    include_non_interactive: bool,

    #[clap(flatten)]
    remote: InteractiveRemoteOptions,

    #[clap(flatten)]
    config_overrides: TuiCli,
}

#[derive(Debug, Parser)]
struct ForkCommand {
    /// 会话 ID（UUID）。提供后会分叉该会话。
    /// 省略时可用 --last 选择最近一次记录的会话。
    #[arg(value_name = "会话ID")]
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
    /// 在 macOS 的 Seatbelt 沙箱中运行命令。
    #[clap(visible_alias = "seatbelt")]
    Macos(SeatbeltCommand),

    /// 在 Linux 沙箱下运行命令（默认使用 `bubblewrap`）。
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
        help = "从标准输入读取 API 密钥（例如：`printenv OPENAI_API_KEY | codex login --with-api-key`）"
    )]
    with_api_key: bool,

    #[arg(
        long = "api-key",
        num_args = 0..=1,
        default_missing_value = "",
        value_name = "API密钥",
        help = "（已弃用）此前可直接传 API 密钥；现在会退出并提示改用 --with-api-key",
        hide = true
    )]
    api_key: Option<String>,

    #[arg(long = "device-auth", help = "使用设备码流程登录。")]
    use_device_code: bool,

    /// 实验性：使用自定义 OAuth issuer 基础 URL（高级）
    /// 覆盖 OAuth issuer 基础 URL（高级）
    #[arg(long = "experimental_issuer", value_name = "URL", hide = true)]
    issuer_base_url: Option<String>,

    /// 实验性：使用自定义 OAuth client ID（高级）
    #[arg(long = "experimental_client-id", value_name = "客户端ID", hide = true)]
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
    /// 省略时直接运行应用服务器；指定子命令可执行工具能力。
    #[command(subcommand)]
    subcommand: Option<AppServerSubcommand>,

    /// 传输监听地址。支持：`stdio://`（默认）、
    /// `ws://IP:PORT`、`off`。
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

    #[command(flatten)]
    auth: codex_app_server::AppServerWebsocketAuthArgs,
}

#[derive(Debug, Parser)]
struct ExecServerCommand {
    /// 传输监听地址。支持：`ws://IP:PORT`（默认）。
    #[arg(
        long = "listen",
        value_name = "URL",
        default_value = "ws://127.0.0.1:0"
    )]
    listen: String,
}

#[derive(Debug, clap::Subcommand)]
#[allow(clippy::enum_variant_names)]
enum AppServerSubcommand {
    /// [实验性] 为应用服务器协议生成 TypeScript 代码绑定。
    GenerateTs(GenerateTsCommand),

    /// [实验性] 为应用服务器协议生成 JSON Schema 文件。
    GenerateJsonSchema(GenerateJsonSchemaCommand),

    /// [内部] 为 Codex 工具链生成内部 JSON Schema 文件。
    #[clap(hide = true)]
    GenerateInternalJsonSchema(GenerateInternalJsonSchemaCommand),
}

#[derive(Debug, Args)]
struct GenerateTsCommand {
    /// 输出目录（写入 TypeScript 文件）
    #[arg(short = 'o', long = "out", value_name = "目录")]
    out_dir: PathBuf,

    /// 可选：用于格式化生成文件的 Prettier 可执行文件路径。
    #[arg(short = 'p', long = "prettier", value_name = "Prettier路径")]
    prettier: Option<PathBuf>,

    /// 在输出中包含实验性方法和字段。
    #[arg(long = "experimental", default_value_t = false)]
    experimental: bool,
}

#[derive(Debug, Args)]
struct GenerateJsonSchemaCommand {
    /// 输出目录（写入 Schema 汇总文件）
    #[arg(short = 'o', long = "out", value_name = "目录")]
    out_dir: PathBuf,

    /// 在输出中包含实验性方法和字段。
    #[arg(long = "experimental", default_value_t = false)]
    experimental: bool,
}

#[derive(Debug, Args)]
struct GenerateInternalJsonSchemaCommand {
    /// 输出目录（写入内部 JSON Schema 文件）
    #[arg(short = 'o', long = "out", value_name = "目录")]
    out_dir: PathBuf,
}

#[derive(Debug, Parser)]
struct StdioToUdsCommand {
    /// 要连接的 Unix 域套接字路径。
    #[arg(value_name = "套接字路径")]
    socket_path: PathBuf,
}

fn format_exit_messages(exit_info: AppExitInfo, color_enabled: bool) -> Vec<String> {
    let AppExitInfo {
        token_usage,
        thread_id: conversation_id,
        thread_name,
        ..
    } = exit_info;

    let mut lines = Vec::new();
    if !token_usage.is_zero() {
        lines.push(format!(
            "{}",
            codex_protocol::protocol::FinalOutput::from(token_usage)
        ));
    }

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
            if action == UpdateAction::StandaloneWindows {
                let (cmd, args) = action.command_args();
                // Run the standalone PowerShell installer with PowerShell
                // itself. Routing this through `cmd.exe /C` would parse
                // PowerShell metacharacters like `|` before PowerShell sees
                // the installer command.
                std::process::Command::new(cmd).args(args).status()?
            } else {
                // On Windows, run via cmd.exe so .CMD/.BAT are correctly resolved (PATHEXT semantics).
                std::process::Command::new("cmd")
                    .args(["/C", &cmd_str])
                    .status()?
            }
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
    #[arg(long = "enable", value_name = "功能", action = clap::ArgAction::Append, global = true)]
    enable: Vec<String>,

    /// 禁用功能（可重复）。等价于 `-c features.<name>=false`。
    #[arg(long = "disable", value_name = "功能", action = clap::ArgAction::Append, global = true)]
    disable: Vec<String>,
}

#[derive(Debug, Default, Parser, Clone)]
struct InteractiveRemoteOptions {
    /// 将 TUI 连接到远程应用服务器 WebSocket 端点。
    ///
    /// 支持格式：`ws://host:port` 或 `wss://host:port`。
    #[arg(long = "remote", value_name = "地址")]
    remote: Option<String>,

    /// 包含远程应用服务器 WebSocket 访问令牌的环境变量名。
    #[arg(long = "remote-auth-token-env", value_name = "环境变量")]
    remote_auth_token_env: Option<String>,
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
            anyhow::bail!("未知的功能开关：{feature}")
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
    /// 列出已知功能及其所处阶段与当前状态。
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
    if let Some(codex_self_exe) = arg0_paths.codex_self_exe.as_deref() {
        // Library crates use this stable fact source for launcher-agnostic
        // hints without depending on `current_exe()` inside test binaries.
        unsafe { std::env::set_var(CODEX_SELF_EXE_ENV_VAR, codex_self_exe) };
    }

    let raw_args = std::env::args_os().collect::<Vec<_>>();
    if let Some(argv0) = raw_args.first()
        && codex_ztok::is_alias_invocation(argv0)
    {
        run_ztok_subcommand(
            raw_args.into_iter().skip(1).collect(),
            CliConfigOverrides::default(),
            None,
        )
        .await?;
        return Ok(());
    }
    if raw_args
        .get(1)
        .and_then(|arg| arg.to_str())
        .is_some_and(|arg| arg == "ztok")
    {
        run_ztok_subcommand(
            raw_args.into_iter().skip(2).collect(),
            CliConfigOverrides::default(),
            None,
        )
        .await?;
        return Ok(());
    }

    let MultitoolCli {
        feature_toggles,
        remote,
        mut interactive,
        subcommand,
    } = parse_multitool_cli_from_env();

    let mut root_config_overrides = std::mem::take(&mut interactive.config_overrides);
    // Fold --enable/--disable into config overrides so they flow to all subcommands.
    let toggle_overrides = feature_toggles.to_overrides()?;
    root_config_overrides.raw_overrides.extend(toggle_overrides);
    let root_remote = remote.remote;
    let root_remote_auth_token_env = remote.remote_auth_token_env;

    match subcommand {
        None => {
            prepend_config_flags(
                &mut interactive.config_overrides,
                root_config_overrides.clone(),
            );
            let exit_info = run_interactive_tui(
                interactive,
                root_remote.clone(),
                root_remote_auth_token_env.clone(),
                arg0_paths.clone(),
            )
            .await?;
            handle_app_exit(exit_info)?;
        }
        Some(Subcommand::Zoffsec(zoffsec_cli)) => {
            let ZoffsecCommand { start, subcommand } = zoffsec_cli;
            match subcommand {
                None => {
                    let mut zoffsec_interactive = start.interactive;
                    prepend_config_flags(
                        &mut zoffsec_interactive.config_overrides,
                        root_config_overrides.clone(),
                    );
                    apply_zoffsec_overrides(
                        &mut zoffsec_interactive.config_overrides,
                        start.template,
                    );
                    let exit_info = run_interactive_tui(
                        zoffsec_interactive,
                        root_remote.clone(),
                        root_remote_auth_token_env.clone(),
                        arg0_paths.clone(),
                    )
                    .await?;
                    handle_app_exit(exit_info)?;
                }
                Some(ZoffsecSubcommand::Clean(clean)) => {
                    reject_remote_mode_for_subcommand(
                        root_remote.as_deref(),
                        root_remote_auth_token_env.as_deref(),
                        "zoffsec clean",
                    )?;
                    run_zoffsec_clean_command(clean, root_config_overrides.clone()).await?;
                }
                Some(ZoffsecSubcommand::Resume(resume)) => {
                    interactive = finalize_zoffsec_resume_interactive(
                        interactive,
                        root_config_overrides.clone(),
                        *resume,
                    );
                    let exit_info = run_interactive_tui(
                        interactive,
                        root_remote.clone(),
                        root_remote_auth_token_env.clone(),
                        arg0_paths.clone(),
                    )
                    .await?;
                    handle_app_exit(exit_info)?;
                }
            }
        }
        Some(Subcommand::Exec(mut exec_cli)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "exec",
            )?;
            prepend_config_flags(
                &mut exec_cli.config_overrides,
                root_config_overrides.clone(),
            );
            codex_exec::run_main(exec_cli, arg0_paths.clone()).await?;
        }
        Some(Subcommand::Review(review_args)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "review",
            )?;
            let mut exec_cli = ExecCli::try_parse_from(["codex", "exec"])?;
            exec_cli.command = Some(ExecCommand::Review(review_args));
            prepend_config_flags(
                &mut exec_cli.config_overrides,
                root_config_overrides.clone(),
            );
            codex_exec::run_main(exec_cli, arg0_paths.clone()).await?;
        }
        Some(Subcommand::McpServer) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "mcp-server",
            )?;
            codex_mcp_server::run_main(arg0_paths.clone(), root_config_overrides).await?;
        }
        Some(Subcommand::Mcp(mut mcp_cli)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "mcp",
            )?;
            // Propagate any root-level config overrides (e.g. `-c key=value`).
            prepend_config_flags(&mut mcp_cli.config_overrides, root_config_overrides.clone());
            mcp_cli.run().await?;
        }
        Some(Subcommand::Ztok(ztok_cli)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "ztok",
            )?;
            run_ztok_subcommand(
                ztok_cli.args,
                root_config_overrides.clone(),
                interactive.config_profile.clone(),
            )
            .await?;
        }
        Some(Subcommand::Federation(federation_cli)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "federation",
            )?;
            federation_cli.run().await?;
        }
        Some(Subcommand::Plugin(plugin_cli)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "plugin",
            )?;
            let PluginCli {
                mut config_overrides,
                subcommand,
            } = plugin_cli;
            prepend_config_flags(&mut config_overrides, root_config_overrides.clone());
            match subcommand {
                PluginSubcommand::Marketplace(mut marketplace_cli) => {
                    prepend_config_flags(&mut marketplace_cli.config_overrides, config_overrides);
                    marketplace_cli.run().await?;
                }
            }
        }
        Some(Subcommand::Tldr(tldr_cli)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "ztldr",
            )?;
            tldr_cmd::run_tldr_command(tldr_cli).await?;
        }
        Some(Subcommand::Zmemory(zmemory_cli)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "zmemory",
            )?;
            run_zmemory_command(zmemory_cli).await?;
        }
        Some(Subcommand::AppServer(app_server_cli)) => {
            let AppServerCommand {
                subcommand,
                listen,
                analytics_default_enabled,
                auth,
            } = app_server_cli;
            reject_remote_mode_for_app_server_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                subcommand.as_ref(),
            )?;
            match subcommand {
                None => {
                    let transport = listen;
                    let auth = auth.try_into_settings()?;
                    codex_app_server::run_main_with_transport(
                        arg0_paths.clone(),
                        root_config_overrides,
                        codex_core::config_loader::LoaderOverrides::default(),
                        analytics_default_enabled,
                        transport,
                        codex_protocol::protocol::SessionSource::VSCode,
                        auth,
                    )
                    .await?;
                }
                Some(AppServerSubcommand::GenerateTs(gen_cli)) => {
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
                    codex_app_server_protocol::generate_json_with_experimental(
                        &gen_cli.out_dir,
                        gen_cli.experimental,
                    )?;
                }
                Some(AppServerSubcommand::GenerateInternalJsonSchema(gen_cli)) => {
                    codex_app_server_protocol::generate_internal_json_schema(&gen_cli.out_dir)?;
                }
            }
        }
        #[cfg(target_os = "macos")]
        Some(Subcommand::App(app_cli)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "app",
            )?;
            app_cmd::run_app(app_cli).await?;
        }
        Some(Subcommand::Resume(ResumeCommand {
            session_id,
            last,
            all,
            include_non_interactive,
            remote,
            config_overrides,
        })) => {
            interactive = finalize_resume_interactive(
                interactive,
                root_config_overrides.clone(),
                session_id,
                last,
                all,
                include_non_interactive,
                config_overrides,
            );
            let exit_info = run_interactive_tui(
                interactive,
                remote.remote.or(root_remote.clone()),
                remote
                    .remote_auth_token_env
                    .or(root_remote_auth_token_env.clone()),
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
                remote
                    .remote_auth_token_env
                    .or(root_remote_auth_token_env.clone()),
                arg0_paths.clone(),
            )
            .await?;
            handle_app_exit(exit_info)?;
        }
        Some(Subcommand::Login(mut login_cli)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "login",
            )?;
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
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "logout",
            )?;
            prepend_config_flags(
                &mut logout_cli.config_overrides,
                root_config_overrides.clone(),
            );
            run_logout(logout_cli.config_overrides).await;
        }
        Some(Subcommand::Completion(completion_cli)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "completion",
            )?;
            print_completion(completion_cli);
        }
        Some(Subcommand::Cloud(mut cloud_cli)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "cloud",
            )?;
            prepend_config_flags(
                &mut cloud_cli.config_overrides,
                root_config_overrides.clone(),
            );
            codex_cloud_tasks::run_main(cloud_cli, arg0_paths.codex_linux_sandbox_exe.clone())
                .await?;
        }
        Some(Subcommand::Sandbox(sandbox_args)) => match sandbox_args.cmd {
            SandboxCommand::Macos(mut seatbelt_cli) => {
                reject_remote_mode_for_subcommand(
                    root_remote.as_deref(),
                    root_remote_auth_token_env.as_deref(),
                    "sandbox macos",
                )?;
                prepend_config_flags(
                    &mut seatbelt_cli.config_overrides,
                    root_config_overrides.clone(),
                );
                codex_cli::run_command_under_seatbelt(
                    seatbelt_cli,
                    arg0_paths.codex_linux_sandbox_exe.clone(),
                )
                .await?;
            }
            SandboxCommand::Linux(mut landlock_cli) => {
                reject_remote_mode_for_subcommand(
                    root_remote.as_deref(),
                    root_remote_auth_token_env.as_deref(),
                    "sandbox linux",
                )?;
                prepend_config_flags(
                    &mut landlock_cli.config_overrides,
                    root_config_overrides.clone(),
                );
                codex_cli::run_command_under_landlock(
                    landlock_cli,
                    arg0_paths.codex_linux_sandbox_exe.clone(),
                )
                .await?;
            }
            SandboxCommand::Windows(mut windows_cli) => {
                reject_remote_mode_for_subcommand(
                    root_remote.as_deref(),
                    root_remote_auth_token_env.as_deref(),
                    "sandbox windows",
                )?;
                prepend_config_flags(
                    &mut windows_cli.config_overrides,
                    root_config_overrides.clone(),
                );
                codex_cli::run_command_under_windows(
                    windows_cli,
                    arg0_paths.codex_linux_sandbox_exe.clone(),
                )
                .await?;
            }
        },
        Some(Subcommand::Debug(DebugCommand { subcommand })) => match subcommand {
            DebugSubcommand::AppServer(cmd) => {
                reject_remote_mode_for_subcommand(
                    root_remote.as_deref(),
                    root_remote_auth_token_env.as_deref(),
                    "debug app-server",
                )?;
                run_debug_app_server_command(cmd).await?;
            }
            DebugSubcommand::PromptInput(cmd) => {
                reject_remote_mode_for_subcommand(
                    root_remote.as_deref(),
                    root_remote_auth_token_env.as_deref(),
                    "debug prompt-input",
                )?;
                run_debug_prompt_input_command(
                    cmd,
                    root_config_overrides,
                    interactive,
                    arg0_paths.clone(),
                )
                .await?;
            }
            DebugSubcommand::ClearMemories => {
                reject_remote_mode_for_subcommand(
                    root_remote.as_deref(),
                    root_remote_auth_token_env.as_deref(),
                    "debug clear-memories",
                )?;
                run_debug_clear_memories_command(&root_config_overrides, &interactive).await?;
            }
        },
        Some(Subcommand::Execpolicy(ExecpolicyCommand { sub })) => match sub {
            ExecpolicySubcommand::Check(cmd) => {
                reject_remote_mode_for_subcommand(
                    root_remote.as_deref(),
                    root_remote_auth_token_env.as_deref(),
                    "execpolicy check",
                )?;
                run_execpolicycheck(cmd)?
            }
        },
        Some(Subcommand::Apply(mut apply_cli)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "apply",
            )?;
            prepend_config_flags(
                &mut apply_cli.config_overrides,
                root_config_overrides.clone(),
            );
            run_apply_command(apply_cli, /*cwd*/ None).await?;
        }
        Some(Subcommand::ResponsesApiProxy(args)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "responses-api-proxy",
            )?;
            tokio::task::spawn_blocking(move || codex_responses_api_proxy::run_main(args))
                .await??;
        }
        Some(Subcommand::Responses(ResponsesCommand {})) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "responses",
            )?;
            run_responses_command(root_config_overrides).await?;
        }
        Some(Subcommand::StdioToUds(cmd)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "stdio-to-uds",
            )?;
            let socket_path = cmd.socket_path;
            tokio::task::spawn_blocking(move || codex_stdio_to_uds::run(socket_path.as_path()))
                .await??;
        }
        Some(Subcommand::ExecServer(cmd)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "exec-server",
            )?;
            run_exec_server_command(cmd, &arg0_paths).await?;
        }
        Some(Subcommand::Features(FeaturesCli { sub })) => match sub {
            FeaturesSubcommand::List => {
                reject_remote_mode_for_subcommand(
                    root_remote.as_deref(),
                    root_remote_auth_token_env.as_deref(),
                    "features list",
                )?;
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
                reject_remote_mode_for_subcommand(
                    root_remote.as_deref(),
                    root_remote_auth_token_env.as_deref(),
                    "features enable",
                )?;
                enable_feature_in_config(&interactive, &feature).await?;
            }
            FeaturesSubcommand::Disable(FeatureSetArgs { feature }) => {
                reject_remote_mode_for_subcommand(
                    root_remote.as_deref(),
                    root_remote_auth_token_env.as_deref(),
                    "features disable",
                )?;
                disable_feature_in_config(&interactive, &feature).await?;
            }
        },
    }

    Ok(())
}

async fn run_ztok_subcommand(
    args: Vec<std::ffi::OsString>,
    config_overrides: CliConfigOverrides,
    config_profile: Option<String>,
) -> anyhow::Result<()> {
    let session_id = std::env::var(CODEX_THREAD_ID_ENV_VAR)
        .ok()
        .filter(|thread_id| !thread_id.trim().is_empty());
    unsafe {
        std::env::remove_var(codex_ztok::ZTOK_RUNTIME_SETTINGS_ENV_VAR);
        std::env::remove_var(codex_ztok::ZTOK_SESSION_ID_ENV_VAR);
        std::env::remove_var(codex_ztok::ZTOK_BEHAVIOR_ENV_VAR);
    }

    if !args.iter().any(|arg| {
        arg.to_str()
            .is_some_and(|value| matches!(value, "-h" | "--help" | "-V" | "--version"))
    }) {
        let cli_kv_overrides = config_overrides
            .parse_overrides()
            .map_err(anyhow::Error::msg)?;
        let config = Config::load_with_cli_overrides_and_harness_overrides(
            cli_kv_overrides,
            ConfigOverrides {
                config_profile,
                ..Default::default()
            },
        )
        .await?;
        unsafe {
            std::env::set_var(
                codex_ztok::ZTOK_RUNTIME_SETTINGS_ENV_VAR,
                codex_ztok::encode_runtime_settings_env(
                    session_id.as_deref(),
                    config.ztok.behavior.as_str(),
                )?,
            );
        }
    }

    codex_ztok::run_from_os_args(args)
}

async fn run_exec_server_command(
    cmd: ExecServerCommand,
    arg0_paths: &Arg0DispatchPaths,
) -> anyhow::Result<()> {
    let codex_self_exe = arg0_paths
        .codex_self_exe
        .clone()
        .ok_or_else(|| anyhow::anyhow!("Codex executable path is not configured"))?;
    let runtime_paths = codex_exec_server::ExecServerRuntimePaths::new(
        codex_self_exe,
        arg0_paths.codex_linux_sandbox_exe.clone(),
    )?;
    codex_exec_server::run_main(&cmd.listen, runtime_paths)
        .await
        .map_err(anyhow::Error::from_boxed)
}

async fn enable_feature_in_config(interactive: &TuiCli, feature: &str) -> anyhow::Result<()> {
    FeatureToggles::validate_feature(feature)?;
    let codex_home = find_codex_home()?;
    ConfigEditsBuilder::new(&codex_home)
        .with_profile(interactive.config_profile.as_deref())
        .set_feature_enabled(feature, /*enabled*/ true)
        .apply()
        .await?;
    println!("已在 config.toml 中启用功能 `{feature}`。");
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
    println!("已在 config.toml 中禁用功能 `{feature}`。");
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
        "已启用开发中功能：{feature}。开发中功能尚未完成，行为可能不稳定。如需关闭此警告，请在 {} 中设置 `suppress_unstable_features_warning = true`。",
        config_path.display()
    );
}

async fn run_debug_prompt_input_command(
    cmd: DebugPromptInputCommand,
    root_config_overrides: CliConfigOverrides,
    interactive: TuiCli,
    arg0_paths: Arg0DispatchPaths,
) -> anyhow::Result<()> {
    let mut cli_kv_overrides = root_config_overrides
        .parse_overrides()
        .map_err(anyhow::Error::msg)?;
    if interactive.web_search {
        cli_kv_overrides.push((
            "web_search".to_string(),
            toml::Value::String("live".to_string()),
        ));
    }

    let approval_policy = if interactive.full_auto {
        Some(AskForApproval::OnRequest)
    } else if interactive.dangerously_bypass_approvals_and_sandbox {
        Some(AskForApproval::Never)
    } else {
        interactive.approval_policy.map(Into::into)
    };
    let sandbox_mode = if interactive.full_auto {
        Some(codex_protocol::config_types::SandboxMode::WorkspaceWrite)
    } else if interactive.dangerously_bypass_approvals_and_sandbox {
        Some(codex_protocol::config_types::SandboxMode::DangerFullAccess)
    } else {
        interactive.sandbox_mode.map(Into::into)
    };
    let overrides = ConfigOverrides {
        model: interactive.model,
        config_profile: interactive.config_profile,
        approval_policy,
        sandbox_mode,
        cwd: interactive.cwd,
        codex_self_exe: arg0_paths.codex_self_exe,
        codex_linux_sandbox_exe: arg0_paths.codex_linux_sandbox_exe,
        main_execve_wrapper_exe: arg0_paths.main_execve_wrapper_exe,
        show_raw_agent_reasoning: interactive.oss.then_some(true),
        ephemeral: Some(true),
        additional_writable_roots: interactive.add_dir,
        ..Default::default()
    };
    let config =
        Config::load_with_cli_overrides_and_harness_overrides(cli_kv_overrides, overrides).await?;

    let mut input = interactive
        .images
        .into_iter()
        .chain(cmd.images)
        .map(|path| UserInput::LocalImage { path })
        .collect::<Vec<_>>();
    if let Some(prompt) = cmd.prompt.or(interactive.prompt) {
        input.push(UserInput::Text {
            text: prompt.replace("\r\n", "\n").replace('\r', "\n"),
            text_elements: Vec::new(),
        });
    }

    let prompt_input = codex_core::build_prompt_input(config, input).await?;
    println!("{}", serde_json::to_string_pretty(&prompt_input)?);

    Ok(())
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
        state_db.clear_memory_data().await?;
        cleared_state_db = true;
    }

    clear_memory_roots_contents(&config.codex_home).await?;

    let mut message = if cleared_state_db {
        format!("Cleared memory state from {}.", state_path.display())
    } else {
        format!("No state db found at {}.", state_path.display())
    };
    message.push_str(&format!(
        " Cleared memory directories under {}.",
        config.codex_home.display()
    ));

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

fn reject_remote_mode_for_subcommand(
    remote: Option<&str>,
    remote_auth_token_env: Option<&str>,
    subcommand: &str,
) -> anyhow::Result<()> {
    if let Some(remote) = remote {
        anyhow::bail!(
            "`--remote {remote}` is only supported for interactive TUI commands, not `codex {subcommand}`"
        );
    }
    if remote_auth_token_env.is_some() {
        anyhow::bail!(
            "`--remote-auth-token-env` is only supported for interactive TUI commands, not `codex {subcommand}`"
        );
    }
    Ok(())
}

fn reject_remote_mode_for_app_server_subcommand(
    remote: Option<&str>,
    remote_auth_token_env: Option<&str>,
    subcommand: Option<&AppServerSubcommand>,
) -> anyhow::Result<()> {
    let subcommand_name = match subcommand {
        None => "app-server",
        Some(AppServerSubcommand::GenerateTs(_)) => "app-server generate-ts",
        Some(AppServerSubcommand::GenerateJsonSchema(_)) => "app-server generate-json-schema",
        Some(AppServerSubcommand::GenerateInternalJsonSchema(_)) => {
            "app-server generate-internal-json-schema"
        }
    };
    reject_remote_mode_for_subcommand(remote, remote_auth_token_env, subcommand_name)
}

fn read_remote_auth_token_from_env_var_with<F>(
    env_var_name: &str,
    get_var: F,
) -> anyhow::Result<String>
where
    F: FnOnce(&str) -> Result<String, std::env::VarError>,
{
    let auth_token = get_var(env_var_name)
        .map_err(|_| anyhow::anyhow!("environment variable `{env_var_name}` is not set"))?;
    let auth_token = auth_token.trim().to_string();
    if auth_token.is_empty() {
        anyhow::bail!("environment variable `{env_var_name}` is empty");
    }
    Ok(auth_token)
}

fn read_remote_auth_token_from_env_var(env_var_name: &str) -> anyhow::Result<String> {
    read_remote_auth_token_from_env_var_with(env_var_name, |name| std::env::var(name))
}

async fn run_interactive_tui(
    mut interactive: TuiCli,
    remote: Option<String>,
    remote_auth_token_env: Option<String>,
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
                "TERM is set to \"dumb\". Refusing to start the interactive TUI because no terminal is available for a confirmation prompt (stdin/stderr is not a TTY). Run in a supported terminal or unset TERM.",
            ));
        }

        eprintln!(
            "WARNING: TERM is set to \"dumb\". Codex's interactive TUI may not work in this terminal."
        );
        if !confirm("Continue anyway? [y/N]: ")? {
            return Ok(AppExitInfo::fatal(
                "Refusing to start the interactive TUI because TERM is set to \"dumb\". Run in a supported terminal or unset TERM.",
            ));
        }
    }

    let normalized_remote = remote
        .as_deref()
        .map(codex_tui::normalize_remote_addr)
        .transpose()
        .map_err(std::io::Error::other)?;
    if remote_auth_token_env.is_some() && normalized_remote.is_none() {
        return Ok(AppExitInfo::fatal(
            "`--remote-auth-token-env` requires `--remote`.",
        ));
    }
    let remote_auth_token = remote_auth_token_env
        .as_deref()
        .map(read_remote_auth_token_from_env_var)
        .transpose()
        .map_err(std::io::Error::other)?;
    codex_tui::run_main(
        interactive,
        arg0_paths,
        codex_core::config_loader::LoaderOverrides::default(),
        normalized_remote,
        remote_auth_token,
    )
    .await
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
    include_non_interactive: bool,
    resume_cli: TuiCli,
) -> TuiCli {
    // Start with the parsed interactive CLI so resume shares the same
    // configuration surface area as `codex` without additional flags.
    let resume_session_id = session_id;
    interactive.resume_picker = resume_session_id.is_none() && !last;
    interactive.resume_last = last;
    interactive.resume_session_id = resume_session_id;
    interactive.resume_show_all = show_all;
    interactive.resume_include_non_interactive = include_non_interactive;

    // Merge resume-scoped flags and overrides with highest precedence.
    merge_interactive_cli_flags(&mut interactive, resume_cli);

    // Propagate any root-level config overrides (e.g. `-c key=value`).
    prepend_config_flags(&mut interactive.config_overrides, root_config_overrides);

    interactive
}

fn finalize_zoffsec_resume_interactive(
    interactive: TuiCli,
    root_config_overrides: CliConfigOverrides,
    resume: codex_cli::zoffsec_cmd::ZoffsecResumeCommand,
) -> TuiCli {
    let mut interactive = finalize_resume_interactive(
        interactive,
        root_config_overrides,
        resume.session_id,
        resume.last,
        resume.all,
        /*include_non_interactive*/ false,
        resume.interactive,
    );
    interactive.resume_zoffsec_clean = true;
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
    if let Some(provider) = subcommand_cli.provider {
        interactive.provider = Some(provider);
    }
    if let Some(oss_provider) = subcommand_cli.oss_provider {
        interactive.oss_provider = Some(oss_provider);
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
    if subcommand_cli.federation_enable {
        interactive.federation_enable = true;
    }
    if let Some(name) = subcommand_cli.federation_name {
        interactive.federation_name = Some(name);
    }
    if let Some(role) = subcommand_cli.federation_role {
        interactive.federation_role = Some(role);
    }
    if let Some(scope) = subcommand_cli.federation_scope {
        interactive.federation_scope = Some(scope);
    }
    if let Some(state_root) = subcommand_cli.federation_state_root {
        interactive.federation_state_root = Some(state_root);
    }
    if let Some(instance_id) = subcommand_cli.federation_instance_id {
        interactive.federation_instance_id = Some(instance_id);
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

fn parse_multitool_cli_from_env() -> MultitoolCli {
    let raw_args = std::env::args_os().collect::<Vec<_>>();
    if raw_args.is_empty() {
        return MultitoolCli::parse();
    }
    parse_multitool_cli(raw_args)
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
            if err.use_stderr() {
                let _ = std::io::stderr().write_all(rendered.as_bytes());
            } else {
                let _ = std::io::stdout().write_all(rendered.as_bytes());
            }
            std::process::exit(err.exit_code());
        }
    }
}

fn localized_multitool_command() -> clap::Command {
    localize_clap_command(MultitoolCli::command())
}

fn localize_clap_command(mut cmd: clap::Command) -> clap::Command {
    cmd.build();
    localize_clap_command_tree(cmd)
}

fn localize_clap_command_tree(cmd: clap::Command) -> clap::Command {
    localize_help_subcommand(cmd.mut_subcommands(localize_clap_command_tree))
}

fn localize_help_subcommand(cmd: clap::Command) -> clap::Command {
    if cmd
        .get_subcommands()
        .any(|subcmd| subcmd.get_name() == "help")
    {
        cmd.mut_subcommand("help", |subcmd| {
            let subcmd = subcmd.about("显示此消息或指定子命令的帮助");
            if subcmd
                .get_arguments()
                .any(|arg| arg.get_id().as_str() == "subcommand")
            {
                subcmd.mut_arg("subcommand", |arg| arg.help("显示指定子命令的帮助"))
            } else {
                subcmd
            }
        })
    } else {
        cmd
    }
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
        .replace(
            "Print this message or the help of the given\nsubcommand(s)",
            "显示此消息或指定子命令的帮助",
        )
        .replace(
            "Print this message or the help of the given\n                 subcommand(s)",
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
        .replace("error: invalid value", "错误：无效的值")
        .replace(
            "For more information, try '--help'.",
            "更多信息请使用 '--help'。",
        )
        .replace("[PROMPT]", "[提示]")
        .replace("<FILE>", "<文件>")
        .replace("<MODEL>", "<模型>")
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
            mut interactive,
            subcommand,
            feature_toggles: _,
            remote: _,
        } = cli;
        let root_overrides = std::mem::take(&mut interactive.config_overrides);

        let Subcommand::Resume(ResumeCommand {
            session_id,
            last,
            all,
            include_non_interactive,
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
            include_non_interactive,
            resume_cli,
        )
    }

    fn finalize_fork_from_args(args: &[&str]) -> TuiCli {
        let cli = MultitoolCli::try_parse_from(args).expect("parse");
        let MultitoolCli {
            mut interactive,
            subcommand,
            feature_toggles: _,
            remote: _,
        } = cli;
        let root_overrides = std::mem::take(&mut interactive.config_overrides);

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

    fn finalize_zoffsec_resume_from_args(args: &[&str]) -> TuiCli {
        let cli = MultitoolCli::try_parse_from(args).expect("parse");
        let MultitoolCli {
            mut interactive,
            subcommand,
            feature_toggles: _,
            remote: _,
        } = cli;
        let root_overrides = std::mem::take(&mut interactive.config_overrides);

        let Subcommand::Zoffsec(ZoffsecCommand {
            subcommand: Some(ZoffsecSubcommand::Resume(resume)),
            ..
        }) = subcommand.expect("zoffsec present")
        else {
            unreachable!()
        };

        finalize_zoffsec_resume_interactive(interactive, root_overrides, *resume)
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

    #[test]
    fn debug_prompt_input_parses_prompt_and_images() {
        let cli = MultitoolCli::try_parse_from([
            "codex",
            "debug",
            "prompt-input",
            "hello",
            "--image",
            "/tmp/a.png,/tmp/b.png",
        ])
        .expect("parse");

        let Some(Subcommand::Debug(DebugCommand {
            subcommand: DebugSubcommand::PromptInput(cmd),
        })) = cli.subcommand
        else {
            panic!("expected debug prompt-input subcommand");
        };

        assert_eq!(cmd.prompt.as_deref(), Some("hello"));
        assert_eq!(
            cmd.images,
            vec![PathBuf::from("/tmp/a.png"), PathBuf::from("/tmp/b.png")]
        );
    }

    #[test]
    fn responses_subcommand_is_hidden_from_help_but_parses() {
        let help = localize_help_output(localized_multitool_command().render_help().to_string());
        assert!(!help.contains("responses"));

        let cli = MultitoolCli::try_parse_from(["codex", "responses"]).expect("parse");
        assert!(matches!(cli.subcommand, Some(Subcommand::Responses(_))));
    }

    #[test]
    fn help_output_lists_ztok_and_localizes_tail_defaults() {
        let help = localize_help_output(localized_multitool_command().render_help().to_string());
        assert!(help.contains("ztok"));
        assert!(help.contains("显示此消息或指定子命令的帮助"));
        assert!(help.contains("显示帮助"));
        assert!(help.contains("显示版本"));
        assert!(!help.contains("Print this message or the help of the given subcommand(s)"));
        assert!(!help.contains("Print help"));
        assert!(!help.contains("Print version"));
    }

    #[test]
    fn autogenerated_help_subcommand_is_localized() {
        let mut command = localized_multitool_command();
        command.build();

        let help_subcommand = command.find_subcommand("help").expect("help subcommand");
        assert_eq!(
            help_subcommand.get_about().map(ToString::to_string),
            Some("显示此消息或指定子命令的帮助".to_string())
        );

        if let Some(arg) = help_subcommand
            .get_arguments()
            .find(|arg| arg.get_id().as_str() == "subcommand")
        {
            assert_eq!(
                arg.get_help().map(ToString::to_string),
                Some("显示指定子命令的帮助".to_string())
            );
        }
    }

    #[test]
    fn ztldr_help_output_localizes_nested_help_subcommand() {
        let rendered = localized_multitool_command()
            .try_get_matches_from(["codex", "ztldr", "--help"])
            .expect_err("help should short-circuit")
            .to_string();
        let help = localize_help_output(rendered);

        assert!(help.contains("显示此消息或指定子命令的帮助"));
        assert!(!help.contains("Print this message or the help"));
    }

    #[test]
    fn resume_short_alias_r_parses() {
        let cli = MultitoolCli::try_parse_from(["codex", "r", "--last"]).expect("parse");
        assert!(matches!(cli.subcommand, Some(Subcommand::Resume(_))));
    }

    #[test]
    fn plugin_marketplace_add_parses_under_plugin() {
        let cli =
            MultitoolCli::try_parse_from(["codex", "plugin", "marketplace", "add", "owner/repo"])
                .expect("parse");

        assert!(matches!(cli.subcommand, Some(Subcommand::Plugin(_))));
    }

    #[test]
    fn plugin_marketplace_upgrade_parses_under_plugin() {
        let cli =
            MultitoolCli::try_parse_from(["codex", "plugin", "marketplace", "upgrade", "debug"])
                .expect("parse");

        assert!(matches!(cli.subcommand, Some(Subcommand::Plugin(_))));
    }

    #[test]
    fn plugin_marketplace_remove_parses_under_plugin() {
        let cli =
            MultitoolCli::try_parse_from(["codex", "plugin", "marketplace", "remove", "debug"])
                .expect("parse");

        assert!(matches!(cli.subcommand, Some(Subcommand::Plugin(_))));
    }

    #[test]
    fn marketplace_no_longer_parses_at_top_level() {
        let add_result =
            MultitoolCli::try_parse_from(["codex", "marketplace", "add", "owner/repo"]);
        assert!(add_result.is_err());

        let upgrade_result =
            MultitoolCli::try_parse_from(["codex", "marketplace", "upgrade", "debug"]);
        assert!(upgrade_result.is_err());

        let remove_result =
            MultitoolCli::try_parse_from(["codex", "marketplace", "remove", "debug"]);
        assert!(remove_result.is_err());
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
        let lines = format_exit_messages(exit_info, /*color_enabled*/ false);
        assert!(lines.is_empty());
    }

    #[test]
    fn format_exit_messages_includes_resume_hint_without_color() {
        let exit_info = sample_exit_info(
            Some("123e4567-e89b-12d3-a456-426614174000"),
            /*thread_name*/ None,
        );
        let lines = format_exit_messages(exit_info, /*color_enabled*/ false);
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
        let exit_info = sample_exit_info(
            Some("123e4567-e89b-12d3-a456-426614174000"),
            /*thread_name*/ None,
        );
        let lines = format_exit_messages(exit_info, /*color_enabled*/ true);
        assert_eq!(lines.len(), 2);
        assert!(lines[1].contains("\u{1b}[36m"));
    }

    #[test]
    fn format_exit_messages_prefers_thread_name() {
        let exit_info = sample_exit_info(
            Some("123e4567-e89b-12d3-a456-426614174000"),
            Some("my-thread"),
        );
        let lines = format_exit_messages(exit_info, /*color_enabled*/ false);
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
    fn resume_include_non_interactive_flag_sets_source_filter_override() {
        let interactive =
            finalize_resume_from_args(["codex", "resume", "--include-non-interactive"].as_ref());

        assert!(interactive.resume_picker);
        assert!(interactive.resume_include_non_interactive);
    }

    #[test]
    fn resume_merges_option_flags_and_full_auto() {
        let interactive = finalize_resume_from_args(
            [
                "codex",
                "resume",
                "sid",
                "--oss",
                "-P",
                "oss",
                "--local-provider",
                "ollama",
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
        assert_eq!(interactive.provider.as_deref(), Some("oss"));
        assert_eq!(interactive.oss_provider.as_deref(), Some("ollama"));
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
    fn fork_merges_provider_flags() {
        let interactive = finalize_fork_from_args(
            [
                "codex",
                "fork",
                "sid",
                "-P",
                "openai",
                "--local-provider",
                "lmstudio",
            ]
            .as_ref(),
        );

        assert_eq!(interactive.provider.as_deref(), Some("openai"));
        assert_eq!(interactive.oss_provider.as_deref(), Some("lmstudio"));
        assert!(!interactive.fork_picker);
        assert!(!interactive.fork_last);
        assert_eq!(interactive.fork_session_id.as_deref(), Some("sid"));
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
    fn config_flag_parses_for_interactive_root() {
        let cli =
            MultitoolCli::try_parse_from(["codex", "--config", "model=\"o3\""]).expect("parse");
        assert_eq!(
            cli.interactive.config_overrides.raw_overrides,
            vec!["model=\"o3\"".to_string()]
        );
    }

    #[test]
    fn remote_auth_token_env_flag_parses_for_interactive_root() {
        let cli = MultitoolCli::try_parse_from([
            "codex",
            "--remote-auth-token-env",
            "CODEX_REMOTE_AUTH_TOKEN",
            "--remote",
            "ws://127.0.0.1:4500",
        ])
        .expect("parse");
        assert_eq!(
            cli.remote.remote_auth_token_env.as_deref(),
            Some("CODEX_REMOTE_AUTH_TOKEN")
        );
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
    fn zoffsec_subcommand_registers_at_top_level() {
        let cli =
            MultitoolCli::try_parse_from(["codex", "zoffsec", "--template", "reverse", "triage"])
                .expect("parse");

        let Some(Subcommand::Zoffsec(command)) = cli.subcommand else {
            panic!("expected zoffsec subcommand");
        };

        assert_eq!(command.start.template.as_str(), "reverse");
        assert_eq!(command.start.interactive.prompt.as_deref(), Some("triage"));
        assert!(command.subcommand.is_none());
    }

    #[test]
    fn finalize_zoffsec_resume_enables_clean_before_resume() {
        let interactive = finalize_zoffsec_resume_from_args(&[
            "codex",
            "zoffsec",
            "resume",
            "--last",
            "-m",
            "gpt-5.1-test",
        ]);

        assert!(interactive.resume_last);
        assert!(interactive.resume_zoffsec_clean);
        assert_eq!(interactive.model.as_deref(), Some("gpt-5.1-test"));
    }

    #[test]
    fn finalize_resume_preserves_federation_flags() {
        let interactive = finalize_resume_from_args(&[
            "codex",
            "resume",
            "--last",
            "--federation-enable",
            "--federation-name",
            "resume-worker",
            "--federation-role",
            "worker",
            "--federation-scope",
            "repo",
            "--federation-state-root",
            "/tmp/fed",
            "--federation-instance-id",
            "instance-1",
        ]);

        assert!(interactive.resume_last);
        assert!(interactive.federation_enable);
        assert_eq!(
            interactive.federation_name.as_deref(),
            Some("resume-worker")
        );
        assert_eq!(interactive.federation_role.as_deref(), Some("worker"));
        assert_eq!(interactive.federation_scope.as_deref(), Some("repo"));
        assert_eq!(
            interactive.federation_state_root,
            Some(PathBuf::from("/tmp/fed"))
        );
        assert_eq!(
            interactive.federation_instance_id.as_deref(),
            Some("instance-1")
        );
    }

    #[test]
    fn finalize_fork_preserves_federation_flags() {
        let interactive = finalize_fork_from_args(&[
            "codex",
            "fork",
            "--last",
            "--federation-enable",
            "--federation-name",
            "fork-worker",
        ]);

        assert!(interactive.fork_last);
        assert!(interactive.federation_enable);
        assert_eq!(interactive.federation_name.as_deref(), Some("fork-worker"));
    }

    #[test]
    fn reject_remote_mode_for_non_interactive_subcommands() {
        let err = reject_remote_mode_for_subcommand(
            Some("127.0.0.1:4500"),
            /*remote_auth_token_env*/ None,
            "exec",
        )
        .expect_err("non-interactive subcommands should reject --remote");
        assert!(
            err.to_string()
                .contains("only supported for interactive TUI commands")
        );
    }

    #[test]
    fn reject_remote_auth_token_env_for_non_interactive_subcommands() {
        let err = reject_remote_mode_for_subcommand(
            /*remote*/ None,
            Some("CODEX_REMOTE_AUTH_TOKEN"),
            "exec",
        )
        .expect_err("non-interactive subcommands should reject --remote-auth-token-env");
        assert!(
            err.to_string()
                .contains("only supported for interactive TUI commands")
        );
    }

    #[test]
    fn reject_remote_auth_token_env_for_app_server_generate_internal_json_schema() {
        let subcommand =
            AppServerSubcommand::GenerateInternalJsonSchema(GenerateInternalJsonSchemaCommand {
                out_dir: PathBuf::from("/tmp/out"),
            });
        let err = reject_remote_mode_for_app_server_subcommand(
            /*remote*/ None,
            Some("CODEX_REMOTE_AUTH_TOKEN"),
            Some(&subcommand),
        )
        .expect_err("non-interactive app-server subcommands should reject --remote-auth-token-env");
        assert!(err.to_string().contains("generate-internal-json-schema"));
    }

    #[test]
    fn read_remote_auth_token_from_env_var_reports_missing_values() {
        let err = read_remote_auth_token_from_env_var_with("CODEX_REMOTE_AUTH_TOKEN", |_| {
            Err(std::env::VarError::NotPresent)
        })
        .expect_err("missing env vars should be rejected");
        assert!(err.to_string().contains("is not set"));
    }

    #[test]
    fn read_remote_auth_token_from_env_var_trims_values() {
        let auth_token =
            read_remote_auth_token_from_env_var_with("CODEX_REMOTE_AUTH_TOKEN", |_| {
                Ok("  bearer-token  ".to_string())
            })
            .expect("env var should parse");
        assert_eq!(auth_token, "bearer-token");
    }

    #[test]
    fn read_remote_auth_token_from_env_var_rejects_empty_values() {
        let err = read_remote_auth_token_from_env_var_with("CODEX_REMOTE_AUTH_TOKEN", |_| {
            Ok(" \n\t ".to_string())
        })
        .expect_err("empty env vars should be rejected");
        assert!(err.to_string().contains("is empty"));
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
    fn app_server_listen_off_parses() {
        let app_server = app_server_from_args(["codex", "app-server", "--listen", "off"].as_ref());
        assert_eq!(app_server.listen, codex_app_server::AppServerTransport::Off);
    }

    #[test]
    fn app_server_listen_invalid_url_fails_to_parse() {
        let parse_result =
            MultitoolCli::try_parse_from(["codex", "app-server", "--listen", "http://foo"]);
        assert!(parse_result.is_err());
    }

    #[test]
    fn app_server_capability_token_flags_parse() {
        let app_server = app_server_from_args(
            [
                "codex",
                "app-server",
                "--ws-auth",
                "capability-token",
                "--ws-token-file",
                "/tmp/codex-token",
            ]
            .as_ref(),
        );
        assert_eq!(
            app_server.auth.ws_auth,
            Some(codex_app_server::WebsocketAuthCliMode::CapabilityToken)
        );
        assert_eq!(
            app_server.auth.ws_token_file,
            Some(PathBuf::from("/tmp/codex-token"))
        );
    }

    #[test]
    fn app_server_signed_bearer_flags_parse() {
        let app_server = app_server_from_args(
            [
                "codex",
                "app-server",
                "--ws-auth",
                "signed-bearer-token",
                "--ws-shared-secret-file",
                "/tmp/codex-secret",
                "--ws-issuer",
                "issuer",
                "--ws-audience",
                "audience",
                "--ws-max-clock-skew-seconds",
                "9",
            ]
            .as_ref(),
        );
        assert_eq!(
            app_server.auth.ws_auth,
            Some(codex_app_server::WebsocketAuthCliMode::SignedBearerToken)
        );
        assert_eq!(
            app_server.auth.ws_shared_secret_file,
            Some(PathBuf::from("/tmp/codex-secret"))
        );
        assert_eq!(app_server.auth.ws_issuer.as_deref(), Some("issuer"));
        assert_eq!(app_server.auth.ws_audience.as_deref(), Some("audience"));
        assert_eq!(app_server.auth.ws_max_clock_skew_seconds, Some(9));
    }

    #[test]
    fn app_server_rejects_removed_insecure_non_loopback_flag() {
        let parse_result = MultitoolCli::try_parse_from([
            "codex",
            "app-server",
            "--allow-unauthenticated-non-loopback-ws",
        ]);
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
    fn ztldr_daemon_status_parses() {
        let cli = MultitoolCli::try_parse_from(["codex", "ztldr", "daemon", "status"])
            .expect("parse should succeed");
        assert!(matches!(cli.subcommand, Some(Subcommand::Tldr(_))));
    }

    #[test]
    fn zmemory_read_parses_uri() {
        let cli = MultitoolCli::try_parse_from(["codex", "zmemory", "read", "core://agent"])
            .expect("parse should succeed");
        let Some(Subcommand::Zmemory(zmemory_cli)) = cli.subcommand else {
            panic!("expected zmemory subcommand");
        };
        let zmemory_cmd::ZmemorySubcommand::Read(read) = zmemory_cli.subcommand else {
            panic!("expected zmemory read");
        };
        assert_eq!(read.uri, "core://agent");
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
    fn feature_toggles_accept_removed_image_detail_original_flag() {
        let toggles = FeatureToggles {
            enable: vec!["image_detail_original".to_string()],
            disable: Vec::new(),
        };
        let overrides = toggles.to_overrides().expect("valid features");
        assert_eq!(
            overrides,
            vec!["features.image_detail_original=true".to_string(),]
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
        assert_eq!(err.to_string(), "未知的功能开关：does_not_exist");
    }
}
