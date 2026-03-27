use crate::tracking;
use crate::utils::detect_package_manager;
use crate::utils::resolved_command;
use crate::utils::strip_ansi;
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
use crate::parser::truncate_output;

/// 匹配真实的 Playwright JSON reporter 输出（suites → specs → tests → results）
#[derive(Debug, Deserialize)]
struct PlaywrightJsonOutput {
    stats: PlaywrightStats,
    #[serde(default)]
    suites: Vec<PlaywrightSuite>,
}

#[derive(Debug, Deserialize)]
struct PlaywrightStats {
    expected: usize,
    unexpected: usize,
    skipped: usize,
    /// 耗时，单位为毫秒（真实 Playwright 输出中是浮点数）
    #[serde(default)]
    duration: f64,
}

/// 文件级或 describe 级别的 suite
#[derive(Debug, Deserialize)]
struct PlaywrightSuite {
    title: String,
    #[serde(default)]
    file: Option<String>,
    /// 单个测试 spec（测试函数）
    #[serde(default)]
    specs: Vec<PlaywrightSpec>,
    /// 嵌套的 describe 块
    #[serde(default)]
    suites: Vec<PlaywrightSuite>,
}

/// 单个测试函数（可能在多个浏览器/项目中运行）
#[derive(Debug, Deserialize)]
struct PlaywrightSpec {
    title: String,
    /// 跨所有项目的整体通过/失败状态
    ok: bool,
    /// 按项目/浏览器区分的执行记录
    #[serde(default)]
    tests: Vec<PlaywrightExecution>,
}

/// 某个浏览器/项目中的一次测试执行
#[derive(Debug, Deserialize)]
struct PlaywrightExecution {
    /// `"expected"`、`"unexpected"`、`"skipped"`、`"flaky"`
    status: String,
    #[serde(default)]
    results: Vec<PlaywrightAttempt>,
}

/// 一次测试执行中的单次尝试/结果
#[derive(Debug, Deserialize)]
struct PlaywrightAttempt {
    /// `"passed"`、`"failed"`、`"timedOut"`、`"interrupted"`
    status: String,
    /// 错误详情（Playwright >= v1.30 中为数组）
    #[serde(default)]
    errors: Vec<PlaywrightError>,
}

#[derive(Debug, Deserialize)]
struct PlaywrightError {
    #[serde(default)]
    message: String,
}

/// Playwright JSON 输出解析器
pub struct PlaywrightParser;

impl OutputParser for PlaywrightParser {
    type Output = TestResult;

    fn parse(input: &str) -> ParseResult<TestResult> {
        // 第 1 层：尝试解析 JSON
        match serde_json::from_str::<PlaywrightJsonOutput>(input) {
            Ok(json) => {
                let mut failures = Vec::new();
                let mut total = 0;
                collect_test_results(&json.suites, &mut total, &mut failures);

                let result = TestResult {
                    total,
                    passed: json.stats.expected,
                    failed: json.stats.unexpected,
                    skipped: json.stats.skipped,
                    duration_ms: Some(json.stats.duration as u64),
                    failures,
                };

                ParseResult::Full(result)
            }
            Err(e) => {
                // 第 2 层：尝试用正则提取
                match extract_playwright_regex(input) {
                    Some(result) => {
                        ParseResult::Degraded(result, vec![format!("JSON 解析失败：{e}")])
                    }
                    None => {
                        // 第 3 层：直接透传
                        ParseResult::Passthrough(truncate_output(input, /*max_chars*/ 500))
                    }
                }
            }
        }
    }
}

fn collect_test_results(
    suites: &[PlaywrightSuite],
    total: &mut usize,
    failures: &mut Vec<TestFailure>,
) {
    for suite in suites {
        let file_path = suite.file.as_deref().unwrap_or(&suite.title);

        for spec in &suite.specs {
            *total += 1;

            if !spec.ok {
                // 找出第一个失败的执行记录及其错误信息
                let error_msg = spec
                    .tests
                    .iter()
                    .find(|t| t.status == "unexpected")
                    .and_then(|t| {
                        t.results
                            .iter()
                            .find(|r| r.status == "failed" || r.status == "timedOut")
                    })
                    .and_then(|r| r.errors.first())
                    .map(|e| e.message.clone())
                    .unwrap_or_else(|| "测试失败".to_string());

                failures.push(TestFailure {
                    test_name: spec.title.clone(),
                    file_path: file_path.to_string(),
                    error_message: error_msg,
                    stack_trace: None,
                });
            }
        }

        // 递归处理嵌套 suite（describe 块）
        collect_test_results(&suite.suites, total, failures);
    }
}

/// 第 2 层：使用正则提取测试统计信息（降级模式）
fn extract_playwright_regex(output: &str) -> Option<TestResult> {
    lazy_static::lazy_static! {
        static ref SUMMARY_RE: Regex = crate::utils::compile_regex(
            r"(\d+)\s+(passed|failed|flaky|skipped)"
        );
        static ref DURATION_RE: Regex = crate::utils::compile_regex(
            r"\((\d+(?:\.\d+)?)(ms|s|m)\)"
        );
    }

    let clean_output = strip_ansi(output);

    let mut passed = 0;
    let mut failed = 0;
    let mut skipped = 0;

    // 解析摘要计数
    for caps in SUMMARY_RE.captures_iter(&clean_output) {
        let count: usize = caps[1].parse().unwrap_or(0);
        match &caps[2] {
            "passed" => passed = count,
            "failed" => failed = count,
            "skipped" => skipped = count,
            _ => {}
        }
    }

    // 解析耗时
    let duration_ms = DURATION_RE.captures(&clean_output).and_then(|caps| {
        let value: f64 = caps[1].parse().ok()?;
        let unit = &caps[2];
        Some(match unit {
            "ms" => value as u64,
            "s" => (value * 1000.0) as u64,
            "m" => (value * 60000.0) as u64,
            _ => value as u64,
        })
    });

    // 仅在提取到有效数据时返回
    let total = passed + failed + skipped;
    if total > 0 {
        Some(TestResult {
            total,
            passed,
            failed,
            skipped,
            duration_ms,
            failures: extract_failures_regex(&clean_output),
        })
    } else {
        None
    }
}

/// 使用正则提取失败项
fn extract_failures_regex(output: &str) -> Vec<TestFailure> {
    lazy_static::lazy_static! {
        static ref TEST_PATTERN: Regex = crate::utils::compile_regex(
            r"[×✗]\s+.*?›\s+([^›]+\.spec\.[tj]sx?)"
        );
    }

    let mut failures = Vec::new();

    for caps in TEST_PATTERN.captures_iter(output) {
        if let Some(spec) = caps.get(1) {
            failures.push(TestFailure {
                test_name: caps[0].to_string(),
                file_path: spec.as_str().to_string(),
                error_message: String::new(),
                stack_trace: None,
            });
        }
    }

    failures
}

pub fn run(args: &[String], verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    // 不使用 `which playwright`，它可能命中 pyenv shim 或其他非 Node
    // 可执行文件。统一通过包管理器解析。
    let pm = detect_package_manager();
    let mut cmd = match pm {
        "pnpm" => {
            let mut c = resolved_command("pnpm");
            c.arg("exec").arg("--").arg("playwright");
            c
        }
        "yarn" => {
            let mut c = resolved_command("yarn");
            c.arg("exec").arg("--").arg("playwright");
            c
        }
        _ => {
            let mut c = resolved_command("npx");
            c.arg("--no-install").arg("--").arg("playwright");
            c
        }
    };

    // 仅在 `playwright test` 时注入 `--reporter=json`
    let is_test = args.first().map(|a| a == "test").unwrap_or(false);
    if is_test {
        cmd.arg("test");
        cmd.arg("--reporter=json");
        // 移除用户传入的 --reporter，避免与强制 JSON reporter 冲突
        for arg in &args[1..] {
            if !arg.starts_with("--reporter") {
                cmd.arg(arg);
            }
        }
    } else {
        for arg in args {
            cmd.arg(arg);
        }
    }

    if verbose > 0 {
        eprintln!("运行：playwright {}", args.join(" "));
    }

    let output = cmd
        .output()
        .context("运行 playwright 失败（可尝试：npm install -g playwright）")?;

    let stdout = crate::utils::decode_output(&output.stdout);
    let stderr = crate::utils::decode_output(&output.stderr);
    let raw = format!("{stdout}\n{stderr}");

    // 使用 PlaywrightParser 解析输出
    let parse_result = PlaywrightParser::parse(&stdout);
    let mode = FormatMode::from_verbosity(verbose);

    let filtered = match parse_result {
        ParseResult::Full(data) => {
            if verbose > 0 {
                eprintln!("playwright test（Tier 1：完整 JSON 解析）");
            }
            data.format(mode)
        }
        ParseResult::Degraded(data, warnings) => {
            if verbose > 0 {
                emit_degradation_warning("playwright", &warnings.join(", "));
            }
            data.format(mode)
        }
        ParseResult::Passthrough(raw) => {
            emit_passthrough_warning("playwright", "所有解析层级均失败");
            raw
        }
    };

    println!("{filtered}");

    timer.track(
        &format!("playwright {}", args.join(" ")),
        &format!("rtk playwright {}", args.join(" ")),
        &raw,
        &filtered,
    );

    // 保留退出码，方便 CI/CD 使用
    if !output.status.success() {
        std::process::exit(output.status.code().unwrap_or(1));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_playwright_parser_json() {
        // 真实 Playwright JSON 结构：suites → specs，duration 为浮点数
        let json = r#"{
            "config": {},
            "stats": {
                "startTime": "2026-01-01T00:00:00.000Z",
                "expected": 1,
                "unexpected": 0,
                "skipped": 0,
                "flaky": 0,
                "duration": 7300.5
            },
            "suites": [
                {
                    "title": "auth",
                    "specs": [],
                    "suites": [
                        {
                            "title": "login.spec.ts",
                            "specs": [
                                {
                                    "title": "should login",
                                    "ok": true,
                                    "tests": [
                                        {
                                            "status": "expected",
                                            "results": [{"status": "passed", "errors": [], "duration": 2300}]
                                        }
                                    ]
                                }
                            ],
                            "suites": []
                        }
                    ]
                }
            ],
            "errors": []
        }"#;

        let result = PlaywrightParser::parse(json);
        assert_eq!(result.tier(), 1);
        assert!(result.is_ok());

        let data = result.unwrap();
        assert_eq!(data.passed, 1);
        assert_eq!(data.failed, 0);
        assert_eq!(data.duration_ms, Some(7300));
    }

    #[test]
    fn test_playwright_parser_json_float_duration() {
        // 真实 Playwright 输出使用浮点数 duration（例如 3519.7039999999997）
        let json = r#"{
            "stats": {
                "startTime": "2026-02-18T10:17:53.187Z",
                "expected": 4,
                "unexpected": 0,
                "skipped": 0,
                "flaky": 0,
                "duration": 3519.7039999999997
            },
            "suites": [],
            "errors": []
        }"#;

        let result = PlaywrightParser::parse(json);
        assert_eq!(result.tier(), 1);
        assert!(result.is_ok());

        let data = result.unwrap();
        assert_eq!(data.passed, 4);
        assert_eq!(data.duration_ms, Some(3519));
    }

    #[test]
    fn test_playwright_parser_json_with_failure() {
        let json = r#"{
            "stats": {
                "expected": 0,
                "unexpected": 1,
                "skipped": 0,
                "duration": 1500.0
            },
            "suites": [
                {
                    "title": "my.spec.ts",
                    "specs": [
                        {
                            "title": "should work",
                            "ok": false,
                            "tests": [
                                {
                                    "status": "unexpected",
                                    "results": [
                                        {
                                            "status": "failed",
                                            "errors": [{"message": "Expected true to be false"}],
                                            "duration": 500
                                        }
                                    ]
                                }
                            ]
                        }
                    ],
                    "suites": []
                }
            ],
            "errors": []
        }"#;

        let result = PlaywrightParser::parse(json);
        assert_eq!(result.tier(), 1);
        assert!(result.is_ok());

        let data = result.unwrap();
        assert_eq!(data.failed, 1);
        assert_eq!(data.failures.len(), 1);
        assert_eq!(data.failures[0].test_name, "should work");
        assert_eq!(data.failures[0].error_message, "Expected true to be false");
    }

    #[test]
    fn test_playwright_parser_regex_fallback() {
        let text = "3 passed (7.3s)";
        let result = PlaywrightParser::parse(text);
        assert_eq!(result.tier(), 2); // 降级解析
        assert!(result.is_ok());

        let data = result.unwrap();
        assert_eq!(data.passed, 3);
        assert_eq!(data.failed, 0);
    }

    #[test]
    fn test_playwright_parser_passthrough() {
        let invalid = "random output";
        let result = PlaywrightParser::parse(invalid);
        assert_eq!(result.tier(), 3); // 透传
        assert!(!result.is_ok());
    }
}
