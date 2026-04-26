use clap::Args;
use clap::FromArgMatches;
use clap::Parser;
use codex_utils_cli::ApprovalModeCliArg;
use codex_utils_cli::CliConfigOverrides;
use codex_utils_cli::SharedCliOptions;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(version)]
pub struct Cli {
    /// 可选的会话启动提示。
    #[arg(
        value_name = "提示",
        value_hint = clap::ValueHint::Other,
        help = "可选的会话启动提示。"
    )]
    pub prompt: Option<String>,

    // Internal controls set by the top-level `codex resume` subcommand.
    // These are not exposed as user flags on the base `codex` command.
    #[clap(skip)]
    pub resume_picker: bool,

    #[clap(skip)]
    pub resume_last: bool,

    /// Internal: resume a specific recorded session by id (UUID). Set by the
    /// top-level `codex resume <SESSION_ID>` wrapper; not exposed as a public flag.
    #[clap(skip)]
    pub resume_session_id: Option<String>,

    /// Internal: show all sessions (disables cwd filtering and shows CWD column).
    #[clap(skip)]
    pub resume_show_all: bool,

    /// Internal: include non-interactive sessions in resume listings.
    #[clap(skip)]
    pub resume_include_non_interactive: bool,

    /// Internal: clean the selected zoffsec rollout before resuming it.
    #[clap(skip)]
    pub resume_zoffsec_clean: bool,

    // Internal controls set by the top-level `codex fork` subcommand.
    // These are not exposed as user flags on the base `codex` command.
    #[clap(skip)]
    pub fork_picker: bool,

    #[clap(skip)]
    pub fork_last: bool,

    /// Internal: fork a specific recorded session by id (UUID). Set by the
    /// top-level `codex fork <SESSION_ID>` wrapper; not exposed as a public flag.
    #[clap(skip)]
    pub fork_session_id: Option<String>,

    /// Internal: show all sessions (disables cwd filtering and shows CWD column).
    #[clap(skip)]
    pub fork_show_all: bool,

    #[clap(flatten)]
    pub shared: TuiSharedCliOptions,

    /// 快捷设置 model_provider，等价于 `-c model_provider=<PROVIDER>`，但
    /// 优先级低于显式的 `-c model_provider=...`。
    #[arg(
        short = 'P',
        long = "provider",
        value_name = "PROVIDER",
        help = "快捷设置 model_provider，等价于 `-c model_provider=<PROVIDER>`，优先级低于显式的 `-c model_provider=...`。"
    )]
    pub provider: Option<String>,

    /// 指定要使用的本地提供方（lmstudio 或 ollama）。
    /// 如果与 --oss 一起使用时未指定，将使用配置默认值或显示选择。
    #[arg(
        long = "local-provider",
        help = "指定要使用的本地提供方（lmstudio 或 ollama）。如果与 --oss 一起使用时未指定，将使用配置默认值或显示选择。"
    )]
    pub oss_provider: Option<String>,

    /// 配置模型在执行命令前何时需要人工批准。
    #[arg(
        long = "ask-for-approval",
        short = 'a',
        help = "配置模型在执行命令前何时需要人工批准。"
    )]
    pub approval_policy: Option<ApprovalModeCliArg>,

    /// 启用 federation 启动桥接。
    #[arg(
        long = "zfeder-enable",
        default_value_t = false,
        help = "启用 federation 启动桥接。"
    )]
    pub zfeder_enable: bool,

    /// 启用 federation 时向其他实例暴露的名称。
    #[arg(
        long = "zfeder-name",
        value_name = "NAME",
        help = "启用 federation 时向其他实例暴露的名称。"
    )]
    pub zfeder_name: Option<String>,

    /// 启用 federation 时向其他实例暴露的角色标签。
    #[arg(
        long = "zfeder-role",
        value_name = "ROLE",
        help = "启用 federation 时向其他实例暴露的角色标签。"
    )]
    pub zfeder_role: Option<String>,

    /// 启用 federation 时向其他实例暴露的任务 scope。
    #[arg(
        long = "zfeder-scope",
        value_name = "SCOPE",
        help = "启用 federation 时向其他实例暴露的任务 scope。"
    )]
    pub zfeder_scope: Option<String>,

    /// 覆盖 federation state root，默认使用 `<CODEX_HOME>/federation`。
    #[arg(
        long = "zfeder-state-root",
        value_name = "PATH",
        help = "覆盖 federation state root，默认使用 `<CODEX_HOME>/federation`。"
    )]
    pub zfeder_state_root: Option<PathBuf>,

    /// 覆盖 federation 实例 id。
    #[arg(
        long = "zfeder-instance-id",
        value_name = "UUID",
        help = "覆盖 federation 实例 id。"
    )]
    pub zfeder_instance_id: Option<String>,

    /// Enable live web search. When enabled, the native Responses `web_search` tool is available to the model (no per‑call approval).
    #[arg(
        long = "search",
        default_value_t = false,
        help = "启用实时网络搜索。启用后，原生 Responses `web_search` 工具可供模型使用（无需每次调用批准）。"
    )]
    pub web_search: bool,

    /// 禁用备用屏幕模式
    ///
    /// 以内联模式运行 TUI，保留终端滚动历史记录。这在严格遵循 xterm 规范并禁用
    /// 备用屏幕缓冲区中滚动的终端复用器（如 Zellij）中很有用。
    #[arg(
        long = "no-alt-screen",
        default_value_t = false,
        help = "禁用备用屏幕模式",
        long_help = "禁用备用屏幕模式\n\n以内联模式运行 TUI，保留终端滚动历史记录。这在严格遵循 xterm 规范并禁用备用屏幕缓冲区中滚动的终端复用器（如 Zellij）中很有用。"
    )]
    pub no_alt_screen: bool,

    #[clap(flatten)]
    pub config_overrides: CliConfigOverrides,
}

impl std::ops::Deref for Cli {
    type Target = SharedCliOptions;

    fn deref(&self) -> &Self::Target {
        &self.shared.0
    }
}

impl std::ops::DerefMut for Cli {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.shared.0
    }
}

#[derive(Debug, Default)]
pub struct TuiSharedCliOptions(SharedCliOptions);

impl TuiSharedCliOptions {
    pub fn into_inner(self) -> SharedCliOptions {
        self.0
    }
}

impl std::ops::Deref for TuiSharedCliOptions {
    type Target = SharedCliOptions;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for TuiSharedCliOptions {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Args for TuiSharedCliOptions {
    fn augment_args(cmd: clap::Command) -> clap::Command {
        mark_tui_args(SharedCliOptions::augment_args(cmd))
    }

    fn augment_args_for_update(cmd: clap::Command) -> clap::Command {
        mark_tui_args(SharedCliOptions::augment_args_for_update(cmd))
    }
}

impl FromArgMatches for TuiSharedCliOptions {
    fn from_arg_matches(matches: &clap::ArgMatches) -> Result<Self, clap::Error> {
        SharedCliOptions::from_arg_matches(matches).map(Self)
    }

    fn update_from_arg_matches(&mut self, matches: &clap::ArgMatches) -> Result<(), clap::Error> {
        self.0.update_from_arg_matches(matches)
    }
}

fn mark_tui_args(cmd: clap::Command) -> clap::Command {
    cmd.mut_arg("dangerously_bypass_approvals_and_sandbox", |arg| {
        arg.conflicts_with("approval_policy")
    })
}
