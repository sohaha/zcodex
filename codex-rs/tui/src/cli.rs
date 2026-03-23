use clap::Parser;
use clap::ValueHint;
use codex_utils_cli::ApprovalModeCliArg;
use codex_utils_cli::CliConfigOverrides;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(version)]
pub struct Cli {
    /// 可选的初始用户提示词，用于开始会话。
    #[arg(value_name = "PROMPT", value_hint = clap::ValueHint::Other)]
    pub prompt: Option<String>,

    /// 可选：附加到初始提示词的图片。
    #[arg(long = "image", short = 'i', value_name = "FILE", value_delimiter = ',', num_args = 1..)]
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
    #[arg(long, short = 'm')]
    pub model: Option<String>,

    /// 用于选择本地开源模型提供方的便捷开关。等价于 -c
    /// model_provider=oss；并会检查本地 LM Studio 或 Ollama 服务是否正在运行。
    #[arg(long = "oss", default_value_t = false)]
    pub oss: bool,

    /// 指定使用哪个本地提供方（lmstudio 或 ollama）。
    /// 若未与 --oss 一同指定，则使用配置默认值或显示选择界面。
    #[arg(long = "local-provider")]
    pub oss_provider: Option<String>,

    /// 从 config.toml 中选择配置档以指定默认选项。
    #[arg(long = "profile", short = 'p')]
    pub config_profile: Option<String>,

    /// 选择执行模型生成的 shell 命令时使用的沙箱策略。
    #[arg(long = "sandbox", short = 's')]
    pub sandbox_mode: Option<codex_utils_cli::SandboxModeCliArg>,

    /// 配置模型在执行命令前何时需要人工批准。
    #[arg(long = "ask-for-approval", short = 'a')]
    pub approval_policy: Option<ApprovalModeCliArg>,

    /// 低阻力沙箱自动执行的便捷别名（-a on-request, --sandbox workspace-write）。
    #[arg(long = "full-auto", default_value_t = false)]
    pub full_auto: bool,

    /// 跳过所有确认提示，并在无沙箱情况下执行命令。
    /// 极度危险。仅适用于外部环境本身已提供沙箱保护的场景。
    #[arg(
        long = "dangerously-bypass-approvals-and-sandbox",
        alias = "yolo",
        default_value_t = false,
        conflicts_with_all = ["approval_policy", "full_auto"]
    )]
    pub dangerously_bypass_approvals_and_sandbox: bool,

    /// 指定智能体使用该目录作为工作根目录。
    #[clap(long = "cd", short = 'C', value_name = "DIR")]
    pub cwd: Option<PathBuf>,

    /// 启用实时网页搜索。启用后，模型可直接使用原生 Responses `web_search` 工具（无需逐次批准）。
    #[arg(long = "search", default_value_t = false)]
    pub web_search: bool,

    /// 除主工作区外，额外允许写入的目录。
    #[arg(long = "add-dir", value_name = "DIR", value_hint = ValueHint::DirPath)]
    pub add_dir: Vec<PathBuf>,

    /// 禁用备用屏模式。
    ///
    /// 以行内模式运行 TUI，并保留终端滚动历史。这对像 Zellij 这类严格遵循 xterm 规范、
    /// 会在备用屏缓冲区中禁用滚动回溯的终端复用器很有用。
    #[arg(long = "no-alt-screen", default_value_t = false)]
    pub no_alt_screen: bool,

    #[clap(skip)]
    pub config_overrides: CliConfigOverrides,
}
