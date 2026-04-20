#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used))]

mod aws_cmd;
mod cargo_cmd;
mod compression;
mod compression_json;
mod compression_log;
mod container;
mod curl_cmd;
mod deps;
mod diff_cmd;
mod env_cmd;
mod filter;
mod find_cmd;
mod format_cmd;
mod gh_cmd;
mod git;
mod go_cmd;
mod golangci_cmd;
mod grep_cmd;
mod gt_cmd;
mod json_cmd;
mod lint_cmd;
mod local_llm;
mod log_cmd;
mod ls;
mod mypy_cmd;
mod next_cmd;
mod npm_cmd;
pub mod parser;
mod pip_cmd;
mod playwright_cmd;
mod pnpm_cmd;
mod prettier_cmd;
mod prisma_cmd;
mod psql_cmd;
mod pytest_cmd;
mod read;
mod rewrite;
mod ruff_cmd;
mod runner;
mod session_dedup;
mod summary;
mod tee;
mod tracking;
mod tree;
mod tsc_cmd;
mod utils;
mod vitest_cmd;
mod wc_cmd;
mod wget_cmd;

use anyhow::Context;
use anyhow::Result;
use clap::CommandFactory;
use clap::Parser;
use clap::Subcommand;
use clap::error::ErrorKind;
use std::ffi::OsString;
use std::path::Path;
use std::path::PathBuf;

const ZTOK_ALIAS_NAME: &str = "ztok";

pub fn alias_name() -> &'static str {
    ZTOK_ALIAS_NAME
}

pub use rewrite::ShellCommandPassthroughReason;
pub use rewrite::ShellCommandRewriteAnalysis;
pub use rewrite::ShellCommandRewriteKind;
pub use rewrite::analyze_shell_command;
pub use rewrite::rewrite_shell_command;
pub use session_dedup::ZTOK_SESSION_ID_ENV_VAR;

pub fn is_alias_invocation(argv0: &OsString) -> bool {
    Path::new(argv0)
        .file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|file_name| file_name == ZTOK_ALIAS_NAME)
}

#[derive(Parser)]
#[command(
    name = "ztok",
    version,
    propagate_version = true,
    about = "Token Killer - 最小化 LLM token 消耗",
    long_about = "高性能 CLI 代理，在输出进入 LLM 上下文前进行过滤与摘要。",
    disable_help_flag = true,
    disable_version_flag = true,
    disable_help_subcommand = true,
    subcommand_help_heading = "命令",
    help_template = "\
{before-help}{about-with-newline}
用法: {usage}

命令:
{subcommands}

选项:
{options}{after-help}"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// 详细级别 (-v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,

    /// 超紧凑模式：ASCII 图标、行内格式（Level 2 优化）
    #[arg(long, global = true)]
    ultra_compact: bool,

    /// 为子进程设置 SKIP_ENV_VALIDATION=1（Next.js、tsc、lint、prisma）
    #[arg(long = "skip-env", global = true)]
    skip_env: bool,

    /// 显示帮助信息
    #[arg(short = 'h', long = "help", action = clap::ArgAction::Help, global = true)]
    help: Option<bool>,

    /// 显示版本
    #[arg(short = 'V', long = "version", action = clap::ArgAction::Version, global = true)]
    version: Option<bool>,
}

#[derive(Subcommand)]
enum Commands {
    /// 列出目录内容，输出更省 token
    Ls {
        /// 传给 ls 的参数（支持原生 ls 的 -l、-a、-h、-R 等）
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// 目录树，输出更省 token
    Tree {
        /// 传给 tree 的参数（支持原生 tree 的 -L、-d、-a 等）
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// 读取文件并智能过滤
    Read {
        /// 要读取的文件（可传多个，行为类似 cat）
        #[arg(required = true, num_args = 1..)]
        files: Vec<PathBuf>,
        /// 过滤级别：none、minimal、aggressive
        #[arg(short, long, default_value = "none")]
        level: filter::FilterLevel,
        /// 最大行数
        #[arg(short, long, conflicts_with = "tail_lines")]
        max_lines: Option<usize>,
        /// 仅保留最后 N 行
        #[arg(long, conflicts_with = "max_lines")]
        tail_lines: Option<usize>,
        /// 显示行号
        #[arg(short = 'n', long)]
        line_numbers: bool,
    },

    /// 生成 2 行技术摘要（基于启发式）
    Smart {
        /// 要分析的文件
        file: PathBuf,
        /// 模型：heuristic
        #[arg(short, long, default_value = "heuristic")]
        model: String,
        /// 强制下载模型
        #[arg(long)]
        force_download: bool,
    },

    /// Git 命令，紧凑输出
    Git {
        /// 执行前切换目录（等价 git -C <path>，可重复）
        #[arg(short = 'C', action = clap::ArgAction::Append)]
        directory: Vec<String>,

        /// Git 配置覆盖（等价 git -c key=value，可重复）
        #[arg(short = 'c', action = clap::ArgAction::Append)]
        config_override: Vec<String>,

        /// 设置 .git 目录路径
        #[arg(long = "git-dir")]
        git_dir: Option<String>,

        /// 设置工作区路径
        #[arg(long = "work-tree")]
        work_tree: Option<String>,

        /// 禁用分页器（等价 git --no-pager）
        #[arg(long = "no-pager")]
        no_pager: bool,

        /// 跳过可选锁（等价 git --no-optional-locks）
        #[arg(long = "no-optional-locks")]
        no_optional_locks: bool,

        /// 按裸仓库处理（等价 git --bare）
        #[arg(long)]
        bare: bool,

        /// 按字面匹配 pathspec（等价 git --literal-pathspecs）
        #[arg(long = "literal-pathspecs")]
        literal_pathspecs: bool,

        #[command(subcommand)]
        command: GitCommands,
    },

    /// GitHub CLI (gh) 命令
    Gh {
        /// 子命令：pr、issue、run、repo
        subcommand: String,
        /// 附加参数
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// AWS CLI 紧凑输出（强制 JSON，压缩）
    Aws {
        /// AWS 服务子命令（如 sts、s3、ec2、ecs、rds、cloudformation）
        subcommand: String,
        /// 附加参数
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// PostgreSQL 客户端紧凑输出（去边框、压缩表格）
    Psql {
        /// psql 参数
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// pnpm 命令超紧凑输出
    Pnpm {
        /// pnpm 全局过滤参数（可重复：--filter @app1 --filter @app2）
        #[arg(long, short = 'F')]
        filter: Vec<String>,

        #[command(subcommand)]
        command: PnpmCommands,
    },

    /// 运行命令，仅显示错误/警告
    Err {
        /// 要运行的命令
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },

    /// 运行测试，仅显示失败项
    Test {
        /// 测试命令（如 cargo test）
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },

    /// 通用 shell 命令入口
    Shell {
        /// 输出过滤模式：raw、err、test
        #[arg(long, value_enum, default_value_t = runner::ShellFilter::Raw)]
        filter: runner::ShellFilter,
        /// 要运行的命令
        #[arg(required = true, num_args = 1.., trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },

    /// 仅显示 JSON 结构，不显示值
    Json {
        /// JSON 文件
        file: PathBuf,
        /// 最大深度
        #[arg(short, long, default_value = "5")]
        depth: usize,
        /// 仅显示键和类型，不显示值
        #[arg(long)]
        keys_only: bool,
    },

    /// 汇总项目依赖
    Deps {
        /// 项目路径
        #[arg(default_value = ".")]
        path: PathBuf,
    },

    /// 显示环境变量（过滤，敏感信息打码）
    Env {
        /// 按名称过滤（如 PATH、AWS）
        #[arg(short, long)]
        filter: Option<String>,
        /// 显示全部（含敏感）
        #[arg(long)]
        show_all: bool,
    },

    /// 查找文件，紧凑树形输出（支持原生 find 参数如 -name、-type）
    Find {
        /// 全部 find 参数（支持 ZTOK 与原生 find 语法）
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// 超精简 diff（仅保留变更行）
    Diff {
        /// 第一个文件，或用 - 代表 stdin（统一 diff）
        file1: PathBuf,
        /// 第二个文件（stdin 模式可省略）
        file2: Option<PathBuf>,
    },

    /// 过滤并去重日志输出
    Log {
        /// 日志文件（省略则读 stdin）
        file: Option<PathBuf>,
    },

    /// Docker 命令紧凑输出
    Docker {
        #[command(subcommand)]
        command: DockerCommands,
    },

    /// kubectl 命令紧凑输出
    Kubectl {
        #[command(subcommand)]
        command: KubectlCommands,
    },

    /// 运行命令并给出启发式摘要
    Summary {
        /// 要运行并摘要的命令
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },

    /// 紧凑 grep：去空白、截断、按文件分组
    Grep {
        /// 要搜索的模式
        pattern: String,
        /// 搜索路径
        #[arg(default_value = ".")]
        path: String,
        /// 最大行长度
        #[arg(short = 'l', long, default_value = "80")]
        max_len: usize,
        /// 最大结果数
        #[arg(short, long, default_value = "200")]
        max: usize,
        /// 仅显示匹配片段（不显示整行）
        #[arg(short, long)]
        context_only: bool,
        /// 按文件类型过滤（如 ts、py、rust）
        #[arg(short = 't', long)]
        file_type: Option<String>,
        /// 显示行号（始终开启，仅为 grep/rg 兼容）
        #[arg(short = 'n', long)]
        line_numbers: bool,
        /// 额外 ripgrep 参数（如 -i、-A 3、-w、--glob）
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        extra_args: Vec<String>,
    },

    /// 下载并紧凑输出（去进度条）
    Wget {
        /// 下载 URL
        url: String,
        /// 输出到 stdout 而非文件
        #[arg(short = 'O', long)]
        stdout: bool,
        /// 额外 wget 参数
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// 紧凑的字/行/字节统计（去路径和对齐）
    Wc {
        /// 传给 wc 的参数（文件、-l、-w、-c 等）
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Vitest 命令紧凑输出
    Vitest {
        /// Vitest 参数
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Prisma 命令紧凑输出（无 ASCII art）
    Prisma {
        #[command(subcommand)]
        command: PrismaCommands,
    },

    /// TypeScript 编译器，错误分组输出
    Tsc {
        /// TypeScript 编译器参数
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Next.js build 紧凑输出
    Next {
        /// Next.js build 参数
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// ESLint 规则违规分组输出
    Lint {
        /// Linter 参数
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Prettier 格式检查紧凑输出
    Prettier {
        /// Prettier 参数（如 --check、--write）
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// 通用格式检查（prettier、black、ruff format）
    Format {
        /// 格式化器参数（自动从项目文件检测）
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Playwright E2E 测试紧凑输出
    Playwright {
        /// Playwright 参数
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Cargo 命令紧凑输出
    Cargo {
        #[command(subcommand)]
        command: CargoCommands,
    },

    /// npm run 过滤输出（去模板信息）
    Npm {
        /// npm run 参数（脚本名 + 选项）
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// npx 智能路由（tsc、eslint、prisma → 专用过滤）
    Npx {
        /// npx 参数（命令 + 选项）
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// curl 自动识别 JSON 并输出 schema
    Curl {
        /// curl 参数（URL + 选项）
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Ruff linter/formatter 紧凑输出
    Ruff {
        /// Ruff 参数（如 check、format --check）
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Pytest 测试运行器紧凑输出
    Pytest {
        /// Pytest 参数
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Mypy 类型检查器，错误分组输出
    Mypy {
        /// Mypy 参数
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// pip 包管理器紧凑输出（自动识别 uv）
    Pip {
        /// pip 参数（如 list、outdated、install）
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Go 命令紧凑输出
    Go {
        #[command(subcommand)]
        command: GoCommands,
    },

    /// Graphite (gt) 叠栈 PR 命令紧凑输出
    Gt {
        #[command(subcommand)]
        command: GtCommands,
    },

    /// golangci-lint 紧凑输出
    #[command(name = "golangci-lint")]
    GolangciLint {
        /// golangci-lint 参数
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

#[derive(Subcommand)]
enum GitCommands {
    /// 精简 diff 输出
    Diff {
        /// Git 参数（支持 git diff 的 --stat、--cached 等）
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// 单行提交历史
    Log {
        /// Git 参数（支持 git log 的 --oneline、--graph、--all 等）
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// 紧凑状态（支持 git status 的全部参数）
    Status {
        /// Git 参数（支持 git status 的 --porcelain、--short、-s 等）
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// 紧凑 show（提交摘要 + 统计 + 压缩 diff）
    Show {
        /// Git 参数（支持 git show 的全部参数）
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// 添加文件 → "已完成 / 已暂存 ..."
    Add {
        /// 要添加的文件与参数（支持 git add 的 -A、-p、--all 等）
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// 提交 → "已提交 <hash>"
    Commit {
        /// Git commit 参数（支持 -a、-m、--amend、--allow-empty 等）
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// 推送 → "已推送 <branch>"
    Push {
        /// Git push 参数（支持 -u、remote、branch 等）
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// 拉取 → "已拉取 <stats>"
    Pull {
        /// Git pull 参数（支持 --rebase、remote、branch 等）
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// 紧凑分支列表（当前/本地/远端）
    Branch {
        /// Git branch 参数（支持 -d、-D、-m 等）
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// fetch → "已拉取（N 个新引用）"
    Fetch {
        /// Git fetch 参数
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// stash 管理（list、show、pop、apply、drop）
    Stash {
        /// 子命令：list、show、pop、apply、drop、push
        subcommand: Option<String>,
        /// 附加参数
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// 紧凑 worktree 列表
    Worktree {
        /// Git worktree 参数（add、remove、prune，或空参列出）
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// 透传：直接运行不支持的 git 子命令
    #[command(external_subcommand)]
    Other(Vec<OsString>),
}

#[derive(Subcommand)]
enum PnpmCommands {
    /// 列出已安装包（超密集）
    List {
        /// 深度（默认：0）
        #[arg(short, long, default_value = "0")]
        depth: usize,
        /// 额外 pnpm 参数
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// 显示过期包（精简："pkg: old → new"）
    Outdated {
        /// 额外 pnpm 参数
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// 安装包（过滤进度条）
    Install {
        /// 要安装的包
        packages: Vec<String>,
        /// 额外 pnpm 参数
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// 构建（通用透传，不做框架特定过滤）
    Build {
        /// 额外构建参数
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// 类型检查（委托给 tsc 过滤）
    Typecheck {
        /// 额外类型检查参数
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// 透传：直接运行不支持的 pnpm 子命令
    #[command(external_subcommand)]
    Other(Vec<OsString>),
}

#[derive(Subcommand)]
enum DockerCommands {
    /// 列出运行中的容器
    Ps,
    /// 列出镜像
    Images,
    /// 显示容器日志（去重）
    Logs { container: String },
    /// Docker Compose 命令紧凑输出
    Compose {
        #[command(subcommand)]
        command: ComposeCommands,
    },
    /// 透传：直接运行不支持的 docker 子命令
    #[command(external_subcommand)]
    Other(Vec<OsString>),
}

#[derive(Subcommand)]
enum ComposeCommands {
    /// 列出 compose 服务（紧凑）
    Ps,
    /// 显示 compose 日志（去重）
    Logs {
        /// 可选服务名
        service: Option<String>,
    },
    /// 构建 compose 服务（摘要）
    Build {
        /// 可选服务名
        service: Option<String>,
    },
    /// 透传：直接运行不支持的 compose 子命令
    #[command(external_subcommand)]
    Other(Vec<OsString>),
}

#[derive(Subcommand)]
enum KubectlCommands {
    /// 列出 pods
    Pods {
        #[arg(short, long)]
        namespace: Option<String>,
        /// 所有命名空间
        #[arg(short = 'A', long)]
        all: bool,
    },
    /// 列出 services
    Services {
        #[arg(short, long)]
        namespace: Option<String>,
        /// 所有命名空间
        #[arg(short = 'A', long)]
        all: bool,
    },
    /// 显示 pod 日志（去重）
    Logs {
        pod: String,
        #[arg(short, long)]
        container: Option<String>,
    },
    /// 透传：直接运行不支持的 kubectl 子命令
    #[command(external_subcommand)]
    Other(Vec<OsString>),
}

#[derive(Subcommand)]
enum PrismaCommands {
    /// 生成 Prisma Client（去 ASCII art）
    Generate {
        /// 额外 prisma 参数
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// 管理迁移
    Migrate {
        #[command(subcommand)]
        command: PrismaMigrateCommands,
    },
    /// 推送 schema 到数据库
    DbPush {
        /// 额外 prisma 参数
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

#[derive(Subcommand)]
enum PrismaMigrateCommands {
    /// 创建并应用迁移
    Dev {
        /// 迁移名
        #[arg(short, long)]
        name: Option<String>,
        /// 额外参数
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// 检查迁移状态
    Status {
        /// 额外参数
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// 部署迁移到生产
    Deploy {
        /// 额外参数
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

#[derive(Subcommand)]
enum CargoCommands {
    /// build 紧凑输出（去除 Compiling 行，保留错误）
    Build {
        /// 额外 cargo build 参数
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// test 仅输出失败
    Test {
        /// 额外 cargo test 参数
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Clippy 按规则分组警告
    Clippy {
        /// 额外 cargo clippy 参数
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// check 紧凑输出（去除 Checking 行，保留错误）
    Check {
        /// 额外 cargo check 参数
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// install 紧凑输出（去除依赖编译，保留安装/错误）
    Install {
        /// 额外 cargo install 参数
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// nextest 仅输出失败
    Nextest {
        /// 额外 cargo nextest 参数（如 run、list、--lib）
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// 透传：直接运行不支持的 cargo 子命令
    #[command(external_subcommand)]
    Other(Vec<OsString>),
}

#[derive(Subcommand)]
enum GoCommands {
    /// 运行测试并紧凑输出（JSON 流式，约 90% token 缩减）
    Test {
        /// 额外 go test 参数
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// 构建紧凑输出（仅错误）
    Build {
        /// 额外 go build 参数
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// vet 紧凑输出
    Vet {
        /// 额外 go vet 参数
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// 透传：直接运行不支持的 go 子命令
    #[command(external_subcommand)]
    Other(Vec<OsString>),
}

/// ZTOK-only subcommands that should never fall back to raw execution.
/// If Clap fails to parse these, show the Clap error directly instead of
/// treating them as passthrough commands from `$PATH`.
const REMOVED_BUILTIN_COMMANDS: &[&str] = &[
    "gain",
    "discover",
    "learn",
    "init",
    "config",
    "proxy",
    "hook-audit",
    "cc-economics",
    "verify",
    "rewrite",
];

const RAW_DOUBLE_DASH_WRAPPERS: &[&str] = &["env", "command", "nice", "stdbuf", "ionice", "chrt"];

fn merge_pnpm_args(filters: &[String], args: &[String]) -> Vec<String> {
    filters
        .iter()
        .map(|filter| format!("--filter={filter}"))
        .chain(args.iter().cloned())
        .collect()
}

fn merge_pnpm_args_os(filters: &[String], args: &[OsString]) -> Vec<OsString> {
    filters
        .iter()
        .map(|filter| OsString::from(format!("--filter={filter}")))
        .chain(args.iter().cloned())
        .collect()
}

fn validate_pnpm_filters(filters: &[String], command: &PnpmCommands) -> Option<String> {
    match command {
        PnpmCommands::Typecheck { .. } if !filters.is_empty() => Some(
            "[ztok] warning: --filter 还不支持 pnpm tsc，子命令前的 filters 会被忽略".to_string(),
        ),
        _ => None,
    }
}

fn should_show_parse_error(args: &[OsString]) -> bool {
    let Some(first_arg) = first_subcommand_arg(args) else {
        return true;
    };

    if uses_raw_double_dash_wrapper(args, first_arg.as_ref()) {
        return false;
    }

    if REMOVED_BUILTIN_COMMANDS.contains(&first_arg.as_ref()) {
        return true;
    }

    Cli::command().get_subcommands().any(|subcommand| {
        subcommand.get_name() == first_arg
            || subcommand
                .get_all_aliases()
                .any(|alias| alias == first_arg.as_ref())
    })
}

fn uses_raw_double_dash_wrapper(args: &[OsString], first_arg: &str) -> bool {
    args.iter()
        .position(|arg| arg == "--")
        .is_some_and(|boundary| {
            boundary + 1 < args.len() && RAW_DOUBLE_DASH_WRAPPERS.contains(&first_arg)
        })
}

fn first_subcommand_arg(args: &[OsString]) -> Option<std::borrow::Cow<'_, str>> {
    first_subcommand_index(args).map(|index| args[index].to_string_lossy())
}

fn first_subcommand_index(args: &[OsString]) -> Option<usize> {
    let mut parsing_global_flags = true;

    for (index, arg) in args.iter().enumerate() {
        let arg = arg.to_string_lossy();
        if parsing_global_flags {
            if arg == "--" {
                parsing_global_flags = false;
                continue;
            }
            if matches!(
                arg.as_ref(),
                "-v" | "--verbose" | "--ultra-compact" | "--skip-env"
            ) || is_global_short_flag_cluster(arg.as_ref())
            {
                continue;
            }
        }

        return Some(index);
    }

    None
}

fn is_global_short_flag_cluster(arg: &str) -> bool {
    arg.strip_prefix('-')
        .filter(|flags| !flags.is_empty() && !flags.starts_with('-'))
        .is_some_and(|flags| flags.chars().all(|flag| matches!(flag, 'v')))
}

fn rewrite_grep_option_first_args(args: &[OsString]) -> Option<Vec<OsString>> {
    let command_index = first_subcommand_index(args)?;
    if args.get(command_index)?.to_string_lossy() != "grep" {
        return None;
    }

    let rewritten_grep_args = rewrite_grep_subcommand_args(&args[command_index + 1..])?;
    let mut rewritten = args[..=command_index].to_vec();
    rewritten.extend(rewritten_grep_args);
    Some(rewritten)
}

fn rewrite_grep_subcommand_args(args: &[OsString]) -> Option<Vec<OsString>> {
    let first_arg = args.first()?.to_string_lossy();
    if !is_grep_option_like(first_arg.as_ref()) {
        return None;
    }

    let mut leading_options = Vec::new();
    let mut index = 0;

    while let Some(arg) = args.get(index) {
        let arg = arg.to_string_lossy();
        if !is_grep_option_like(arg.as_ref()) {
            break;
        }
        if grep_option_blocks_rewrite(arg.as_ref()) {
            return None;
        }

        leading_options.push(args[index].clone());
        if grep_option_uses_next_arg(arg.as_ref()) {
            index += 1;
            leading_options.push(args.get(index)?.clone());
        }
        index += 1;
    }

    let pattern = args.get(index)?.clone();
    index += 1;

    let mut rewritten = vec![pattern];
    if let Some(path) = args.get(index)
        && !is_grep_option_like(path.to_string_lossy().as_ref())
    {
        rewritten.push(path.clone());
        index += 1;
    }

    rewritten.extend(leading_options);
    rewritten.extend(args[index..].iter().cloned());
    Some(rewritten)
}

fn is_grep_option_like(arg: &str) -> bool {
    arg.starts_with('-') && arg != "-" && arg != "--"
}

fn grep_option_blocks_rewrite(arg: &str) -> bool {
    matches!(arg, "-e" | "-f" | "--regexp" | "--file")
        || arg.starts_with("--regexp=")
        || arg.starts_with("--file=")
        || arg.starts_with("-e")
        || arg.starts_with("-f")
}

fn grep_option_uses_next_arg(arg: &str) -> bool {
    if arg.starts_with("--") {
        if arg.contains('=') {
            return false;
        }
        return matches!(
            arg,
            "--after-context"
                | "--before-context"
                | "--binary-files"
                | "--color"
                | "--colors"
                | "--context"
                | "--glob"
                | "--iglob"
                | "--max-count"
                | "--max-columns"
                | "--max-depth"
                | "--max-filesize"
                | "--path-separator"
                | "--pre"
                | "--pre-glob"
                | "--replace"
                | "--sort"
                | "--sortr"
                | "--threads"
                | "--type"
                | "--type-add"
                | "--type-not"
        );
    }

    matches!(
        arg,
        "-A" | "-B" | "-C" | "-D" | "-d" | "-g" | "-j" | "-m" | "-M" | "-O" | "-t" | "-T"
    )
}

fn run_fallback(args: &[OsString], parse_error: clap::Error) -> Result<()> {
    if args.is_empty() || should_show_parse_error(args) {
        parse_error.exit();
    }

    let Some(command_index) = first_subcommand_index(args) else {
        parse_error.exit();
    };
    let fallback_args = &args[command_index..];

    let rendered_args = fallback_args
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>();

    let raw_command = rendered_args.join(" ");
    let timer = tracking::TimedExecution::start();

    let status = utils::resolved_command(&rendered_args[0])
        .args(&fallback_args[1..])
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status();

    match status {
        Ok(status) => {
            timer.track_passthrough(&raw_command, &format!("ztok fallback: {raw_command}"));
            if !status.success() {
                std::process::exit(status.code().unwrap_or(1));
            }
        }
        Err(err) => {
            eprintln!("[ztok: {err}]");
            std::process::exit(127);
        }
    }

    Ok(())
}

#[derive(Subcommand)]
enum GtCommands {
    /// 紧凑 stack log 输出
    Log {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// 紧凑 submit 输出
    Submit {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// 紧凑 sync 输出
    Sync {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// 紧凑 restack 输出
    Restack {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// 紧凑 create 输出
    Create {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// 分支信息与管理
    Branch {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// 透传：git 透传检测或直接执行 gt
    #[command(external_subcommand)]
    Other(Vec<OsString>),
}

pub fn run_from_os_args(args: Vec<OsString>) -> Result<()> {
    let initial_args = std::iter::once(OsString::from(ZTOK_ALIAS_NAME)).chain(args.iter().cloned());
    let cli = match Cli::try_parse_from(initial_args) {
        Ok(cli) => cli,
        Err(e) => {
            if matches!(e.kind(), ErrorKind::DisplayHelp | ErrorKind::DisplayVersion) {
                e.exit();
            }
            if let Some(rewritten_args) = rewrite_grep_option_first_args(&args) {
                let retry_args = std::iter::once(OsString::from(ZTOK_ALIAS_NAME))
                    .chain(rewritten_args.iter().cloned());
                if let Ok(cli) = Cli::try_parse_from(retry_args) {
                    return run_cli(cli);
                }
            }
            return run_fallback(&args, e);
        }
    };

    run_cli(cli)
}

fn run_cli(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Ls { args } => {
            ls::run(&args, cli.verbose)?;
        }

        Commands::Tree { args } => {
            tree::run(&args, cli.verbose)?;
        }

        Commands::Read {
            files,
            level,
            max_lines,
            tail_lines,
            line_numbers,
        } => {
            for file in files {
                if file == Path::new("-") {
                    read::run_stdin(level, max_lines, tail_lines, line_numbers, cli.verbose)?;
                } else {
                    read::run(
                        &file,
                        level,
                        max_lines,
                        tail_lines,
                        line_numbers,
                        cli.verbose,
                    )?;
                }
            }
        }

        Commands::Smart {
            file,
            model,
            force_download,
        } => {
            local_llm::run(&file, &model, force_download, cli.verbose)?;
        }

        Commands::Git {
            directory,
            config_override,
            git_dir,
            work_tree,
            no_pager,
            no_optional_locks,
            bare,
            literal_pathspecs,
            command,
        } => {
            // Build global git args (inserted between "git" and subcommand)
            let mut global_args: Vec<String> = Vec::new();
            for dir in &directory {
                global_args.push("-C".to_string());
                global_args.push(dir.clone());
            }
            for cfg in &config_override {
                global_args.push("-c".to_string());
                global_args.push(cfg.clone());
            }
            if let Some(ref dir) = git_dir {
                global_args.push("--git-dir".to_string());
                global_args.push(dir.clone());
            }
            if let Some(ref tree) = work_tree {
                global_args.push("--work-tree".to_string());
                global_args.push(tree.clone());
            }
            if no_pager {
                global_args.push("--no-pager".to_string());
            }
            if no_optional_locks {
                global_args.push("--no-optional-locks".to_string());
            }
            if bare {
                global_args.push("--bare".to_string());
            }
            if literal_pathspecs {
                global_args.push("--literal-pathspecs".to_string());
            }

            match command {
                GitCommands::Diff { args } => {
                    git::run(
                        git::GitCommand::Diff,
                        &args,
                        /*max_lines*/ None,
                        cli.verbose,
                        &global_args,
                    )?;
                }
                GitCommands::Log { args } => {
                    git::run(
                        git::GitCommand::Log,
                        &args,
                        /*max_lines*/ None,
                        cli.verbose,
                        &global_args,
                    )?;
                }
                GitCommands::Status { args } => {
                    git::run(
                        git::GitCommand::Status,
                        &args,
                        /*max_lines*/ None,
                        cli.verbose,
                        &global_args,
                    )?;
                }
                GitCommands::Show { args } => {
                    git::run(
                        git::GitCommand::Show,
                        &args,
                        /*max_lines*/ None,
                        cli.verbose,
                        &global_args,
                    )?;
                }
                GitCommands::Add { args } => {
                    git::run(
                        git::GitCommand::Add,
                        &args,
                        /*max_lines*/ None,
                        cli.verbose,
                        &global_args,
                    )?;
                }
                GitCommands::Commit { args } => {
                    git::run(
                        git::GitCommand::Commit,
                        &args,
                        /*max_lines*/ None,
                        cli.verbose,
                        &global_args,
                    )?;
                }
                GitCommands::Push { args } => {
                    git::run(
                        git::GitCommand::Push,
                        &args,
                        /*max_lines*/ None,
                        cli.verbose,
                        &global_args,
                    )?;
                }
                GitCommands::Pull { args } => {
                    git::run(
                        git::GitCommand::Pull,
                        &args,
                        /*max_lines*/ None,
                        cli.verbose,
                        &global_args,
                    )?;
                }
                GitCommands::Branch { args } => {
                    git::run(
                        git::GitCommand::Branch,
                        &args,
                        /*max_lines*/ None,
                        cli.verbose,
                        &global_args,
                    )?;
                }
                GitCommands::Fetch { args } => {
                    git::run(
                        git::GitCommand::Fetch,
                        &args,
                        /*max_lines*/ None,
                        cli.verbose,
                        &global_args,
                    )?;
                }
                GitCommands::Stash { subcommand, args } => {
                    git::run(
                        git::GitCommand::Stash { subcommand },
                        &args,
                        /*max_lines*/ None,
                        cli.verbose,
                        &global_args,
                    )?;
                }
                GitCommands::Worktree { args } => {
                    git::run(
                        git::GitCommand::Worktree,
                        &args,
                        /*max_lines*/ None,
                        cli.verbose,
                        &global_args,
                    )?;
                }
                GitCommands::Other(args) => {
                    git::run_passthrough(&args, &global_args, cli.verbose)?;
                }
            }
        }

        Commands::Gh { subcommand, args } => {
            gh_cmd::run(&subcommand, &args, cli.verbose, cli.ultra_compact)?;
        }

        Commands::Aws { subcommand, args } => {
            aws_cmd::run(&subcommand, &args, cli.verbose)?;
        }

        Commands::Psql { args } => {
            psql_cmd::run(&args, cli.verbose)?;
        }

        Commands::Pnpm { filter, command } => {
            if let Some(warning) = validate_pnpm_filters(&filter, &command) {
                eprintln!("{warning}");
            }

            match command {
                PnpmCommands::List { depth, args } => {
                    pnpm_cmd::run(
                        pnpm_cmd::PnpmCommand::List { depth },
                        &merge_pnpm_args(&filter, &args),
                        cli.verbose,
                    )?;
                }
                PnpmCommands::Outdated { args } => {
                    pnpm_cmd::run(
                        pnpm_cmd::PnpmCommand::Outdated,
                        &merge_pnpm_args(&filter, &args),
                        cli.verbose,
                    )?;
                }
                PnpmCommands::Install { packages, args } => {
                    pnpm_cmd::run(
                        pnpm_cmd::PnpmCommand::Install { packages },
                        &merge_pnpm_args(&filter, &args),
                        cli.verbose,
                    )?;
                }
                PnpmCommands::Build { args } => {
                    let mut build_args = merge_pnpm_args(&filter, &args);
                    build_args.insert(0, "build".into());
                    let os_args: Vec<OsString> =
                        build_args.into_iter().map(OsString::from).collect();
                    pnpm_cmd::run_passthrough(&os_args, cli.verbose)?;
                }
                PnpmCommands::Typecheck { args } => {
                    tsc_cmd::run(&args, cli.verbose)?;
                }
                PnpmCommands::Other(args) => {
                    pnpm_cmd::run_passthrough(&merge_pnpm_args_os(&filter, &args), cli.verbose)?;
                }
            }
        }

        Commands::Err { command } => {
            runner::run_err(&command, cli.verbose)?;
        }

        Commands::Test { command } => {
            runner::run_test(&command, cli.verbose)?;
        }

        Commands::Shell { filter, command } => {
            runner::run_shell(&command, filter, cli.verbose)?;
        }

        Commands::Json {
            file,
            depth,
            keys_only,
        } => {
            if file == Path::new("-") {
                json_cmd::run_stdin(depth, keys_only, cli.verbose)?;
            } else {
                json_cmd::run(&file, depth, keys_only, cli.verbose)?;
            }
        }

        Commands::Deps { path } => {
            deps::run(&path, cli.verbose)?;
        }

        Commands::Env { filter, show_all } => {
            env_cmd::run(filter.as_deref(), show_all, cli.verbose)?;
        }

        Commands::Find { args } => {
            find_cmd::run_from_args(&args, cli.verbose)?;
        }

        Commands::Diff { file1, file2 } => {
            if let Some(f2) = file2 {
                diff_cmd::run(&file1, &f2, cli.verbose)?;
            } else {
                diff_cmd::run_stdin(cli.verbose)?;
            }
        }

        Commands::Log { file } => {
            if let Some(f) = file {
                log_cmd::run_file(&f, cli.verbose)?;
            } else {
                log_cmd::run_stdin(cli.verbose)?;
            }
        }

        Commands::Docker { command } => match command {
            DockerCommands::Ps => {
                container::run(container::ContainerCmd::DockerPs, &[], cli.verbose)?;
            }
            DockerCommands::Images => {
                container::run(container::ContainerCmd::DockerImages, &[], cli.verbose)?;
            }
            DockerCommands::Logs { container: c } => {
                container::run(container::ContainerCmd::DockerLogs, &[c], cli.verbose)?;
            }
            DockerCommands::Compose { command: compose } => match compose {
                ComposeCommands::Ps => {
                    container::run_compose_ps(cli.verbose)?;
                }
                ComposeCommands::Logs { service } => {
                    container::run_compose_logs(service.as_deref(), cli.verbose)?;
                }
                ComposeCommands::Build { service } => {
                    container::run_compose_build(service.as_deref(), cli.verbose)?;
                }
                ComposeCommands::Other(args) => {
                    container::run_compose_passthrough(&args, cli.verbose)?;
                }
            },
            DockerCommands::Other(args) => {
                container::run_docker_passthrough(&args, cli.verbose)?;
            }
        },

        Commands::Kubectl { command } => match command {
            KubectlCommands::Pods { namespace, all } => {
                let mut args: Vec<String> = Vec::new();
                if all {
                    args.push("-A".to_string());
                } else if let Some(n) = namespace {
                    args.push("-n".to_string());
                    args.push(n);
                }
                container::run(container::ContainerCmd::KubectlPods, &args, cli.verbose)?;
            }
            KubectlCommands::Services { namespace, all } => {
                let mut args: Vec<String> = Vec::new();
                if all {
                    args.push("-A".to_string());
                } else if let Some(n) = namespace {
                    args.push("-n".to_string());
                    args.push(n);
                }
                container::run(container::ContainerCmd::KubectlServices, &args, cli.verbose)?;
            }
            KubectlCommands::Logs { pod, container: c } => {
                let mut args = vec![pod];
                if let Some(cont) = c {
                    args.push("-c".to_string());
                    args.push(cont);
                }
                container::run(container::ContainerCmd::KubectlLogs, &args, cli.verbose)?;
            }
            KubectlCommands::Other(args) => {
                container::run_kubectl_passthrough(&args, cli.verbose)?;
            }
        },

        Commands::Summary { command } => {
            let cmd = command.join(" ");
            summary::run(&cmd, cli.verbose)?;
        }

        Commands::Grep {
            pattern,
            path,
            max_len,
            max,
            context_only,
            file_type,
            line_numbers: _, // no-op: line numbers always enabled in grep_cmd::run
            extra_args,
        } => {
            grep_cmd::run(
                grep_cmd::GrepOptions {
                    pattern: &pattern,
                    path: &path,
                    max_line_len: max_len,
                    max_results: max,
                    context_only,
                    file_type: file_type.as_deref(),
                    extra_args: &extra_args,
                },
                cli.verbose,
            )?;
        }

        Commands::Wget { url, stdout, args } => {
            if stdout {
                wget_cmd::run_stdout(&url, &args, cli.verbose)?;
            } else {
                wget_cmd::run(&url, &args, cli.verbose)?;
            }
        }

        Commands::Wc { args } => {
            wc_cmd::run(&args, cli.verbose)?;
        }

        Commands::Vitest { args } => {
            vitest_cmd::run(&args, cli.verbose)?;
        }

        Commands::Prisma { command } => match command {
            PrismaCommands::Generate { args } => {
                prisma_cmd::run(prisma_cmd::PrismaCommand::Generate, &args, cli.verbose)?;
            }
            PrismaCommands::Migrate { command } => match command {
                PrismaMigrateCommands::Dev { name, args } => {
                    prisma_cmd::run(
                        prisma_cmd::PrismaCommand::Migrate {
                            subcommand: prisma_cmd::MigrateSubcommand::Dev { name },
                        },
                        &args,
                        cli.verbose,
                    )?;
                }
                PrismaMigrateCommands::Status { args } => {
                    prisma_cmd::run(
                        prisma_cmd::PrismaCommand::Migrate {
                            subcommand: prisma_cmd::MigrateSubcommand::Status,
                        },
                        &args,
                        cli.verbose,
                    )?;
                }
                PrismaMigrateCommands::Deploy { args } => {
                    prisma_cmd::run(
                        prisma_cmd::PrismaCommand::Migrate {
                            subcommand: prisma_cmd::MigrateSubcommand::Deploy,
                        },
                        &args,
                        cli.verbose,
                    )?;
                }
            },
            PrismaCommands::DbPush { args } => {
                prisma_cmd::run(prisma_cmd::PrismaCommand::DbPush, &args, cli.verbose)?;
            }
        },

        Commands::Tsc { args } => {
            tsc_cmd::run(&args, cli.verbose)?;
        }

        Commands::Next { args } => {
            next_cmd::run(&args, cli.verbose)?;
        }

        Commands::Lint { args } => {
            lint_cmd::run(&args, cli.verbose)?;
        }

        Commands::Prettier { args } => {
            prettier_cmd::run(&args, cli.verbose)?;
        }

        Commands::Format { args } => {
            format_cmd::run(&args, cli.verbose)?;
        }

        Commands::Playwright { args } => {
            playwright_cmd::run(&args, cli.verbose)?;
        }

        Commands::Cargo { command } => match command {
            CargoCommands::Build { args } => {
                cargo_cmd::run(cargo_cmd::CargoCommand::Build, &args, cli.verbose)?;
            }
            CargoCommands::Test { args } => {
                cargo_cmd::run(cargo_cmd::CargoCommand::Test, &args, cli.verbose)?;
            }
            CargoCommands::Clippy { args } => {
                cargo_cmd::run(cargo_cmd::CargoCommand::Clippy, &args, cli.verbose)?;
            }
            CargoCommands::Check { args } => {
                cargo_cmd::run(cargo_cmd::CargoCommand::Check, &args, cli.verbose)?;
            }
            CargoCommands::Install { args } => {
                cargo_cmd::run(cargo_cmd::CargoCommand::Install, &args, cli.verbose)?;
            }
            CargoCommands::Nextest { args } => {
                cargo_cmd::run(cargo_cmd::CargoCommand::Nextest, &args, cli.verbose)?;
            }
            CargoCommands::Other(args) => {
                cargo_cmd::run_passthrough(&args, cli.verbose)?;
            }
        },

        Commands::Npm { args } => {
            npm_cmd::run(&args, cli.verbose, cli.skip_env)?;
        }

        Commands::Curl { args } => {
            curl_cmd::run(&args, cli.verbose)?;
        }

        Commands::Npx { args } => {
            if args.is_empty() {
                anyhow::bail!("npx requires a command argument");
            }

            // Intelligent routing: delegate to specialized filters
            match args[0].as_str() {
                "tsc" | "typescript" => {
                    tsc_cmd::run(&args[1..], cli.verbose)?;
                }
                "eslint" => {
                    lint_cmd::run(&args[1..], cli.verbose)?;
                }
                "prisma" => {
                    // Route to prisma_cmd based on subcommand
                    if args.len() > 1 {
                        let prisma_args: Vec<String> = args[2..].to_vec();
                        match args[1].as_str() {
                            "generate" => {
                                prisma_cmd::run(
                                    prisma_cmd::PrismaCommand::Generate,
                                    &prisma_args,
                                    cli.verbose,
                                )?;
                            }
                            "db" if args.len() > 2 && args[2] == "push" => {
                                prisma_cmd::run(
                                    prisma_cmd::PrismaCommand::DbPush,
                                    &args[3..],
                                    cli.verbose,
                                )?;
                            }
                            _ => {
                                // Passthrough other prisma subcommands
                                let timer = tracking::TimedExecution::start();
                                let mut cmd = utils::resolved_command("npx");
                                for arg in &args {
                                    cmd.arg(arg);
                                }
                                let status = cmd.status().context("运行 npx prisma 失败")?;
                                let args_str = args.join(" ");
                                timer.track_passthrough(
                                    &format!("npx {args_str}"),
                                    &format!("ztok npx {args_str} (passthrough)"),
                                );
                                if !status.success() {
                                    std::process::exit(status.code().unwrap_or(1));
                                }
                            }
                        }
                    } else {
                        let timer = tracking::TimedExecution::start();
                        let status = utils::resolved_command("npx")
                            .arg("prisma")
                            .status()
                            .context("运行 npx prisma 失败")?;
                        timer.track_passthrough("npx prisma", "ztok npx prisma (passthrough)");
                        if !status.success() {
                            std::process::exit(status.code().unwrap_or(1));
                        }
                    }
                }
                "next" => {
                    next_cmd::run(&args[1..], cli.verbose)?;
                }
                "prettier" => {
                    prettier_cmd::run(&args[1..], cli.verbose)?;
                }
                "playwright" => {
                    playwright_cmd::run(&args[1..], cli.verbose)?;
                }
                _ => {
                    // Generic passthrough with npm boilerplate filter
                    npm_cmd::run(&args, cli.verbose, cli.skip_env)?;
                }
            }
        }

        Commands::Ruff { args } => {
            ruff_cmd::run(&args, cli.verbose)?;
        }

        Commands::Pytest { args } => {
            pytest_cmd::run(&args, cli.verbose)?;
        }

        Commands::Mypy { args } => {
            mypy_cmd::run(&args, cli.verbose)?;
        }

        Commands::Pip { args } => {
            pip_cmd::run(&args, cli.verbose)?;
        }

        Commands::Go { command } => match command {
            GoCommands::Test { args } => {
                go_cmd::run_test(&args, cli.verbose)?;
            }
            GoCommands::Build { args } => {
                go_cmd::run_build(&args, cli.verbose)?;
            }
            GoCommands::Vet { args } => {
                go_cmd::run_vet(&args, cli.verbose)?;
            }
            GoCommands::Other(args) => {
                go_cmd::run_other(&args, cli.verbose)?;
            }
        },

        Commands::Gt { command } => match command {
            GtCommands::Log { args } => {
                gt_cmd::run_log(&args, cli.verbose)?;
            }
            GtCommands::Submit { args } => {
                gt_cmd::run_submit(&args, cli.verbose)?;
            }
            GtCommands::Sync { args } => {
                gt_cmd::run_sync(&args, cli.verbose)?;
            }
            GtCommands::Restack { args } => {
                gt_cmd::run_restack(&args, cli.verbose)?;
            }
            GtCommands::Create { args } => {
                gt_cmd::run_create(&args, cli.verbose)?;
            }
            GtCommands::Branch { args } => {
                gt_cmd::run_branch(&args, cli.verbose)?;
            }
            GtCommands::Other(args) => {
                gt_cmd::run_other(&args, cli.verbose)?;
            }
        },

        Commands::GolangciLint { args } => {
            golangci_cmd::run(&args, cli.verbose)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn test_git_commit_single_message() {
        let cli = Cli::try_parse_from(["ztok", "git", "commit", "-m", "fix: typo"]).unwrap();
        match cli.command {
            Commands::Git {
                command: GitCommands::Commit { args },
                ..
            } => {
                assert_eq!(args, vec!["-m", "fix: typo"]);
            }
            _ => panic!("Expected Git Commit command"),
        }
    }

    #[test]
    fn test_git_commit_multiple_messages() {
        let cli = Cli::try_parse_from([
            "ztok",
            "git",
            "commit",
            "-m",
            "feat: add support",
            "-m",
            "Body paragraph here.",
        ])
        .unwrap();
        match cli.command {
            Commands::Git {
                command: GitCommands::Commit { args },
                ..
            } => {
                assert_eq!(
                    args,
                    vec!["-m", "feat: add support", "-m", "Body paragraph here."]
                );
            }
            _ => panic!("Expected Git Commit command"),
        }
    }

    // #327: git commit -am "msg" was rejected by Clap
    #[test]
    fn test_git_commit_am_flag() {
        let cli = Cli::try_parse_from(["ztok", "git", "commit", "-am", "quick fix"]).unwrap();
        match cli.command {
            Commands::Git {
                command: GitCommands::Commit { args },
                ..
            } => {
                assert_eq!(args, vec!["-am", "quick fix"]);
            }
            _ => panic!("Expected Git Commit command"),
        }
    }

    #[test]
    fn test_git_commit_amend() {
        let cli =
            Cli::try_parse_from(["ztok", "git", "commit", "--amend", "-m", "new msg"]).unwrap();
        match cli.command {
            Commands::Git {
                command: GitCommands::Commit { args },
                ..
            } => {
                assert_eq!(args, vec!["--amend", "-m", "new msg"]);
            }
            _ => panic!("Expected Git Commit command"),
        }
    }

    #[test]
    fn test_git_global_options_parsing() {
        let cli =
            Cli::try_parse_from(["ztok", "git", "--no-pager", "--no-optional-locks", "status"])
                .unwrap();
        match cli.command {
            Commands::Git {
                no_pager,
                no_optional_locks,
                bare,
                literal_pathspecs,
                ..
            } => {
                assert!(no_pager);
                assert!(no_optional_locks);
                assert!(!bare);
                assert!(!literal_pathspecs);
            }
            _ => panic!("Expected Git command"),
        }
    }

    #[test]
    fn test_git_commit_long_flag_multiple() {
        let cli = Cli::try_parse_from([
            "ztok",
            "git",
            "commit",
            "--message",
            "title",
            "--message",
            "body",
            "--message",
            "footer",
        ])
        .unwrap();
        match cli.command {
            Commands::Git {
                command: GitCommands::Commit { args },
                ..
            } => {
                assert_eq!(
                    args,
                    vec![
                        "--message",
                        "title",
                        "--message",
                        "body",
                        "--message",
                        "footer"
                    ]
                );
            }
            _ => panic!("Expected Git Commit command"),
        }
    }

    #[test]
    fn test_try_parse_valid_git_status() {
        let result = Cli::try_parse_from(["ztok", "git", "status"]);
        assert!(result.is_ok(), "git status should parse successfully");
    }

    #[test]
    fn test_try_parse_valid_cargo_build() {
        let result = Cli::try_parse_from(["ztok", "cargo", "build", "-p", "codex-cli"]);
        assert!(result.is_ok(), "cargo build should parse successfully");
    }

    #[test]
    fn test_try_parse_help_is_display_help() {
        match Cli::try_parse_from(["ztok", "--help"]) {
            Err(e) => assert_eq!(e.kind(), ErrorKind::DisplayHelp),
            Ok(_) => panic!("Expected DisplayHelp error"),
        }
    }

    #[test]
    fn test_help_output_is_localized() {
        let mut command = Cli::command();
        let help = command.render_long_help().to_string();
        assert!(help.contains("用法: "));
        assert!(help.contains("命令:\n"));
        assert!(help.contains("选项:\n"));
        assert!(help.contains("显示帮助信息"));
        assert!(help.contains("显示版本"));
        assert!(!help.contains("Print version"));
    }

    #[test]
    fn test_try_parse_version_is_display_version() {
        match Cli::try_parse_from(["ztok", "--version"]) {
            Err(e) => assert_eq!(e.kind(), ErrorKind::DisplayVersion),
            Ok(_) => panic!("Expected DisplayVersion error"),
        }
    }

    #[test]
    fn test_try_parse_version_after_global_flags_is_display_version() {
        match Cli::try_parse_from(["ztok", "--verbose", "--version"]) {
            Err(e) => assert_eq!(e.kind(), ErrorKind::DisplayVersion),
            Ok(_) => panic!("Expected DisplayVersion error"),
        }
    }

    #[test]
    fn test_try_parse_unknown_subcommand_is_error() {
        match Cli::try_parse_from(["ztok", "nonexistent-command"]) {
            Err(e) => assert!(!matches!(
                e.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            )),
            Ok(_) => panic!("Expected parse error for unknown subcommand"),
        }
    }

    #[test]
    fn test_try_parse_git_with_dash_c_succeeds() {
        let result = Cli::try_parse_from(["ztok", "git", "-C", "/path", "status"]);
        assert!(
            result.is_ok(),
            "git -C /path status should parse successfully"
        );
        if let Ok(cli) = result {
            match cli.command {
                Commands::Git { directory, .. } => {
                    assert_eq!(directory, vec!["/path"]);
                }
                _ => panic!("Expected Git command"),
            }
        }
    }

    #[test]
    fn test_first_subcommand_arg_skips_global_flags() {
        let args = vec![
            OsString::from("--verbose"),
            OsString::from("--ultra-compact"),
            OsString::from("-vv"),
            OsString::from("--skip-env"),
            OsString::from("rewrite"),
        ];
        assert_eq!(first_subcommand_arg(&args).as_deref(), Some("rewrite"));
    }

    #[test]
    fn test_first_subcommand_arg_respects_double_dash_boundary() {
        let args = vec![
            OsString::from("--verbose"),
            OsString::from("--"),
            OsString::from("custom-fallback"),
            OsString::from("alpha"),
        ];
        assert_eq!(
            first_subcommand_arg(&args).as_deref(),
            Some("custom-fallback")
        );
        assert_eq!(first_subcommand_index(&args), Some(2));
    }

    #[test]
    fn test_rewrite_grep_subcommand_args_reorders_leading_grep_flags() {
        let args = vec![
            OsString::from("-RInE"),
            OsString::from("needle"),
            OsString::from("."),
            OsString::from("--exclude-dir=.git"),
        ];

        let rewritten = rewrite_grep_subcommand_args(&args).expect("grep args should rewrite");

        assert_eq!(
            rewritten,
            vec![
                OsString::from("needle"),
                OsString::from("."),
                OsString::from("-RInE"),
                OsString::from("--exclude-dir=.git"),
            ]
        );
    }

    #[test]
    fn test_rewrite_grep_option_first_args_preserves_global_flags() {
        let args = vec![
            OsString::from("--verbose"),
            OsString::from("grep"),
            OsString::from("-RInE"),
            OsString::from("needle"),
            OsString::from("."),
        ];

        let rewritten = rewrite_grep_option_first_args(&args).expect("grep args should rewrite");

        assert_eq!(
            rewritten,
            vec![
                OsString::from("--verbose"),
                OsString::from("grep"),
                OsString::from("needle"),
                OsString::from("."),
                OsString::from("-RInE"),
            ]
        );
    }

    #[test]
    fn test_rewrite_grep_subcommand_args_skips_pattern_flags() {
        let args = vec![
            OsString::from("-e"),
            OsString::from("needle"),
            OsString::from("."),
        ];

        assert!(rewrite_grep_subcommand_args(&args).is_none());
    }

    #[test]
    fn test_should_show_parse_error_for_removed_command_after_global_flags() {
        let args = vec![
            OsString::from("--verbose"),
            OsString::from("--ultra-compact"),
            OsString::from("rewrite"),
        ];
        assert!(should_show_parse_error(&args));
    }

    #[test]
    fn test_should_show_parse_error_for_builtin_command_after_global_flags() {
        let args = vec![OsString::from("-vv"), OsString::from("read")];
        assert!(should_show_parse_error(&args));
    }

    #[test]
    fn test_should_not_show_parse_error_for_unknown_command_after_global_flags() {
        let args = vec![
            OsString::from("--skip-env"),
            OsString::from("--ultra-compact"),
            OsString::from("custom-fallback"),
        ];
        assert!(!should_show_parse_error(&args));
    }

    #[test]
    fn test_should_show_parse_error_for_builtin_command_after_double_dash() {
        let args = vec![
            OsString::from("--verbose"),
            OsString::from("--"),
            OsString::from("read"),
        ];
        assert!(should_show_parse_error(&args));
    }

    #[test]
    fn test_should_not_show_parse_error_for_unknown_command_after_double_dash() {
        let args = vec![
            OsString::from("--verbose"),
            OsString::from("--"),
            OsString::from("custom-fallback"),
        ];
        assert!(!should_show_parse_error(&args));
    }

    #[test]
    fn test_should_not_show_parse_error_for_env_wrapper_after_double_dash() {
        let args = vec![
            OsString::from("--verbose"),
            OsString::from("--"),
            OsString::from("env"),
            OsString::from("FOO=1"),
            OsString::from("git"),
            OsString::from("status"),
        ];
        assert!(!should_show_parse_error(&args));
    }

    #[test]
    fn test_should_show_parse_error_for_removed_command_after_double_dash() {
        let args = vec![
            OsString::from("--verbose"),
            OsString::from("--"),
            OsString::from("rewrite"),
        ];
        assert!(should_show_parse_error(&args));
    }
}
