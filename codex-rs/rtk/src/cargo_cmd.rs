use crate::tracking;
use crate::utils::resolved_command;
use crate::utils::truncate;
use anyhow::Context;
use anyhow::Result;
use std::collections::HashMap;
use std::ffi::OsString;
use std::sync::OnceLock;

#[derive(Debug, Clone)]
pub enum CargoCommand {
    Build,
    Test,
    Clippy,
    Check,
    Install,
    Nextest,
}

pub fn run(cmd: CargoCommand, args: &[String], verbose: u8) -> Result<()> {
    match cmd {
        CargoCommand::Build => run_build(args, verbose),
        CargoCommand::Test => run_test(args, verbose),
        CargoCommand::Clippy => run_clippy(args, verbose),
        CargoCommand::Check => run_check(args, verbose),
        CargoCommand::Install => run_install(args, verbose),
        CargoCommand::Nextest => run_nextest(args, verbose),
    }
}

/// 从原始命令行重建参数，并保留 `--` 分隔符。
/// `clap` 会在解析后移除 `--`，但 cargo 子命令需要它来区分
/// 自身参数和测试运行器参数（例如 `cargo test -- --nocapture`）。
fn restore_double_dash(args: &[String]) -> Vec<String> {
    let raw_args: Vec<String> = std::env::args().collect();
    restore_double_dash_with_raw(args, &raw_args)
}

/// 可测试版本，显式接收 `raw_args`。
fn restore_double_dash_with_raw(args: &[String], raw_args: &[String]) -> Vec<String> {
    if args.is_empty() {
        return args.to_vec();
    }

    // 在原始命令行中查找 `--`
    let sep_pos = match raw_args.iter().position(|a| a == "--") {
        Some(pos) => pos,
        None => return args.to_vec(),
    };

    // 统计解析后的参数中，有多少原本位于 `--` 之前。
    // `--` 前通常是位置参数（如测试名），之后通常是标志位。
    let args_before_sep = raw_args[..sep_pos]
        .iter()
        .filter(|a| args.contains(a))
        .count();

    let mut result = Vec::with_capacity(args.len() + 1);
    result.extend_from_slice(&args[..args_before_sep]);
    result.push("--".to_string());
    result.extend_from_slice(&args[args_before_sep..]);
    result
}

/// 带过滤逻辑的通用 cargo 命令执行器
fn run_cargo_filtered<F>(subcommand: &str, args: &[String], verbose: u8, filter_fn: F) -> Result<()>
where
    F: Fn(&str) -> String,
{
    let timer = tracking::TimedExecution::start();

    let mut cmd = resolved_command("cargo");
    cmd.arg(subcommand);

    let restored_args = restore_double_dash(args);
    for arg in &restored_args {
        cmd.arg(arg);
    }

    if verbose > 0 {
        eprintln!("运行：cargo {} {}", subcommand, restored_args.join(" "));
    }

    let output = cmd
        .output()
        .with_context(|| format!("运行 cargo {subcommand} 失败"))?;
    let stdout = crate::utils::decode_output(&output.stdout);
    let stderr = crate::utils::decode_output(&output.stderr);
    let raw = format!("{stdout}\n{stderr}");

    let exit_code = output
        .status
        .code()
        .unwrap_or(if output.status.success() { 0 } else { 1 });
    let filtered = filter_fn(&raw);

    if let Some(hint) = crate::tee::tee_and_hint(&raw, &format!("cargo_{subcommand}"), exit_code) {
        println!("{filtered}\n{hint}");
    } else {
        println!("{filtered}");
    }

    timer.track(
        &format!("cargo {} {}", subcommand, restored_args.join(" ")),
        &format!("rtk cargo {} {}", subcommand, restored_args.join(" ")),
        &raw,
        &filtered,
    );

    if !output.status.success() {
        std::process::exit(exit_code);
    }

    Ok(())
}

fn run_build(args: &[String], verbose: u8) -> Result<()> {
    run_cargo_filtered("build", args, verbose, filter_cargo_build)
}

fn run_test(args: &[String], verbose: u8) -> Result<()> {
    run_cargo_filtered("test", args, verbose, filter_cargo_test)
}

fn run_clippy(args: &[String], verbose: u8) -> Result<()> {
    run_cargo_filtered("clippy", args, verbose, filter_cargo_clippy)
}

fn run_check(args: &[String], verbose: u8) -> Result<()> {
    run_cargo_filtered("check", args, verbose, filter_cargo_build)
}

fn run_install(args: &[String], verbose: u8) -> Result<()> {
    run_cargo_filtered("install", args, verbose, filter_cargo_install)
}

fn run_nextest(args: &[String], verbose: u8) -> Result<()> {
    run_cargo_filtered("nextest", args, verbose, filter_cargo_nextest)
}

/// 将 crate 名称和版本格式化为展示字符串
fn format_crate_info(name: &str, version: &str, fallback: &str) -> String {
    if name.is_empty() {
        fallback.to_string()
    } else if version.is_empty() {
        name.to_string()
    } else {
        format!("{name} {version}")
    }
}

/// 过滤 `cargo install` 输出：移除依赖编译噪音，保留安装/替换/错误信息
fn filter_cargo_install(output: &str) -> String {
    let mut errors: Vec<String> = Vec::new();
    let mut error_count = 0;
    let mut compiled = 0;
    let mut in_error = false;
    let mut current_error = Vec::new();
    let mut installed_crate = String::new();
    let mut installed_version = String::new();
    let mut replaced_lines: Vec<String> = Vec::new();
    let mut already_installed = false;
    let mut ignored_line = String::new();

    for line in output.lines() {
        let trimmed = line.trim_start();

        // 去除噪音：依赖编译、下载、锁定等
        if trimmed.starts_with("Compiling") {
            compiled += 1;
            continue;
        }
        if trimmed.starts_with("Downloading")
            || trimmed.starts_with("Downloaded")
            || trimmed.starts_with("Locking")
            || trimmed.starts_with("Updating")
            || trimmed.starts_with("Adding")
            || trimmed.starts_with("Finished")
            || trimmed.starts_with("Blocking waiting for file lock")
        {
            continue;
        }

        // 保留：Installing 行（提取 crate 名称和版本）
        if trimmed.starts_with("Installing") {
            let rest = trimmed.strip_prefix("Installing").unwrap_or("").trim();
            if !rest.is_empty() && !rest.starts_with('/') {
                if let Some((name, version)) = rest.split_once(' ') {
                    installed_crate = name.to_string();
                    installed_version = version.to_string();
                } else {
                    installed_crate = rest.to_string();
                }
            }
            continue;
        }

        // 保留：Installed 行（如果尚未记录则提取 crate 和版本）
        if trimmed.starts_with("Installed") {
            let rest = trimmed.strip_prefix("Installed").unwrap_or("").trim();
            if !rest.is_empty() && installed_crate.is_empty() {
                let mut parts = rest.split_whitespace();
                if let (Some(name), Some(version)) = (parts.next(), parts.next()) {
                    installed_crate = name.to_string();
                    installed_version = version.to_string();
                }
            }
            continue;
        }

        // 保留：Replacing/Replaced 行
        if trimmed.starts_with("Replacing") || trimmed.starts_with("Replaced") {
            replaced_lines.push(trimmed.to_string());
            continue;
        }

        // 保留："Ignored package"（已是最新）
        if trimmed.starts_with("Ignored package") {
            already_installed = true;
            ignored_line = trimmed.to_string();
            continue;
        }

        // 保留可操作的警告（例如 "be sure to add `/path` to your PATH"）
        // 跳过类似 "warning: `crate` generated N warnings" 的摘要行
        if line.starts_with("warning:") {
            if !(line.contains("generated") && line.contains("warning")) {
                replaced_lines.push(line.to_string());
            }
            continue;
        }

        // 识别错误块
        if line.starts_with("error[") || line.starts_with("error:") {
            if line.contains("aborting due to") || line.contains("could not compile") {
                continue;
            }
            if in_error && !current_error.is_empty() {
                errors.push(current_error.join("\n"));
                current_error.clear();
            }
            error_count += 1;
            in_error = true;
            current_error.push(line.to_string());
        } else if in_error {
            if line.trim().is_empty() && current_error.len() > 3 {
                errors.push(current_error.join("\n"));
                current_error.clear();
                in_error = false;
            } else {
                current_error.push(line.to_string());
            }
        }
    }

    if !current_error.is_empty() {
        errors.push(current_error.join("\n"));
    }

    // 已安装 / 已是最新
    if already_installed {
        let info = ignored_line.split('`').nth(1).unwrap_or(&ignored_line);
        return format!("✓ cargo install：{info} 已安装");
    }

    // 错误
    if error_count > 0 {
        let crate_info = format_crate_info(&installed_crate, &installed_version, "");
        let deps_info = if compiled > 0 {
            format!("，已编译 {compiled} 个依赖")
        } else {
            String::new()
        };

        let mut result = String::new();
        if crate_info.is_empty() {
            result.push_str(&format!("cargo install：{error_count} 个错误{deps_info}\n"));
        } else {
            result.push_str(&format!(
                "cargo install：{error_count} 个错误（{crate_info}{deps_info}）\n"
            ));
        }
        result.push_str("═══════════════════════════════════════\n");

        for (i, err) in errors.iter().enumerate().take(15) {
            result.push_str(err);
            result.push('\n');
            if i < errors.len() - 1 {
                result.push('\n');
            }
        }

        if errors.len() > 15 {
            result.push_str(&format!("\n... +{} 个问题\n", errors.len() - 15));
        }

        return result.trim().to_string();
    }

    // 成功
    let crate_info = format_crate_info(&installed_crate, &installed_version, "包");

    let mut result = format!("✓ cargo install（{crate_info}，已编译 {compiled} 个依赖）");

    for line in &replaced_lines {
        result.push_str(&format!("\n  {line}"));
    }

    result
}

/// 将完整的失败块（标题 + 正文）加入列表，然后清空缓冲区。
fn flush_failure_block(header: &mut String, body: &mut Vec<String>, failures: &mut Vec<String>) {
    if header.is_empty() {
        return;
    }
    let mut block = header.clone();
    if !body.is_empty() {
        block.push('\n');
        block.push_str(&body.join("\n"));
    }
    failures.push(block);
    header.clear();
    body.clear();
}

/// 过滤 `cargo nextest` 输出：展示失败项和紧凑摘要
fn filter_cargo_nextest(output: &str) -> String {
    static SUMMARY_RE: OnceLock<regex::Regex> = OnceLock::new();
    let summary_re = SUMMARY_RE.get_or_init(|| {
        crate::utils::compile_regex(
            r"Summary \[\s*([\d.]+)s\]\s+(\d+) tests? run:\s+(\d+) passed(?:,\s+(\d+) failed)?(?:,\s+(\d+) skipped)?"
        )
    });

    static STARTING_RE: OnceLock<regex::Regex> = OnceLock::new();
    let starting_re = STARTING_RE.get_or_init(|| {
        crate::utils::compile_regex(r"Starting \d+ tests? across (\d+) binar(?:y|ies)")
    });

    let mut failures: Vec<String> = Vec::new();
    let mut in_failure_block = false;
    let mut past_summary = false;
    let mut current_failure_header = String::new();
    let mut current_failure_body = Vec::new();
    let mut summary_line = String::new();
    let mut binaries: u32 = 0;
    let mut has_cancel_line = false;

    for line in output.lines() {
        let trimmed = line.trim();

        // 去除编译噪音
        if trimmed.starts_with("Compiling")
            || trimmed.starts_with("Downloading")
            || trimmed.starts_with("Downloaded")
            || trimmed.starts_with("Finished")
            || trimmed.starts_with("Locking")
            || trimmed.starts_with("Updating")
        {
            continue;
        }

        // 去除分隔线（────）
        if trimmed.starts_with("────") {
            continue;
        }

        // 跳过摘要后的重复回顾行（重复 FAIL + "error: test run failed"）
        if past_summary {
            continue;
        }

        // 从 Starting 行解析二进制数量
        if trimmed.starts_with("Starting") {
            if let Some(caps) = starting_re.captures(trimmed)
                && let Some(m) = caps.get(1)
            {
                binaries = m.as_str().parse().unwrap_or(0);
            }
            continue;
        }

        // 去除 PASS 行
        if trimmed.starts_with("PASS") {
            if in_failure_block {
                flush_failure_block(
                    &mut current_failure_header,
                    &mut current_failure_body,
                    &mut failures,
                );
                in_failure_block = false;
            }
            continue;
        }

        // 识别 FAIL 行
        if trimmed.starts_with("FAIL") {
            // 如有上一个失败块，先收尾
            if in_failure_block {
                flush_failure_block(
                    &mut current_failure_header,
                    &mut current_failure_body,
                    &mut failures,
                );
            }
            current_failure_header = trimmed.to_string();
            in_failure_block = true;
            continue;
        }

        // 取消提示
        if trimmed.starts_with("Cancelling") || trimmed.starts_with("Canceling") {
            has_cancel_line = true;
            continue;
        }

        // `nextest` 的 run ID 行
        if trimmed.starts_with("Nextest run ID") {
            continue;
        }

        // 解析摘要
        if trimmed.starts_with("Summary") {
            summary_line = trimmed.to_string();
            if in_failure_block {
                flush_failure_block(
                    &mut current_failure_header,
                    &mut current_failure_body,
                    &mut failures,
                );
                in_failure_block = false;
            }
            past_summary = true;
            continue;
        }

        // 收集失败正文（stdout/stderr 段落）
        if in_failure_block {
            current_failure_body.push(line.to_string());
        }
    }

    // 收尾最后一个失败块
    if in_failure_block {
        flush_failure_block(
            &mut current_failure_header,
            &mut current_failure_body,
            &mut failures,
        );
    }

    // 用正则解析摘要
    if let Some(caps) = summary_re.captures(&summary_line) {
        let duration = caps.get(1).map_or("?", |m| m.as_str());
        let passed: u32 = caps
            .get(3)
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(0);
        let failed: u32 = caps
            .get(4)
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(0);
        let skipped: u32 = caps
            .get(5)
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(0);

        let binary_text = if binaries == 1 {
            "1 个二进制".to_string()
        } else if binaries > 1 {
            format!("{binaries} 个二进制")
        } else {
            String::new()
        };

        if failed == 0 {
            // 全部通过：输出紧凑单行
            let mut parts = vec![format!("{passed} 通过")];
            if skipped > 0 {
                parts.push(format!("{skipped} 跳过"));
            }
            let meta = if binary_text.is_empty() {
                format!("{duration}s")
            } else {
                format!("{binary_text}, {duration}s")
            };
            return format!("✓ cargo nextest：{}（{}）", parts.join(", "), meta);
        }

        // 有失败：先展示失败详情，再展示摘要
        let mut result = String::new();

        for failure in &failures {
            result.push_str(failure);
            result.push('\n');
        }

        if has_cancel_line {
            result.push_str("因测试失败已取消\n");
        }

        let mut summary_parts = vec![format!("{passed} 通过")];
        if failed > 0 {
            summary_parts.push(format!("{failed} 失败"));
        }
        if skipped > 0 {
            summary_parts.push(format!("{skipped} 跳过"));
        }
        let meta = if binary_text.is_empty() {
            format!("{duration}s")
        } else {
            format!("{binary_text}, {duration}s")
        };
        result.push_str(&format!(
            "cargo nextest：{}（{}）",
            summary_parts.join(", "),
            meta
        ));

        return result.trim().to_string();
    }

    // 回退：如果摘要正则未匹配，则输出已收集内容
    if !failures.is_empty() {
        let mut result = String::new();
        for failure in &failures {
            result.push_str(failure);
            result.push('\n');
        }
        if !summary_line.is_empty() {
            result.push_str(&summary_line);
        }
        return result.trim().to_string();
    }

    if !summary_line.is_empty() {
        return summary_line;
    }

    // 空输出或无法识别
    String::new()
}

/// 过滤 `cargo build/check` 输出：移除 "Compiling"/"Checking" 行，保留错误和摘要
fn filter_cargo_build(output: &str) -> String {
    let mut errors: Vec<String> = Vec::new();
    let mut warnings = 0;
    let mut error_count = 0;
    let mut compiled = 0;
    let mut in_error = false;
    let mut current_error = Vec::new();

    for line in output.lines() {
        if line.trim_start().starts_with("Compiling") || line.trim_start().starts_with("Checking") {
            compiled += 1;
            continue;
        }
        if line.trim_start().starts_with("Downloading")
            || line.trim_start().starts_with("Downloaded")
        {
            continue;
        }
        if line.trim_start().starts_with("Finished") {
            continue;
        }

        // 识别错误/警告块
        if line.starts_with("error[") || line.starts_with("error:") {
            // 跳过 "error: aborting due to" 这类摘要行
            if line.contains("aborting due to") || line.contains("could not compile") {
                continue;
            }
            if in_error && !current_error.is_empty() {
                errors.push(current_error.join("\n"));
                current_error.clear();
            }
            error_count += 1;
            in_error = true;
            current_error.push(line.to_string());
        } else if line.starts_with("warning:")
            && line.contains("generated")
            && line.contains("warning")
        {
            // "warning: `crate` generated N warnings" summary line
            continue;
        } else if line.starts_with("warning:") || line.starts_with("warning[") {
            if in_error && !current_error.is_empty() {
                errors.push(current_error.join("\n"));
                current_error.clear();
            }
            warnings += 1;
            in_error = true;
            current_error.push(line.to_string());
        } else if in_error {
            if line.trim().is_empty() && current_error.len() > 3 {
                errors.push(current_error.join("\n"));
                current_error.clear();
                in_error = false;
            } else {
                current_error.push(line.to_string());
            }
        }
    }

    if !current_error.is_empty() {
        errors.push(current_error.join("\n"));
    }

    if error_count == 0 && warnings == 0 {
        return format!("✓ cargo build（已编译 {compiled} 个 crate）");
    }

    let mut result = String::new();
    result.push_str(&format!(
        "cargo build：{error_count} 个错误，{warnings} 个警告（{compiled} 个 crate）\n"
    ));
    result.push_str("═══════════════════════════════════════\n");

    for (i, err) in errors.iter().enumerate().take(15) {
        result.push_str(err);
        result.push('\n');
        if i < errors.len() - 1 {
            result.push('\n');
        }
    }

    if errors.len() > 15 {
        result.push_str(&format!("\n... +{} 个问题\n", errors.len() - 15));
    }

    result.trim().to_string()
}

/// 用于紧凑展示的聚合测试结果
#[derive(Debug, Default, Clone)]
struct AggregatedTestResult {
    passed: usize,
    failed: usize,
    ignored: usize,
    measured: usize,
    filtered_out: usize,
    suites: usize,
    duration_secs: f64,
    has_duration: bool,
}

impl AggregatedTestResult {
    /// 解析测试结果摘要行
    /// 格式："test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s"
    fn parse_line(line: &str) -> Option<Self> {
        static RE: OnceLock<regex::Regex> = OnceLock::new();
        let re = RE.get_or_init(|| {
            crate::utils::compile_regex(
                r"test result: (\w+)\.\s+(\d+) passed;\s+(\d+) failed;\s+(\d+) ignored;\s+(\d+) measured;\s+(\d+) filtered out(?:;\s+finished in ([\d.]+)s)?"
            )
        });

        let caps = re.captures(line)?;
        let status = caps.get(1)?.as_str();

        // 仅在状态为 "ok"（全部通过）时参与聚合
        if status != "ok" {
            return None;
        }

        let passed = caps.get(2)?.as_str().parse().ok()?;
        let failed = caps.get(3)?.as_str().parse().ok()?;
        let ignored = caps.get(4)?.as_str().parse().ok()?;
        let measured = caps.get(5)?.as_str().parse().ok()?;
        let filtered_out = caps.get(6)?.as_str().parse().ok()?;

        let (duration_secs, has_duration) = if let Some(duration_match) = caps.get(7) {
            (duration_match.as_str().parse().unwrap_or(0.0), true)
        } else {
            (0.0, false)
        };

        Some(Self {
            passed,
            failed,
            ignored,
            measured,
            filtered_out,
            suites: 1,
            duration_secs,
            has_duration,
        })
    }

    /// 合并另一条测试结果
    fn merge(&mut self, other: &Self) {
        self.passed += other.passed;
        self.failed += other.failed;
        self.ignored += other.ignored;
        self.measured += other.measured;
        self.filtered_out += other.filtered_out;
        self.suites += other.suites;
        self.duration_secs += other.duration_secs;
        self.has_duration = self.has_duration && other.has_duration;
    }

    /// 格式化为紧凑单行
    fn format_compact(&self) -> String {
        let mut parts = vec![format!("{} 通过", self.passed)];

        if self.ignored > 0 {
            parts.push(format!("{} 忽略", self.ignored));
        }
        if self.filtered_out > 0 {
            parts.push(format!("{} 已过滤", self.filtered_out));
        }

        let counts = parts.join(", ");

        let suite_text = if self.suites == 1 {
            "1 个套件".to_string()
        } else {
            format!("{} 个套件", self.suites)
        };

        if self.has_duration {
            format!(
                "✓ cargo test：{}（{}，{:.2}s）",
                counts, suite_text, self.duration_secs
            )
        } else {
            format!("✓ cargo test：{counts}（{suite_text}）")
        }
    }
}

/// 过滤 `cargo test` 输出：仅展示失败项和摘要
fn filter_cargo_test(output: &str) -> String {
    let mut failures: Vec<String> = Vec::new();
    let mut summary_lines: Vec<String> = Vec::new();
    let mut in_failure_section = false;
    let mut current_failure = Vec::new();

    for line in output.lines() {
        // 跳过编译行
        if line.trim_start().starts_with("Compiling")
            || line.trim_start().starts_with("Downloading")
            || line.trim_start().starts_with("Downloaded")
            || line.trim_start().starts_with("Finished")
        {
            continue;
        }

        // 跳过 "running N tests" 和单条 "test ... ok" 行
        if line.starts_with("running ") || (line.starts_with("test ") && line.ends_with("... ok")) {
            continue;
        }

        // 识别 failures 段落
        if line == "failures:" {
            in_failure_section = true;
            continue;
        }

        if in_failure_section {
            if line.starts_with("test result:") {
                in_failure_section = false;
                summary_lines.push(line.to_string());
            } else if line.starts_with("    ") || line.starts_with("---- ") {
                current_failure.push(line.to_string());
            } else if line.trim().is_empty() && !current_failure.is_empty() {
                failures.push(current_failure.join("\n"));
                current_failure.clear();
            } else if !line.trim().is_empty() {
                current_failure.push(line.to_string());
            }
        }

        // 收集测试结果摘要
        if !in_failure_section && line.starts_with("test result:") {
            summary_lines.push(line.to_string());
        }
    }

    if !current_failure.is_empty() {
        failures.push(current_failure.join("\n"));
    }

    let mut result = String::new();

    if failures.is_empty() && !summary_lines.is_empty() {
        // 全部通过：尝试聚合
        let mut aggregated: Option<AggregatedTestResult> = None;
        let mut all_parsed = true;

        for line in &summary_lines {
            if let Some(parsed) = AggregatedTestResult::parse_line(line) {
                if let Some(ref mut agg) = aggregated {
                    agg.merge(&parsed);
                } else {
                    aggregated = Some(parsed);
                }
            } else {
                all_parsed = false;
                break;
            }
        }

        // 若所有行都解析成功且至少有一个套件，则返回紧凑格式
        if all_parsed
            && let Some(agg) = aggregated
            && agg.suites > 0
        {
            return agg.format_compact();
        }

        // 回退：若正则失败，则保留原始行为
        for line in &summary_lines {
            result.push_str(&format!("✓ {line}\n"));
        }
        return result.trim().to_string();
    }

    if !failures.is_empty() {
        result.push_str(&format!("失败（{}）：\n", failures.len()));
        result.push_str("═══════════════════════════════════════\n");
        for (i, failure) in failures.iter().enumerate().take(10) {
            result.push_str(&format!(
                "{}. {}\n",
                i + 1,
                truncate(failure, /*max_len*/ 200)
            ));
        }
        if failures.len() > 10 {
            result.push_str(&format!("\n... +{} 个失败\n", failures.len() - 10));
        }
        result.push('\n');
    }

    for line in &summary_lines {
        result.push_str(&format!("{line}\n"));
    }

    if result.trim().is_empty() {
        // 回退：展示最后几行有意义的内容
        let meaningful: Vec<&str> = output
            .lines()
            .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with("Compiling"))
            .collect();
        for line in meaningful.iter().rev().take(5).rev() {
            result.push_str(&format!("{line}\n"));
        }
    }

    result.trim().to_string()
}

/// 过滤 `cargo clippy` 输出：按 lint 规则分组警告
fn filter_cargo_clippy(output: &str) -> String {
    let mut by_rule: HashMap<String, Vec<String>> = HashMap::new();
    let mut error_count = 0;
    let mut warning_count = 0;

    // 解析 clippy 输出行
    // 格式："warning: description\n  --> file:line:col\n  |\n  | code\n"
    let mut current_rule = String::new();

    for line in output.lines() {
        // 跳过编译行
        if line.trim_start().starts_with("Compiling")
            || line.trim_start().starts_with("Checking")
            || line.trim_start().starts_with("Downloading")
            || line.trim_start().starts_with("Downloaded")
            || line.trim_start().starts_with("Finished")
        {
            continue;
        }

        // "warning: unused variable [unused_variables]" 或 "warning: description [clippy::rule_name]"
        if (line.starts_with("warning:") || line.starts_with("warning["))
            || (line.starts_with("error:") || line.starts_with("error["))
        {
            // 跳过摘要行："warning: `rtk` (bin) generated 5 warnings"
            if line.contains("generated") && line.contains("warning") {
                continue;
            }
            // 跳过 "error: aborting" / "error: could not compile"
            if line.contains("aborting due to") || line.contains("could not compile") {
                continue;
            }

            let is_error = line.starts_with("error");
            if is_error {
                error_count += 1;
            } else {
                warning_count += 1;
            }

            // 从方括号中提取规则名
            current_rule = if let Some(bracket_start) = line.rfind('[') {
                if let Some(bracket_end) = line.rfind(']') {
                    line[bracket_start + 1..bracket_end].to_string()
                } else {
                    line.to_string()
                }
            } else {
                // 没有方括号时，直接使用消息本身作为规则名
                let prefix = if is_error { "error: " } else { "warning: " };
                line.strip_prefix(prefix).unwrap_or(line).to_string()
            };
        } else if line.trim_start().starts_with("--> ") {
            let location = line.trim_start().trim_start_matches("--> ").to_string();
            if !current_rule.is_empty() {
                by_rule
                    .entry(current_rule.clone())
                    .or_default()
                    .push(location);
            }
        }
    }

    if error_count == 0 && warning_count == 0 {
        return "✓ cargo clippy：未发现问题".to_string();
    }

    let mut result = String::new();
    result.push_str(&format!(
        "cargo clippy：{error_count} 个错误，{warning_count} 个警告\n"
    ));
    result.push_str("═══════════════════════════════════════\n");

    // 按出现频率排序规则
    let mut rule_counts: Vec<_> = by_rule.iter().collect();
    rule_counts.sort_by(|a, b| b.1.len().cmp(&a.1.len()));

    for (rule, locations) in rule_counts.iter().take(15) {
        result.push_str(&format!("  {} ({}x)\n", rule, locations.len()));
        for loc in locations.iter().take(3) {
            result.push_str(&format!("    {loc}\n"));
        }
        if locations.len() > 3 {
            result.push_str(&format!("    ... +{} 个\n", locations.len() - 3));
        }
    }

    if by_rule.len() > 15 {
        result.push_str(&format!("\n... +{} 个规则\n", by_rule.len() - 15));
    }

    result.trim().to_string()
}

/// 直接透传不受支持的 cargo 子命令
pub fn run_passthrough(args: &[OsString], verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    if verbose > 0 {
        eprintln!("cargo 透传：{args:?}");
    }
    let status = resolved_command("cargo")
        .args(args)
        .status()
        .context("运行 cargo 失败")?;

    let args_str = tracking::args_display(args);
    timer.track_passthrough(
        &format!("cargo {args_str}"),
        &format!("rtk cargo {args_str} (passthrough)"),
    );

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_restore_double_dash_with_separator() {
        // `rtk cargo test -- --nocapture` → clap 会给出 ["--nocapture"]
        let args: Vec<String> = vec!["--nocapture".into()];
        let raw = vec![
            "rtk".into(),
            "cargo".into(),
            "test".into(),
            "--".into(),
            "--nocapture".into(),
        ];
        let result = restore_double_dash_with_raw(&args, &raw);
        assert_eq!(result, vec!["--", "--nocapture"]);
    }

    #[test]
    fn test_restore_double_dash_with_test_name() {
        // `rtk cargo test my_test -- --nocapture` → clap 会给出 ["my_test", "--nocapture"]
        let args: Vec<String> = vec!["my_test".into(), "--nocapture".into()];
        let raw = vec![
            "rtk".into(),
            "cargo".into(),
            "test".into(),
            "my_test".into(),
            "--".into(),
            "--nocapture".into(),
        ];
        let result = restore_double_dash_with_raw(&args, &raw);
        assert_eq!(result, vec!["my_test", "--", "--nocapture"]);
    }

    #[test]
    fn test_restore_double_dash_without_separator() {
        // `rtk cargo test my_test` → 没有 `--`，参数保持不变
        let args: Vec<String> = vec!["my_test".into()];
        let raw = vec![
            "rtk".into(),
            "cargo".into(),
            "test".into(),
            "my_test".into(),
        ];
        let result = restore_double_dash_with_raw(&args, &raw);
        assert_eq!(result, vec!["my_test"]);
    }

    #[test]
    fn test_restore_double_dash_empty_args() {
        let args: Vec<String> = vec![];
        let raw = vec!["rtk".into(), "cargo".into(), "test".into()];
        let result = restore_double_dash_with_raw(&args, &raw);
        assert!(result.is_empty());
    }

    #[test]
    fn test_restore_double_dash_clippy() {
        // `rtk cargo clippy -- -D warnings` → clap 会给出 ["-D", "warnings"]
        let args: Vec<String> = vec!["-D".into(), "warnings".into()];
        let raw = vec![
            "rtk".into(),
            "cargo".into(),
            "clippy".into(),
            "--".into(),
            "-D".into(),
            "warnings".into(),
        ];
        let result = restore_double_dash_with_raw(&args, &raw);
        assert_eq!(result, vec!["--", "-D", "warnings"]);
    }

    #[test]
    fn test_filter_cargo_build_success() {
        let output = r#"   Compiling libc v0.2.153
   Compiling cfg-if v1.0.0
   Compiling rtk v0.5.0
    Finished dev [unoptimized + debuginfo] target(s) in 15.23s
"#;
        let result = filter_cargo_build(output);
        assert!(result.contains("✓ cargo build"));
        assert!(result.contains("已编译 3 个 crate"));
    }

    #[test]
    fn test_filter_cargo_build_errors() {
        let output = r#"   Compiling rtk v0.5.0
error[E0308]: mismatched types
 --> src/main.rs:10:5
  |
10|     "hello"
  |     ^^^^^^^ expected `i32`, found `&str`

error: aborting due to 1 previous error
"#;
        let result = filter_cargo_build(output);
        assert!(result.contains("1 个错误，0 个警告（1 个 crate）"));
        assert!(result.contains("E0308"));
        assert!(result.contains("mismatched types"));
    }

    #[test]
    fn test_filter_cargo_test_all_pass() {
        let output = r#"   Compiling rtk v0.5.0
    Finished test [unoptimized + debuginfo] target(s) in 2.53s
     Running target/debug/deps/rtk-abc123

running 15 tests
test utils::tests::test_truncate_short_string ... ok
test utils::tests::test_truncate_long_string ... ok
test utils::tests::test_strip_ansi_simple ... ok

test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
"#;
        let result = filter_cargo_test(output);
        assert!(
            result.contains("✓ cargo test：15 通过（1 个套件，0.01s）"),
            "应输出紧凑格式，实际得到：{result}"
        );
        assert!(!result.contains("Compiling"));
        assert!(!result.contains("test utils"));
    }

    #[test]
    fn test_filter_cargo_test_failures() {
        let output = r#"running 5 tests
test foo::test_a ... ok
test foo::test_b ... FAILED
test foo::test_c ... ok

failures:

---- foo::test_b stdout ----
thread 'foo::test_b' panicked at 'assert_eq!(1, 2)'

failures:
    foo::test_b

test result: FAILED. 4 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
"#;
        let result = filter_cargo_test(output);
        assert!(result.contains("失败"));
        assert!(result.contains("test_b"));
        assert!(result.contains("test result:"));
    }

    #[test]
    fn test_filter_cargo_test_multi_suite_all_pass() {
        let output = r#"   Compiling rtk v0.5.0
    Finished test [unoptimized + debuginfo] target(s) in 2.53s
     Running unittests src/lib.rs (target/debug/deps/rtk-abc123)

running 50 tests
test result: ok. 50 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.45s

     Running unittests src/main.rs (target/debug/deps/rtk-def456)

running 30 tests
test result: ok. 30 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.30s

     Running tests/integration.rs (target/debug/deps/integration-ghi789)

running 25 tests
test result: ok. 25 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.25s

   Doc-tests rtk

running 32 tests
test result: ok. 32 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.45s
"#;
        let result = filter_cargo_test(output);
        assert!(
            result.contains("✓ cargo test：137 通过（4 个套件，1.45s）"),
            "应输出聚合格式，实际得到：{result}"
        );
        assert!(!result.contains("running"));
    }

    #[test]
    fn test_filter_cargo_test_multi_suite_with_failures() {
        let output = r#"     Running unittests src/lib.rs

running 20 tests
test result: ok. 20 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s

     Running unittests src/main.rs

running 15 tests
test foo::test_bad ... FAILED

failures:

---- foo::test_bad stdout ----
thread panicked at 'assertion failed'

test result: FAILED. 14 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s

     Running tests/integration.rs

running 10 tests
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
"#;
        let result = filter_cargo_test(output);
        // 存在失败时不应聚合
        assert!(result.contains("失败"), "实际得到：{result}");
        assert!(result.contains("test_bad"), "实际得到：{result}");
        assert!(result.contains("test result:"), "实际得到：{result}");
        // 应保留各自的摘要
        assert!(result.contains("20 passed"), "实际得到：{result}");
        assert!(result.contains("14 passed"), "实际得到：{result}");
        assert!(result.contains("10 passed"), "实际得到：{result}");
    }

    #[test]
    fn test_filter_cargo_test_all_suites_zero_tests() {
        let output = r#"     Running unittests src/empty1.rs

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/empty2.rs

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/empty3.rs

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
"#;
        let result = filter_cargo_test(output);
        assert!(
            result.contains("✓ cargo test：0 通过（3 个套件，0.00s）"),
            "零测试时应输出紧凑格式，实际得到：{result}"
        );
    }

    #[test]
    fn test_filter_cargo_test_with_ignored_and_filtered() {
        let output = r#"     Running unittests src/lib.rs

running 50 tests
test result: ok. 45 passed; 0 failed; 3 ignored; 0 measured; 2 filtered out; finished in 0.50s

     Running tests/integration.rs

running 20 tests
test result: ok. 18 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.20s
"#;
        let result = filter_cargo_test(output);
        assert!(
            result.contains("✓ cargo test：63 通过, 5 忽略, 2 已过滤（2 个套件，0.70s）"),
            "包含 ignored 和 filtered 时应输出紧凑格式，实际得到：{result}"
        );
    }

    #[test]
    fn test_filter_cargo_test_single_suite_compact() {
        let output = r#"     Running unittests src/main.rs

running 15 tests
test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
"#;
        let result = filter_cargo_test(output);
        assert!(
            result.contains("✓ cargo test：15 通过（1 个套件，0.01s）"),
            "单套件时应使用单数形式，实际得到：{result}"
        );
    }

    #[test]
    fn test_filter_cargo_test_regex_fallback() {
        let output = r#"     Running unittests src/main.rs

running 15 tests
test result: MALFORMED LINE WITHOUT PROPER FORMAT
"#;
        let result = filter_cargo_test(output);
        // 应回退到原始行为（展示带勾号的结果行）
        assert!(
            result.contains("✓ test result: MALFORMED"),
            "应回退到默认格式，实际得到：{result}"
        );
    }

    #[test]
    fn test_filter_cargo_clippy_clean() {
        let output = r#"    Checking rtk v0.5.0
    Finished dev [unoptimized + debuginfo] target(s) in 1.53s
"#;
        let result = filter_cargo_clippy(output);
        assert!(result.contains("✓ cargo clippy：未发现问题"));
    }

    #[test]
    fn test_filter_cargo_clippy_warnings() {
        let output = r#"    Checking rtk v0.5.0
warning: unused variable: `x` [unused_variables]
 --> src/main.rs:10:9
  |
10|     let x = 5;
  |         ^ help: if this is intentional, prefix it with an underscore: `_x`

warning: this function has too many arguments [clippy::too_many_arguments]
 --> src/git.rs:16:1
  |
16| pub fn run(a: i32, b: i32, c: i32, d: i32, e: i32, f: i32, g: i32, h: i32) {}
  |

warning: `rtk` (bin) generated 2 warnings
    Finished dev [unoptimized + debuginfo] target(s) in 1.53s
"#;
        let result = filter_cargo_clippy(output);
        assert!(result.contains("cargo clippy：0 个错误，2 个警告"));
        assert!(result.contains("unused_variables"));
        assert!(result.contains("clippy::too_many_arguments"));
    }

    #[test]
    fn test_filter_cargo_install_success() {
        let output = r#"  Installing rtk v0.11.0
  Downloading crates ...
  Downloaded anyhow v1.0.80
  Downloaded clap v4.5.0
   Compiling libc v0.2.153
   Compiling cfg-if v1.0.0
   Compiling anyhow v1.0.80
   Compiling clap v4.5.0
   Compiling rtk v0.11.0
    Finished `release` profile [optimized] target(s) in 45.23s
  Replacing /Users/user/.cargo/bin/rtk
   Replaced package `rtk v0.9.4` with `rtk v0.11.0` (/Users/user/.cargo/bin/rtk)
"#;
        let result = filter_cargo_install(output);
        assert!(result.contains("✓ cargo install"), "实际得到：{result}");
        assert!(result.contains("rtk v0.11.0"), "实际得到：{result}");
        assert!(result.contains("已编译 5 个依赖"), "实际得到：{result}");
        assert!(result.contains("Replaced"), "实际得到：{result}");
        assert!(!result.contains("Compiling"), "实际得到：{result}");
        assert!(!result.contains("Downloading"), "实际得到：{result}");
    }

    #[test]
    fn test_filter_cargo_install_replace() {
        let output = r#"  Installing rtk v0.11.0
   Compiling rtk v0.11.0
    Finished `release` profile [optimized] target(s) in 10.0s
  Replacing /Users/user/.cargo/bin/rtk
   Replaced package `rtk v0.9.4` with `rtk v0.11.0` (/Users/user/.cargo/bin/rtk)
"#;
        let result = filter_cargo_install(output);
        assert!(result.contains("✓ cargo install"), "实际得到：{result}");
        assert!(result.contains("Replacing"), "实际得到：{result}");
        assert!(result.contains("Replaced"), "实际得到：{result}");
    }

    #[test]
    fn test_filter_cargo_install_error() {
        let output = r#"  Installing rtk v0.11.0
   Compiling rtk v0.11.0
error[E0308]: mismatched types
 --> src/main.rs:10:5
  |
10|     "hello"
  |     ^^^^^^^ expected `i32`, found `&str`

error: aborting due to 1 previous error
"#;
        let result = filter_cargo_install(output);
        assert!(
            result.contains("cargo install：1 个错误"),
            "实际得到：{result}"
        );
        assert!(result.contains("E0308"), "实际得到：{result}");
        assert!(result.contains("mismatched types"), "实际得到：{result}");
        assert!(!result.contains("aborting"), "实际得到：{result}");
    }

    #[test]
    fn test_filter_cargo_install_already_installed() {
        let output = r#"  Ignored package `rtk v0.11.0`, is already installed
"#;
        let result = filter_cargo_install(output);
        assert!(result.contains("已安装"), "实际得到：{result}");
        assert!(result.contains("rtk v0.11.0"), "实际得到：{result}");
    }

    #[test]
    fn test_filter_cargo_install_up_to_date() {
        let output = r#"  Ignored package `cargo-deb v2.1.0 (/Users/user/cargo-deb)`, is already installed
"#;
        let result = filter_cargo_install(output);
        assert!(result.contains("已安装"), "实际得到：{result}");
        assert!(result.contains("cargo-deb v2.1.0"), "实际得到：{result}");
    }

    #[test]
    fn test_filter_cargo_install_empty_output() {
        let result = filter_cargo_install("");
        assert!(result.contains("✓ cargo install"), "实际得到：{result}");
        assert!(result.contains("已编译 0 个依赖"), "实际得到：{result}");
    }

    #[test]
    fn test_filter_cargo_install_path_warning() {
        let output = r#"  Installing rtk v0.11.0
   Compiling rtk v0.11.0
    Finished `release` profile [optimized] target(s) in 10.0s
  Replacing /Users/user/.cargo/bin/rtk
   Replaced package `rtk v0.9.4` with `rtk v0.11.0` (/Users/user/.cargo/bin/rtk)
warning: be sure to add `/Users/user/.cargo/bin` to your PATH
"#;
        let result = filter_cargo_install(output);
        assert!(result.contains("✓ cargo install"), "实际得到：{result}");
        assert!(
            result.contains("be sure to add"),
            "应保留 PATH 警告：{result}"
        );
        assert!(result.contains("Replaced"), "实际得到：{result}");
    }

    #[test]
    fn test_filter_cargo_install_multiple_errors() {
        let output = r#"  Installing rtk v0.11.0
   Compiling rtk v0.11.0
error[E0308]: mismatched types
 --> src/main.rs:10:5
  |
10|     "hello"
  |     ^^^^^^^ expected `i32`, found `&str`

error[E0425]: cannot find value `foo`
 --> src/lib.rs:20:9
  |
20|     foo
  |     ^^^ not found in this scope

error: aborting due to 2 previous errors
"#;
        let result = filter_cargo_install(output);
        assert!(result.contains("2 个错误"), "应展示 2 个错误：{result}");
        assert!(result.contains("E0308"), "实际得到：{result}");
        assert!(result.contains("E0425"), "实际得到：{result}");
        assert!(!result.contains("aborting"), "实际得到：{result}");
    }

    #[test]
    fn test_filter_cargo_install_locking_and_blocking() {
        let output = r#"  Locking 45 packages to latest compatible versions
  Blocking waiting for file lock on package cache
  Downloading crates ...
  Downloaded serde v1.0.200
   Compiling serde v1.0.200
   Compiling rtk v0.11.0
    Finished `release` profile [optimized] target(s) in 30.0s
  Installing rtk v0.11.0
"#;
        let result = filter_cargo_install(output);
        assert!(result.contains("✓ cargo install"), "实际得到：{result}");
        assert!(!result.contains("Locking"), "实际得到：{result}");
        assert!(!result.contains("Blocking"), "实际得到：{result}");
        assert!(!result.contains("Downloading"), "实际得到：{result}");
    }

    #[test]
    fn test_filter_cargo_install_from_path() {
        let output = r#"  Installing /Users/user/projects/rtk
   Compiling rtk v0.11.0
    Finished `release` profile [optimized] target(s) in 10.0s
"#;
        let result = filter_cargo_install(output);
        // 基于路径的安装：不会从路径中提取 crate 信息
        assert!(result.contains("✓ cargo install"), "实际得到：{result}");
        assert!(result.contains("已编译 1 个依赖"), "实际得到：{result}");
    }

    #[test]
    fn test_format_crate_info() {
        assert_eq!(format_crate_info("rtk", "v0.11.0", ""), "rtk v0.11.0");
        assert_eq!(format_crate_info("rtk", "", ""), "rtk");
        assert_eq!(format_crate_info("", "", "package"), "package");
        assert_eq!(format_crate_info("", "v0.1.0", "fallback"), "fallback");
    }

    #[test]
    fn test_filter_cargo_nextest_all_pass() {
        let output = r#"   Compiling rtk v0.15.2
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.04s
────────────────────────────
    Starting 301 tests across 1 binary
        PASS [   0.009s] (1/301) rtk::bin/rtk cargo_cmd::tests::test_one
        PASS [   0.008s] (2/301) rtk::bin/rtk cargo_cmd::tests::test_two
        PASS [   0.007s] (301/301) rtk::bin/rtk cargo_cmd::tests::test_last
────────────────────────────
     Summary [   0.192s] 301 tests run: 301 passed, 0 skipped
"#;
        let result = filter_cargo_nextest(output);
        assert_eq!(
            result, "✓ cargo nextest：301 通过（1 个二进制, 0.192s）",
            "实际得到：{result}"
        );
    }

    #[test]
    fn test_filter_cargo_nextest_with_failures() {
        let output = r#"    Starting 4 tests across 1 binary (1 test skipped)
        PASS [   0.006s] (1/4) test-proj tests::passing_test
        FAIL [   0.006s] (2/4) test-proj tests::failing_test

  stderr ───

    thread 'tests::failing_test' panicked at src/lib.rs:15:9:
    assertion `left == right` failed
      left: 1
     right: 2

  Cancelling due to test failure: 2 tests still running
        PASS [   0.007s] (3/4) test-proj tests::another_passing
        FAIL [   0.006s] (4/4) test-proj tests::another_failing

  stderr ───

    thread 'tests::another_failing' panicked at src/lib.rs:20:9:
    something went wrong

────────────────────────────
     Summary [   0.007s] 4 tests run: 2 passed, 2 failed, 1 skipped
        FAIL [   0.006s] (2/4) test-proj tests::failing_test
        FAIL [   0.006s] (4/4) test-proj tests::another_failing
error: test run failed
"#;
        let result = filter_cargo_nextest(output);
        assert!(
            result.contains("tests::failing_test"),
            "应包含第一个失败项：{result}"
        );
        assert!(
            result.contains("tests::another_failing"),
            "应包含第二个失败项：{result}"
        );
        assert!(result.contains("panicked"), "应包含 stderr 详情：{result}");
        assert!(
            result.contains("2 通过, 2 失败, 1 跳过"),
            "应包含摘要：{result}"
        );
        assert!(!result.contains("PASS"), "不应包含 PASS 行：{result}");
        // 摘要后的 FAIL 回顾行不应制造重复的 FAIL 标题
        // （测试名自然可能同时出现在标题和 stderr 正文中）
        assert_eq!(
            result.matches("FAIL [").count(),
            2,
            "应只有 2 个 FAIL 标题（不能重复）：{result}"
        );
        assert!(
            !result.contains("error: test run failed"),
            "不应包含摘要后的错误行：{result}"
        );
    }

    #[test]
    fn test_filter_cargo_nextest_with_skipped() {
        let output = r#"    Starting 50 tests across 2 binaries (3 tests skipped)
        PASS [   0.010s] (1/50) rtk::bin/rtk test_one
        PASS [   0.010s] (50/50) rtk::bin/rtk test_last
────────────────────────────
     Summary [   0.500s] 50 tests run: 50 passed, 3 skipped
"#;
        let result = filter_cargo_nextest(output);
        assert_eq!(
            result, "✓ cargo nextest：50 通过, 3 跳过（2 个二进制, 0.500s）",
            "实际得到：{result}"
        );
    }

    #[test]
    fn test_filter_cargo_nextest_single_failure_detail() {
        let output = r#"    Starting 2 tests across 1 binary
        PASS [   0.005s] (1/2) proj tests::good
        FAIL [   0.005s] (2/2) proj tests::bad

  stderr ───

    thread 'tests::bad' panicked at src/lib.rs:5:9:
    assertion failed: false

────────────────────────────
     Summary [   0.010s] 2 tests run: 1 passed, 1 failed
        FAIL [   0.005s] (2/2) proj tests::bad
error: test run failed
"#;
        let result = filter_cargo_nextest(output);
        assert!(
            result.contains("assertion failed: false"),
            "应展示 panic 信息：{result}"
        );
        assert!(result.contains("1 通过, 1 失败"), "应展示摘要：{result}");
        // 摘要后的回顾行不应重复 FAIL 标题
        assert_eq!(
            result.matches("FAIL [").count(),
            1,
            "应只有 1 个 FAIL 标题（不能重复）：{result}"
        );
    }

    #[test]
    fn test_filter_cargo_nextest_multiple_binaries() {
        let output = r#"    Starting 100 tests across 5 binaries
        PASS [   0.010s] (100/100) test_last
────────────────────────────
     Summary [   1.234s] 100 tests run: 100 passed, 0 skipped
"#;
        let result = filter_cargo_nextest(output);
        assert_eq!(
            result, "✓ cargo nextest：100 通过（5 个二进制, 1.234s）",
            "实际得到：{result}"
        );
    }

    #[test]
    fn test_filter_cargo_nextest_compilation_stripped() {
        let output = r#"   Compiling serde v1.0.200
   Compiling rtk v0.15.2
   Downloading crates ...
    Finished `test` profile [unoptimized + debuginfo] target(s) in 5.00s
────────────────────────────
    Starting 10 tests across 1 binary
        PASS [   0.010s] (10/10) test_last
────────────────────────────
     Summary [   0.050s] 10 tests run: 10 passed, 0 skipped
"#;
        let result = filter_cargo_nextest(output);
        assert!(!result.contains("Compiling"), "应去除 Compiling：{result}");
        assert!(
            !result.contains("Downloading"),
            "应去除 Downloading：{result}"
        );
        assert!(!result.contains("Finished"), "应去除 Finished：{result}");
        assert!(
            result.contains("✓ cargo nextest：10 通过"),
            "实际得到：{result}"
        );
    }

    #[test]
    fn test_filter_cargo_nextest_empty() {
        let result = filter_cargo_nextest("");
        assert!(result.is_empty(), "实际得到：{result}");
    }

    #[test]
    fn test_filter_cargo_nextest_cancellation_notice() {
        let output = r#"    Starting 3 tests across 1 binary
        FAIL [   0.005s] (1/3) proj tests::bad

  stderr ───

    thread panicked at 'oops'

  Cancelling due to test failure: 2 tests still running
────────────────────────────
     Summary [   0.010s] 3 tests run: 2 passed, 1 failed
        FAIL [   0.005s] (1/3) proj tests::bad
error: test run failed
"#;
        let result = filter_cargo_nextest(output);
        assert!(
            result.contains("因测试失败已取消"),
            "应包含取消提示：{result}"
        );
        assert!(result.contains("1 失败"), "应展示失败数量：{result}");
        // 摘要后的回顾行不应重复 FAIL 标题
        assert_eq!(
            result.matches("FAIL [").count(),
            1,
            "应只有 1 个 FAIL 标题（不能重复）：{result}"
        );
    }

    #[test]
    fn test_filter_cargo_nextest_summary_regex_fallback() {
        let output = r#"    Starting 5 tests across 1 binary
        PASS [   0.005s] (5/5) test_last
────────────────────────────
     Summary MALFORMED LINE
"#;
        let result = filter_cargo_nextest(output);
        assert!(
            result.contains("Summary MALFORMED"),
            "应回退到原始摘要：{result}"
        );
    }
}
