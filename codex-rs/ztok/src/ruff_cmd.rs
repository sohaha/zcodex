use crate::tracking;
use crate::utils::resolved_command;
use crate::utils::truncate;
use anyhow::Context;
use anyhow::Result;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
struct RuffLocation {
    #[allow(dead_code)]
    row: usize,
    #[allow(dead_code)]
    column: usize,
}

#[derive(Debug, Deserialize)]
struct RuffFix {
    #[allow(dead_code)]
    applicability: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RuffDiagnostic {
    code: String,
    #[allow(dead_code)]
    message: String,
    #[allow(dead_code)]
    location: RuffLocation,
    #[allow(dead_code)]
    end_location: Option<RuffLocation>,
    filename: String,
    fix: Option<RuffFix>,
}

pub fn run(args: &[String], verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    // 检测子命令：check、format 或 version
    let is_check = args.is_empty()
        || args[0] == "check"
        || (!args[0].starts_with('-') && args[0] != "format" && args[0] != "version");

    let is_format = args.iter().any(|a| a == "format");

    let mut cmd = resolved_command("ruff");

    if is_check {
        // 为 check 命令强制启用 JSON 输出
        if !args.contains(&"--output-format".to_string()) {
            cmd.arg("check").arg("--output-format=json");
        } else {
            cmd.arg("check");
        }

        // 追加用户参数（若首参是 `check` 则跳过）
        let start_idx = if !args.is_empty() && args[0] == "check" {
            1
        } else {
            0
        };
        for arg in &args[start_idx..] {
            cmd.arg(arg);
        }

        // 若未指定路径，则默认使用当前目录
        if args
            .iter()
            .skip(start_idx)
            .all(|a| a.starts_with('-') || a.contains('='))
        {
            cmd.arg(".");
        }
    } else {
        // format 或其他命令：直接透传
        for arg in args {
            cmd.arg(arg);
        }
    }

    if verbose > 0 {
        eprintln!("运行：ruff {}", args.join(" "));
    }

    let output = cmd
        .output()
        .context("运行 ruff 失败。请确认已安装：pip install ruff")?;

    let stdout = crate::utils::decode_output(&output.stdout);
    let stderr = crate::utils::decode_output(&output.stderr);
    let raw = format!("{stdout}\n{stderr}");

    let filtered = if is_check && !stdout.trim().is_empty() {
        filter_ruff_check_json(&stdout)
    } else if is_format {
        filter_ruff_format(&raw)
    } else {
        // 回退处理其他命令（如 version）
        raw.trim().to_string()
    };

    println!("{filtered}");

    timer.track(
        &format!("ruff {}", args.join(" ")),
        &format!("ztok ruff {}", args.join(" ")),
        &raw,
        &filtered,
    );

    // 保留退出码，兼容 CI/CD
    if !output.status.success() {
        std::process::exit(output.status.code().unwrap_or(1));
    }

    Ok(())
}

/// 过滤 `ruff check` 的 JSON 输出，按规则和文件分组
pub fn filter_ruff_check_json(output: &str) -> String {
    let diagnostics: Result<Vec<RuffDiagnostic>, _> = serde_json::from_str(output);

    let diagnostics = match diagnostics {
        Ok(d) => d,
        Err(e) => {
            // JSON 解析失败时回退
            return format!(
                "Ruff check (JSON 解析失败: {})\n{}",
                e,
                truncate(output, /*max_len*/ 500)
            );
        }
    };

    if diagnostics.is_empty() {
        return "✓ Ruff: 未发现问题".to_string();
    }

    let total_issues = diagnostics.len();
    let fixable_count = diagnostics.iter().filter(|d| d.fix.is_some()).count();

    // 统计唯一文件数
    let unique_files: std::collections::HashSet<_> =
        diagnostics.iter().map(|d| &d.filename).collect();
    let total_files = unique_files.len();

    // 按规则编码分组
    let mut by_rule: HashMap<String, usize> = HashMap::new();
    for diag in &diagnostics {
        *by_rule.entry(diag.code.clone()).or_insert(0) += 1;
    }

    // 按文件分组
    let mut by_file: HashMap<&str, usize> = HashMap::new();
    for diag in &diagnostics {
        *by_file.entry(&diag.filename).or_insert(0) += 1;
    }

    let mut file_counts: Vec<_> = by_file.iter().collect();
    file_counts.sort_by(|a, b| b.1.cmp(a.1));

    // 构造输出
    let mut result = String::new();
    result.push_str(&format!(
        "Ruff: {total_files} 个文件，{total_issues} 个问题"
    ));

    if fixable_count > 0 {
        result.push_str(&format!("（{fixable_count} 个可修复）"));
    }
    result.push('\n');
    result.push_str("═══════════════════════════════════════\n");

    // 显示高频规则
    let mut rule_counts: Vec<_> = by_rule.iter().collect();
    rule_counts.sort_by(|a, b| b.1.cmp(a.1));

    if !rule_counts.is_empty() {
        result.push_str("高频规则：\n");
        for (rule, count) in rule_counts.iter().take(10) {
            result.push_str(&format!("  {rule}（{count} 次）\n"));
        }
        result.push('\n');
    }

    // 显示高频文件
    result.push_str("高频文件：\n");
    for (file, count) in file_counts.iter().take(10) {
        let short_path = compact_path(file);
        result.push_str(&format!("  {short_path}（{count} 个问题）\n"));

        // 显示该文件中最常见的 3 条规则
        let mut file_rules: HashMap<String, usize> = HashMap::new();
        for diag in diagnostics.iter().filter(|d| &d.filename == *file) {
            *file_rules.entry(diag.code.clone()).or_insert(0) += 1;
        }

        let mut file_rule_counts: Vec<_> = file_rules.iter().collect();
        file_rule_counts.sort_by(|a, b| b.1.cmp(a.1));

        for (rule, count) in file_rule_counts.iter().take(3) {
            result.push_str(&format!("    {rule} ({count})\n"));
        }
    }

    if file_counts.len() > 10 {
        result.push_str(&format!("\n... +{} 个文件\n", file_counts.len() - 10));
    }

    if fixable_count > 0 {
        result.push_str(&format!(
            "\n💡 运行 `ruff check --fix` 自动修复 {fixable_count} 个问题\n"
        ));
    }

    result.trim().to_string()
}

/// 过滤 `ruff format` 输出，显示需要格式化的文件
pub fn filter_ruff_format(output: &str) -> String {
    let mut files_to_format: Vec<String> = Vec::new();
    let mut files_checked = 0;

    for line in output.lines() {
        let trimmed = line.trim();
        let lower = trimmed.to_lowercase();

        // 统计 `would reformat` 行（check 模式，不区分大小写）
        if lower.contains("would reformat:") {
            // 从 `Would reformat: path/to/file.py` 中提取文件名
            if let Some(filename) = trimmed.split(':').nth(1) {
                files_to_format.push(filename.trim().to_string());
            }
        }

        // 统计检查过的文件总数，例如 `3 files left unchanged`
        if lower.contains("left unchanged") {
            // 精确查找 `X file(s) left unchanged` 模式
            // 按逗号切分，兼容 `2 个文件 would be reformatted, 3 files left unchanged`
            let parts: Vec<&str> = trimmed.split(',').collect();
            for part in parts {
                let part_lower = part.to_lowercase();
                if part_lower.contains("left unchanged") {
                    let words: Vec<&str> = part.split_whitespace().collect();
                    // 查找位于 `file/files` 之前的数字
                    for (i, word) in words.iter().enumerate() {
                        if (word == &"file" || word == &"files")
                            && i > 0
                            && let Ok(count) = words[i - 1].parse::<usize>()
                        {
                            files_checked = count;
                            break;
                        }
                    }
                    break;
                }
            }
        }
    }

    let output_lower = output.to_lowercase();

    // 检查是否所有文件都已格式化
    if files_to_format.is_empty() && output_lower.contains("left unchanged") {
        return "✓ Ruff format: 所有文件格式正确".to_string();
    }

    let mut result = String::new();

    if output_lower.contains("would reformat") {
        // check 模式：显示需要格式化的文件
        if files_to_format.is_empty() {
            result.push_str("✓ Ruff format: 所有文件格式正确\n");
        } else {
            result.push_str(&format!(
                "Ruff format: {} 个文件需要格式化\n",
                files_to_format.len()
            ));
            result.push_str("═══════════════════════════════════════\n");

            for (i, file) in files_to_format.iter().take(10).enumerate() {
                result.push_str(&format!("{}. {}\n", i + 1, compact_path(file)));
            }

            if files_to_format.len() > 10 {
                result.push_str(&format!("\n... +{} 个文件\n", files_to_format.len() - 10));
            }

            if files_checked > 0 {
                result.push_str(&format!("\n✓ {files_checked} 个文件已格式化\n"));
            }

            result.push_str("\n💡 运行 `ruff format` 格式化这些文件\n");
        }
    } else {
        // write 模式或其他输出：显示摘要
        result.push_str(output.trim());
    }

    result.trim().to_string()
}

/// 压缩文件路径（去除常见公共前缀）
fn compact_path(path: &str) -> String {
    let path = path.replace('\\', "/");

    if let Some(pos) = path.rfind("/src/") {
        format!("src/{}", &path[pos + 5..])
    } else if let Some(pos) = path.rfind("/lib/") {
        format!("lib/{}", &path[pos + 5..])
    } else if let Some(pos) = path.rfind("/tests/") {
        format!("tests/{}", &path[pos + 7..])
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
    fn test_filter_ruff_check_no_issues() {
        let output = "[]";
        let result = filter_ruff_check_json(output);
        assert!(result.contains("✓ Ruff"));
        assert!(result.contains("未发现问题"));
    }

    #[test]
    fn test_filter_ruff_check_with_issues() {
        let output = r#"[
  {
    "code": "F401",
    "message": "`os` imported but unused",
    "location": {"row": 1, "column": 8},
    "end_location": {"row": 1, "column": 10},
    "filename": "src/main.py",
    "fix": {"applicability": "safe"}
  },
  {
    "code": "F401",
    "message": "`sys` imported but unused",
    "location": {"row": 2, "column": 8},
    "end_location": {"row": 2, "column": 11},
    "filename": "src/main.py",
    "fix": null
  },
  {
    "code": "E501",
    "message": "Line too long (100 > 88 characters)",
    "location": {"row": 10, "column": 89},
    "end_location": {"row": 10, "column": 100},
    "filename": "src/utils.py",
    "fix": null
  }
]"#;
        let result = filter_ruff_check_json(output);
        assert!(result.contains("3 个问题"));
        assert!(result.contains("2 个文件"));
        assert!(result.contains("1 个可修复"));
        assert!(result.contains("F401"));
        assert!(result.contains("E501"));
        assert!(result.contains("main.py"));
        assert!(result.contains("utils.py"));
        assert!(result.contains("F401（2 次）"));
    }

    #[test]
    fn test_filter_ruff_format_all_formatted() {
        let output = "5 files left unchanged";
        let result = filter_ruff_format(output);
        assert!(result.contains("✓ Ruff format"));
        assert!(result.contains("所有文件格式正确"));
    }

    #[test]
    fn test_filter_ruff_format_needs_formatting() {
        let output = r#"Would reformat: src/main.py
Would reformat: tests/test_utils.py
2 个文件 would be reformatted, 3 files left unchanged"#;
        let result = filter_ruff_format(output);
        assert!(result.contains("2 个文件需要格式化"));
        assert!(result.contains("main.py"));
        assert!(result.contains("test_utils.py"));
        assert!(result.contains("3 个文件已格式化"));
    }

    #[test]
    fn test_compact_path() {
        assert_eq!(
            compact_path("/Users/foo/project/src/main.py"),
            "src/main.py"
        );
        assert_eq!(compact_path("/home/user/app/lib/utils.py"), "lib/utils.py");
        assert_eq!(
            compact_path("C:\\Users\\foo\\project\\tests\\test.py"),
            "tests/test.py"
        );
        assert_eq!(compact_path("relative/file.py"), "file.py");
    }
}
