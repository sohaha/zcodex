use anyhow::Context;
use anyhow::Result;
use clap::Args;
use clap::Parser;
use clap::Subcommand;
use serde_json::Value as JsonValue;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

const DEFAULT_OUTPUT_LINE_LIMIT: usize = 200;
const DEFAULT_TAIL_LINE_LIMIT: usize = 40;
const DEFAULT_READ_LINE_LIMIT: usize = 200;
const DEFAULT_JSON_DEPTH: usize = 5;
const DEFAULT_DEPS_LIMIT: usize = 20;
const DEFAULT_LOG_MATCH_LIMIT: usize = 80;
const RTK_ALIAS_NAME: &str = "rtk";

#[derive(Debug, Parser)]
pub struct RtkCli {
    #[command(subcommand)]
    pub command: RtkCommand,
}

#[derive(Debug, Subcommand)]
pub enum RtkCommand {
    /// Git commands with compact output.
    Git(TrailingArgs),

    /// Ripgrep with compact output.
    Rg(TrailingArgs),

    /// Grep with compact output.
    Grep(TrailingArgs),

    /// Read a file with bounded output.
    Read(ReadArgs),

    /// List directory contents with compact output.
    Ls(TrailingArgs),

    /// Show directory tree with compact output.
    Tree(TrailingArgs),

    /// Run find with bounded output.
    Find(TrailingArgs),

    /// Show JSON structure without leaf values.
    Json(JsonArgs),

    /// Run a command and keep log-worthy lines.
    Log(CommandWrapperArgs),

    /// Run a command and keep errors/warnings.
    Err(CommandWrapperArgs),

    /// Run a test command and keep failures plus summary.
    Test(CommandWrapperArgs),

    /// Show environment variables with sensitive values masked.
    Env(EnvArgs),

    /// Summarize common dependency manifests.
    Deps(DepsArgs),
}

#[derive(Debug, Args)]
pub struct TrailingArgs {
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub args: Vec<String>,
}

#[derive(Debug, Args)]
pub struct CommandWrapperArgs {
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub command: Vec<String>,
}

#[derive(Debug, Args)]
pub struct ReadArgs {
    pub file: PathBuf,

    #[arg(short, long, default_value_t = DEFAULT_READ_LINE_LIMIT)]
    pub max_lines: usize,

    #[arg(short = 'n', long, default_value_t = false)]
    pub line_numbers: bool,
}

#[derive(Debug, Args)]
pub struct JsonArgs {
    pub file: PathBuf,

    #[arg(short, long, default_value_t = DEFAULT_JSON_DEPTH)]
    pub depth: usize,
}

#[derive(Debug, Args)]
pub struct EnvArgs {
    #[arg(short, long)]
    pub filter: Option<String>,

    #[arg(long, default_value_t = false)]
    pub show_all: bool,
}

#[derive(Debug, Args)]
pub struct DepsArgs {
    #[arg(default_value = ".")]
    pub path: PathBuf,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FilterMode {
    Generic,
    Search,
    ErrorOnly,
    Test,
    Log,
}

struct CapturedOutput {
    status: std::process::ExitStatus,
    stdout: String,
    stderr: String,
}

pub fn alias_name() -> &'static str {
    RTK_ALIAS_NAME
}

pub fn is_alias_invocation(argv0: &OsString) -> bool {
    Path::new(argv0)
        .file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|file_name| file_name == RTK_ALIAS_NAME)
}

pub fn run(cli: RtkCli) -> Result<()> {
    match cli.command {
        RtkCommand::Git(args) => run_git(args.args),
        RtkCommand::Rg(args) => run_search("rg", args.args),
        RtkCommand::Grep(args) => run_search("grep", args.args),
        RtkCommand::Read(args) => {
            print_output(render_read(args)?);
            Ok(())
        }
        RtkCommand::Ls(args) => run_ls(args.args),
        RtkCommand::Tree(args) => run_external("tree", args.args, FilterMode::Generic),
        RtkCommand::Find(args) => run_external("find", args.args, FilterMode::Search),
        RtkCommand::Json(args) => {
            print_output(render_json(args)?);
            Ok(())
        }
        RtkCommand::Log(args) => run_wrapped_command(args.command, FilterMode::Log),
        RtkCommand::Err(args) => run_wrapped_command(args.command, FilterMode::ErrorOnly),
        RtkCommand::Test(args) => run_wrapped_command(args.command, FilterMode::Test),
        RtkCommand::Env(args) => {
            print_output(render_env(args));
            Ok(())
        }
        RtkCommand::Deps(args) => {
            print_output(render_deps(args.path)?);
            Ok(())
        }
    }
}

fn run_git(mut args: Vec<String>) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("status") if !contains_any(&args, &["--short", "--porcelain"]) => {
            args.push("--short".to_string());
        }
        Some("diff") if !contains_any(&args, &["--stat", "--name-only", "--shortstat"]) => {
            args.push("--stat".to_string());
        }
        Some("log") if !contains_any(&args, &["--oneline", "--stat", "--pretty"]) => {
            args.push("--oneline".to_string());
            if !contains_any(&args, &["-n", "--max-count"]) {
                args.push("-n".to_string());
                args.push("20".to_string());
            }
        }
        _ => {}
    }

    run_external("git", args, FilterMode::Generic)
}

fn run_search(program: &str, mut args: Vec<String>) -> Result<()> {
    if program == "rg" {
        if !contains_any(&args, &["--line-number", "-n"]) {
            args.push("--line-number".to_string());
        }
        if !contains_any(&args, &["--with-filename", "-H", "--no-filename", "-I"]) {
            args.push("--with-filename".to_string());
        }
        if !contains_any(&args, &["--color", "--color=never", "--no-color"]) {
            args.push("--color=never".to_string());
        }
    } else if !contains_any(&args, &["-n", "--line-number"]) {
        args.insert(0, "-n".to_string());
        args.insert(1, "-H".to_string());
    }

    run_external(program, args, FilterMode::Search)
}

fn run_ls(mut args: Vec<String>) -> Result<()> {
    if !contains_any(&args, &["-1", "-l", "-m", "-x", "-C"]) {
        args.insert(0, "-1".to_string());
    }
    run_external("ls", args, FilterMode::Generic)
}

fn run_wrapped_command(command: Vec<String>, mode: FilterMode) -> Result<()> {
    let (program, args) = command
        .split_first()
        .context("rtk wrapper commands require a program to run")?;
    run_external(program, args.to_vec(), mode)
}

fn run_external(program: &str, args: Vec<String>, mode: FilterMode) -> Result<()> {
    let output = capture_command(program, &args)?;
    let stdout = filter_output(&output.stdout, mode);
    let stderr = filter_output(&output.stderr, mode);

    if output.status.success() {
        if !stdout.is_empty() {
            print_output(stdout);
        }
        if !stderr.is_empty() {
            eprint_output(stderr);
        }
        return Ok(());
    }

    if !stderr.is_empty() {
        eprint_output(stderr);
    } else if !stdout.is_empty() {
        eprint_output(stdout);
    }

    handle_exit_status(output.status)
}

fn capture_command(program: &str, args: &[String]) -> Result<CapturedOutput> {
    let output = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("failed to run `{program}`"))?;

    Ok(CapturedOutput {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

fn filter_output(text: &str, mode: FilterMode) -> String {
    match mode {
        FilterMode::Generic => normalize_and_limit(text, DEFAULT_OUTPUT_LINE_LIMIT),
        FilterMode::Search => normalize_and_limit(text, DEFAULT_OUTPUT_LINE_LIMIT),
        FilterMode::ErrorOnly => {
            select_interesting_lines(text, &error_keywords(), DEFAULT_TAIL_LINE_LIMIT)
        }
        FilterMode::Test => select_test_lines(text),
        FilterMode::Log => select_interesting_lines(text, &log_keywords(), DEFAULT_LOG_MATCH_LIMIT),
    }
}

fn normalize_and_limit(text: &str, max_lines: usize) -> String {
    let lines = cleaned_lines(text);
    join_with_truncation(lines, max_lines)
}

fn select_interesting_lines(text: &str, keywords: &[&str], max_lines: usize) -> String {
    let lines = cleaned_lines(text);
    if lines.is_empty() {
        return String::new();
    }

    let matching_indexes = lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| line_matches_any(line, keywords).then_some(index))
        .collect::<Vec<_>>();

    if matching_indexes.is_empty() {
        return join_with_truncation(
            last_n_lines(&lines, DEFAULT_TAIL_LINE_LIMIT),
            DEFAULT_TAIL_LINE_LIMIT,
        );
    }

    let mut indexes = BTreeSet::new();
    for index in matching_indexes {
        let start = index.saturating_sub(1);
        let end = (index + 1).min(lines.len().saturating_sub(1));
        indexes.extend(start..=end);
    }

    let selected = indexes
        .into_iter()
        .map(|index| lines[index].clone())
        .collect::<Vec<_>>();

    join_with_truncation(selected, max_lines)
}

fn select_test_lines(text: &str) -> String {
    let lines = cleaned_lines(text);
    if lines.is_empty() {
        return String::new();
    }

    let keywords = [
        "fail",
        "failed",
        "error",
        "panic",
        "assertion",
        "test result",
        "failures:",
        "traceback",
    ];
    let mut selected = Vec::new();
    for line in &lines {
        if line_matches_any(line, &keywords) {
            selected.push(line.clone());
        }
    }

    if selected.is_empty() {
        selected.extend(last_n_lines(&lines, DEFAULT_TAIL_LINE_LIMIT));
    } else {
        let tail = last_n_lines(&lines, 10);
        for line in tail {
            if !selected.contains(&line) {
                selected.push(line);
            }
        }
    }

    join_with_truncation(selected, DEFAULT_LOG_MATCH_LIMIT)
}

fn render_read(args: ReadArgs) -> Result<String> {
    let content = std::fs::read_to_string(&args.file)
        .with_context(|| format!("failed to read {}", args.file.display()))?;
    let lines = content
        .lines()
        .take(args.max_lines)
        .enumerate()
        .map(|(index, line)| {
            if args.line_numbers {
                format!("{:>4} {line}", index + 1)
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>();

    let mut rendered = lines.join("\n");
    if content.lines().count() > args.max_lines {
        rendered.push_str(&format!("\n... truncated to {} lines", args.max_lines));
    }
    Ok(rendered)
}

fn render_json(args: JsonArgs) -> Result<String> {
    let content = std::fs::read_to_string(&args.file)
        .with_context(|| format!("failed to read {}", args.file.display()))?;
    let value: JsonValue = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse JSON from {}", args.file.display()))?;

    let mut lines = Vec::new();
    describe_json("$", &value, 0, args.depth, &mut lines);
    Ok(lines.join("\n"))
}

fn render_env(args: EnvArgs) -> String {
    render_env_pairs(std::env::vars(), args)
}

fn render_deps(path: PathBuf) -> Result<String> {
    let mut sections = Vec::new();

    let cargo_toml = path.join("Cargo.toml");
    if cargo_toml.exists() {
        sections.push(render_cargo_deps(&cargo_toml)?);
    }

    let package_json = path.join("package.json");
    if package_json.exists() {
        sections.push(render_package_json_deps(&package_json)?);
    }

    let pyproject_toml = path.join("pyproject.toml");
    if pyproject_toml.exists() {
        sections.push(render_pyproject_deps(&pyproject_toml)?);
    }

    if sections.is_empty() {
        anyhow::bail!(
            "no supported dependency manifest found in {}",
            path.display()
        );
    }

    Ok(sections.join("\n\n"))
}

fn render_cargo_deps(path: &Path) -> Result<String> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let value: toml::Value = toml::from_str(&content)
        .with_context(|| format!("failed to parse TOML from {}", path.display()))?;

    let package_name = value
        .get("package")
        .and_then(|package| package.get("name"))
        .and_then(toml::Value::as_str)
        .unwrap_or("<workspace>");

    let dependencies = extract_toml_table_keys(&value, "dependencies");
    let dev_dependencies = extract_toml_table_keys(&value, "dev-dependencies");
    let workspace_dependencies = value
        .get("workspace")
        .and_then(|workspace| workspace.get("dependencies"))
        .and_then(toml::Value::as_table)
        .map(|table| table.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();

    let mut lines = vec![format!("cargo: {package_name}")];
    if !dependencies.is_empty() {
        lines.push(format!(
            "dependencies ({}): {}",
            dependencies.len(),
            summarize_names(&dependencies)
        ));
    }
    if !dev_dependencies.is_empty() {
        lines.push(format!(
            "dev-dependencies ({}): {}",
            dev_dependencies.len(),
            summarize_names(&dev_dependencies)
        ));
    }
    if !workspace_dependencies.is_empty() {
        lines.push(format!(
            "workspace dependencies ({}): {}",
            workspace_dependencies.len(),
            summarize_names(&workspace_dependencies)
        ));
    }

    Ok(lines.join("\n"))
}

fn render_package_json_deps(path: &Path) -> Result<String> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let value: JsonValue = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse JSON from {}", path.display()))?;

    let name = value
        .get("name")
        .and_then(JsonValue::as_str)
        .unwrap_or("<package>");
    let dependencies = extract_json_object_keys(&value, "dependencies");
    let dev_dependencies = extract_json_object_keys(&value, "devDependencies");
    let scripts = extract_json_object_keys(&value, "scripts");

    let mut lines = vec![format!("npm: {name}")];
    if !scripts.is_empty() {
        lines.push(format!(
            "scripts ({}): {}",
            scripts.len(),
            summarize_names(&scripts)
        ));
    }
    if !dependencies.is_empty() {
        lines.push(format!(
            "dependencies ({}): {}",
            dependencies.len(),
            summarize_names(&dependencies)
        ));
    }
    if !dev_dependencies.is_empty() {
        lines.push(format!(
            "devDependencies ({}): {}",
            dev_dependencies.len(),
            summarize_names(&dev_dependencies)
        ));
    }

    Ok(lines.join("\n"))
}

fn render_pyproject_deps(path: &Path) -> Result<String> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let value: toml::Value = toml::from_str(&content)
        .with_context(|| format!("failed to parse TOML from {}", path.display()))?;

    let project_name = value
        .get("project")
        .and_then(|project| project.get("name"))
        .and_then(toml::Value::as_str)
        .or_else(|| {
            value
                .get("tool")
                .and_then(|tool| tool.get("poetry"))
                .and_then(|poetry| poetry.get("name"))
                .and_then(toml::Value::as_str)
        })
        .unwrap_or("<python-project>");

    let project_dependencies = value
        .get("project")
        .and_then(|project| project.get("dependencies"))
        .and_then(toml::Value::as_array)
        .map(|deps| {
            deps.iter()
                .filter_map(toml::Value::as_str)
                .map(package_name_from_requirement)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let poetry_dependencies = value
        .get("tool")
        .and_then(|tool| tool.get("poetry"))
        .and_then(|poetry| poetry.get("dependencies"))
        .and_then(toml::Value::as_table)
        .map(|table| {
            table
                .keys()
                .filter(|name| name.as_str() != "python")
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let mut lines = vec![format!("python: {project_name}")];
    if !project_dependencies.is_empty() {
        lines.push(format!(
            "project dependencies ({}): {}",
            project_dependencies.len(),
            summarize_names(&project_dependencies)
        ));
    }
    if !poetry_dependencies.is_empty() {
        lines.push(format!(
            "poetry dependencies ({}): {}",
            poetry_dependencies.len(),
            summarize_names(&poetry_dependencies)
        ));
    }

    Ok(lines.join("\n"))
}

fn describe_json(
    path: &str,
    value: &JsonValue,
    depth: usize,
    max_depth: usize,
    out: &mut Vec<String>,
) {
    let indent = "  ".repeat(depth);
    match value {
        JsonValue::Object(map) => {
            out.push(format!("{indent}{path}: object({})", map.len()));
            if depth >= max_depth {
                return;
            }
            for (key, child) in map {
                describe_json(key, child, depth + 1, max_depth, out);
            }
        }
        JsonValue::Array(items) => {
            out.push(format!("{indent}{path}: array({})", items.len()));
            if depth >= max_depth {
                return;
            }
            if let Some(first) = items.first() {
                describe_json("[0]", first, depth + 1, max_depth, out);
            }
        }
        JsonValue::String(_) => out.push(format!("{indent}{path}: string")),
        JsonValue::Number(_) => out.push(format!("{indent}{path}: number")),
        JsonValue::Bool(_) => out.push(format!("{indent}{path}: bool")),
        JsonValue::Null => out.push(format!("{indent}{path}: null")),
    }
}

fn extract_toml_table_keys(value: &toml::Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(toml::Value::as_table)
        .map(|table| table.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default()
}

fn extract_json_object_keys(value: &JsonValue, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(JsonValue::as_object)
        .map(|map| map.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default()
}

fn summarize_names(names: &[String]) -> String {
    let mut sorted = names.to_vec();
    sorted.sort();
    let extra = sorted.len().saturating_sub(DEFAULT_DEPS_LIMIT);
    let mut rendered = sorted
        .into_iter()
        .take(DEFAULT_DEPS_LIMIT)
        .collect::<Vec<_>>()
        .join(", ");
    if extra > 0 {
        rendered.push_str(&format!(", ... +{extra} more"));
    }
    rendered
}

fn package_name_from_requirement(requirement: &str) -> String {
    let stop_chars = [' ', '<', '>', '=', '!', '~', '(', '['];
    let end = requirement.find(stop_chars).unwrap_or(requirement.len());
    requirement[..end].to_string()
}

fn render_env_pairs<I>(vars: I, args: EnvArgs) -> String
where
    I: IntoIterator<Item = (String, String)>,
{
    let filter = args.filter.as_ref().map(|value| value.to_ascii_lowercase());
    let mut sorted = BTreeMap::new();
    for (key, value) in vars {
        if filter
            .as_ref()
            .is_none_or(|pattern| key.to_ascii_lowercase().contains(pattern))
        {
            sorted.insert(key, value);
        }
    }

    sorted
        .into_iter()
        .map(|(key, value)| {
            let rendered = if args.show_all || !looks_sensitive(&key) {
                value
            } else {
                "*****".to_string()
            };
            format!("{key}={rendered}")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn cleaned_lines(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut last_was_blank = false;
    for raw in text.lines() {
        let line = raw.trim_end().to_string();
        let is_blank = line.is_empty();
        if is_blank && last_was_blank {
            continue;
        }
        last_was_blank = is_blank;
        lines.push(line);
    }

    while matches!(lines.last(), Some(line) if line.is_empty()) {
        lines.pop();
    }

    lines
}

fn join_with_truncation(lines: Vec<String>, max_lines: usize) -> String {
    let total = lines.len();
    let mut rendered = lines.into_iter().take(max_lines).collect::<Vec<_>>();
    if total > max_lines {
        rendered.push(format!("... truncated {} lines", total - max_lines));
    }
    rendered.join("\n")
}

fn last_n_lines(lines: &[String], n: usize) -> Vec<String> {
    lines
        .iter()
        .skip(lines.len().saturating_sub(n))
        .cloned()
        .collect::<Vec<_>>()
}

fn line_matches_any(line: &str, keywords: &[&str]) -> bool {
    let lowercase = line.to_ascii_lowercase();
    keywords.iter().any(|keyword| lowercase.contains(keyword))
}

fn error_keywords() -> [&'static str; 9] {
    [
        "error",
        "warning",
        "failed",
        "panic",
        "traceback",
        "exception",
        "caused by",
        "fatal",
        "denied",
    ]
}

fn log_keywords() -> [&'static str; 11] {
    [
        "error",
        "warning",
        "warn",
        "panic",
        "failed",
        "timeout",
        "exception",
        "traceback",
        "denied",
        "refused",
        "killed",
    ]
}

fn looks_sensitive(key: &str) -> bool {
    let uppercase = key.to_ascii_uppercase();
    let allowlist = ["PATH", "PWD", "HOME", "SHELL", "TERM"];
    if allowlist.contains(&uppercase.as_str()) {
        return false;
    }

    [
        "TOKEN",
        "SECRET",
        "PASSWORD",
        "PASS",
        "KEY",
        "CREDENTIAL",
        "COOKIE",
    ]
    .iter()
    .any(|pattern| uppercase.contains(pattern))
}

fn contains_any(args: &[String], candidates: &[&str]) -> bool {
    args.iter().any(|arg| {
        candidates
            .iter()
            .any(|candidate| arg == candidate || arg.starts_with(candidate))
    })
}

fn print_output(text: String) {
    if !text.is_empty() {
        println!("{text}");
    }
}

fn eprint_output(text: String) {
    if !text.is_empty() {
        eprintln!("{text}");
    }
}

#[cfg(unix)]
fn handle_exit_status(status: std::process::ExitStatus) -> ! {
    use std::os::unix::process::ExitStatusExt;

    if let Some(code) = status.code() {
        std::process::exit(code);
    } else if let Some(signal) = status.signal() {
        std::process::exit(128 + signal);
    } else {
        std::process::exit(1);
    }
}

#[cfg(windows)]
fn handle_exit_status(status: std::process::ExitStatus) -> ! {
    if let Some(code) = status.code() {
        std::process::exit(code);
    } else {
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn detects_alias_from_argv0() {
        assert!(is_alias_invocation(&OsString::from("/tmp/rtk")));
        assert!(!is_alias_invocation(&OsString::from("/tmp/codex")));
    }

    #[test]
    fn masks_sensitive_env_values() {
        let rendered = render_env_pairs(
            [("TEST_API_TOKEN".to_string(), "secret-token".to_string())],
            EnvArgs {
                filter: Some("TEST_API_TOKEN".to_string()),
                show_all: false,
            },
        );
        assert_eq!(rendered, "TEST_API_TOKEN=*****");
    }

    #[test]
    fn json_structure_hides_leaf_values() {
        let file = tempfile::NamedTempFile::new().expect("tempfile");
        std::fs::write(
            file.path(),
            r#"{"user":{"name":"alice","roles":["admin"]}}"#,
        )
        .expect("write json");

        let rendered = render_json(JsonArgs {
            file: file.path().to_path_buf(),
            depth: 4,
        })
        .expect("render json");

        assert!(rendered.contains("$: object(1)"));
        assert!(rendered.contains("name: string"));
        assert!(!rendered.contains("alice"));
    }

    #[test]
    fn test_filter_keeps_failure_summary() {
        let filtered = select_test_lines(
            "running 2 tests\nfoo ... ok\nbar ... FAILED\n\nfailures:\nbar\n\ntest result: FAILED. 1 passed; 1 failed\n",
        );

        assert!(filtered.contains("FAILED"));
        assert!(filtered.contains("test result: FAILED"));
    }
}
