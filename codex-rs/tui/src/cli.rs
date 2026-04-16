use clap::Parser;
use clap::ValueHint;
use codex_utils_cli::ApprovalModeCliArg;
use codex_utils_cli::CliConfigOverrides;
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
    /// 可选的初始提示附件图片。
    #[arg(
        long = "image",
        short = 'i',
        value_name = "文件",
        value_delimiter = ',',
        num_args = 1..,
        help = "可选的初始提示附件图片。"
    )]
    pub images: Vec<PathBuf>,
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

    /// Internal: clean the selected CTF rollout before resuming it.
    #[clap(skip)]
    pub resume_ctf_clean: bool,

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

    /// 智能体应使用的模型。
    #[arg(long, short = 'm', help = "智能体应使用的模型。")]
    pub model: Option<String>,

    /// 便捷标志，用于选择本地开源模型提供方。等价于 -c
    /// model_provider=oss；验证本地 LM Studio 或 Ollama 服务器是否正在运行。
    #[arg(
        long = "oss",
        default_value_t = false,
        help = "便捷标志，用于选择本地开源模型提供方。等价于 -c model_provider=oss；验证本地 LM Studio 或 Ollama 服务器是否正在运行。"
    )]
    pub oss: bool,

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

    /// 来自 config.toml 的配置配置文件，用于指定默认选项。
    #[arg(
        long = "profile",
        short = 'p',
        help = "来自 config.toml 的配置配置文件，用于指定默认选项。"
    )]
    pub config_profile: Option<String>,

    /// 选择执行模型生成的 shell
    /// 命令时要使用的沙箱策略。
    #[arg(
        long = "sandbox",
        short = 's',
        help = "选择执行模型生成的 shell 命令时要使用的沙箱策略。"
    )]
    pub sandbox_mode: Option<codex_utils_cli::SandboxModeCliArg>,

    /// 配置模型在执行命令前何时需要人工批准。
    #[arg(
        long = "ask-for-approval",
        short = 'a',
        help = "配置模型在执行命令前何时需要人工批准。"
    )]
    pub approval_policy: Option<ApprovalModeCliArg>,

    /// 低摩擦沙箱自动执行的便捷别名（-a on-request, --sandbox workspace-write）。
    #[arg(
        long = "full-auto",
        default_value_t = false,
        help = "低摩擦沙箱自动执行的便捷别名（-a on-request, --sandbox workspace-write）。"
    )]
    pub full_auto: bool,

    /// 跳过所有确认提示并在无沙箱的情况下执行命令。
    /// 极度危险。仅适用于在外部沙箱环境中运行。
    #[arg(
        long = "dangerously-bypass-approvals-and-sandbox",
        alias = "yolo",
        default_value_t = false,
        conflicts_with_all = ["approval_policy", "full_auto"],
        help = "跳过所有确认提示并在无沙箱的情况下执行命令。极度危险。仅适用于在外部沙箱环境中运行。"
    )]
    pub dangerously_bypass_approvals_and_sandbox: bool,

    /// 告诉智能体使用指定目录作为其工作根目录。
    #[clap(
        long = "cd",
        short = 'C',
        value_name = "目录",
        help = "告诉智能体使用指定目录作为其工作根目录。"
    )]
    pub cwd: Option<PathBuf>,

    /// 启用实时网络搜索。启用后，原生 Responses `web_search` 工具可供模型使用（无需每次调用批准）。
    #[arg(
        long = "search",
        default_value_t = false,
        help = "启用实时网络搜索。启用后，原生 Responses `web_search` 工具可供模型使用（无需每次调用批准）。"
    )]
    pub web_search: bool,

    /// 除主工作区外还应可写入的附加目录。
    #[arg(
        long = "add-dir",
        value_name = "目录",
        value_hint = ValueHint::DirPath,
        help = "除主工作区外还应可写入的附加目录。"
    )]
    pub add_dir: Vec<PathBuf>,

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

    #[clap(skip)]
    pub config_overrides: CliConfigOverrides,
}
