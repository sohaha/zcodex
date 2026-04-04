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
use codex_state::StateRuntime;
use codex_state::state_db_path;
use codex_tui::AppExitInfo;
use codex_tui::Cli as TuiCli;
use codex_tui::ExitReason;
use codex_tui::update_action::UpdateAction;
use codex_utils_cli::CliConfigOverrides;
use codex_ztok::alias_name as ztok_alias_name;
use codex_ztok::is_alias_invocation as is_ztok_alias_invocation;
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
mod tldr_cmd;
#[cfg(not(windows))]
mod wsl_paths;
mod zmemory_cmd;

use crate::mcp_cmd::McpCli;
use crate::tldr_cmd::TldrCli;
use crate::zmemory_cmd::ZmemoryCli;
use crate::zmemory_cmd::run_zmemory_command;

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
    Ztok(ZtokArgs),

    /// 运行原生 TLDR 代码上下文分析命令。
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
    /// 要生成补全脚本的 shell 类型。
    #[clap(value_enum, default_value_t = Shell::Bash)]
    shell: Shell,
}

#[derive(Debug, Args)]
#[command(disable_help_flag = true, disable_version_flag = true)]
struct ZtokArgs {
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
    /// 用于调试应用服务器的工具。
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
    /// 向应用服务器的 v2 接口发送消息。
    SendMessageV2(DebugAppServerSendMessageV2Command),
}

#[derive(Debug, Parser)]
struct DebugAppServerSendMessageV2Command {
    /// 要发送的用户消息。
    #[arg(value_name = "用户消息", required = true)]
    user_message: String,
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

    #[command(flatten)]
    auth: codex_app_server::AppServerWebsocketAuthArgs,
}

#[derive(Debug, clap::Subcommand)]
#[allow(clippy::enum_variant_names)]
enum AppServerSubcommand {
    /// [实验性] 启动本地 OpenAI 兼容 HTTP 服务。
    #[clap(name = "openai-compat")]
    OpenAiCompat(codex_app_server::OpenAiCompatServerArgs),

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

    if token_usage.is_zero() {
        return Vec::new();
    }

    let mut lines = vec![format_localized_token_usage(&token_usage)];

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

fn format_localized_token_usage(token_usage: &codex_protocol::protocol::TokenUsage) -> String {
    let mut line = format!(
        "Token 使用量：总计={} 输入={}",
        codex_protocol::num_format::format_with_separators(token_usage.blended_total()),
        codex_protocol::num_format::format_with_separators(token_usage.non_cached_input()),
    );

    if token_usage.cached_input() > 0 {
        line.push_str(&format!(
            "（+ {} 缓存）",
            codex_protocol::num_format::format_with_separators(token_usage.cached_input())
        ));
    }

    line.push_str(&format!(
        " 输出={}",
        codex_protocol::num_format::format_with_separators(token_usage.output_tokens)
    ));

    if token_usage.reasoning_output_tokens > 0 {
        line.push_str(&format!(
            "（推理 {}）",
            codex_protocol::num_format::format_with_separators(token_usage.reasoning_output_tokens)
        ));
    }

    line
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
            codex_ztok::run_from_os_args(ztok_cli.args)?;
        }
        Some(Subcommand::Tldr(tldr_cli)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "tldr",
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
                Some(AppServerSubcommand::OpenAiCompat(openai_cli)) => {
                    codex_app_server::run_openai_compat_server(
                        arg0_paths.clone(),
                        root_config_overrides,
                        codex_core::config_loader::LoaderOverrides::default(),
                        openai_cli,
                    )
                    .await?;
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
                codex_cli::debug_sandbox::run_command_under_seatbelt(
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
                codex_cli::debug_sandbox::run_command_under_landlock(
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
                codex_cli::debug_sandbox::run_command_under_windows(
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
                    let status = if enabled { "已启用" } else { "未启用" };
                    println!("{name:<name_width$}  {stage:<stage_width$}  {status}");
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
        "已启用开发中的功能：{feature}。开发中的功能尚未完善，行为可能不稳定。若要关闭此警告，请在 {} 中设置 `suppress_unstable_features_warning = true`。",
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
        format!("已清除 {} 中的记忆状态。", state_path.display())
    } else {
        format!("未在 {} 找到状态数据库。", state_path.display())
    };

    if removed_memory_root {
        message.push_str(&format!(" 已移除 {}。", memory_root.display()));
    } else {
        message.push_str(&format!(" 未在 {} 找到记忆目录。", memory_root.display()));
    }

    println!("{message}");

    Ok(())
}

/// 在前面插入根级覆盖项，使其优先级低于子命令后显式给出的 CLI 覆盖项。
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
        anyhow::bail!("`--remote {remote}` 仅支持交互式 TUI 命令，不支持 `codex {subcommand}`");
    }
    if remote_auth_token_env.is_some() {
        anyhow::bail!(
            "`--remote-auth-token-env` 仅支持交互式 TUI 命令，不支持 `codex {subcommand}`"
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
        Some(AppServerSubcommand::OpenAiCompat(_)) => "app-server openai-compat",
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
    let auth_token =
        get_var(env_var_name).map_err(|_| anyhow::anyhow!("环境变量 `{env_var_name}` 未设置"))?;
    let auth_token = auth_token.trim().to_string();
    if auth_token.is_empty() {
        anyhow::bail!("环境变量 `{env_var_name}` 为空");
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

    let normalized_remote = remote
        .as_deref()
        .map(codex_tui::normalize_remote_addr)
        .transpose()
        .map_err(std::io::Error::other)?;
    if remote_auth_token_env.is_some() && normalized_remote.is_none() {
        return Ok(AppExitInfo::fatal(
            "`--remote-auth-token-env` 需要配合 `--remote` 使用。",
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

/// 合并 `codex resume`/`codex fork` 上提供的标志，使其优先于根级标志。
/// 仅覆盖子命令作用域 CLI 中显式设置的字段，同时追加最高优先级的 `-c key=value`。
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

fn parse_multitool_cli_from_env() -> MultitoolCli {
    let raw_args = std::env::args_os().collect::<Vec<_>>();
    if raw_args.is_empty() {
        return MultitoolCli::parse();
    }

    let parsed_args = if is_ztok_alias_invocation(&raw_args[0]) {
        let mut injected_args = Vec::with_capacity(raw_args.len() + 1);
        injected_args.push(raw_args[0].clone());
        injected_args.push(ztok_alias_name().into());
        injected_args.extend(raw_args.into_iter().skip(1));
        injected_args
    } else {
        raw_args
    };

    let mut cli = parse_multitool_cli(parsed_args.clone());
    restore_ztok_explicit_double_dash(&parsed_args, &mut cli);
    cli
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

fn restore_ztok_explicit_double_dash(raw_args: &[std::ffi::OsString], cli: &mut MultitoolCli) {
    let Some(Subcommand::Ztok(ztok_cli)) = cli.subcommand.as_mut() else {
        return;
    };

    if ztok_cli.args.first().is_some_and(|arg| arg == "--") {
        return;
    }

    if ztok_subcommand_uses_explicit_double_dash(raw_args, ztok_cli.args.len()) {
        ztok_cli.args.insert(0, "--".into());
    }
}

fn ztok_subcommand_uses_explicit_double_dash(
    raw_args: &[std::ffi::OsString],
    ztok_arg_count: usize,
) -> bool {
    raw_args.iter().enumerate().skip(1).any(|(index, arg)| {
        arg == ztok_alias_name()
            && raw_args.get(index + 1).is_some_and(|next| next == "--")
            && raw_args.len().saturating_sub(index + 2) == ztok_arg_count
    })
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
        .replace(
            "Print this message or the help of the given\nsubcommand(s)",
            "显示此消息或指定子命令的帮助",
        )
        .replace(
            "Print this message or the help of the given\n                   subcommand(s)",
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
        .replace("<OSS_PROVIDER>", "<提供方>")
        .replace("<CONFIG_PROFILE>", "<配置文件>")
        .replace("<SANDBOX_MODE>", "<沙箱策略>")
        .replace("<APPROVAL_POLICY>", "<批准策略>")
        .replace("<DIR>", "<目录>")
        .replace("<ADDR>", "<地址>")
        .replace("<ENV_VAR>", "<环境变量>")
        .replace("Optional user prompt to start the session", "可选的会话启动提示")
        .replace(
            "Optional image(s) to attach to the initial prompt",
            "可选的初始提示附件图片",
        )
        .replace("Model the agent should use", "智能体应使用的模型")
        .replace(
            "Convenience flag to select the local open source model provider. Equivalent to -c model_provider=oss; verifies a local LM Studio or Ollama server is running",
            "便捷标志，用于选择本地开源模型提供方。等价于 -c model_provider=oss；验证本地 LM Studio 或 Ollama 服务器是否正在运行",
        )
        .replace(
            "Convenience flag to select the local open source model provider. Equivalent to -c model_provider=oss; verifies a\n          local LM Studio or Ollama server is running",
            "便捷标志，用于选择本地开源模型提供方。等价于 -c model_provider=oss；验证本地 LM Studio 或 Ollama 服务器是否正在运行",
        )
        .replace(
            "Specify which local provider to use (lmstudio or ollama). If not specified with --oss, will use config default or show selection",
            "指定要使用的本地提供方（lmstudio 或 ollama）。如果与 --oss 一起使用时未指定，将使用配置默认值或显示选择",
        )
        .replace(
            "Specify which local provider to use (lmstudio or ollama). If not specified with --oss, will use config default\n          or show selection",
            "指定要使用的本地提供方（lmstudio 或 ollama）。如果与 --oss 一起使用时未指定，将使用配置默认值或显示选择",
        )
        .replace(
            "Configuration profile from config.toml to specify default options",
            "来自 config.toml 的配置配置文件，用于指定默认选项",
        )
        .replace(
            "Select the sandbox policy to use when executing model-generated shell commands",
            "选择执行模型生成的 shell 命令时要使用的沙箱策略",
        )
        .replace(
            "Configure when the model requires human approval before executing a command",
            "配置模型在执行命令前何时需要人工批准",
        )
        .replace(
            "Convenience alias for low-friction sandboxed automatic execution (-a on-request, --sandbox workspace-write)",
            "低摩擦沙箱自动执行的便捷别名（-a on-request, --sandbox workspace-write）",
        )
        .replace(
            "Skip all confirmation prompts and execute commands without sandboxing. EXTREMELY DANGEROUS. Intended solely for running in environments that are externally sandboxed",
            "跳过所有确认提示并在无沙箱的情况下执行命令。极度危险。仅适用于在外部沙箱环境中运行",
        )
        .replace(
            "Tell the agent to use the specified directory as its working root",
            "告诉智能体使用指定目录作为其工作根目录",
        )
        .replace("Enable live web search.", "启用实时网络搜索。")
        .replace(
            "When enabled, the native Responses `web_search` tool is available to the model (no per‑call approval)",
            "启用后，原生 Responses `web_search` 工具可供模型使用（无需每次调用批准）",
        )
        .replace(
            "Additional directories that should be writable alongside the primary workspace",
            "除主工作区外还应可写入的附加目录",
        )
        .replace("Disable alternate screen mode", "禁用备用屏幕模式")
        .replace(
            "Runs the TUI in inline mode, preserving terminal scrollback history. This is useful in terminal multiplexers like Zellij that follow the xterm spec strictly and disable scrollback in alternate screen buffers.",
            "以内联模式运行 TUI，保留终端滚动历史记录。这在严格遵循 xterm 规范并禁用备用屏幕缓冲区中滚动的终端复用器（如 Zellij）中很有用。",
        )
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
                "Token 使用量：总计=2 输入=0 输出=2".to_string(),
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
    fn format_localized_token_usage_preserves_cached_and_reasoning_breakdown() {
        let usage = TokenUsage {
            input_tokens: 8,
            cached_input_tokens: 3,
            output_tokens: 5,
            reasoning_output_tokens: 2,
            total_tokens: 13,
        };

        assert_eq!(
            format_localized_token_usage(&usage),
            "Token 使用量：总计=10 输入=5（+ 3 缓存） 输出=5（推理 2）"
        );
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
                "Token 使用量：总计=2 输入=0 输出=2".to_string(),
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
    fn reject_remote_mode_for_non_interactive_subcommands() {
        let err = reject_remote_mode_for_subcommand(
            Some("127.0.0.1:4500"),
            /*remote_auth_token_env*/ None,
            "exec",
        )
        .expect_err("non-interactive subcommands should reject --remote");
        assert!(err.to_string().contains("仅支持交互式 TUI 命令"));
    }

    #[test]
    fn reject_remote_auth_token_env_for_non_interactive_subcommands() {
        let err = reject_remote_mode_for_subcommand(
            /*remote*/ None,
            Some("CODEX_REMOTE_AUTH_TOKEN"),
            "exec",
        )
        .expect_err("non-interactive subcommands should reject --remote-auth-token-env");
        assert!(err.to_string().contains("仅支持交互式 TUI 命令"));
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
        assert!(err.to_string().contains("未设置"));
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
        assert!(err.to_string().contains("为空"));
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
    fn ztok_parse_restores_explicit_double_dash_for_raw_wrapper_commands() {
        let raw_args = vec![
            std::ffi::OsString::from("codex"),
            std::ffi::OsString::from("ztok"),
            std::ffi::OsString::from("--"),
            std::ffi::OsString::from("env"),
            std::ffi::OsString::from("FOO=1"),
            std::ffi::OsString::from("git"),
            std::ffi::OsString::from("status"),
        ];

        let mut cli = parse_multitool_cli(raw_args.clone());
        restore_ztok_explicit_double_dash(&raw_args, &mut cli);

        let Some(Subcommand::Ztok(ztok_cli)) = cli.subcommand else {
            panic!("expected ztok subcommand");
        };
        assert_eq!(ztok_cli.args, vec!["--", "env", "FOO=1", "git", "status"]);
    }

    #[test]
    fn tldr_daemon_ping_parses() {
        let cli = MultitoolCli::try_parse_from(["codex", "tldr", "daemon", "ping"])
            .expect("parse should succeed");
        assert!(matches!(cli.subcommand, Some(Subcommand::Tldr(_))));
    }

    #[test]
    fn short_resume_alias_parses() {
        let cli = MultitoolCli::try_parse_from(["codex", "r"]).expect("parse should succeed");
        assert!(matches!(cli.subcommand, Some(Subcommand::Resume(_))));
    }

    #[test]
    fn localize_help_output_translates_usage_header() {
        assert_eq!(localize_help_output("Usage:\n".to_string()), "用法：\n");
    }

    #[test]
    fn ztok_parse_does_not_inject_double_dash_without_explicit_boundary() {
        let cli = parse_multitool_cli(["codex", "ztok", "env", "FOO=1", "git", "status"]);

        let Some(Subcommand::Ztok(ztok_cli)) = cli.subcommand else {
            panic!("expected ztok subcommand");
        };

        assert_eq!(
            ztok_cli.args,
            vec![
                std::ffi::OsString::from("env"),
                std::ffi::OsString::from("FOO=1"),
                std::ffi::OsString::from("git"),
                std::ffi::OsString::from("status"),
            ]
        );
    }

    #[test]
    fn tldr_extract_parses_path_and_optional_language() {
        let cli = MultitoolCli::try_parse_from([
            "codex",
            "tldr",
            "extract",
            "--project",
            ".",
            "--lang",
            "rust",
            "src/lib.rs",
        ])
        .expect("parse should succeed");

        let Some(Subcommand::Tldr(tldr_cli)) = cli.subcommand else {
            panic!("expected tldr subcommand");
        };
        let tldr_cmd::TldrSubcommand::Extract(args) = tldr_cli.subcommand else {
            panic!("expected extract subcommand");
        };

        assert_eq!(args.path, std::path::PathBuf::from("src/lib.rs"));
        assert!(matches!(args.lang, Some(tldr_cmd::CliLanguage::Rust)));
    }

    #[test]
    fn tldr_daemon_status_parses() {
        let cli = MultitoolCli::try_parse_from(["codex", "tldr", "daemon", "status"])
            .expect("parse should succeed");
        assert!(matches!(cli.subcommand, Some(Subcommand::Tldr(_))));
    }

    #[test]
    fn localize_help_output_replaces_wrapped_help_line() {
        let localized = localize_help_output(
            "help             Print this message or the help of the given
                   subcommand(s)
"
            .to_string(),
        );

        assert!(localized.contains("显示此消息或指定子命令的帮助"));
        assert!(!localized.contains("Print this message or the help of the given"));
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
        assert_eq!(err.to_string(), "未知的功能开关：does_not_exist");
    }
}
