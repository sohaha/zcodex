use crate::tracking;
use anyhow::Result;
use lazy_static::lazy_static;
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::io::BufRead;
use std::io::{self};
use std::path::Path;

lazy_static! {
    static ref TIMESTAMP_RE: Regex =
        crate::utils::compile_regex(r"^\d{4}[-/]\d{2}[-/]\d{2}[T ]\d{2}:\d{2}:\d{2}[.,]?\d*\s*");
    static ref UUID_RE: Regex = crate::utils::compile_regex(
        r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}"
    );
    static ref HEX_RE: Regex = crate::utils::compile_regex(r"0x[0-9a-fA-F]+");
    static ref NUM_RE: Regex = crate::utils::compile_regex(r"\b\d{4,}\b");
    static ref PATH_RE: Regex = crate::utils::compile_regex(r"/[\w./\-]+");
}

/// 过滤并去重日志输出
pub fn run_file(file: &Path, verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    if verbose > 0 {
        eprintln!("分析日志：{}", file.display());
    }

    let content = fs::read_to_string(file)?;
    let result = analyze_logs(&content);
    println!("{result}");
    timer.track(
        &format!("cat {}", file.display()),
        "rtk log",
        &content,
        &result,
    );
    Ok(())
}

/// 过滤来自 stdin 的日志
pub fn run_stdin(_verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    let mut content = String::new();
    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        content.push_str(&line?);
        content.push('\n');
    }

    let result = analyze_logs(&content);
    println!("{result}");

    timer.track("log (stdin)", "rtk log (stdin)", &content, &result);

    Ok(())
}

/// 供其他模块调用
pub fn run_stdin_str(content: &str) -> String {
    analyze_logs(content)
}

fn analyze_logs(content: &str) -> String {
    let mut result = Vec::new();
    let mut error_counts: HashMap<String, usize> = HashMap::new();
    let mut warn_counts: HashMap<String, usize> = HashMap::new();
    let mut info_counts: HashMap<String, usize> = HashMap::new();
    let mut unique_errors: Vec<String> = Vec::new();
    let mut unique_warnings: Vec<String> = Vec::new();

    // 使用模块级 lazy_static 正则做归一化

    for line in content.lines() {
        let line_lower = line.to_lowercase();

        // 为去重做归一化
        let normalized =
            normalize_log_line(line, &TIMESTAMP_RE, &UUID_RE, &HEX_RE, &NUM_RE, &PATH_RE);

        // 分类
        if line_lower.contains("error")
            || line_lower.contains("fatal")
            || line_lower.contains("panic")
        {
            let count = error_counts.entry(normalized.clone()).or_insert(0);
            if *count == 0 {
                unique_errors.push(line.to_string());
            }
            *count += 1;
        } else if line_lower.contains("warn") {
            let count = warn_counts.entry(normalized.clone()).or_insert(0);
            if *count == 0 {
                unique_warnings.push(line.to_string());
            }
            *count += 1;
        } else if line_lower.contains("info") {
            *info_counts.entry(normalized).or_insert(0) += 1;
        }
    }

    // 摘要
    let total_errors: usize = error_counts.values().sum();
    let total_warnings: usize = warn_counts.values().sum();
    let total_info: usize = info_counts.values().sum();

    result.push("📊 日志摘要".to_string());
    result.push(format!(
        "   ❌ {} 个错误（{} 个唯一）",
        total_errors,
        error_counts.len()
    ));
    result.push(format!(
        "   ⚠️  {} 个警告（{} 个唯一）",
        total_warnings,
        warn_counts.len()
    ));
    result.push(format!("   ℹ️  {total_info} 条信息"));
    result.push(String::new());

    // 带计数的错误列表
    if !unique_errors.is_empty() {
        result.push("❌ 错误：".to_string());

        // 按次数排序
        let mut error_list: Vec<_> = error_counts.iter().collect();
        error_list.sort_by(|a, b| b.1.cmp(a.1));

        for (normalized, count) in error_list.iter().take(10) {
            // 找到原始消息
            let original = unique_errors
                .iter()
                .find(|e| {
                    &normalize_log_line(e, &TIMESTAMP_RE, &UUID_RE, &HEX_RE, &NUM_RE, &PATH_RE)
                        == *normalized
                })
                .map(std::string::String::as_str)
                .unwrap_or(normalized);

            let truncated = if original.len() > 100 {
                let t: String = original.chars().take(97).collect();
                format!("{t}...")
            } else {
                original.to_string()
            };

            if **count > 1 {
                result.push(format!("   [×{count}] {truncated}"));
            } else {
                result.push(format!("   {truncated}"));
            }
        }

        if error_list.len() > 10 {
            result.push(format!("   ... +{} 条唯一错误", error_list.len() - 10));
        }
        result.push(String::new());
    }

    // 带计数的警告列表
    if !unique_warnings.is_empty() {
        result.push("⚠️  警告：".to_string());

        let mut warn_list: Vec<_> = warn_counts.iter().collect();
        warn_list.sort_by(|a, b| b.1.cmp(a.1));

        for (normalized, count) in warn_list.iter().take(5) {
            let original = unique_warnings
                .iter()
                .find(|w| {
                    &normalize_log_line(w, &TIMESTAMP_RE, &UUID_RE, &HEX_RE, &NUM_RE, &PATH_RE)
                        == *normalized
                })
                .map(std::string::String::as_str)
                .unwrap_or(normalized);

            let truncated = if original.len() > 100 {
                let t: String = original.chars().take(97).collect();
                format!("{t}...")
            } else {
                original.to_string()
            };

            if **count > 1 {
                result.push(format!("   [×{count}] {truncated}"));
            } else {
                result.push(format!("   {truncated}"));
            }
        }

        if warn_list.len() > 5 {
            result.push(format!("   ... +{} 条唯一警告", warn_list.len() - 5));
        }
    }

    result.join("\n")
}

fn normalize_log_line(
    line: &str,
    timestamp_re: &Regex,
    uuid_re: &Regex,
    hex_re: &Regex,
    num_re: &Regex,
    path_re: &Regex,
) -> String {
    let mut normalized = timestamp_re.replace_all(line, "").to_string();
    normalized = uuid_re.replace_all(&normalized, "<UUID>").to_string();
    normalized = hex_re.replace_all(&normalized, "<HEX>").to_string();
    normalized = num_re.replace_all(&normalized, "<NUM>").to_string();
    normalized = path_re.replace_all(&normalized, "<PATH>").to_string();
    normalized.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyze_logs() {
        let logs = r#"
2024-01-01 10:00:00 ERROR: Connection failed to /api/server
2024-01-01 10:00:01 ERROR: Connection failed to /api/server
2024-01-01 10:00:02 ERROR: Connection failed to /api/server
2024-01-01 10:00:03 WARN: Retrying connection
2024-01-01 10:00:04 INFO: Connected
"#;
        let result = analyze_logs(logs);
        assert!(result.contains("×3"));
        assert!(result.contains("错误"));
    }

    #[test]
    fn test_analyze_logs_multibyte() {
        let logs = format!(
            "2024-01-01 10:00:00 ERROR: {} connection failed\n\
             2024-01-01 10:00:01 WARN: {} retry attempt\n",
            "ข้อผิดพลาด".repeat(15),
            "คำเตือน".repeat(15)
        );
        let result = analyze_logs(&logs);
        // 即使遇到超长多字节消息也不应 panic
        assert!(result.contains("错误"));
    }
}
