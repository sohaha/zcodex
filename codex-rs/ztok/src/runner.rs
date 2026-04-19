use crate::tracking;
use anyhow::Context;
use anyhow::Result;
use clap::ValueEnum;
use regex::Regex;
use std::process::Command;
use std::process::Output;
use std::process::Stdio;

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum ShellFilter {
    Raw,
    Err,
    Test,
}

pub fn run_shell(command: &[String], filter: ShellFilter, verbose: u8) -> Result<()> {
    match filter {
        ShellFilter::Raw => run_raw(command, verbose),
        ShellFilter::Err => run_err(command, verbose),
        ShellFilter::Test => run_test(command, verbose),
    }
}

fn run_raw(command: &[String], verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();
    let command_display = command.join(" ");

    if verbose > 0 {
        eprintln!("运行 shell 命令：{command_display}");
    }

    let output = execute_command(command).context("执行 shell 命令失败")?;
    let stdout = crate::utils::decode_output(&output.stdout);
    let stderr = crate::utils::decode_output(&output.stderr);
    let raw = join_streams(&stdout, &stderr);
    let rendered = if raw.trim().is_empty() {
        render_empty_output(output.status.code(), output.status.success())
    } else {
        raw.clone()
    };

    let exit_code = output
        .status
        .code()
        .unwrap_or(if output.status.success() { 0 } else { 1 });
    if let Some(hint) = crate::tee::tee_and_hint(&raw, "shell", exit_code) {
        println!("{rendered}\n{hint}");
    } else {
        println!("{rendered}");
    }
    timer.track(&command_display, "ztok shell", &raw, &rendered);
    if exit_code != 0 {
        std::process::exit(exit_code);
    }
    Ok(())
}

/// 运行命令并过滤输出，只显示错误和警告
pub fn run_err(command: &[String], verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();
    let command_display = command.join(" ");

    if verbose > 0 {
        eprintln!("运行：{command_display}");
    }

    let output = execute_command(command).context("执行命令失败")?;

    let stdout = crate::utils::decode_output(&output.stdout);
    let stderr = crate::utils::decode_output(&output.stderr);
    let raw = join_streams(&stdout, &stderr);
    let filtered = filter_errors(&raw);
    let mut ztok = String::new();

    if filtered.is_empty() {
        if output.status.success() {
            ztok.push_str("命令执行成功（无错误）");
        } else {
            ztok.push_str(&format!(
                "命令执行失败（退出码：{:?}）\n",
                output.status.code()
            ));
            let lines: Vec<&str> = raw.lines().collect();
            for line in lines.iter().rev().take(10).rev() {
                ztok.push_str(&format!("  {line}\n"));
            }
        }
    } else {
        ztok.push_str(&filtered);
    }

    let exit_code = output
        .status
        .code()
        .unwrap_or(if output.status.success() { 0 } else { 1 });
    if let Some(hint) = crate::tee::tee_and_hint(&raw, "err", exit_code) {
        println!("{ztok}\n{hint}");
    } else {
        println!("{ztok}");
    }
    timer.track(&command_display, "ztok run-err", &raw, &ztok);
    if exit_code != 0 {
        std::process::exit(exit_code);
    }
    Ok(())
}

/// 运行测试并只显示失败项
pub fn run_test(command: &[String], verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();
    let command_display = command.join(" ");

    if verbose > 0 {
        eprintln!("运行测试：{command_display}");
    }

    let output = execute_command(command).context("执行测试命令失败")?;

    let stdout = crate::utils::decode_output(&output.stdout);
    let stderr = crate::utils::decode_output(&output.stderr);
    let raw = join_streams(&stdout, &stderr);

    let exit_code = output
        .status
        .code()
        .unwrap_or(if output.status.success() { 0 } else { 1 });
    let summary = extract_test_summary(&raw, &command_display);
    if let Some(hint) = crate::tee::tee_and_hint(&raw, "test", exit_code) {
        println!("{summary}\n{hint}");
    } else {
        println!("{summary}");
    }
    timer.track(&command_display, "ztok run-test", &raw, &summary);
    if exit_code != 0 {
        std::process::exit(exit_code);
    }
    Ok(())
}

fn execute_command(command: &[String]) -> Result<Output> {
    let (program, args) = command.split_first().context("未提供命令")?;

    Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("启动命令失败：{program}"))
}

fn join_streams(stdout: &str, stderr: &str) -> String {
    match (stdout.is_empty(), stderr.is_empty()) {
        (false, false) if stdout.ends_with('\n') => format!("{stdout}{stderr}"),
        (false, false) => format!("{stdout}\n{stderr}"),
        (false, true) => stdout.to_string(),
        (true, false) => stderr.to_string(),
        (true, true) => String::new(),
    }
}

fn render_empty_output(exit_code: Option<i32>, success: bool) -> String {
    if success {
        "命令执行成功（无输出）".to_string()
    } else {
        format!("命令执行失败（退出码：{exit_code:?}）")
    }
}

fn filter_errors(output: &str) -> String {
    lazy_static::lazy_static! {
        static ref ERROR_PATTERNS: Vec<Regex> = vec![
            // 通用错误
            crate::utils::compile_regex(r"(?i)^.*error[\s:\[].*$"),
            crate::utils::compile_regex(r"(?i)^.*\berr\b.*$"),
            crate::utils::compile_regex(r"(?i)^.*warning[\s:\[].*$"),
            crate::utils::compile_regex(r"(?i)^.*\bwarn\b.*$"),
            crate::utils::compile_regex(r"(?i)^.*failed.*$"),
            crate::utils::compile_regex(r"(?i)^.*failure.*$"),
            crate::utils::compile_regex(r"(?i)^.*exception.*$"),
            crate::utils::compile_regex(r"(?i)^.*panic.*$"),
            // Rust 特定格式
            crate::utils::compile_regex(r"^error\[E\d+\]:.*$"),
            crate::utils::compile_regex(r"^\s*--> .*:\d+:\d+$"),
            // Python 格式
            crate::utils::compile_regex(r"^Traceback.*$"),
            crate::utils::compile_regex(r#"^\s*File ".*", line \d+.*$"#),
            // JavaScript / TypeScript 格式
            crate::utils::compile_regex(r"^\s*at .*:\d+:\d+.*$"),
            // Go 格式
            crate::utils::compile_regex(r"^.*\.go:\d+:.*$"),
        ];
    }

    let lines: Vec<&str> = output.lines().collect();
    let mut include = vec![false; lines.len()];

    for (idx, line) in lines.iter().enumerate() {
        if ERROR_PATTERNS.iter().any(|p| p.is_match(line)) {
            include[idx] = true;
            if idx > 0 {
                include[idx - 1] = true;
            }
            if idx + 1 < lines.len() {
                include[idx + 1] = true;
            }
        }
    }

    lines
        .iter()
        .zip(include)
        .filter(|&(_line, keep)| keep)
        .map(|(line, _keep)| (*line).to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

fn extract_test_summary(output: &str, command: &str) -> String {
    let mut result = Vec::new();
    let lines: Vec<&str> = output.lines().collect();

    // 检测测试框架
    let is_cargo = command.contains("cargo test");
    let is_pytest = command.contains("pytest");
    let is_jest =
        command.contains("jest") || command.contains("npm test") || command.contains("yarn test");
    let is_go = command.contains("go test");

    // 收集失败项
    let mut failures = Vec::new();
    let mut in_failure = false;
    let mut failure_lines = Vec::new();

    for line in lines.iter() {
        // Cargo test 输出
        if is_cargo {
            if line.contains("test result:") {
                result.push(line.to_string());
            }
            if line.contains("FAILED") && !line.contains("test result") {
                failures.push(line.to_string());
            }
            if line.starts_with("failures:") {
                in_failure = true;
            }
            if in_failure && line.starts_with("    ") {
                failure_lines.push(line.to_string());
            }
        }

        // pytest 输出
        if is_pytest {
            if line.contains(" passed") || line.contains(" failed") || line.contains(" error") {
                result.push(line.to_string());
            }
            if line.contains("FAILED") {
                failures.push(line.to_string());
            }
        }

        // Jest 输出
        if is_jest {
            if line.contains("Tests:") || line.contains("Test Suites:") {
                result.push(line.to_string());
            }
            if line.contains("✕") || line.contains("FAIL") {
                failures.push(line.to_string());
            }
        }

        // Go test 输出
        if is_go {
            if line.starts_with("ok") || line.starts_with("FAIL") || line.starts_with("---") {
                result.push(line.to_string());
            }
            if line.contains("FAIL") {
                failures.push(line.to_string());
            }
        }
    }

    // 组装输出
    let mut output = String::new();

    if !failures.is_empty() {
        output.push_str("失败：\n");
        for f in failures.iter().take(10) {
            output.push_str(&format!("  {f}\n"));
        }
        if failures.len() > 10 {
            output.push_str(&format!("  ... +{} 个失败\n", failures.len() - 10));
        }
        output.push('\n');
    }

    if !result.is_empty() {
        output.push_str("摘要：\n");
        for r in &result {
            output.push_str(&format!("  {r}\n"));
        }
    } else {
        // 回退：显示最后几行
        output.push_str("输出（最后 5 行）：\n");
        let start = lines.len().saturating_sub(5);
        for line in &lines[start..] {
            if !line.trim().is_empty() {
                output.push_str(&format!("  {line}\n"));
            }
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_errors() {
        let output = "before\nwarning: boom\nafter";
        let filtered = filter_errors(output);
        assert!(filtered.contains("before"));
        assert!(filtered.contains("warning: boom"));
        assert!(filtered.contains("after"));
    }
}
