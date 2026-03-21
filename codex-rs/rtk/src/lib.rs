#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used))]

mod aws_cmd;
mod cargo_cmd;
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
mod ruff_cmd;
mod runner;
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
use clap::Parser;
use clap::Subcommand;
use clap::error::ErrorKind;
use std::ffi::OsString;
use std::path::Path;
use std::path::PathBuf;

const RTK_ALIAS_NAME: &str = "rtk";

pub fn alias_name() -> &'static str {
    RTK_ALIAS_NAME
}

pub fn is_alias_invocation(argv0: &OsString) -> bool {
    Path::new(argv0)
        .file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|file_name| file_name == RTK_ALIAS_NAME)
}

#[derive(Parser)]
#[command(
    name = "rtk",
    version,
    about = "Rust Token Killer - Minimize LLM token consumption",
    long_about = "A high-performance CLI proxy designed to filter and summarize system outputs before they reach your LLM context."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Verbosity level (-v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,

    /// Ultra-compact mode: ASCII icons, inline format (Level 2 optimizations)
    #[arg(short = 'u', long, global = true)]
    ultra_compact: bool,

    /// Set SKIP_ENV_VALIDATION=1 for child processes (Next.js, tsc, lint, prisma)
    #[arg(long = "skip-env", global = true)]
    skip_env: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// List directory contents with token-optimized output (proxy to native ls)
    Ls {
        /// Arguments passed to ls (supports all native ls flags like -l, -a, -h, -R)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Directory tree with token-optimized output (proxy to native tree)
    Tree {
        /// Arguments passed to tree (supports all native tree flags like -L, -d, -a)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Read file with intelligent filtering
    Read {
        /// File to read
        file: PathBuf,
        /// Filter: none, minimal, aggressive
        #[arg(short, long, default_value = "minimal")]
        level: filter::FilterLevel,
        /// Max lines
        #[arg(short, long, conflicts_with = "tail_lines")]
        max_lines: Option<usize>,
        /// Keep only last N lines
        #[arg(long, conflicts_with = "max_lines")]
        tail_lines: Option<usize>,
        /// Show line numbers
        #[arg(short = 'n', long)]
        line_numbers: bool,
    },

    /// Generate 2-line technical summary (heuristic-based)
    Smart {
        /// File to analyze
        file: PathBuf,
        /// Model: heuristic
        #[arg(short, long, default_value = "heuristic")]
        model: String,
        /// Force model download
        #[arg(long)]
        force_download: bool,
    },

    /// Git commands with compact output
    Git {
        /// Change to directory before executing (like git -C <path>, can be repeated)
        #[arg(short = 'C', action = clap::ArgAction::Append)]
        directory: Vec<String>,

        /// Git configuration override (like git -c key=value, can be repeated)
        #[arg(short = 'c', action = clap::ArgAction::Append)]
        config_override: Vec<String>,

        /// Set the path to the .git directory
        #[arg(long = "git-dir")]
        git_dir: Option<String>,

        /// Set the path to the working tree
        #[arg(long = "work-tree")]
        work_tree: Option<String>,

        /// Disable pager (like git --no-pager)
        #[arg(long = "no-pager")]
        no_pager: bool,

        /// Skip optional locks (like git --no-optional-locks)
        #[arg(long = "no-optional-locks")]
        no_optional_locks: bool,

        /// Treat repository as bare (like git --bare)
        #[arg(long)]
        bare: bool,

        /// Treat pathspecs literally (like git --literal-pathspecs)
        #[arg(long = "literal-pathspecs")]
        literal_pathspecs: bool,

        #[command(subcommand)]
        command: GitCommands,
    },

    /// GitHub CLI (gh) commands with token-optimized output
    Gh {
        /// Subcommand: pr, issue, run, repo
        subcommand: String,
        /// Additional arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// AWS CLI with compact output (force JSON, compress)
    Aws {
        /// AWS service subcommand (e.g., sts, s3, ec2, ecs, rds, cloudformation)
        subcommand: String,
        /// Additional arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// PostgreSQL client with compact output (strip borders, compress tables)
    Psql {
        /// psql arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// pnpm commands with ultra-compact output
    Pnpm {
        #[command(subcommand)]
        command: PnpmCommands,
    },

    /// Run command and show only errors/warnings
    Err {
        /// Command to run
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },

    /// Run tests and show only failures
    Test {
        /// Test command (e.g. cargo test)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },

    /// Show JSON structure without values
    Json {
        /// JSON file
        file: PathBuf,
        /// Max depth
        #[arg(short, long, default_value = "5")]
        depth: usize,
    },

    /// Summarize project dependencies
    Deps {
        /// Project path
        #[arg(default_value = ".")]
        path: PathBuf,
    },

    /// Show environment variables (filtered, sensitive masked)
    Env {
        /// Filter by name (e.g. PATH, AWS)
        #[arg(short, long)]
        filter: Option<String>,
        /// Show all (include sensitive)
        #[arg(long)]
        show_all: bool,
    },

    /// Find files with compact tree output (accepts native find flags like -name, -type)
    Find {
        /// All find arguments (supports both RTK and native find syntax)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Ultra-condensed diff (only changed lines)
    Diff {
        /// First file or - for stdin (unified diff)
        file1: PathBuf,
        /// Second file (optional if stdin)
        file2: Option<PathBuf>,
    },

    /// Filter and deduplicate log output
    Log {
        /// Log file (omit for stdin)
        file: Option<PathBuf>,
    },

    /// Docker commands with compact output
    Docker {
        #[command(subcommand)]
        command: DockerCommands,
    },

    /// Kubectl commands with compact output
    Kubectl {
        #[command(subcommand)]
        command: KubectlCommands,
    },

    /// Run command and show heuristic summary
    Summary {
        /// Command to run and summarize
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },

    /// Compact grep - strips whitespace, truncates, groups by file
    Grep {
        /// Pattern to search
        pattern: String,
        /// Path to search in
        #[arg(default_value = ".")]
        path: String,
        /// Max line length
        #[arg(short = 'l', long, default_value = "80")]
        max_len: usize,
        /// Max results to show
        #[arg(short, long, default_value = "50")]
        max: usize,
        /// Show only match context (not full line)
        #[arg(short, long)]
        context_only: bool,
        /// Filter by file type (e.g., ts, py, rust)
        #[arg(short = 't', long)]
        file_type: Option<String>,
        /// Show line numbers (always on, accepted for grep/rg compatibility)
        #[arg(short = 'n', long)]
        line_numbers: bool,
        /// Extra ripgrep arguments (e.g., -i, -A 3, -w, --glob)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        extra_args: Vec<String>,
    },

    /// Download with compact output (strips progress bars)
    Wget {
        /// URL to download
        url: String,
        /// Output to stdout instead of file
        #[arg(short = 'O', long)]
        stdout: bool,
        /// Additional wget arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Word/line/byte count with compact output (strips paths and padding)
    Wc {
        /// Arguments passed to wc (files, flags like -l, -w, -c)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Vitest commands with compact output
    Vitest {
        #[command(subcommand)]
        command: VitestCommands,
    },

    /// Prisma commands with compact output (no ASCII art)
    Prisma {
        #[command(subcommand)]
        command: PrismaCommands,
    },

    /// TypeScript compiler with grouped error output
    Tsc {
        /// TypeScript compiler arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Next.js build with compact output
    Next {
        /// Next.js build arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// ESLint with grouped rule violations
    Lint {
        /// Linter arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Prettier format checker with compact output
    Prettier {
        /// Prettier arguments (e.g., --check, --write)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Universal format checker (prettier, black, ruff format)
    Format {
        /// Formatter arguments (auto-detects formatter from project files)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Playwright E2E tests with compact output
    Playwright {
        /// Playwright arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Cargo commands with compact output
    Cargo {
        #[command(subcommand)]
        command: CargoCommands,
    },

    /// npm run with filtered output (strip boilerplate)
    Npm {
        /// npm run arguments (script name + options)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// npx with intelligent routing (tsc, eslint, prisma -> specialized filters)
    Npx {
        /// npx arguments (command + options)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Curl with auto-JSON detection and schema output
    Curl {
        /// Curl arguments (URL + options)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Ruff linter/formatter with compact output
    Ruff {
        /// Ruff arguments (e.g., check, format --check)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Pytest test runner with compact output
    Pytest {
        /// Pytest arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Mypy type checker with grouped error output
    Mypy {
        /// Mypy arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Pip package manager with compact output (auto-detects uv)
    Pip {
        /// Pip arguments (e.g., list, outdated, install)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Go commands with compact output
    Go {
        #[command(subcommand)]
        command: GoCommands,
    },

    /// Graphite (gt) stacked PR commands with compact output
    Gt {
        #[command(subcommand)]
        command: GtCommands,
    },

    /// golangci-lint with compact output
    #[command(name = "golangci-lint")]
    GolangciLint {
        /// golangci-lint arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

#[derive(Subcommand)]
enum GitCommands {
    /// Condensed diff output
    Diff {
        /// Git arguments (supports all git diff flags like --stat, --cached, etc)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// One-line commit history
    Log {
        /// Git arguments (supports all git log flags like --oneline, --graph, --all)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Compact status (supports all git status flags)
    Status {
        /// Git arguments (supports all git status flags like --porcelain, --short, -s)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Compact show (commit summary + stat + compacted diff)
    Show {
        /// Git arguments (supports all git show flags)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Add files → "ok"
    Add {
        /// Files and flags to add (supports all git add flags like -A, -p, --all, etc)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Commit → "ok \<hash\>"
    Commit {
        /// Git commit arguments (supports -a, -m, --amend, --allow-empty, etc)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Push → "ok \<branch\>"
    Push {
        /// Git push arguments (supports -u, remote, branch, etc.)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Pull → "ok \<stats\>"
    Pull {
        /// Git pull arguments (supports --rebase, remote, branch, etc.)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Compact branch listing (current/local/remote)
    Branch {
        /// Git branch arguments (supports -d, -D, -m, etc.)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Fetch → "ok fetched (N new refs)"
    Fetch {
        /// Git fetch arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Stash management (list, show, pop, apply, drop)
    Stash {
        /// Subcommand: list, show, pop, apply, drop, push
        subcommand: Option<String>,
        /// Additional arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Compact worktree listing
    Worktree {
        /// Git worktree arguments (add, remove, prune, or empty for list)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Passthrough: runs any unsupported git subcommand directly
    #[command(external_subcommand)]
    Other(Vec<OsString>),
}

#[derive(Subcommand)]
enum PnpmCommands {
    /// List installed packages (ultra-dense)
    List {
        /// Depth level (default: 0)
        #[arg(short, long, default_value = "0")]
        depth: usize,
        /// Additional pnpm arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Show outdated packages (condensed: "pkg: old → new")
    Outdated {
        /// Additional pnpm arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Install packages (filter progress bars)
    Install {
        /// Packages to install
        packages: Vec<String>,
        /// Additional pnpm arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Build (generic passthrough, no framework-specific filter)
    Build {
        /// Additional build arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Typecheck (delegates to tsc filter)
    Typecheck {
        /// Additional typecheck arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Passthrough: runs any unsupported pnpm subcommand directly
    #[command(external_subcommand)]
    Other(Vec<OsString>),
}

#[derive(Subcommand)]
enum DockerCommands {
    /// List running containers
    Ps,
    /// List images
    Images,
    /// Show container logs (deduplicated)
    Logs { container: String },
    /// Docker Compose commands with compact output
    Compose {
        #[command(subcommand)]
        command: ComposeCommands,
    },
    /// Passthrough: runs any unsupported docker subcommand directly
    #[command(external_subcommand)]
    Other(Vec<OsString>),
}

#[derive(Subcommand)]
enum ComposeCommands {
    /// List compose services (compact)
    Ps,
    /// Show compose logs (deduplicated)
    Logs {
        /// Optional service name
        service: Option<String>,
    },
    /// Build compose services (summary)
    Build {
        /// Optional service name
        service: Option<String>,
    },
    /// Passthrough: runs any unsupported compose subcommand directly
    #[command(external_subcommand)]
    Other(Vec<OsString>),
}

#[derive(Subcommand)]
enum KubectlCommands {
    /// List pods
    Pods {
        #[arg(short, long)]
        namespace: Option<String>,
        /// All namespaces
        #[arg(short = 'A', long)]
        all: bool,
    },
    /// List services
    Services {
        #[arg(short, long)]
        namespace: Option<String>,
        /// All namespaces
        #[arg(short = 'A', long)]
        all: bool,
    },
    /// Show pod logs (deduplicated)
    Logs {
        pod: String,
        #[arg(short, long)]
        container: Option<String>,
    },
    /// Passthrough: runs any unsupported kubectl subcommand directly
    #[command(external_subcommand)]
    Other(Vec<OsString>),
}

#[derive(Subcommand)]
enum VitestCommands {
    /// Run tests with filtered output (90% token reduction)
    Run {
        /// Additional vitest arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

#[derive(Subcommand)]
enum PrismaCommands {
    /// Generate Prisma Client (strip ASCII art)
    Generate {
        /// Additional prisma arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Manage migrations
    Migrate {
        #[command(subcommand)]
        command: PrismaMigrateCommands,
    },
    /// Push schema to database
    DbPush {
        /// Additional prisma arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

#[derive(Subcommand)]
enum PrismaMigrateCommands {
    /// Create and apply migration
    Dev {
        /// Migration name
        #[arg(short, long)]
        name: Option<String>,
        /// Additional arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Check migration status
    Status {
        /// Additional arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Deploy migrations to production
    Deploy {
        /// Additional arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

#[derive(Subcommand)]
enum CargoCommands {
    /// Build with compact output (strip Compiling lines, keep errors)
    Build {
        /// Additional cargo build arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Test with failures-only output
    Test {
        /// Additional cargo test arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Clippy with warnings grouped by lint rule
    Clippy {
        /// Additional cargo clippy arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Check with compact output (strip Checking lines, keep errors)
    Check {
        /// Additional cargo check arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Install with compact output (strip dep compilation, keep installed/errors)
    Install {
        /// Additional cargo install arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Nextest with failures-only output
    Nextest {
        /// Additional cargo nextest arguments (e.g., run, list, --lib)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Passthrough: runs any unsupported cargo subcommand directly
    #[command(external_subcommand)]
    Other(Vec<OsString>),
}

#[derive(Subcommand)]
enum GoCommands {
    /// Run tests with compact output (90% token reduction via JSON streaming)
    Test {
        /// Additional go test arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Build with compact output (errors only)
    Build {
        /// Additional go build arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Vet with compact output
    Vet {
        /// Additional go vet arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Passthrough: runs any unsupported go subcommand directly
    #[command(external_subcommand)]
    Other(Vec<OsString>),
}

/// RTK-only subcommands that should never fall back to raw execution.
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

fn run_fallback(args: &[OsString], parse_error: clap::Error) -> Result<()> {
    if args.is_empty() {
        parse_error.exit();
    }

    let rendered_args = args
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>();

    if REMOVED_BUILTIN_COMMANDS.contains(&rendered_args[0].as_str()) {
        parse_error.exit();
    }

    let raw_command = rendered_args.join(" ");
    let timer = tracking::TimedExecution::start();

    let status = utils::resolved_command(&rendered_args[0])
        .args(&args[1..])
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status();

    match status {
        Ok(status) => {
            timer.track_passthrough(&raw_command, &format!("rtk fallback: {raw_command}"));
            if !status.success() {
                std::process::exit(status.code().unwrap_or(1));
            }
        }
        Err(err) => {
            eprintln!("[rtk: {err}]");
            std::process::exit(127);
        }
    }

    Ok(())
}

#[derive(Subcommand)]
enum GtCommands {
    /// Compact stack log output
    Log {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Compact submit output
    Submit {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Compact sync output
    Sync {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Compact restack output
    Restack {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Compact create output
    Create {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Branch info and management
    Branch {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Passthrough: git-passthrough detection or direct gt execution
    #[command(external_subcommand)]
    Other(Vec<OsString>),
}

pub fn run_from_os_args(args: Vec<OsString>) -> Result<()> {
    let cli = match Cli::try_parse_from(
        std::iter::once(OsString::from(RTK_ALIAS_NAME)).chain(args.iter().cloned()),
    ) {
        Ok(cli) => cli,
        Err(e) => {
            if matches!(e.kind(), ErrorKind::DisplayHelp | ErrorKind::DisplayVersion) {
                e.exit();
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
            file,
            level,
            max_lines,
            tail_lines,
            line_numbers,
        } => {
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

        Commands::Pnpm { command } => match command {
            PnpmCommands::List { depth, args } => {
                pnpm_cmd::run(pnpm_cmd::PnpmCommand::List { depth }, &args, cli.verbose)?;
            }
            PnpmCommands::Outdated { args } => {
                pnpm_cmd::run(pnpm_cmd::PnpmCommand::Outdated, &args, cli.verbose)?;
            }
            PnpmCommands::Install { packages, args } => {
                pnpm_cmd::run(
                    pnpm_cmd::PnpmCommand::Install { packages },
                    &args,
                    cli.verbose,
                )?;
            }
            PnpmCommands::Build { args } => {
                let mut build_args: Vec<String> = vec!["build".into()];
                build_args.extend(args);
                let os_args: Vec<OsString> = build_args.into_iter().map(OsString::from).collect();
                pnpm_cmd::run_passthrough(&os_args, cli.verbose)?;
            }
            PnpmCommands::Typecheck { args } => {
                tsc_cmd::run(&args, cli.verbose)?;
            }
            PnpmCommands::Other(args) => {
                pnpm_cmd::run_passthrough(&args, cli.verbose)?;
            }
        },

        Commands::Err { command } => {
            runner::run_err(&command, cli.verbose)?;
        }

        Commands::Test { command } => {
            runner::run_test(&command, cli.verbose)?;
        }

        Commands::Json { file, depth } => {
            if file == Path::new("-") {
                json_cmd::run_stdin(depth, cli.verbose)?;
            } else {
                json_cmd::run(&file, depth, cli.verbose)?;
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

        Commands::Vitest { command } => match command {
            VitestCommands::Run { args } => {
                vitest_cmd::run(vitest_cmd::VitestCommand::Run, &args, cli.verbose)?;
            }
        },

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
                                let status = cmd.status().context("Failed to run npx prisma")?;
                                let args_str = args.join(" ");
                                timer.track_passthrough(
                                    &format!("npx {args_str}"),
                                    &format!("rtk npx {args_str} (passthrough)"),
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
                            .context("Failed to run npx prisma")?;
                        timer.track_passthrough("npx prisma", "rtk npx prisma (passthrough)");
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
        let cli = Cli::try_parse_from(["rtk", "git", "commit", "-m", "fix: typo"]).unwrap();
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
            "rtk",
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
        let cli = Cli::try_parse_from(["rtk", "git", "commit", "-am", "quick fix"]).unwrap();
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
            Cli::try_parse_from(["rtk", "git", "commit", "--amend", "-m", "new msg"]).unwrap();
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
            Cli::try_parse_from(["rtk", "git", "--no-pager", "--no-optional-locks", "status"])
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
            "rtk",
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
        let result = Cli::try_parse_from(["rtk", "git", "status"]);
        assert!(result.is_ok(), "git status should parse successfully");
    }

    #[test]
    fn test_try_parse_help_is_display_help() {
        match Cli::try_parse_from(["rtk", "--help"]) {
            Err(e) => assert_eq!(e.kind(), ErrorKind::DisplayHelp),
            Ok(_) => panic!("Expected DisplayHelp error"),
        }
    }

    #[test]
    fn test_try_parse_version_is_display_version() {
        match Cli::try_parse_from(["rtk", "--version"]) {
            Err(e) => assert_eq!(e.kind(), ErrorKind::DisplayVersion),
            Ok(_) => panic!("Expected DisplayVersion error"),
        }
    }

    #[test]
    fn test_try_parse_unknown_subcommand_is_error() {
        match Cli::try_parse_from(["rtk", "nonexistent-command"]) {
            Err(e) => assert!(!matches!(
                e.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            )),
            Ok(_) => panic!("Expected parse error for unknown subcommand"),
        }
    }

    #[test]
    fn test_try_parse_git_with_dash_c_succeeds() {
        let result = Cli::try_parse_from(["rtk", "git", "-C", "/path", "status"]);
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
}
