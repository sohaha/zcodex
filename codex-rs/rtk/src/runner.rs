use crate::tracking;
use anyhow::Context;
use anyhow::Result;
use regex::Regex;
use std::process::Command;
use std::process::Output;
use std::process::Stdio;

/// Run a command and filter output to show only errors/warnings
pub fn run_err(command: &[String], verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();
    let command_display = command.join(" ");

    if verbose > 0 {
        eprintln!("Running: {command_display}");
    }

    let output = execute_command(command).context("Failed to execute command")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let raw = format!("{stdout}\n{stderr}");
    let filtered = filter_errors(&raw);
    let mut rtk = String::new();

    if filtered.is_empty() {
        if output.status.success() {
            rtk.push_str("✅ Command completed successfully (no errors)");
        } else {
            rtk.push_str(&format!(
                "❌ Command failed (exit code: {:?})\n",
                output.status.code()
            ));
            let lines: Vec<&str> = raw.lines().collect();
            for line in lines.iter().rev().take(10).rev() {
                rtk.push_str(&format!("  {line}\n"));
            }
        }
    } else {
        rtk.push_str(&filtered);
    }

    let exit_code = output
        .status
        .code()
        .unwrap_or(if output.status.success() { 0 } else { 1 });
    if let Some(hint) = crate::tee::tee_and_hint(&raw, "err", exit_code) {
        println!("{rtk}\n{hint}");
    } else {
        println!("{rtk}");
    }
    timer.track(&command_display, "rtk run-err", &raw, &rtk);
    if exit_code != 0 {
        std::process::exit(exit_code);
    }
    Ok(())
}

/// Run tests and show only failures
pub fn run_test(command: &[String], verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();
    let command_display = command.join(" ");

    if verbose > 0 {
        eprintln!("Running tests: {command_display}");
    }

    let output = execute_command(command).context("Failed to execute test command")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let raw = format!("{stdout}\n{stderr}");

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
    timer.track(&command_display, "rtk run-test", &raw, &summary);
    if exit_code != 0 {
        std::process::exit(exit_code);
    }
    Ok(())
}

fn execute_command(command: &[String]) -> Result<Output> {
    let (program, args) = command.split_first().context("No command provided")?;

    Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("Failed to spawn command: {program}"))
}

fn filter_errors(output: &str) -> String {
    lazy_static::lazy_static! {
        static ref ERROR_PATTERNS: Vec<Regex> = vec![
            // Generic errors
            Regex::new(r"(?i)^.*error[\s:\[].*$").unwrap(),
            Regex::new(r"(?i)^.*\berr\b.*$").unwrap(),
            Regex::new(r"(?i)^.*warning[\s:\[].*$").unwrap(),
            Regex::new(r"(?i)^.*\bwarn\b.*$").unwrap(),
            Regex::new(r"(?i)^.*failed.*$").unwrap(),
            Regex::new(r"(?i)^.*failure.*$").unwrap(),
            Regex::new(r"(?i)^.*exception.*$").unwrap(),
            Regex::new(r"(?i)^.*panic.*$").unwrap(),
            // Rust specific
            Regex::new(r"^error\[E\d+\]:.*$").unwrap(),
            Regex::new(r"^\s*--> .*:\d+:\d+$").unwrap(),
            // Python
            Regex::new(r"^Traceback.*$").unwrap(),
            Regex::new(r#"^\s*File ".*", line \d+.*$"#).unwrap(),
            // JavaScript/TypeScript
            Regex::new(r"^\s*at .*:\d+:\d+.*$").unwrap(),
            // Go
            Regex::new(r"^.*\.go:\d+:.*$").unwrap(),
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
        .filter_map(|(line, keep)| keep.then(|| (*line).to_string()))
        .collect::<Vec<_>>()
        .join("\n")
}

fn extract_test_summary(output: &str, command: &str) -> String {
    let mut result = Vec::new();
    let lines: Vec<&str> = output.lines().collect();

    // Detect test framework
    let is_cargo = command.contains("cargo test");
    let is_pytest = command.contains("pytest");
    let is_jest =
        command.contains("jest") || command.contains("npm test") || command.contains("yarn test");
    let is_go = command.contains("go test");

    // Collect failures
    let mut failures = Vec::new();
    let mut in_failure = false;
    let mut failure_lines = Vec::new();

    for line in lines.iter() {
        // Cargo test
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

        // Pytest
        if is_pytest {
            if line.contains(" passed") || line.contains(" failed") || line.contains(" error") {
                result.push(line.to_string());
            }
            if line.contains("FAILED") {
                failures.push(line.to_string());
            }
        }

        // Jest
        if is_jest {
            if line.contains("Tests:") || line.contains("Test Suites:") {
                result.push(line.to_string());
            }
            if line.contains("✕") || line.contains("FAIL") {
                failures.push(line.to_string());
            }
        }

        // Go test
        if is_go {
            if line.starts_with("ok") || line.starts_with("FAIL") || line.starts_with("---") {
                result.push(line.to_string());
            }
            if line.contains("FAIL") {
                failures.push(line.to_string());
            }
        }
    }

    // Build output
    let mut output = String::new();

    if !failures.is_empty() {
        output.push_str("❌ FAILURES:\n");
        for f in failures.iter().take(10) {
            output.push_str(&format!("  {f}\n"));
        }
        if failures.len() > 10 {
            output.push_str(&format!("  ... +{} more failures\n", failures.len() - 10));
        }
        output.push('\n');
    }

    if !result.is_empty() {
        output.push_str("📊 SUMMARY:\n");
        for r in &result {
            output.push_str(&format!("  {r}\n"));
        }
    } else {
        // Fallback: show last few lines
        output.push_str("📊 OUTPUT (last 5 lines):\n");
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
