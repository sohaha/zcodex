use crate::tracking;
use crate::utils::resolved_command;
use crate::utils::strip_ansi;
use crate::utils::tool_exists;
use crate::utils::truncate;
use anyhow::Context;
use anyhow::Result;
use regex::Regex;
use std::collections::HashMap;

pub fn run(args: &[String], verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    let mut cmd = if tool_exists("mypy") {
        resolved_command("mypy")
    } else {
        let mut c = resolved_command("python3");
        c.arg("-m").arg("mypy");
        c
    };

    for arg in args {
        cmd.arg(arg);
    }

    if verbose > 0 {
        eprintln!("运行：mypy {}", args.join(" "));
    }

    let output = cmd
        .output()
        .context("运行 mypy 失败。请确认已安装：pip install mypy")?;

    let stdout = crate::utils::decode_output(&output.stdout);
    let stderr = crate::utils::decode_output(&output.stderr);
    let raw = format!("{stdout}\n{stderr}");
    let clean = strip_ansi(&raw);

    let filtered = filter_mypy_output(&clean);

    println!("{filtered}");

    timer.track(
        &format!("mypy {}", args.join(" ")),
        &format!("ztok mypy {}", args.join(" ")),
        &raw,
        &filtered,
    );

    if !output.status.success() {
        std::process::exit(output.status.code().unwrap_or(1));
    }
    Ok(())
}

struct MypyError {
    file: String,
    line: usize,
    code: String,
    message: String,
    context_lines: Vec<String>,
}

pub fn filter_mypy_output(output: &str) -> String {
    lazy_static::lazy_static! {
        // `file.py:12: error: Message [error-code]`
        // `file.py:12:5: error: Message [error-code]`
        static ref MYPY_DIAG: Regex = crate::utils::compile_regex(
            r"^(.+?):(\d+)(?::\d+)?: (error|warning|note): (.+?)(?:\s+\[(.+)\])?$"
        );
    }

    let lines: Vec<&str> = output.lines().collect();
    let mut errors: Vec<MypyError> = Vec::new();
    let mut fileless_lines: Vec<String> = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];

        // 跳过 mypy 自己的摘要行
        if line.starts_with("Found ") && line.contains(" error") {
            i += 1;
            continue;
        }
        // 跳过 `Success: no issues found`
        if line.starts_with("Success:") {
            i += 1;
            continue;
        }

        if let Some(caps) = MYPY_DIAG.captures(line) {
            let severity = &caps[3];
            let file = caps[1].to_string();
            let line_num: usize = caps[2].parse().unwrap_or(0);
            let message = caps[4].to_string();
            let code = caps
                .get(5)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();

            if severity == "note" {
                // 如果文件和行号一致，则把 note 挂到前一个错误上
                if let Some(last) = errors.last_mut()
                    && last.file == file
                {
                    last.context_lines.push(message);
                    i += 1;
                    continue;
                }
                // 没有关联父错误的独立 note：按无文件错误展示
                fileless_lines.push(line.to_string());
                i += 1;
                continue;
            }

            let mut err = MypyError {
                file,
                line: line_num,
                code,
                message,
                context_lines: Vec::new(),
            };

            // 捕获后续连续的 note 行
            i += 1;
            while i < lines.len() {
                if let Some(next_caps) = MYPY_DIAG.captures(lines[i])
                    && &next_caps[3] == "note"
                    && next_caps[1] == err.file
                {
                    let note_msg = next_caps[4].to_string();
                    err.context_lines.push(note_msg);
                    i += 1;
                    continue;
                }
                break;
            }

            errors.push(err);
        } else if line.contains("error:") && !line.trim().is_empty() {
            // 无文件归属的错误（如配置错误、导入错误）
            fileless_lines.push(line.to_string());
            i += 1;
        } else {
            i += 1;
        }
    }

    // 完全没有错误
    if errors.is_empty() && fileless_lines.is_empty() {
        if output.contains("Success: no issues found") || output.contains("no issues found") {
            return "mypy: 未发现问题".to_string();
        }
        return "mypy: 未发现问题".to_string();
    }

    // 按文件分组
    let mut by_file: HashMap<String, Vec<&MypyError>> = HashMap::new();
    for err in &errors {
        by_file.entry(err.file.clone()).or_default().push(err);
    }

    // 按错误码统计
    let mut by_code: HashMap<String, usize> = HashMap::new();
    for err in &errors {
        if !err.code.is_empty() {
            *by_code.entry(err.code.clone()).or_insert(0) += 1;
        }
    }

    let mut result = String::new();

    // 先输出无文件归属的错误
    for line in &fileless_lines {
        result.push_str(line);
        result.push('\n');
    }
    if !fileless_lines.is_empty() && !errors.is_empty() {
        result.push('\n');
    }

    if !errors.is_empty() {
        result.push_str(&format!(
            "mypy: {} 个错误，{} 个文件\n",
            errors.len(),
            by_file.len()
        ));
        result.push_str("═══════════════════════════════════════\n");

        // 错误码汇总（仅在存在 2 个及以上不同错误码时显示）
        let mut code_counts: Vec<_> = by_code.iter().collect();
        code_counts.sort_by(|a, b| b.1.cmp(a.1));

        if code_counts.len() > 1 {
            let codes_str: Vec<String> = code_counts
                .iter()
                .take(5)
                .map(|(code, count)| format!("{code}（{count} 次）"))
                .collect();
            result.push_str(&format!("错误码：{}\n\n", codes_str.join(", ")));
        }

        // 按错误数排序文件（错误最多的在前）
        let mut files_sorted: Vec<_> = by_file.iter().collect();
        files_sorted.sort_by(|a, b| b.1.len().cmp(&a.1.len()));

        for (file, file_errors) in &files_sorted {
            result.push_str(&format!("{}（{} 个错误）\n", file, file_errors.len()));

            for err in *file_errors {
                if err.code.is_empty() {
                    result.push_str(&format!(
                        "  行{}：{}\n",
                        err.line,
                        truncate(&err.message, /*max_len*/ 120)
                    ));
                } else {
                    result.push_str(&format!(
                        "  行{}：[{}] {}\n",
                        err.line,
                        err.code,
                        truncate(&err.message, /*max_len*/ 120)
                    ));
                }
                for ctx in &err.context_lines {
                    result.push_str(&format!("    {}\n", truncate(ctx, /*max_len*/ 120)));
                }
            }
            result.push('\n');
        }
    }

    result.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_mypy_errors_grouped_by_file() {
        let output = "\
src/server/auth.py:12: error: Incompatible return value type (got \"str\", expected \"int\")  [return-value]
src/server/auth.py:15: error: Argument 1 has incompatible type \"int\"; expected \"str\"  [arg-type]
src/models/user.py:8: error: Name \"foo\" is not defined  [name-defined]
src/models/user.py:10: error: Incompatible types in assignment  [assignment]
src/models/user.py:20: error: Missing return statement  [return]
Found 5 errors in 2 files (checked 10 source files)
";
        let result = filter_mypy_output(output);
        assert!(result.contains("mypy: 5 个错误，2 个文件"));
        // `user.py` 有 3 个错误，`auth.py` 有 2 个，`user.py` 应排在前面
        let user_pos = result.find("user.py").unwrap();
        let auth_pos = result.find("auth.py").unwrap();
        assert!(
            user_pos < auth_pos,
            "`user.py`（3 个错误）应排在 `auth.py`（2 个错误）前面"
        );
        assert!(result.contains("user.py（3 个错误）"));
        assert!(result.contains("auth.py（2 个错误）"));
    }

    #[test]
    fn test_filter_mypy_with_column_numbers() {
        let output = "\
src/api.py:10:5: error: Incompatible return value type  [return-value]
";
        let result = filter_mypy_output(output);
        assert!(result.contains("行10："));
        assert!(result.contains("[return-value]"));
        assert!(result.contains("Incompatible return value type"));
    }

    #[test]
    fn test_filter_mypy_top_codes_summary() {
        let output = "\
a.py:1: error: Error one  [return-value]
a.py:2: error: Error two  [return-value]
a.py:3: error: Error three  [return-value]
b.py:1: error: Error four  [name-defined]
c.py:1: error: Error five  [arg-type]
Found 5 errors in 3 files
";
        let result = filter_mypy_output(output);
        assert!(result.contains("错误码："));
        assert!(result.contains("return-value（3 次）"));
        assert!(result.contains("name-defined（1 次）"));
        assert!(result.contains("arg-type（1 次）"));
    }

    #[test]
    fn test_filter_mypy_single_code_no_summary() {
        let output = "\
a.py:1: error: Error one  [return-value]
a.py:2: error: Error two  [return-value]
b.py:1: error: Error three  [return-value]
Found 3 errors in 2 files
";
        let result = filter_mypy_output(output);
        assert!(
            !result.contains("错误码："),
            "错误码在只有一种 code 时不应出现"
        );
    }

    #[test]
    fn test_filter_mypy_every_error_shown() {
        let output = "\
src/api.py:10: error: Type \"str\" not assignable to \"int\"  [assignment]
src/api.py:20: error: Missing return statement  [return]
src/api.py:30: error: Name \"bar\" is not defined  [name-defined]
";
        let result = filter_mypy_output(output);
        assert!(result.contains("Type \"str\" not assignable to \"int\""));
        assert!(result.contains("Missing return statement"));
        assert!(result.contains("Name \"bar\" is not defined"));
        assert!(result.contains("行10："));
        assert!(result.contains("行20："));
        assert!(result.contains("行30："));
    }

    #[test]
    fn test_filter_mypy_note_continuation() {
        let output = "\
src/app.py:10: error: Incompatible types in assignment  [assignment]
src/app.py:10: note: Expected type \"int\"
src/app.py:10: note: Got type \"str\"
src/app.py:20: error: Missing return statement  [return]
";
        let result = filter_mypy_output(output);
        assert!(result.contains("Incompatible types in assignment"));
        assert!(result.contains("Expected type \"int\""));
        assert!(result.contains("Got type \"str\""));
        assert!(result.contains("行10："));
        assert!(result.contains("行20："));
    }

    #[test]
    fn test_filter_mypy_fileless_errors() {
        let output = "\
mypy: error: No module named 'nonexistent'
src/api.py:10: error: Name \"foo\" is not defined  [name-defined]
Found 1 error in 1 file
";
        let result = filter_mypy_output(output);
        // 无文件归属的错误应原样出现在分组输出之前
        assert!(result.contains("mypy: error: No module named 'nonexistent'"));
        assert!(result.contains("api.py（1 个错误"));
        let fileless_pos = result.find("No module named").unwrap();
        let grouped_pos = result.find("api.py").unwrap();
        assert!(
            fileless_pos < grouped_pos,
            "无文件归属的错误应出现在按文件分组的错误之前"
        );
    }

    #[test]
    fn test_filter_mypy_no_errors() {
        let output = "Success: no issues found in 5 source files\n";
        let result = filter_mypy_output(output);
        assert_eq!(result, "mypy: 未发现问题");
    }

    #[test]
    fn test_filter_mypy_no_file_limit() {
        let mut output = String::new();
        for i in 1..=15 {
            output.push_str(&format!(
                "src/file{i}.py:{i}: error: Error in file {i}.  [assignment]\n"
            ));
        }
        output.push_str("Found 15 errors in 15 files\n");
        let result = filter_mypy_output(&output);
        assert!(result.contains("15 个错误，15 个文件"));
        for i in 1..=15 {
            assert!(
                result.contains(&format!("file{i}.py")),
                "file{i}.py missing from output"
            );
        }
    }
}
