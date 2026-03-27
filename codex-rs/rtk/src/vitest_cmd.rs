use anyhow::Context;
use anyhow::Result;
use regex::Regex;
use serde::Deserialize;

use crate::parser::FormatMode;
use crate::parser::OutputParser;
use crate::parser::ParseResult;
use crate::parser::TestFailure;
use crate::parser::TestResult;
use crate::parser::TokenFormatter;
use crate::parser::emit_degradation_warning;
use crate::parser::emit_passthrough_warning;
use crate::parser::extract_json_object;
use crate::parser::truncate_output;
use crate::tracking;
use crate::utils::package_manager_exec;
use crate::utils::strip_ansi;

/// Vitest JSON 输出结构（本工具专用格式）
#[derive(Debug, Deserialize)]
struct VitestJsonOutput {
    #[serde(rename = "testResults")]
    test_results: Vec<VitestTestFile>,
    #[serde(rename = "numTotalTests")]
    num_total_tests: usize,
    #[serde(rename = "numPassedTests")]
    num_passed_tests: usize,
    #[serde(rename = "numFailedTests")]
    num_failed_tests: usize,
    #[serde(rename = "numPendingTests", default)]
    num_pending_tests: usize,
    #[serde(rename = "startTime")]
    start_time: Option<u64>,
    #[serde(rename = "endTime")]
    end_time: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct VitestTestFile {
    name: String,
    #[serde(rename = "assertionResults")]
    assertion_results: Vec<VitestTest>,
}

#[derive(Debug, Deserialize)]
struct VitestTest {
    #[serde(rename = "fullName")]
    full_name: String,
    status: String,
    #[serde(rename = "failureMessages")]
    failure_messages: Vec<String>,
}

/// Vitest JSON 输出解析器
pub struct VitestParser;

impl OutputParser for VitestParser {
    type Output = TestResult;

    fn parse(input: &str) -> ParseResult<TestResult> {
        // Tier 1：尝试 JSON 解析（支持从 pnpm/dotenv 前缀中回退提取）
        let json_result = serde_json::from_str::<VitestJsonOutput>(input).or_else(|first_err| {
            // 回退方案：尝试从带前缀的输出中提取 JSON 对象
            if let Some(extracted) = extract_json_object(input) {
                serde_json::from_str::<VitestJsonOutput>(extracted)
            } else {
                Err(first_err)
            }
        });

        match json_result {
            Ok(json) => {
                let failures = extract_failures_from_json(&json);
                let duration_ms = match (json.start_time, json.end_time) {
                    (Some(start), Some(end)) => Some(end.saturating_sub(start)),
                    _ => None,
                };

                let result = TestResult {
                    total: json.num_total_tests,
                    passed: json.num_passed_tests,
                    failed: json.num_failed_tests,
                    skipped: json.num_pending_tests,
                    duration_ms,
                    failures,
                };

                ParseResult::Full(result)
            }
            Err(e) => {
                // Tier 2：尝试用正则提取（仅在用户覆盖 --reporter 时触发）
                match extract_stats_regex(input) {
                    Some(result) => {
                        ParseResult::Degraded(result, vec![format!("JSON 解析失败：{e}")])
                    }
                    None => {
                        // Tier 3：直通原始输出
                        ParseResult::Passthrough(truncate_output(input, /*max_chars*/ 500))
                    }
                }
            }
        }
    }
}

/// 从 JSON 结构中提取失败用例
fn extract_failures_from_json(json: &VitestJsonOutput) -> Vec<TestFailure> {
    let mut failures = Vec::new();

    for file in &json.test_results {
        for test in &file.assertion_results {
            if test.status == "failed" {
                let error_message = test.failure_messages.join("\n");
                failures.push(TestFailure {
                    test_name: test.full_name.clone(),
                    file_path: file.name.clone(),
                    error_message,
                    stack_trace: None,
                });
            }
        }
    }

    failures
}

/// Tier 2：通过正则提取测试统计信息（降级模式）
fn extract_stats_regex(output: &str) -> Option<TestResult> {
    lazy_static::lazy_static! {
        static ref TEST_FILES_RE: Regex = crate::utils::compile_regex(
            r"Test Files\s+(?:(\d+)\s+failed\s+\|\s+)?(\d+)\s+passed"
        );
        static ref TESTS_RE: Regex = crate::utils::compile_regex(
            r"Tests\s+(?:(\d+)\s+failed\s+\|\s+)?(\d+)\s+passed"
        );
        static ref DURATION_RE: Regex = crate::utils::compile_regex(
            r"Duration\s+([\d.]+)(ms|s)"
        );
    }

    let clean_output = strip_ansi(output);

    let mut passed = 0;
    let mut failed = 0;
    let mut total = 0;

    // 解析测试数量
    if let Some(caps) = TESTS_RE.captures(&clean_output) {
        if let Some(fail_str) = caps.get(1) {
            failed = fail_str.as_str().parse().unwrap_or(0);
        }
        if let Some(pass_str) = caps.get(2) {
            passed = pass_str.as_str().parse().unwrap_or(0);
        }
        total = passed + failed;
    }

    // 解析耗时
    let duration_ms = DURATION_RE.captures(&clean_output).and_then(|caps| {
        let value: f64 = caps[1].parse().ok()?;
        let unit = &caps[2];
        Some(if unit == "ms" {
            value as u64
        } else {
            (value * 1000.0) as u64
        })
    });

    // 仅在成功提取到有效数据时返回结果
    if total > 0 {
        Some(TestResult {
            total,
            passed,
            failed,
            skipped: 0,
            duration_ms,
            failures: extract_failures_regex(&clean_output),
        })
    } else {
        None
    }
}

/// 使用正则提取失败项
fn extract_failures_regex(output: &str) -> Vec<TestFailure> {
    let mut failures = Vec::new();
    let lines: Vec<&str> = output.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        if line.contains('✗') || line.contains("FAIL") {
            let mut error_lines = vec![line.to_string()];
            i += 1;

            // 收集后续缩进行
            while i < lines.len() && lines[i].starts_with("  ") {
                error_lines.push(lines[i].trim().to_string());
                i += 1;
            }

            if !error_lines.is_empty() {
                failures.push(TestFailure {
                    test_name: error_lines[0].clone(),
                    file_path: String::new(),
                    error_message: error_lines[1..].join("\n"),
                    stack_trace: None,
                });
            }
        } else {
            i += 1;
        }
    }

    failures
}

#[derive(Debug, Clone)]
pub enum VitestCommand {
    Run,
}

pub fn run(cmd: VitestCommand, args: &[String], verbose: u8) -> Result<()> {
    match cmd {
        VitestCommand::Run => run_vitest(args, verbose),
    }
}

fn run_vitest(args: &[String], verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    let mut cmd = package_manager_exec("vitest");
    cmd.arg("run"); // 强制关闭 watch 模式

    // 添加 JSON reporter，便于做结构化解析
    cmd.arg("--reporter=json");

    for arg in args {
        cmd.arg(arg);
    }

    let output = cmd.output().context("运行 vitest 失败")?;
    let stdout = crate::utils::decode_output(&output.stdout);
    let stderr = crate::utils::decode_output(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    // 使用 `VitestParser` 解析输出
    let parse_result = VitestParser::parse(&stdout);
    let mode = FormatMode::from_verbosity(verbose);

    let filtered = match parse_result {
        ParseResult::Full(data) => {
            if verbose > 0 {
                eprintln!("vitest run（Tier 1：完整 JSON 解析）");
            }
            data.format(mode)
        }
        ParseResult::Degraded(data, warnings) => {
            if verbose > 0 {
                emit_degradation_warning("vitest", &warnings.join(", "));
            }
            data.format(mode)
        }
        ParseResult::Passthrough(raw) => {
            emit_passthrough_warning("vitest", "所有解析层级均失败");
            raw
        }
    };

    let exit_code = output.status.code().unwrap_or(1);
    if let Some(hint) = crate::tee::tee_and_hint(&combined, "vitest_run", exit_code) {
        println!("{filtered}\n{hint}");
    } else {
        println!("{filtered}");
    }

    timer.track("vitest run", "rtk vitest run", &combined, &filtered);

    // 透传原始退出码
    std::process::exit(exit_code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vitest_parser_json() {
        let json = r#"{
            "numTotalTests": 13,
            "numPassedTests": 13,
            "numFailedTests": 0,
            "numPendingTests": 0,
            "testResults": [],
            "startTime": 1000,
            "endTime": 1450
        }"#;

        let result = VitestParser::parse(json);
        assert_eq!(result.tier(), 1);
        assert!(result.is_ok());

        let data = result.unwrap();
        assert_eq!(data.total, 13);
        assert_eq!(data.passed, 13);
        assert_eq!(data.failed, 0);
        assert_eq!(data.duration_ms, Some(450));
    }

    #[test]
    fn test_vitest_parser_regex_fallback() {
        let text = r#"
 Test Files  2 passed (2)
      Tests  13 passed (13)
   Duration  450ms
        "#;

        let result = VitestParser::parse(text);
        assert_eq!(result.tier(), 2); // 降级解析
        assert!(result.is_ok());

        let data = result.unwrap();
        assert_eq!(data.passed, 13);
        assert_eq!(data.failed, 0);
    }

    #[test]
    fn test_vitest_parser_passthrough() {
        let invalid = "random output with no structure";
        let result = VitestParser::parse(invalid);
        assert_eq!(result.tier(), 3); // 直通原始输出
        assert!(!result.is_ok());
    }

    #[test]
    fn test_strip_ansi() {
        let input = "\x1b[32m✓\x1b[0m 测试通过";
        let output = strip_ansi(input);
        assert_eq!(output, "✓ 测试通过");
        assert!(!output.contains("\x1b"));
    }

    #[test]
    fn test_vitest_parser_with_pnpm_prefix() {
        let input = r#"
Scope: all 6 workspace projects
 WARN  deprecated inflight@1.0.6: This module is not supported

{"numTotalTests": 13, "numPassedTests": 13, "numFailedTests": 0, "numPendingTests": 0, "testResults": [], "startTime": 1000, "endTime": 1450}
"#;
        let result = VitestParser::parse(input);
        assert_eq!(result.tier(), 1, "应以 Tier 1 成功解析（完整解析）");
        assert!(result.is_ok());

        let data = result.unwrap();
        assert_eq!(data.total, 13);
        assert_eq!(data.passed, 13);
        assert_eq!(data.failed, 0);
    }

    #[test]
    fn test_vitest_parser_with_dotenv_prefix() {
        let input = r#"[dotenv] Loading environment variables from .env
[dotenv] Injected 5 variables

{"numTotalTests": 5, "numPassedTests": 4, "numFailedTests": 1, "numPendingTests": 0, "testResults": [], "startTime": 2000, "endTime": 2300}
"#;
        let result = VitestParser::parse(input);
        assert_eq!(result.tier(), 1, "应以 Tier 1 成功解析（完整解析）");
        assert!(result.is_ok());

        let data = result.unwrap();
        assert_eq!(data.total, 5);
        assert_eq!(data.passed, 4);
        assert_eq!(data.failed, 1);
        assert_eq!(data.duration_ms, Some(300));
    }

    #[test]
    fn test_vitest_parser_with_nested_json() {
        let input = r#"prefix text
{"numTotalTests": 2, "numPassedTests": 2, "numFailedTests": 0, "numPendingTests": 0, "testResults": [{"name": "test.js", "assertionResults": [{"fullName": "nested test", "status": "passed", "failureMessages": []}]}], "startTime": 1000, "endTime": 1100}
"#;
        let result = VitestParser::parse(input);
        assert_eq!(result.tier(), 1, "应以 Tier 1 成功解析（完整解析）");
        assert!(result.is_ok());

        let data = result.unwrap();
        assert_eq!(data.total, 2);
        assert_eq!(data.passed, 2);
    }
}
