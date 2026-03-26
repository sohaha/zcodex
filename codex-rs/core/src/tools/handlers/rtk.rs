use super::parse_arguments;
use crate::function_tool::FunctionCallError;
use crate::tools::context::FunctionToolOutput;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::ToolKind;
use async_trait::async_trait;
use serde::Deserialize;
use std::ffi::OsStr;
use std::ffi::OsString;
use std::path::Path;
use std::path::PathBuf;
use tokio::process::Command;
use tokio::time::Duration;
use tokio::time::timeout;

const RTK_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RtkCommandKind {
    Read,
    Grep,
    Find,
    Diff,
    Json,
    Deps,
    Log,
    Ls,
    Tree,
    Wc,
    GitStatus,
    GitDiff,
    GitShow,
    GitLog,
    GitBranch,
    GitStash,
    GitWorktree,
    Summary,
    Err,
}

impl RtkCommandKind {
    fn tool_name(self) -> &'static str {
        match self {
            Self::Read => "rtk_read",
            Self::Grep => "rtk_grep",
            Self::Find => "rtk_find",
            Self::Diff => "rtk_diff",
            Self::Json => "rtk_json",
            Self::Deps => "rtk_deps",
            Self::Log => "rtk_log",
            Self::Ls => "rtk_ls",
            Self::Tree => "rtk_tree",
            Self::Wc => "rtk_wc",
            Self::GitStatus => "rtk_git_status",
            Self::GitDiff => "rtk_git_diff",
            Self::GitShow => "rtk_git_show",
            Self::GitLog => "rtk_git_log",
            Self::GitBranch => "rtk_git_branch",
            Self::GitStash => "rtk_git_stash",
            Self::GitWorktree => "rtk_git_worktree",
            Self::Summary => "rtk_summary",
            Self::Err => "rtk_err",
        }
    }

    fn subcommand(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Grep => "grep",
            Self::Find => "find",
            Self::Diff => "diff",
            Self::Json => "json",
            Self::Deps => "deps",
            Self::Log => "log",
            Self::Ls => "ls",
            Self::Tree => "tree",
            Self::Wc => "wc",
            Self::GitStatus
            | Self::GitDiff
            | Self::GitShow
            | Self::GitLog
            | Self::GitBranch
            | Self::GitStash
            | Self::GitWorktree => "git",
            Self::Summary => "summary",
            Self::Err => "err",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RtkExecutable {
    program: PathBuf,
    prefix_args: Vec<OsString>,
}

#[derive(Clone, Debug)]
pub struct RtkHandler {
    kind: RtkCommandKind,
    executable_override: Option<RtkExecutable>,
}

impl RtkHandler {
    pub fn new(kind: RtkCommandKind) -> Self {
        Self {
            kind,
            executable_override: None,
        }
    }

    #[cfg(test)]
    fn with_executable_override(kind: RtkCommandKind, executable: RtkExecutable) -> Self {
        Self {
            kind,
            executable_override: Some(executable),
        }
    }
}

#[async_trait]
impl ToolHandler for RtkHandler {
    type Output = FunctionToolOutput;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        let ToolInvocation { payload, turn, .. } = invocation;
        let arguments = match payload {
            ToolPayload::Function { arguments } => arguments,
            _ => {
                return Err(FunctionCallError::RespondToModel(format!(
                    "{} handler received unsupported payload",
                    self.kind.tool_name()
                )));
            }
        };

        let executable = match &self.executable_override {
            Some(executable) => executable.clone(),
            None => discover_rtk_executable()?,
        };
        let command_args = build_command_args(self.kind, &arguments)?;
        let output = run_rtk_command(self.kind, &executable, &command_args, &turn.cwd).await?;
        Ok(output)
    }
}

#[derive(Deserialize)]
struct RtkReadArgs {
    path: String,
    #[serde(default = "default_rtk_read_level")]
    level: String,
    #[serde(default)]
    max_lines: Option<usize>,
    #[serde(default)]
    tail_lines: Option<usize>,
    #[serde(default)]
    line_numbers: bool,
}

#[derive(Deserialize)]
struct RtkGrepArgs {
    pattern: String,
    #[serde(default = "default_dot_path")]
    path: String,
    #[serde(default = "default_rtk_grep_max_len")]
    max_len: usize,
    #[serde(default = "default_rtk_grep_max")]
    max: usize,
    #[serde(default)]
    context_only: bool,
    #[serde(default)]
    file_type: Option<String>,
    #[serde(default)]
    line_numbers: bool,
    #[serde(default)]
    extra_args: Vec<String>,
}

#[derive(Deserialize)]
struct RtkFindArgs {
    pattern: String,
    #[serde(default = "default_dot_path")]
    path: String,
    #[serde(default = "default_rtk_find_max")]
    max_results: usize,
    #[serde(default)]
    file_type: Option<String>,
}

#[derive(Deserialize)]
struct RtkDiffArgs {
    left: String,
    right: String,
}

#[derive(Deserialize)]
struct RtkJsonArgs {
    path: String,
    #[serde(default = "default_rtk_json_depth")]
    depth: usize,
}

#[derive(Deserialize)]
struct RtkDepsArgs {
    #[serde(default = "default_dot_path")]
    path: String,
}

#[derive(Deserialize)]
struct RtkLogArgs {
    path: String,
}

#[derive(Deserialize)]
struct RtkLsArgs {
    #[serde(default = "default_dot_path")]
    path: String,
    #[serde(default)]
    all: bool,
}

#[derive(Deserialize)]
struct RtkTreeArgs {
    #[serde(default = "default_dot_path")]
    path: String,
    #[serde(default)]
    all: bool,
    #[serde(default)]
    max_depth: Option<usize>,
}

#[derive(Deserialize)]
struct RtkWcArgs {
    path: String,
    #[serde(default = "default_rtk_wc_mode")]
    mode: String,
}

#[derive(Deserialize)]
struct RtkGitStatusArgs {
    #[serde(default)]
    path: Option<String>,
}

#[derive(Deserialize)]
struct RtkGitDiffArgs {
    #[serde(default)]
    target: Option<String>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    cached: bool,
}

#[derive(Deserialize)]
struct RtkGitShowArgs {
    #[serde(default = "default_rtk_git_show_revision")]
    revision: String,
}

#[derive(Deserialize)]
struct RtkGitLogArgs {
    #[serde(default)]
    revision_range: Option<String>,
    #[serde(default = "default_rtk_git_log_max_count")]
    max_count: usize,
}

#[derive(Deserialize)]
struct RtkGitBranchArgs {
    #[serde(default)]
    all: bool,
    #[serde(default)]
    remotes: bool,
    #[serde(default)]
    contains: Option<String>,
    #[serde(default)]
    merged: bool,
    #[serde(default)]
    no_merged: bool,
}

#[derive(Deserialize)]
struct RtkGitStashArgs {
    #[serde(default = "default_rtk_git_log_max_count")]
    max_count: usize,
}

#[derive(Deserialize)]
struct RtkCommandStringArgs {
    command: String,
}

fn default_dot_path() -> String {
    ".".to_string()
}

fn default_rtk_read_level() -> String {
    "minimal".to_string()
}

fn default_rtk_grep_max_len() -> usize {
    80
}

fn default_rtk_grep_max() -> usize {
    50
}

fn default_rtk_find_max() -> usize {
    50
}

fn default_rtk_json_depth() -> usize {
    5
}

fn default_rtk_wc_mode() -> String {
    "full".to_string()
}

fn default_rtk_git_show_revision() -> String {
    "HEAD".to_string()
}

fn default_rtk_git_log_max_count() -> usize {
    10
}

fn build_command_args(
    kind: RtkCommandKind,
    arguments: &str,
) -> Result<Vec<OsString>, FunctionCallError> {
    match kind {
        RtkCommandKind::Read => build_read_args(arguments),
        RtkCommandKind::Grep => build_grep_args(arguments),
        RtkCommandKind::Find => build_find_args(arguments),
        RtkCommandKind::Diff => build_diff_args(arguments),
        RtkCommandKind::Json => build_json_args(arguments),
        RtkCommandKind::Deps => build_deps_args(arguments),
        RtkCommandKind::Log => build_log_args(arguments),
        RtkCommandKind::Ls => build_ls_args(arguments),
        RtkCommandKind::Tree => build_tree_args(arguments),
        RtkCommandKind::Wc => build_wc_args(arguments),
        RtkCommandKind::GitStatus => build_git_status_args(arguments),
        RtkCommandKind::GitDiff => build_git_diff_args(arguments),
        RtkCommandKind::GitShow => build_git_show_args(arguments),
        RtkCommandKind::GitLog => build_git_log_args(arguments),
        RtkCommandKind::GitBranch => build_git_branch_args(arguments),
        RtkCommandKind::GitStash => build_git_stash_args(arguments),
        RtkCommandKind::GitWorktree => build_git_worktree_args(arguments),
        RtkCommandKind::Summary => build_summary_args(arguments),
        RtkCommandKind::Err => build_err_args(arguments),
    }
}

fn build_read_args(arguments: &str) -> Result<Vec<OsString>, FunctionCallError> {
    let args: RtkReadArgs = parse_arguments(arguments)?;
    let RtkReadArgs {
        path,
        level,
        max_lines,
        tail_lines,
        line_numbers,
    } = args;
    let level = level.trim().to_string();
    if level.is_empty() {
        return Err(FunctionCallError::RespondToModel(
            "level must not be empty".to_string(),
        ));
    }
    if !matches!(level.as_str(), "none" | "minimal" | "aggressive") {
        return Err(FunctionCallError::RespondToModel(
            "level must be one of: none, minimal, aggressive".to_string(),
        ));
    }
    if path.trim().is_empty() {
        return Err(FunctionCallError::RespondToModel(
            "path must not be empty".to_string(),
        ));
    }
    if max_lines == Some(0) {
        return Err(FunctionCallError::RespondToModel(
            "max_lines must be greater than zero".to_string(),
        ));
    }
    if tail_lines == Some(0) {
        return Err(FunctionCallError::RespondToModel(
            "tail_lines must be greater than zero".to_string(),
        ));
    }
    if max_lines.is_some() && tail_lines.is_some() {
        return Err(FunctionCallError::RespondToModel(
            "max_lines and tail_lines cannot both be set".to_string(),
        ));
    }

    let mut command = vec![
        OsString::from(RtkCommandKind::Read.subcommand()),
        OsString::from(path),
        OsString::from("--level"),
        OsString::from(level),
    ];
    if let Some(max_lines) = max_lines {
        command.push(OsString::from("--max-lines"));
        command.push(OsString::from(max_lines.to_string()));
    }
    if let Some(tail_lines) = tail_lines {
        command.push(OsString::from("--tail-lines"));
        command.push(OsString::from(tail_lines.to_string()));
    }
    if line_numbers {
        command.push(OsString::from("--line-numbers"));
    }
    Ok(command)
}

fn build_grep_args(arguments: &str) -> Result<Vec<OsString>, FunctionCallError> {
    let args: RtkGrepArgs = parse_arguments(arguments)?;
    if args.pattern.trim().is_empty() {
        return Err(FunctionCallError::RespondToModel(
            "pattern must not be empty".to_string(),
        ));
    }
    if args.path.trim().is_empty() {
        return Err(FunctionCallError::RespondToModel(
            "path must not be empty".to_string(),
        ));
    }
    if args.max_len == 0 {
        return Err(FunctionCallError::RespondToModel(
            "max_len must be greater than zero".to_string(),
        ));
    }
    if args.max == 0 {
        return Err(FunctionCallError::RespondToModel(
            "max must be greater than zero".to_string(),
        ));
    }

    let mut command = vec![
        OsString::from(RtkCommandKind::Grep.subcommand()),
        OsString::from(args.pattern),
        OsString::from(args.path),
        OsString::from("--max-len"),
        OsString::from(args.max_len.to_string()),
        OsString::from("--max"),
        OsString::from(args.max.to_string()),
    ];
    if args.context_only {
        command.push(OsString::from("--context-only"));
    }
    if let Some(file_type) = args.file_type.filter(|value| !value.trim().is_empty()) {
        command.push(OsString::from("--file-type"));
        command.push(OsString::from(file_type));
    }
    if args.line_numbers {
        command.push(OsString::from("--line-numbers"));
    }
    command.extend(args.extra_args.into_iter().map(OsString::from));
    Ok(command)
}

fn build_find_args(arguments: &str) -> Result<Vec<OsString>, FunctionCallError> {
    let args: RtkFindArgs = parse_arguments(arguments)?;
    if args.pattern.trim().is_empty() {
        return Err(FunctionCallError::RespondToModel(
            "pattern must not be empty".to_string(),
        ));
    }
    if args.path.trim().is_empty() {
        return Err(FunctionCallError::RespondToModel(
            "path must not be empty".to_string(),
        ));
    }
    if args.max_results == 0 {
        return Err(FunctionCallError::RespondToModel(
            "max_results must be greater than zero".to_string(),
        ));
    }

    let mut command = vec![
        OsString::from(RtkCommandKind::Find.subcommand()),
        OsString::from(args.pattern),
        OsString::from(args.path),
        OsString::from("--max"),
        OsString::from(args.max_results.to_string()),
    ];
    if let Some(file_type) = args.file_type.filter(|value| !value.trim().is_empty()) {
        command.push(OsString::from("--file-type"));
        command.push(OsString::from(file_type));
    }
    Ok(command)
}

fn build_diff_args(arguments: &str) -> Result<Vec<OsString>, FunctionCallError> {
    let args: RtkDiffArgs = parse_arguments(arguments)?;
    if args.left.trim().is_empty() || args.right.trim().is_empty() {
        return Err(FunctionCallError::RespondToModel(
            "left and right must not be empty".to_string(),
        ));
    }

    Ok(vec![
        OsString::from(RtkCommandKind::Diff.subcommand()),
        OsString::from(args.left),
        OsString::from(args.right),
    ])
}

fn build_json_args(arguments: &str) -> Result<Vec<OsString>, FunctionCallError> {
    let args: RtkJsonArgs = parse_arguments(arguments)?;
    if args.path.trim().is_empty() {
        return Err(FunctionCallError::RespondToModel(
            "path must not be empty".to_string(),
        ));
    }
    if args.depth == 0 {
        return Err(FunctionCallError::RespondToModel(
            "depth must be greater than zero".to_string(),
        ));
    }

    Ok(vec![
        OsString::from(RtkCommandKind::Json.subcommand()),
        OsString::from(args.path),
        OsString::from("--depth"),
        OsString::from(args.depth.to_string()),
    ])
}

fn build_deps_args(arguments: &str) -> Result<Vec<OsString>, FunctionCallError> {
    let args: RtkDepsArgs = parse_arguments(arguments)?;
    if args.path.trim().is_empty() {
        return Err(FunctionCallError::RespondToModel(
            "path must not be empty".to_string(),
        ));
    }

    Ok(vec![
        OsString::from(RtkCommandKind::Deps.subcommand()),
        OsString::from(args.path),
    ])
}

fn build_log_args(arguments: &str) -> Result<Vec<OsString>, FunctionCallError> {
    let args: RtkLogArgs = parse_arguments(arguments)?;
    if args.path.trim().is_empty() {
        return Err(FunctionCallError::RespondToModel(
            "path must not be empty".to_string(),
        ));
    }

    Ok(vec![
        OsString::from(RtkCommandKind::Log.subcommand()),
        OsString::from(args.path),
    ])
}

fn build_ls_args(arguments: &str) -> Result<Vec<OsString>, FunctionCallError> {
    let args: RtkLsArgs = parse_arguments(arguments)?;
    if args.path.trim().is_empty() {
        return Err(FunctionCallError::RespondToModel(
            "path must not be empty".to_string(),
        ));
    }

    let mut command = vec![OsString::from(RtkCommandKind::Ls.subcommand())];
    if args.all {
        command.push(OsString::from("--all"));
    }
    command.push(OsString::from(args.path));
    Ok(command)
}

fn build_tree_args(arguments: &str) -> Result<Vec<OsString>, FunctionCallError> {
    let args: RtkTreeArgs = parse_arguments(arguments)?;
    if args.path.trim().is_empty() {
        return Err(FunctionCallError::RespondToModel(
            "path must not be empty".to_string(),
        ));
    }
    if args.max_depth == Some(0) {
        return Err(FunctionCallError::RespondToModel(
            "max_depth must be greater than zero".to_string(),
        ));
    }

    let mut command = vec![OsString::from(RtkCommandKind::Tree.subcommand())];
    if args.all {
        command.push(OsString::from("--all"));
    }
    if let Some(max_depth) = args.max_depth {
        command.push(OsString::from("-L"));
        command.push(OsString::from(max_depth.to_string()));
    }
    command.push(OsString::from(args.path));
    Ok(command)
}

fn build_wc_args(arguments: &str) -> Result<Vec<OsString>, FunctionCallError> {
    let args: RtkWcArgs = parse_arguments(arguments)?;
    if args.path.trim().is_empty() {
        return Err(FunctionCallError::RespondToModel(
            "path must not be empty".to_string(),
        ));
    }

    let mode = args.mode.trim().to_string();
    if mode.is_empty() {
        return Err(FunctionCallError::RespondToModel(
            "mode must not be empty".to_string(),
        ));
    }

    let mut command = vec![OsString::from(RtkCommandKind::Wc.subcommand())];
    match mode.as_str() {
        "full" => {}
        "lines" => command.push(OsString::from("-l")),
        "words" => command.push(OsString::from("-w")),
        "bytes" => command.push(OsString::from("-c")),
        "chars" => command.push(OsString::from("-m")),
        _ => {
            return Err(FunctionCallError::RespondToModel(
                "mode must be one of: full, lines, words, bytes, chars".to_string(),
            ));
        }
    }
    command.push(OsString::from(args.path));
    Ok(command)
}

fn build_git_status_args(arguments: &str) -> Result<Vec<OsString>, FunctionCallError> {
    let args: RtkGitStatusArgs = parse_arguments(arguments)?;
    let mut command = vec![
        OsString::from(RtkCommandKind::GitStatus.subcommand()),
        OsString::from("status"),
    ];

    if let Some(path) = args.path {
        let path = path.trim().to_string();
        if path.is_empty() {
            return Err(FunctionCallError::RespondToModel(
                "path must not be empty when provided".to_string(),
            ));
        }
        command.push(OsString::from("--"));
        command.push(OsString::from(path));
    }

    Ok(command)
}

fn build_git_diff_args(arguments: &str) -> Result<Vec<OsString>, FunctionCallError> {
    let args: RtkGitDiffArgs = parse_arguments(arguments)?;
    let mut command = vec![
        OsString::from(RtkCommandKind::GitDiff.subcommand()),
        OsString::from("diff"),
    ];

    if args.cached {
        command.push(OsString::from("--cached"));
    }

    if let Some(target) = args.target {
        let target = target.trim().to_string();
        if target.is_empty() {
            return Err(FunctionCallError::RespondToModel(
                "target must not be empty when provided".to_string(),
            ));
        }
        command.push(OsString::from(target));
    }

    if let Some(path) = args.path {
        let path = path.trim().to_string();
        if path.is_empty() {
            return Err(FunctionCallError::RespondToModel(
                "path must not be empty when provided".to_string(),
            ));
        }
        command.push(OsString::from("--"));
        command.push(OsString::from(path));
    }

    Ok(command)
}

fn build_git_show_args(arguments: &str) -> Result<Vec<OsString>, FunctionCallError> {
    let args: RtkGitShowArgs = parse_arguments(arguments)?;
    let revision = args.revision.trim().to_string();
    if revision.is_empty() {
        return Err(FunctionCallError::RespondToModel(
            "revision must not be empty".to_string(),
        ));
    }

    Ok(vec![
        OsString::from(RtkCommandKind::GitShow.subcommand()),
        OsString::from("show"),
        OsString::from(revision),
    ])
}

fn build_git_log_args(arguments: &str) -> Result<Vec<OsString>, FunctionCallError> {
    let args: RtkGitLogArgs = parse_arguments(arguments)?;
    if args.max_count == 0 {
        return Err(FunctionCallError::RespondToModel(
            "max_count must be greater than zero".to_string(),
        ));
    }

    let mut command = vec![
        OsString::from(RtkCommandKind::GitLog.subcommand()),
        OsString::from("log"),
        OsString::from("-n"),
        OsString::from(args.max_count.to_string()),
    ];

    if let Some(revision_range) = args.revision_range {
        let revision_range = revision_range.trim().to_string();
        if revision_range.is_empty() {
            return Err(FunctionCallError::RespondToModel(
                "revision_range must not be empty when provided".to_string(),
            ));
        }
        command.push(OsString::from(revision_range));
    }

    Ok(command)
}

fn build_git_branch_args(arguments: &str) -> Result<Vec<OsString>, FunctionCallError> {
    let args: RtkGitBranchArgs = parse_arguments(arguments)?;
    if args.all && args.remotes {
        return Err(FunctionCallError::RespondToModel(
            "all and remotes cannot both be true".to_string(),
        ));
    }
    if args.merged && args.no_merged {
        return Err(FunctionCallError::RespondToModel(
            "merged and no_merged cannot both be true".to_string(),
        ));
    }

    let mut command = vec![
        OsString::from(RtkCommandKind::GitBranch.subcommand()),
        OsString::from("branch"),
    ];

    if args.all {
        command.push(OsString::from("--all"));
    } else if args.remotes {
        command.push(OsString::from("--remotes"));
    }

    if let Some(contains) = args.contains {
        let contains = contains.trim().to_string();
        if contains.is_empty() {
            return Err(FunctionCallError::RespondToModel(
                "contains must not be empty when provided".to_string(),
            ));
        }
        command.push(OsString::from("--contains"));
        command.push(OsString::from(contains));
    }

    if args.merged {
        command.push(OsString::from("--merged"));
    }
    if args.no_merged {
        command.push(OsString::from("--no-merged"));
    }

    Ok(command)
}

fn build_git_stash_args(arguments: &str) -> Result<Vec<OsString>, FunctionCallError> {
    let args: RtkGitStashArgs = parse_arguments(arguments)?;
    if args.max_count == 0 {
        return Err(FunctionCallError::RespondToModel(
            "max_count must be greater than zero".to_string(),
        ));
    }

    Ok(vec![
        OsString::from(RtkCommandKind::GitStash.subcommand()),
        OsString::from("stash"),
        OsString::from("list"),
        OsString::from("-n"),
        OsString::from(args.max_count.to_string()),
    ])
}

fn build_git_worktree_args(arguments: &str) -> Result<Vec<OsString>, FunctionCallError> {
    let _: serde_json::Value = parse_arguments(arguments)?;

    Ok(vec![
        OsString::from(RtkCommandKind::GitWorktree.subcommand()),
        OsString::from("worktree"),
        OsString::from("list"),
    ])
}

fn build_summary_args(arguments: &str) -> Result<Vec<OsString>, FunctionCallError> {
    let args: RtkCommandStringArgs = parse_arguments(arguments)?;
    let command = args.command.trim();
    if command.is_empty() {
        return Err(FunctionCallError::RespondToModel(
            "command must not be empty".to_string(),
        ));
    }

    Ok(vec![
        OsString::from(RtkCommandKind::Summary.subcommand()),
        OsString::from(command),
    ])
}

fn build_err_args(arguments: &str) -> Result<Vec<OsString>, FunctionCallError> {
    let args: RtkCommandStringArgs = parse_arguments(arguments)?;
    let command = args.command.trim();
    if command.is_empty() {
        return Err(FunctionCallError::RespondToModel(
            "command must not be empty".to_string(),
        ));
    }

    #[cfg(windows)]
    let shell_prefix = ["cmd", "/C"];
    #[cfg(not(windows))]
    let shell_prefix = ["sh", "-c"];

    let mut result = vec![OsString::from(RtkCommandKind::Err.subcommand())];
    result.extend(shell_prefix.into_iter().map(OsString::from));
    result.push(OsString::from(command));
    Ok(result)
}

fn discover_rtk_executable() -> Result<RtkExecutable, FunctionCallError> {
    if let Ok(current_exe) = std::env::current_exe()
        && let Some(executable) = current_executable_rtk_launcher(current_exe.as_path())
    {
        return Ok(executable);
    }

    if let Ok(current_exe) = std::env::current_exe()
        && let Some(parent) = current_exe.parent()
        && let Some(candidate) = sibling_rtk_launcher(parent)
    {
        return Ok(candidate);
    }

    if let Ok(program) = which::which("codex") {
        return Ok(RtkExecutable {
            program,
            prefix_args: vec![OsString::from("rtk")],
        });
    }

    if let Ok(program) = which::which("rtk") {
        return Ok(RtkExecutable {
            program,
            prefix_args: Vec::new(),
        });
    }

    Err(FunctionCallError::RespondToModel(
        "unable to locate RTK executable; expected `codex` or `rtk` on PATH".to_string(),
    ))
}

fn executable_stem_is_codex(path: &Path) -> bool {
    path.file_stem()
        .and_then(OsStr::to_str)
        .is_some_and(|stem| stem == "codex")
}

fn executable_stem_is_rtk(path: &Path) -> bool {
    path.file_stem()
        .and_then(OsStr::to_str)
        .is_some_and(|stem| stem == "rtk")
}

fn current_executable_rtk_launcher(path: &Path) -> Option<RtkExecutable> {
    if executable_stem_is_codex(path) {
        return Some(RtkExecutable {
            program: path.to_path_buf(),
            prefix_args: vec![OsString::from("rtk")],
        });
    }

    executable_stem_is_rtk(path).then(|| RtkExecutable {
        program: path.to_path_buf(),
        prefix_args: Vec::new(),
    })
}

fn sibling_rtk_launcher(parent: &Path) -> Option<RtkExecutable> {
    for (name, prefix_args) in [
        (
            if cfg!(windows) { "codex.exe" } else { "codex" },
            vec![OsString::from("rtk")],
        ),
        (if cfg!(windows) { "rtk.exe" } else { "rtk" }, Vec::new()),
    ] {
        let candidate = parent.join(name);
        if candidate.is_file() {
            return Some(RtkExecutable {
                program: candidate,
                prefix_args,
            });
        }
    }

    None
}

async fn run_rtk_command(
    kind: RtkCommandKind,
    executable: &RtkExecutable,
    command_args: &[OsString],
    cwd: &Path,
) -> Result<FunctionToolOutput, FunctionCallError> {
    let mut command = Command::new(&executable.program);
    command.current_dir(cwd);
    command.args(&executable.prefix_args);
    command.args(command_args);

    let output = timeout(RTK_TIMEOUT, command.output())
        .await
        .map_err(|_| {
            FunctionCallError::RespondToModel("rtk command timed out after 30 seconds".to_string())
        })?
        .map_err(|err| FunctionCallError::RespondToModel(format!("failed to launch rtk: {err}")))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut content = stdout.trim().to_string();
    if content.is_empty() {
        content = stderr.trim().to_string();
    } else if !stderr.trim().is_empty() {
        content.push_str("\n\n");
        content.push_str(stderr.trim());
    }
    if content.is_empty() {
        content = "RTK command completed with no output.".to_string();
    }

    Ok(FunctionToolOutput::from_text(
        content,
        Some(rtk_command_succeeded(kind, output.status, stdout.as_ref())),
    ))
}

fn rtk_command_succeeded(
    kind: RtkCommandKind,
    status: std::process::ExitStatus,
    stdout: &str,
) -> bool {
    if matches!(kind, RtkCommandKind::Summary) {
        return stdout.trim_start().starts_with("✅ Command:");
    }

    if status.success() {
        return true;
    }

    matches!(kind, RtkCommandKind::Grep)
        && status.code() == Some(1)
        && stdout.trim_start().starts_with("🔍 0 for ")
}

#[cfg(test)]
#[path = "rtk_tests.rs"]
mod tests;
