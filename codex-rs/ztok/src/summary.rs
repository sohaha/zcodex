use crate::compression;
use crate::compression::CompressionHint;
use crate::compression::CompressionIntent;
use crate::compression::CompressionRequest;
use crate::compression::JsonRenderMode;
use crate::compression::LogRenderMode;
use crate::tracking;
use crate::utils::truncate;
use anyhow::Context;
use anyhow::Result;
use regex::Regex;
use std::process::Command;
use std::process::Stdio;

/// 运行命令并给出启发式摘要
pub fn run(command: &str, verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    if verbose > 0 {
        eprintln!("运行并摘要：{command}");
    }

    let output = if cfg!(target_os = "windows") {
        Command::new("cmd")
            .args(["/C", command])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
    } else {
        Command::new("sh")
            .args(["-c", command])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
    }
    .context("执行命令失败")?;

    let stdout = crate::utils::decode_output(&output.stdout);
    let stderr = crate::utils::decode_output(&output.stderr);
    let raw = format!("{stdout}\n{stderr}");

    let summary = summarize_output(&raw, command, output.status.success());
    println!("{summary}");
    timer.track(command, "ztok summary", &raw, &summary);
    if !output.status.success() {
        std::process::exit(output.status.code().unwrap_or(1));
    }
    Ok(())
}

fn summarize_output(output: &str, command: &str, success: bool) -> String {
    let line_count = output.lines().count();
    let mut result = Vec::new();

    // 状态
    let status_icon = if success { "✅" } else { "❌" };
    result.push(format!(
        "{} 命令：{}",
        status_icon,
        truncate(command, /*max_len*/ 60)
    ));
    result.push(format!("   {line_count} 行输出"));
    result.push(String::new());

    // 判断输出类型并生成对应摘要
    let output_type = detect_output_type(output, command);

    match output_type {
        OutputType::TestResults => summarize_tests(output, &mut result),
        OutputType::BuildOutput => summarize_build(output, &mut result),
        OutputType::LogOutput => summarize_logs_quick(output, &mut result),
        OutputType::ListOutput => summarize_list(output, &mut result),
        OutputType::JsonOutput => summarize_json(output, &mut result),
        OutputType::Generic => summarize_generic(output, &mut result),
    }

    result.join("\n")
}

#[derive(Debug)]
enum OutputType {
    TestResults,
    BuildOutput,
    LogOutput,
    ListOutput,
    JsonOutput,
    Generic,
}

fn detect_output_type(output: &str, command: &str) -> OutputType {
    let cmd_lower = command.to_lowercase();
    let out_lower = output.to_lowercase();

    if cmd_lower.contains("test") || out_lower.contains("passed") && out_lower.contains("failed") {
        OutputType::TestResults
    } else if cmd_lower.contains("build")
        || cmd_lower.contains("compile")
        || out_lower.contains("compiling")
    {
        OutputType::BuildOutput
    } else if out_lower.contains("error:")
        || out_lower.contains("warn:")
        || out_lower.contains("[info]")
    {
        OutputType::LogOutput
    } else if output.trim_start().starts_with('{') || output.trim_start().starts_with('[') {
        OutputType::JsonOutput
    } else if output.lines().all(|line| {
        let short_enough = line.len() < 200;
        let compact_columns = if line.contains('\t') {
            false
        } else {
            line.split_whitespace().count() < 10
        };
        short_enough && compact_columns
    }) {
        OutputType::ListOutput
    } else {
        OutputType::Generic
    }
}

fn summarize_tests(output: &str, result: &mut Vec<String>) {
    result.push("📋 测试结果：".to_string());

    let mut passed = 0;
    let mut failed = 0;
    let mut skipped = 0;
    let mut failures = Vec::new();

    for line in output.lines() {
        let lower = line.to_lowercase();
        if lower.contains("passed") || lower.contains("✓") || lower.contains("ok") {
            // 尝试提取数量
            if let Some(n) = extract_number(&lower, "passed") {
                passed = n;
            } else {
                passed += 1;
            }
        }
        if lower.contains("failed") || lower.contains("✗") || lower.contains("fail") {
            if let Some(n) = extract_number(&lower, "failed") {
                failed = n;
            }
            if !line.contains("0 failed") {
                failures.push(line.to_string());
            }
        }
        if (lower.contains("skipped") || lower.contains("ignored"))
            && let Some(n) = extract_number(&lower, "skipped").or(extract_number(&lower, "ignored"))
        {
            skipped = n;
        }
    }

    result.push(format!("   {passed} 通过"));
    if failed > 0 {
        result.push(format!("   {failed} 失败"));
    }
    if skipped > 0 {
        result.push(format!("   ⏭️  {skipped} 跳过"));
    }

    if !failures.is_empty() {
        result.push(String::new());
        result.push("   失败项：".to_string());
        for f in failures.iter().take(5) {
            result.push(format!("   • {}", truncate(f, /*max_len*/ 70)));
        }
    }
}

fn summarize_build(output: &str, result: &mut Vec<String>) {
    result.push("🔨 构建摘要：".to_string());

    let mut errors = 0;
    let mut warnings = 0;
    let mut compiled = 0;
    let mut error_msgs = Vec::new();

    for line in output.lines() {
        let lower = line.to_lowercase();
        if lower.contains("error") && !lower.contains("0 error") {
            errors += 1;
            if error_msgs.len() < 5 {
                error_msgs.push(line.to_string());
            }
        }
        if lower.contains("warning") && !lower.contains("0 warning") {
            warnings += 1;
        }
        if lower.contains("compiling") || lower.contains("compiled") {
            compiled += 1;
        }
    }

    if compiled > 0 {
        result.push(format!("   已编译 {compiled} 个 crate/文件"));
    }
    if errors > 0 {
        result.push(format!("   {errors} 个错误"));
    }
    if warnings > 0 {
        result.push(format!("   {warnings} 个警告"));
    }
    if errors == 0 && warnings == 0 {
        result.push("   构建成功".to_string());
    }

    if !error_msgs.is_empty() {
        result.push(String::new());
        result.push("   错误：".to_string());
        for e in &error_msgs {
            result.push(format!("   • {}", truncate(e, /*max_len*/ 70)));
        }
    }
}

fn summarize_logs_quick(output: &str, result: &mut Vec<String>) {
    let summary = compression::compress(CompressionRequest {
        source_name: "summary.log",
        content: output,
        hint: CompressionHint::Log,
        intent: CompressionIntent::Log {
            mode: LogRenderMode::Summary,
        },
    })
    .map(|compressed| compressed.output)
    .unwrap_or_else(|_| {
        let mut fallback = vec!["日志摘要：".to_string()];
        fallback.push("   无法解析日志摘要".to_string());
        fallback.join("\n")
    });
    result.extend(summary.lines().map(str::to_string));
}

fn summarize_list(output: &str, result: &mut Vec<String>) {
    let lines: Vec<&str> = output.lines().filter(|l| !l.trim().is_empty()).collect();
    result.push(format!("📋 列表（{} 项）：", lines.len()));

    for line in lines.iter().take(10) {
        result.push(format!("   • {}", truncate(line, /*max_len*/ 70)));
    }
    if lines.len() > 10 {
        result.push(format!("   ... +{} 项", lines.len() - 10));
    }
}

fn summarize_json(output: &str, result: &mut Vec<String>) {
    let summary = compression::compress(CompressionRequest {
        source_name: "summary.json",
        content: output,
        hint: CompressionHint::Json,
        intent: CompressionIntent::Json {
            max_depth: 5,
            mode: JsonRenderMode::Summary,
        },
    })
    .map(|compressed| compressed.output)
    .unwrap_or_else(|_| "   JSON 输出：\n   （JSON 无效）".to_string());
    result.extend(summary.lines().map(str::to_string));
}

fn summarize_generic(output: &str, result: &mut Vec<String>) {
    let lines: Vec<&str> = output.lines().collect();

    result.push("📋 输出：".to_string());

    // 前几行
    for line in lines.iter().take(5) {
        if !line.trim().is_empty() {
            result.push(format!("   {}", truncate(line, /*max_len*/ 75)));
        }
    }

    if lines.len() > 10 {
        result.push("   ...".to_string());
        // 最后几行
        for line in lines.iter().skip(lines.len() - 3) {
            if !line.trim().is_empty() {
                result.push(format!("   {}", truncate(line, /*max_len*/ 75)));
            }
        }
    }
}

fn extract_number(text: &str, after: &str) -> Option<usize> {
    let re = Regex::new(&format!(r"(\d+)\s*{after}")).ok()?;
    re.captures(text)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse().ok())
}
