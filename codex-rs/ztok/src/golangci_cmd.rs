use crate::tracking;
use crate::utils::resolved_command;
use crate::utils::truncate;
use anyhow::Context;
use anyhow::Result;
use serde::Deserialize;
use std::collections::HashMap;
use std::ffi::OsString;

const GOLANGCI_SUBCOMMANDS: &[&str] = &[
    "cache",
    "completion",
    "config",
    "custom",
    "fmt",
    "formatters",
    "help",
    "linters",
    "migrate",
    "run",
    "version",
];

const GLOBAL_FLAGS_WITH_VALUE: &[&str] = &[
    "-c",
    "--color",
    "--config",
    "--cpu-profile-path",
    "--mem-profile-path",
    "--trace-path",
];

#[derive(Debug, PartialEq, Eq)]
struct RunInvocation {
    global_args: Vec<String>,
    run_args: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
enum Invocation {
    FilteredRun(RunInvocation),
    Passthrough,
}

#[derive(Debug, Deserialize)]
struct Position {
    #[serde(rename = "Filename")]
    filename: String,
    #[serde(rename = "Line")]
    #[allow(dead_code)]
    line: usize,
    #[serde(rename = "Column")]
    #[allow(dead_code)]
    column: usize,
}

#[derive(Debug, Deserialize)]
struct Issue {
    #[serde(rename = "FromLinter")]
    from_linter: String,
    #[serde(rename = "Text")]
    #[allow(dead_code)]
    text: String,
    #[serde(rename = "Pos")]
    pos: Position,
}

#[derive(Debug, Deserialize)]
struct GolangciOutput {
    #[serde(rename = "Issues")]
    issues: Vec<Issue>,
}

fn parse_major_version(version_output: &str) -> u32 {
    for word in version_output.split_whitespace() {
        if let Some(major) = word
            .split('.')
            .next()
            .and_then(|value| value.parse::<u32>().ok())
            && word.contains('.')
        {
            return major;
        }
    }

    1
}

fn detect_major_version() -> u32 {
    match resolved_command("golangci-lint").arg("--version").output() {
        Ok(output) => {
            let stdout = crate::utils::decode_output(&output.stdout);
            let stderr = crate::utils::decode_output(&output.stderr);
            let version_text = if stdout.trim().is_empty() {
                stderr
            } else {
                stdout
            };
            parse_major_version(&version_text)
        }
        Err(_) => 1,
    }
}

pub fn run(args: &[String], verbose: u8) -> Result<()> {
    match classify_invocation(args) {
        Invocation::FilteredRun(invocation) => run_filtered(args, &invocation, verbose),
        Invocation::Passthrough => run_passthrough(args, verbose),
    }
}

fn run_filtered(original_args: &[String], invocation: &RunInvocation, verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();
    let version = detect_major_version();
    let filtered_args = build_filtered_args(invocation, version);

    let mut cmd = resolved_command("golangci-lint");
    for arg in &filtered_args {
        cmd.arg(arg);
    }

    if verbose > 0 {
        eprintln!("运行：golangci-lint {}", filtered_args.join(" "));
    }

    let output = cmd.output().context(
        "运行 golangci-lint 失败。请确认已安装：go install github.com/golangci/golangci-lint/cmd/golangci-lint@latest",
    )?;

    let stdout = crate::utils::decode_output(&output.stdout);
    let stderr = crate::utils::decode_output(&output.stderr);
    let raw = format!("{stdout}\n{stderr}");
    let json_output = if version >= 2 {
        stdout.lines().next().unwrap_or("")
    } else {
        &stdout
    };
    let filtered = filter_golangci_json(json_output);

    println!("{filtered}");

    if !stderr.trim().is_empty() && verbose > 0 {
        eprintln!("{}", stderr.trim());
    }

    timer.track(
        &format!("golangci-lint {}", original_args.join(" ")),
        &format!("ztok golangci-lint {}", original_args.join(" ")),
        &raw,
        &filtered,
    );

    match output.status.code() {
        Some(0) | Some(1) => Ok(()),
        Some(code) => {
            if !stderr.trim().is_empty() {
                eprintln!("{}", stderr.trim());
            }
            std::process::exit(code);
        }
        None => {
            eprintln!("golangci-lint: 被信号终止");
            std::process::exit(130);
        }
    }
}

fn run_passthrough(args: &[String], verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    if verbose > 0 {
        eprintln!("golangci-lint 透传：{}", args.join(" "));
    }

    let mut cmd = resolved_command("golangci-lint");
    for arg in args {
        cmd.arg(arg);
    }

    let status = cmd.status().context("运行 golangci-lint 失败")?;
    let os_args: Vec<OsString> = args.iter().map(OsString::from).collect();
    let args_str = tracking::args_display(&os_args);
    timer.track_passthrough(
        &format!("golangci-lint {args_str}"),
        &format!("ztok golangci-lint {args_str} (passthrough)"),
    );

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }

    Ok(())
}

fn classify_invocation(args: &[String]) -> Invocation {
    match find_subcommand_index(args) {
        Some(index) if args[index] == "run" => Invocation::FilteredRun(RunInvocation {
            global_args: args[..index].to_vec(),
            run_args: args[index + 1..].to_vec(),
        }),
        _ => Invocation::Passthrough,
    }
}

fn find_subcommand_index(args: &[String]) -> Option<usize> {
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();

        if arg == "--" {
            return None;
        }

        if !arg.starts_with('-') {
            if GOLANGCI_SUBCOMMANDS.contains(&arg) {
                return Some(index);
            }
            return None;
        }

        if let Some(flag) = split_flag_name(arg)
            && golangci_flag_takes_separate_value(arg, flag)
        {
            index += 1;
        }

        index += 1;
    }

    None
}

fn split_flag_name(arg: &str) -> Option<&str> {
    if arg.starts_with("--") {
        return Some(arg.split_once('=').map(|(flag, _)| flag).unwrap_or(arg));
    }

    if arg.starts_with('-') {
        return Some(arg);
    }

    None
}

fn golangci_flag_takes_separate_value(arg: &str, flag: &str) -> bool {
    if !GLOBAL_FLAGS_WITH_VALUE.contains(&flag) {
        return false;
    }

    if arg.starts_with("--") && arg.contains('=') {
        return false;
    }

    true
}

fn build_filtered_args(invocation: &RunInvocation, version: u32) -> Vec<String> {
    let mut args = invocation.global_args.clone();
    args.push("run".to_string());

    if !has_output_flag(&invocation.run_args) {
        if version >= 2 {
            args.push("--output.json.path".to_string());
            args.push("stdout".to_string());
        } else {
            args.push("--out-format=json".to_string());
        }
    }

    args.extend(invocation.run_args.clone());
    args
}

fn has_output_flag(args: &[String]) -> bool {
    args.iter().any(|arg| {
        arg == "--out-format"
            || arg.starts_with("--out-format=")
            || arg == "--output.json.path"
            || arg.starts_with("--output.json.path=")
    })
}

/// 过滤 golangci-lint 的 JSON 输出，按 linter 和文件分组
fn filter_golangci_json(output: &str) -> String {
    let result: Result<GolangciOutput, _> = serde_json::from_str(output);

    let golangci_output = match result {
        Ok(o) => o,
        Err(e) => {
            return format!(
                "golangci-lint (JSON 解析失败: {})\n{}",
                e,
                truncate(output, /*max_len*/ 500)
            );
        }
    };

    let issues = golangci_output.issues;

    if issues.is_empty() {
        return "✓ golangci-lint: 未发现问题".to_string();
    }

    let total_issues = issues.len();
    let unique_files: std::collections::HashSet<_> =
        issues.iter().map(|issue| &issue.pos.filename).collect();
    let total_files = unique_files.len();

    let mut by_linter: HashMap<String, usize> = HashMap::new();
    for issue in &issues {
        *by_linter.entry(issue.from_linter.clone()).or_insert(0) += 1;
    }

    let mut by_file: HashMap<&str, usize> = HashMap::new();
    for issue in &issues {
        *by_file.entry(&issue.pos.filename).or_insert(0) += 1;
    }

    let mut file_counts: Vec<_> = by_file.iter().collect();
    file_counts.sort_by(|a, b| b.1.cmp(a.1));

    let mut result = String::new();
    result.push_str(&format!(
        "golangci-lint: {total_files} 个文件，{total_issues} 个问题\n"
    ));
    result.push_str("═══════════════════════════════════════\n");

    let mut linter_counts: Vec<_> = by_linter.iter().collect();
    linter_counts.sort_by(|a, b| b.1.cmp(a.1));

    if !linter_counts.is_empty() {
        result.push_str("高频 linter:\n");
        for (linter, count) in linter_counts.iter().take(10) {
            result.push_str(&format!("  {linter}（{count} 次）\n"));
        }
        result.push('\n');
    }

    result.push_str("高频文件：\n");
    for (file, count) in file_counts.iter().take(10) {
        let short_path = compact_path(file);
        result.push_str(&format!("  {short_path}（{count} 个问题）\n"));

        let mut file_linters: HashMap<String, usize> = HashMap::new();
        for issue in issues.iter().filter(|issue| &issue.pos.filename == *file) {
            *file_linters.entry(issue.from_linter.clone()).or_insert(0) += 1;
        }

        let mut file_linter_counts: Vec<_> = file_linters.iter().collect();
        file_linter_counts.sort_by(|a, b| b.1.cmp(a.1));

        for (linter, count) in file_linter_counts.iter().take(3) {
            result.push_str(&format!("    {linter} ({count})\n"));
        }
    }

    if file_counts.len() > 10 {
        result.push_str(&format!("\n... +{} 个文件\n", file_counts.len() - 10));
    }

    result.trim().to_string()
}

fn compact_path(path: &str) -> String {
    let path = path.replace('\\', "/");

    if let Some(pos) = path.rfind("/pkg/") {
        format!("pkg/{}", &path[pos + 5..])
    } else if let Some(pos) = path.rfind("/cmd/") {
        format!("cmd/{}", &path[pos + 5..])
    } else if let Some(pos) = path.rfind("/internal/") {
        format!("internal/{}", &path[pos + 10..])
    } else if let Some(pos) = path.rfind('/') {
        path[pos + 1..].to_string()
    } else {
        path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_major_version() {
        assert_eq!(parse_major_version("golangci-lint version 1.59.1"), 1);
        assert_eq!(
            parse_major_version("golangci-lint has version 2.10.0 built with go1.24"),
            2
        );
    }

    #[test]
    fn test_classify_run_with_global_flags() {
        let args = vec![
            "--color".to_string(),
            "always".to_string(),
            "run".to_string(),
            "./...".to_string(),
        ];
        assert_eq!(
            classify_invocation(&args),
            Invocation::FilteredRun(RunInvocation {
                global_args: vec!["--color".to_string(), "always".to_string()],
                run_args: vec!["./...".to_string()],
            })
        );
    }

    #[test]
    fn test_non_run_subcommand_passthrough() {
        let args = vec!["version".to_string()];
        assert_eq!(classify_invocation(&args), Invocation::Passthrough);
    }

    #[test]
    fn test_build_filtered_args_for_v2() {
        let invocation = RunInvocation {
            global_args: vec!["--color".to_string(), "always".to_string()],
            run_args: vec!["./...".to_string()],
        };
        assert_eq!(
            build_filtered_args(&invocation, 2),
            vec![
                "--color".to_string(),
                "always".to_string(),
                "run".to_string(),
                "--output.json.path".to_string(),
                "stdout".to_string(),
                "./...".to_string(),
            ]
        );
    }

    #[test]
    fn test_filter_golangci_no_issues() {
        let output = r#"{"Issues":[]}"#;
        let result = filter_golangci_json(output);
        assert!(result.contains("✓ golangci-lint"));
        assert!(result.contains("未发现问题"));
    }

    #[test]
    fn test_filter_golangci_with_issues() {
        let output = r#"{
  "Issues": [
    {
      "FromLinter": "errcheck",
      "Text": "Error return value not checked",
      "Pos": {"Filename": "main.go", "Line": 42, "Column": 5}
    },
    {
      "FromLinter": "errcheck",
      "Text": "Error return value not checked",
      "Pos": {"Filename": "main.go", "Line": 50, "Column": 10}
    },
    {
      "FromLinter": "gosimple",
      "Text": "Should use strings.Contains",
      "Pos": {"Filename": "utils.go", "Line": 15, "Column": 2}
    }
  ]
}"#;

        let result = filter_golangci_json(output);
        assert!(result.contains("3 个问题"));
        assert!(result.contains("2 个文件"));
        assert!(result.contains("errcheck"));
        assert!(result.contains("gosimple"));
        assert!(result.contains("main.go"));
        assert!(result.contains("utils.go"));
        assert!(result.contains("errcheck（2 次）"));
    }

    #[test]
    fn test_compact_path() {
        assert_eq!(
            compact_path("/Users/foo/project/pkg/handler/server.go"),
            "pkg/handler/server.go"
        );
        assert_eq!(
            compact_path("/home/user/app/cmd/main/main.go"),
            "cmd/main/main.go"
        );
        assert_eq!(
            compact_path("/project/internal/config/loader.go"),
            "internal/config/loader.go"
        );
        assert_eq!(compact_path("relative/file.go"), "file.go");
    }
}
