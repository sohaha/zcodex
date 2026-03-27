use crate::tracking;
use crate::utils::resolved_command;
use crate::utils::truncate;
use anyhow::Context;
use anyhow::Result;
use serde::Deserialize;
use std::collections::HashMap;

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

pub fn run(args: &[String], verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    let mut cmd = resolved_command("golangci-lint");

    // 强制启用 JSON 输出
    let has_format = args
        .iter()
        .any(|a| a == "--out-format" || a.starts_with("--out-format="));

    if !has_format {
        cmd.arg("run").arg("--out-format=json");
    } else {
        cmd.arg("run");
    }

    for arg in args {
        cmd.arg(arg);
    }

    if verbose > 0 {
        eprintln!("运行：golangci-lint run --out-format=json");
    }

    let output = cmd.output().context(
        "运行 golangci-lint 失败。请确认已安装：go install github.com/golangci/golangci-lint/cmd/golangci-lint@latest",
    )?;

    let stdout = crate::utils::decode_output(&output.stdout);
    let stderr = crate::utils::decode_output(&output.stderr);
    let raw = format!("{stdout}\n{stderr}");

    let filtered = filter_golangci_json(&stdout);

    println!("{filtered}");

    // 如有 stderr，也一并输出（配置错误等）
    if !stderr.trim().is_empty() && verbose > 0 {
        eprintln!("{}", stderr.trim());
    }

    timer.track(
        &format!("golangci-lint {}", args.join(" ")),
        &format!("rtk golangci-lint {}", args.join(" ")),
        &raw,
        &filtered,
    );

    // 对于 golangci-lint：exit 0 = 干净，exit 1 = 有 lint 问题，exit 2+ = 配置/构建错误
    // 若为 None，则表示被信号终止（OOM、SIGKILL）—— 总是致命错误
    match output.status.code() {
        Some(0) | Some(1) => Ok(()),
        Some(code) => {
            if !stderr.trim().is_empty() {
                eprintln!("{}", stderr.trim());
            }
            std::process::exit(code);
        }
        None => {
            eprintln!("golangci-lint：被信号终止");
            std::process::exit(130);
        }
    }
}

/// 过滤 golangci-lint 的 JSON 输出，按 linter 和文件分组
fn filter_golangci_json(output: &str) -> String {
    let result: Result<GolangciOutput, _> = serde_json::from_str(output);

    let golangci_output = match result {
        Ok(o) => o,
        Err(e) => {
            // JSON 解析失败时回退
            return format!(
                "golangci-lint（JSON 解析失败：{}）\n{}",
                e,
                truncate(output, /*max_len*/ 500)
            );
        }
    };

    let issues = golangci_output.issues;

    if issues.is_empty() {
        return "✓ golangci-lint：未发现问题".to_string();
    }

    let total_issues = issues.len();

    // 统计唯一文件数
    let unique_files: std::collections::HashSet<_> =
        issues.iter().map(|i| &i.pos.filename).collect();
    let total_files = unique_files.len();

    // 按 linter 分组
    let mut by_linter: HashMap<String, usize> = HashMap::new();
    for issue in &issues {
        *by_linter.entry(issue.from_linter.clone()).or_insert(0) += 1;
    }

    // 按文件分组
    let mut by_file: HashMap<&str, usize> = HashMap::new();
    for issue in &issues {
        *by_file.entry(&issue.pos.filename).or_insert(0) += 1;
    }

    let mut file_counts: Vec<_> = by_file.iter().collect();
    file_counts.sort_by(|a, b| b.1.cmp(a.1));

    // 构建输出
    let mut result = String::new();
    result.push_str(&format!(
        "golangci-lint：{total_files} 个文件，{total_issues} 个问题\n"
    ));
    result.push_str("═══════════════════════════════════════\n");

    // 显示高频 linter
    let mut linter_counts: Vec<_> = by_linter.iter().collect();
    linter_counts.sort_by(|a, b| b.1.cmp(a.1));

    if !linter_counts.is_empty() {
        result.push_str("高频 linter：\n");
        for (linter, count) in linter_counts.iter().take(10) {
            result.push_str(&format!("  {linter} ({count}x)\n"));
        }
        result.push('\n');
    }

    // 显示高频文件
    result.push_str("高频文件：\n");
    for (file, count) in file_counts.iter().take(10) {
        let short_path = compact_path(file);
        result.push_str(&format!("  {short_path}（{count} 个问题）\n"));

        // 显示该文件中最常见的 3 个 linter
        let mut file_linters: HashMap<String, usize> = HashMap::new();
        for issue in issues.iter().filter(|i| &i.pos.filename == *file) {
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

/// 压缩文件路径（移除常见公共前缀）
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
