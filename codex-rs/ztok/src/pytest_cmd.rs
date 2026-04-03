use crate::tracking;
use crate::utils::resolved_command;
use crate::utils::tool_exists;
use crate::utils::truncate;
use anyhow::Context;
use anyhow::Result;

#[derive(Debug, PartialEq)]
enum ParseState {
    Header,
    TestProgress,
    Failures,
    Summary,
}

pub fn run(args: &[String], verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    // 尝试识别 pytest 命令（可能是 `pytest`、`python -m pytest` 等）
    let mut cmd = if tool_exists("pytest") {
        resolved_command("pytest")
    } else {
        // 回退到 `python -m pytest`
        let mut c = resolved_command("python");
        c.arg("-m").arg("pytest");
        c
    };

    // 强制使用短 traceback 和 quiet 模式，得到紧凑输出
    let has_tb_flag = args.iter().any(|a| a.starts_with("--tb"));
    let has_quiet_flag = args.iter().any(|a| a == "-q" || a == "--quiet");

    if !has_tb_flag {
        cmd.arg("--tb=short");
    }
    if !has_quiet_flag {
        cmd.arg("-q");
    }

    for arg in args {
        cmd.arg(arg);
    }

    if verbose > 0 {
        eprintln!("运行：pytest --tb=short -q {}", args.join(" "));
    }

    let output = cmd
        .output()
        .context("运行 pytest 失败。请确认已安装：pip install pytest")?;

    let stdout = crate::utils::decode_output(&output.stdout);
    let stderr = crate::utils::decode_output(&output.stderr);
    let raw = format!("{stdout}\n{stderr}");

    let filtered = filter_pytest_output(&stdout);

    let exit_code = output
        .status
        .code()
        .unwrap_or(if output.status.success() { 0 } else { 1 });
    if let Some(hint) = crate::tee::tee_and_hint(&raw, "pytest", exit_code) {
        println!("{filtered}\n{hint}");
    } else {
        println!("{filtered}");
    }

    // 若存在 stderr，一并输出（例如 import error）
    if !stderr.trim().is_empty() {
        eprintln!("{}", stderr.trim());
    }

    timer.track(
        &format!("pytest {}", args.join(" ")),
        &format!("ztok pytest {}", args.join(" ")),
        &raw,
        &filtered,
    );

    // 保留退出码，兼容 CI/CD
    if !output.status.success() {
        std::process::exit(exit_code);
    }

    Ok(())
}

/// 使用状态机解析 pytest 输出
fn filter_pytest_output(output: &str) -> String {
    let mut state = ParseState::Header;
    let mut test_files: Vec<String> = Vec::new();
    let mut failures: Vec<String> = Vec::new();
    let mut current_failure: Vec<String> = Vec::new();
    let mut summary_line = String::new();

    for line in output.lines() {
        let trimmed = line.trim();

        // 状态切换
        if trimmed.starts_with("===") && trimmed.contains("test session starts") {
            state = ParseState::Header;
            continue;
        } else if trimmed.starts_with("===") && trimmed.contains("FAILURES") {
            state = ParseState::Failures;
            continue;
        } else if trimmed.starts_with("===") && trimmed.contains("short test summary") {
            state = ParseState::Summary;
            // 若当前已有失败块，先保存
            if !current_failure.is_empty() {
                failures.push(current_failure.join("\n"));
                current_failure.clear();
            }
            continue;
        } else if trimmed.starts_with("===")
            && (trimmed.contains("passed") || trimmed.contains("failed"))
        {
            summary_line = trimmed.to_string();
            continue;
        }

        // 按当前状态处理
        match state {
            ParseState::Header => {
                if trimmed.starts_with("collected") {
                    state = ParseState::TestProgress;
                }
            }
            ParseState::TestProgress => {
                // 例如：`tests/test_foo.py ....  [ 40%]`
                if !trimmed.is_empty()
                    && !trimmed.starts_with("===")
                    && (trimmed.contains(".py") || trimmed.contains("%]"))
                {
                    test_files.push(trimmed.to_string());
                }
            }
            ParseState::Failures => {
                // 收集失败详情
                if trimmed.starts_with("___") {
                    // 新的失败分段
                    if !current_failure.is_empty() {
                        failures.push(current_failure.join("\n"));
                        current_failure.clear();
                    }
                    current_failure.push(trimmed.to_string());
                } else if !trimmed.is_empty() && !trimmed.starts_with("===") {
                    current_failure.push(trimmed.to_string());
                }
            }
            ParseState::Summary => {
                // `FAILED` 测试摘要行
                if trimmed.starts_with("FAILED") || trimmed.starts_with("ERROR") {
                    failures.push(trimmed.to_string());
                }
            }
        }
    }

    // 若最后还有未保存的失败块，则补充保存
    if !current_failure.is_empty() {
        failures.push(current_failure.join("\n"));
    }

    // 构造紧凑输出
    build_pytest_summary(&summary_line, &test_files, &failures)
}

fn build_pytest_summary(summary: &str, _test_files: &[String], failures: &[String]) -> String {
    // 解析摘要行
    let (passed, failed, skipped) = parse_summary_line(summary);

    if failed == 0 && passed > 0 {
        return format!("✓ Pytest: {passed} 通过");
    }

    if passed == 0 && failed == 0 {
        return "Pytest: 未收集到测试".to_string();
    }

    let mut result = String::new();
    result.push_str(&format!("Pytest: {passed} 通过，{failed} 失败"));
    if skipped > 0 {
        result.push_str(&format!("，{skipped} 跳过"));
    }
    result.push('\n');
    result.push_str("═══════════════════════════════════════\n");

    if failures.is_empty() {
        return result.trim().to_string();
    }

    // 显示失败项（只保留关键信息）
    result.push_str("\n失败项：\n");

    for (i, failure) in failures.iter().take(5).enumerate() {
        // 提取测试名和关键信息
        let lines: Vec<&str> = failure.lines().collect();

        // 第一行通常是测试名（`___` 之后）
        if let Some(first_line) = lines.first() {
            if first_line.starts_with("___") {
                // 提取 `___` 中间的测试名
                let test_name = first_line.trim_matches('_').trim();
                result.push_str(&format!("{}. ❌ {}\n", i + 1, test_name));
            } else if first_line.starts_with("FAILED") {
                // 摘要格式：`FAILED tests/test_foo.py::test_bar - AssertionError`
                let parts: Vec<&str> = first_line.split(" - ").collect();
                if let Some(test_path) = parts.first() {
                    let test_name = test_path.trim_start_matches("FAILED ");
                    result.push_str(&format!("{}. ❌ {}\n", i + 1, test_name));
                }
                if parts.len() > 1 {
                    result.push_str(&format!("     {}\n", truncate(parts[1], /*max_len*/ 100)));
                }
                continue;
            }
        }

        // 显示相关错误行（断言、错误、文件位置）
        let mut relevant_lines = 0;
        for line in &lines[1..] {
            let line_lower = line.to_lowercase();
            let is_relevant = line.trim().starts_with('>')
                || line.trim().starts_with('E')
                || line_lower.contains("assert")
                || line_lower.contains("error")
                || line.contains(".py:");

            if is_relevant && relevant_lines < 3 {
                result.push_str(&format!("     {}\n", truncate(line, /*max_len*/ 100)));
                relevant_lines += 1;
            }
        }

        if i < failures.len() - 1 {
            result.push('\n');
        }
    }

    if failures.len() > 5 {
        result.push_str(&format!("\n... +{} 个失败\n", failures.len() - 5));
    }

    result.trim().to_string()
}

fn parse_summary_line(summary: &str) -> (usize, usize, usize) {
    let mut passed = 0;
    let mut failed = 0;
    let mut skipped = 0;

    // 解析类似 `=== 4 passed, 1 failed in 0.50s ===` 的摘要行
    let parts: Vec<&str> = summary.split(',').collect();

    for part in parts {
        let words: Vec<&str> = part.split_whitespace().collect();
        for (i, word) in words.iter().enumerate() {
            if i > 0 {
                if word.contains("passed") {
                    if let Ok(n) = words[i - 1].parse::<usize>() {
                        passed = n;
                    }
                } else if word.contains("failed") {
                    if let Ok(n) = words[i - 1].parse::<usize>() {
                        failed = n;
                    }
                } else if word.contains("skipped")
                    && let Ok(n) = words[i - 1].parse::<usize>()
                {
                    skipped = n;
                }
            }
        }
    }

    (passed, failed, skipped)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_pytest_all_pass() {
        let output = r#"=== test session starts ===
platform darwin -- Python 3.11.0
collected 5 items

tests/test_foo.py .....                                            [100%]

=== 5 passed in 0.50s ==="#;

        let result = filter_pytest_output(output);
        assert!(result.contains("✓ Pytest"));
        assert!(result.contains("5 通过"));
    }

    #[test]
    fn test_filter_pytest_with_failures() {
        let output = r#"=== test session starts ===
collected 5 items

tests/test_foo.py ..F..                                            [100%]

=== FAILURES ===
___ test_something ___

    def test_something():
>       assert False
E       assert False

tests/test_foo.py:10: AssertionError

=== short test summary info ===
FAILED tests/test_foo.py::test_something - assert False
=== 4 passed, 1 failed in 0.50s ==="#;

        let result = filter_pytest_output(output);
        assert!(result.contains("4 通过"));
        assert!(result.contains("1 失败"));
        assert!(result.contains("test_something"));
        assert!(result.contains("assert False"));
    }

    #[test]
    fn test_filter_pytest_multiple_failures() {
        let output = r#"=== test session starts ===
collected 3 items

tests/test_foo.py FFF                                              [100%]

=== FAILURES ===
___ test_one ___
E   AssertionError: expected 5

___ test_two ___
E   ValueError: invalid value

=== short test summary info ===
FAILED tests/test_foo.py::test_one - AssertionError: expected 5
FAILED tests/test_foo.py::test_two - ValueError: invalid value
FAILED tests/test_foo.py::test_three - KeyError
=== 3 failed in 0.20s ==="#;

        let result = filter_pytest_output(output);
        assert!(result.contains("3 失败"));
        assert!(result.contains("test_one"));
        assert!(result.contains("test_two"));
        assert!(result.contains("expected 5"));
    }

    #[test]
    fn test_filter_pytest_no_tests() {
        let output = r#"=== test session starts ===
collected 0 items

=== no tests ran in 0.00s ==="#;

        let result = filter_pytest_output(output);
        assert!(result.contains("未收集到测试"));
    }

    #[test]
    fn test_parse_summary_line() {
        assert_eq!(parse_summary_line("=== 5 passed in 0.50s ==="), (5, 0, 0));
        assert_eq!(
            parse_summary_line("=== 4 passed, 1 failed in 0.50s ==="),
            (4, 1, 0)
        );
        assert_eq!(
            parse_summary_line("=== 3 passed, 1 failed, 2 skipped in 1.0s ==="),
            (3, 1, 2)
        );
    }
}
