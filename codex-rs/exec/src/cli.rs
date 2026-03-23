use clap::Args;
use clap::FromArgMatches;
use clap::Parser;
use clap::ValueEnum;
use codex_utils_cli::CliConfigOverrides;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(version)]
pub struct Cli {
    /// 要执行的操作。若省略，则运行新的非交互会话。
    #[command(subcommand)]
    pub command: Option<Command>,

    /// 可选：附加到初始提示词的图片。
    #[arg(
        long = "image",
        short = 'i',
        value_name = "FILE",
        value_delimiter = ',',
        num_args = 1..
    )]
    pub images: Vec<PathBuf>,

    /// 智能体应使用的模型。
    #[arg(long, short = 'm', global = true)]
    pub model: Option<String>,

    /// 使用本地开源模型提供方。
    #[arg(long = "oss", default_value_t = false)]
    pub oss: bool,

    /// 指定使用哪个本地提供方（lmstudio 或 ollama）。
    /// 若未与 --oss 一同指定，则使用配置默认值或显示选择界面。
    #[arg(long = "local-provider")]
    pub oss_provider: Option<String>,

    /// 选择执行模型生成的命令时使用的沙箱策略。
    #[arg(long = "sandbox", short = 's', value_enum)]
    pub sandbox_mode: Option<codex_utils_cli::SandboxModeCliArg>,

    /// 从 config.toml 中选择配置档以指定默认选项。
    #[arg(long = "profile", short = 'p')]
    pub config_profile: Option<String>,

    /// 低阻力沙箱自动执行的便捷别名（-a on-request, --sandbox workspace-write）。
    #[arg(long = "full-auto", default_value_t = false, global = true)]
    pub full_auto: bool,

    /// 跳过所有确认提示，并在无沙箱情况下执行命令。
    /// 极度危险。仅适用于外部环境本身已提供沙箱保护的场景。
    #[arg(
        long = "dangerously-bypass-approvals-and-sandbox",
        alias = "yolo",
        default_value_t = false,
        global = true,
        conflicts_with = "full_auto"
    )]
    pub dangerously_bypass_approvals_and_sandbox: bool,

    /// 指定智能体使用该目录作为工作根目录。
    #[clap(long = "cd", short = 'C', value_name = "DIR")]
    pub cwd: Option<PathBuf>,

    /// 允许在 Git 仓库之外运行 Codex。
    #[arg(long = "skip-git-repo-check", global = true, default_value_t = false)]
    pub skip_git_repo_check: bool,

    /// 除主工作区外，额外允许写入的目录。
    #[arg(long = "add-dir", value_name = "DIR", value_hint = clap::ValueHint::DirPath)]
    pub add_dir: Vec<PathBuf>,

    /// 运行时不将会话文件持久化到磁盘。
    #[arg(long = "ephemeral", global = true, default_value_t = false)]
    pub ephemeral: bool,

    /// 描述模型最终响应结构的 JSON Schema 文件路径。
    #[arg(long = "output-schema", value_name = "FILE")]
    pub output_schema: Option<PathBuf>,

    #[clap(skip)]
    pub config_overrides: CliConfigOverrides,

    /// 指定输出所使用的颜色设置。
    #[arg(long = "color", value_enum, default_value_t = Color::Auto)]
    pub color: Color,

    /// 在 exec 模式下强制使用基于光标的进度更新。
    #[arg(long = "progress-cursor", default_value_t = false)]
    pub progress_cursor: bool,

    /// 以 JSONL 格式将事件写入标准输出。
    #[arg(
        long = "json",
        alias = "experimental-json",
        default_value_t = false,
        global = true
    )]
    pub json: bool,

    /// 指定用于写入智能体最后一条消息的文件。
    #[arg(
        long = "output-last-message",
        short = 'o',
        value_name = "FILE",
        global = true
    )]
    pub last_message_file: Option<PathBuf>,

    /// 智能体的初始指令。若未作为参数提供（或使用 `-`），则从标准输入读取。
    #[arg(value_name = "PROMPT", value_hint = clap::ValueHint::Other)]
    pub prompt: Option<String>,
}

#[derive(Debug, clap::Subcommand)]
pub enum Command {
    /// 通过 ID 恢复先前的会话，或使用 --last 选择最近一次会话。
    Resume(ResumeArgs),

    /// 对当前仓库执行代码评审。
    Review(ReviewArgs),
}

#[derive(Args, Debug)]
struct ResumeArgsRaw {
    // Note: This is the direct clap shape. We reinterpret the positional when --last is set
    // so "codex resume --last <prompt>" treats the positional as a prompt, not a session id.
    /// 会话 ID（UUID）或线程名。若能解析为 UUID，则优先按 UUID 处理。
    /// 省略时可用 --last 选择最近一次记录的会话。
    #[arg(value_name = "SESSION_ID")]
    session_id: Option<String>,

    /// 无需指定 ID，直接恢复最近一次记录的会话（最新）。
    #[arg(long = "last", default_value_t = false)]
    last: bool,

    /// 显示所有会话（关闭 cwd 过滤）。
    #[arg(long = "all", default_value_t = false)]
    all: bool,

    /// 可选：附加到恢复后发送提示词的图片。
    #[arg(
        long = "image",
        short = 'i',
        value_name = "FILE",
        value_delimiter = ',',
        num_args = 1
    )]
    images: Vec<PathBuf>,

    /// 恢复会话后要发送的提示词。若使用 `-`，则从标准输入读取。
    #[arg(value_name = "PROMPT", value_hint = clap::ValueHint::Other)]
    prompt: Option<String>,
}

#[derive(Debug)]
pub struct ResumeArgs {
    /// 会话 ID（UUID）或线程名。若能解析为 UUID，则优先按 UUID 处理。
    /// 省略时可用 --last 选择最近一次记录的会话。
    pub session_id: Option<String>,

    /// 无需指定 ID，直接恢复最近一次记录的会话（最新）。
    pub last: bool,

    /// 显示所有会话（关闭 cwd 过滤）。
    pub all: bool,

    /// 可选：附加到恢复后发送提示词的图片。
    pub images: Vec<PathBuf>,

    /// 恢复会话后要发送的提示词。若使用 `-`，则从标准输入读取。
    pub prompt: Option<String>,
}

impl From<ResumeArgsRaw> for ResumeArgs {
    fn from(raw: ResumeArgsRaw) -> Self {
        // When --last is used without an explicit prompt, treat the positional as the prompt
        // (clap can’t express this conditional positional meaning cleanly).
        let (session_id, prompt) = if raw.last && raw.prompt.is_none() {
            (None, raw.session_id)
        } else {
            (raw.session_id, raw.prompt)
        };
        Self {
            session_id,
            last: raw.last,
            all: raw.all,
            images: raw.images,
            prompt,
        }
    }
}

impl Args for ResumeArgs {
    fn augment_args(cmd: clap::Command) -> clap::Command {
        ResumeArgsRaw::augment_args(cmd)
    }

    fn augment_args_for_update(cmd: clap::Command) -> clap::Command {
        ResumeArgsRaw::augment_args_for_update(cmd)
    }
}

impl FromArgMatches for ResumeArgs {
    fn from_arg_matches(matches: &clap::ArgMatches) -> Result<Self, clap::Error> {
        ResumeArgsRaw::from_arg_matches(matches).map(Self::from)
    }

    fn update_from_arg_matches(&mut self, matches: &clap::ArgMatches) -> Result<(), clap::Error> {
        *self = ResumeArgsRaw::from_arg_matches(matches).map(Self::from)?;
        Ok(())
    }
}

#[derive(Parser, Debug)]
pub struct ReviewArgs {
    /// 评审已暂存、未暂存和未跟踪的变更。
    #[arg(
        long = "uncommitted",
        default_value_t = false,
        conflicts_with_all = ["base", "commit", "prompt"]
    )]
    pub uncommitted: bool,

    /// 基于指定的基线分支评审变更。
    #[arg(
        long = "base",
        value_name = "BRANCH",
        conflicts_with_all = ["uncommitted", "commit", "prompt"]
    )]
    pub base: Option<String>,

    /// 评审某个提交引入的变更。
    #[arg(
        long = "commit",
        value_name = "SHA",
        conflicts_with_all = ["uncommitted", "base", "prompt"]
    )]
    pub commit: Option<String>,

    /// 可选：在评审摘要中显示的提交标题。
    #[arg(long = "title", value_name = "TITLE", requires = "commit")]
    pub commit_title: Option<String>,

    /// 自定义评审说明。若使用 `-`，则从标准输入读取。
    #[arg(value_name = "PROMPT", value_hint = clap::ValueHint::Other)]
    pub prompt: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum Color {
    Always,
    Never,
    #[default]
    Auto,
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn resume_parses_prompt_after_global_flags() {
        const PROMPT: &str = "echo resume-with-global-flags-after-subcommand";
        let cli = Cli::parse_from([
            "codex-exec",
            "resume",
            "--last",
            "--json",
            "--model",
            "gpt-5.2-codex",
            "--dangerously-bypass-approvals-and-sandbox",
            "--skip-git-repo-check",
            "--ephemeral",
            PROMPT,
        ]);

        assert!(cli.ephemeral);
        let Some(Command::Resume(args)) = cli.command else {
            panic!("expected resume command");
        };
        let effective_prompt = args.prompt.clone().or_else(|| {
            if args.last {
                args.session_id.clone()
            } else {
                None
            }
        });
        assert_eq!(effective_prompt.as_deref(), Some(PROMPT));
    }

    #[test]
    fn resume_accepts_output_last_message_flag_after_subcommand() {
        const PROMPT: &str = "echo resume-with-output-file";
        let cli = Cli::parse_from([
            "codex-exec",
            "resume",
            "session-123",
            "-o",
            "/tmp/resume-output.md",
            PROMPT,
        ]);

        assert_eq!(
            cli.last_message_file,
            Some(PathBuf::from("/tmp/resume-output.md"))
        );
        let Some(Command::Resume(args)) = cli.command else {
            panic!("expected resume command");
        };
        assert_eq!(args.session_id.as_deref(), Some("session-123"));
        assert_eq!(args.prompt.as_deref(), Some(PROMPT));
    }
}
