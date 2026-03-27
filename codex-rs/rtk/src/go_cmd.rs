use crate::tracking;
use crate::utils::resolved_command;
use crate::utils::truncate;
use anyhow::Context;
use anyhow::Result;
use serde::Deserialize;
use std::collections::HashMap;
use std::ffi::OsString;

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct GoTestEvent {
    #[serde(rename = "Time")]
    time: Option<String>,
    #[serde(rename = "Action")]
    action: String,
    #[serde(rename = "Package")]
    package: Option<String>,
    #[serde(rename = "Test")]
    test: Option<String>,
    #[serde(rename = "Output")]
    output: Option<String>,
    #[serde(rename = "Elapsed")]
    elapsed: Option<f64>,
    #[serde(rename = "ImportPath")]
    import_path: Option<String>,
    #[serde(rename = "FailedBuild")]
    failed_build: Option<String>,
}

#[derive(Debug, Default)]
struct PackageResult {
    pass: usize,
    fail: usize,
    skip: usize,
    build_failed: bool,
    build_errors: Vec<String>,
    failed_tests: Vec<(String, Vec<String>)>, // （测试名，输出行）
}

pub fn run_test(args: &[String], verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    let mut cmd = resolved_command("go");
    cmd.arg("test");

    // 如果尚未指定，则强制启用 JSON 输出
    if !args.iter().any(|a| a == "-json") {
        cmd.arg("-json");
    }

    for arg in args {
        cmd.arg(arg);
    }

    if verbose > 0 {
        eprintln!("运行：go test -json {}", args.join(" "));
    }

    let output = cmd.output().context("运行 go test 失败。是否已安装 Go？")?;

    let stdout = crate::utils::decode_output(&output.stdout);
    let stderr = crate::utils::decode_output(&output.stderr);
    let raw = format!("{stdout}\n{stderr}");

    let exit_code = output
        .status
        .code()
        .unwrap_or(if output.status.success() { 0 } else { 1 });
    let filtered = filter_go_test_json(&stdout);

    if let Some(hint) = crate::tee::tee_and_hint(&raw, "go_test", exit_code) {
        println!("{filtered}\n{hint}");
    } else {
        println!("{filtered}");
    }

    // 如有 stderr，也一并输出（构建错误等）
    if !stderr.trim().is_empty() {
        eprintln!("{}", stderr.trim());
    }

    timer.track(
        &format!("go test {}", args.join(" ")),
        &format!("rtk go test {}", args.join(" ")),
        &raw,
        &filtered,
    );

    // 保留退出码，兼容 CI/CD
    if !output.status.success() {
        std::process::exit(exit_code);
    }

    Ok(())
}

pub fn run_build(args: &[String], verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    let mut cmd = resolved_command("go");
    cmd.arg("build");

    for arg in args {
        cmd.arg(arg);
    }

    if verbose > 0 {
        eprintln!("运行：go build {}", args.join(" "));
    }

    let output = cmd
        .output()
        .context("运行 go build 失败。是否已安装 Go？")?;

    let stdout = crate::utils::decode_output(&output.stdout);
    let stderr = crate::utils::decode_output(&output.stderr);
    let raw = format!("{stdout}\n{stderr}");

    let exit_code = output
        .status
        .code()
        .unwrap_or(if output.status.success() { 0 } else { 1 });
    let filtered = filter_go_build(&raw);

    if let Some(hint) = crate::tee::tee_and_hint(&raw, "go_build", exit_code) {
        if !filtered.is_empty() {
            println!("{filtered}\n{hint}");
        } else {
            println!("{hint}");
        }
    } else if !filtered.is_empty() {
        println!("{filtered}");
    }

    timer.track(
        &format!("go build {}", args.join(" ")),
        &format!("rtk go build {}", args.join(" ")),
        &raw,
        &filtered,
    );

    // 保留退出码，兼容 CI/CD
    if !output.status.success() {
        std::process::exit(exit_code);
    }

    Ok(())
}

pub fn run_vet(args: &[String], verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    let mut cmd = resolved_command("go");
    cmd.arg("vet");

    for arg in args {
        cmd.arg(arg);
    }

    if verbose > 0 {
        eprintln!("运行：go vet {}", args.join(" "));
    }

    let output = cmd.output().context("运行 go vet 失败。是否已安装 Go？")?;

    let stdout = crate::utils::decode_output(&output.stdout);
    let stderr = crate::utils::decode_output(&output.stderr);
    let raw = format!("{stdout}\n{stderr}");

    let exit_code = output
        .status
        .code()
        .unwrap_or(if output.status.success() { 0 } else { 1 });
    let filtered = filter_go_vet(&raw);

    if let Some(hint) = crate::tee::tee_and_hint(&raw, "go_vet", exit_code) {
        if !filtered.is_empty() {
            println!("{filtered}\n{hint}");
        } else {
            println!("{hint}");
        }
    } else if !filtered.is_empty() {
        println!("{filtered}");
    }

    timer.track(
        &format!("go vet {}", args.join(" ")),
        &format!("rtk go vet {}", args.join(" ")),
        &raw,
        &filtered,
    );

    // 保留退出码，兼容 CI/CD
    if !output.status.success() {
        std::process::exit(exit_code);
    }

    Ok(())
}

pub fn run_other(args: &[OsString], verbose: u8) -> Result<()> {
    if args.is_empty() {
        anyhow::bail!("go: no subcommand specified");
    }

    let timer = tracking::TimedExecution::start();

    let subcommand = args[0].to_string_lossy();
    let mut cmd = resolved_command("go");
    cmd.arg(&*subcommand);

    for arg in &args[1..] {
        cmd.arg(arg);
    }

    if verbose > 0 {
        eprintln!("运行：go {subcommand} ...");
    }

    let output = cmd
        .output()
        .with_context(|| format!("运行 go {subcommand} 失败"))?;

    let stdout = crate::utils::decode_output(&output.stdout);
    let stderr = crate::utils::decode_output(&output.stderr);
    let raw = format!("{stdout}\n{stderr}");

    print!("{stdout}");
    eprint!("{stderr}");

    timer.track(
        &format!("go {subcommand}"),
        &format!("rtk go {subcommand}"),
        &raw,
        &raw, // 不支持的命令不做过滤
    );

    // 保留退出码
    if !output.status.success() {
        std::process::exit(output.status.code().unwrap_or(1));
    }

    Ok(())
}

/// 解析 `go test -json` 输出（NDJSON 格式）
fn filter_go_test_json(output: &str) -> String {
    let mut packages: HashMap<String, PackageResult> = HashMap::new();
    let mut current_test_output: HashMap<(String, String), Vec<String>> = HashMap::new(); // (package, test) -> outputs
    let mut build_output: HashMap<String, Vec<String>> = HashMap::new(); // import_path -> error lines

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let event: GoTestEvent = match serde_json::from_str(trimmed) {
            Ok(e) => e,
            Err(_) => continue, // 跳过非 JSON 行
        };

        // 处理 build-output/build-fail 事件（使用 ImportPath，没有 Package）
        match event.action.as_str() {
            "build-output" => {
                if let (Some(import_path), Some(output_text)) = (&event.import_path, &event.output)
                {
                    let text = output_text.trim_end().to_string();
                    if !text.is_empty() {
                        build_output
                            .entry(import_path.clone())
                            .or_default()
                            .push(text);
                    }
                }
                continue;
            }
            "build-fail" => {
                // build-fail 带有 ImportPath，等包级 fail 到来时再处理
                continue;
            }
            _ => {}
        }

        let package = event.package.unwrap_or_else(|| "unknown".to_string());
        let pkg_result = packages.entry(package.clone()).or_default();

        match event.action.as_str() {
            "pass" => {
                if event.test.is_some() {
                    pkg_result.pass += 1;
                }
            }
            "fail" => {
                if let Some(test) = &event.test {
                    // 单个测试失败
                    pkg_result.fail += 1;

                    // 收集失败测试的输出
                    let key = (package.clone(), test.clone());
                    let outputs = current_test_output.remove(&key).unwrap_or_default();
                    pkg_result.failed_tests.push((test.clone(), outputs));
                } else if event.failed_build.is_some() {
                    // 包级构建失败
                    pkg_result.build_failed = true;
                    // 从 import path 中收集构建错误
                    if let Some(import_path) = &event.failed_build
                        && let Some(errors) = build_output.remove(import_path)
                    {
                        pkg_result.build_errors = errors;
                    }
                }
            }
            "skip" => {
                if event.test.is_some() {
                    pkg_result.skip += 1;
                }
            }
            "output" => {
                // 收集当前测试的输出
                if let (Some(test), Some(output_text)) = (&event.test, &event.output) {
                    let key = (package.clone(), test.clone());
                    current_test_output
                        .entry(key)
                        .or_default()
                        .push(output_text.trim_end().to_string());
                }
            }
            _ => {} // `run`、`pause`、`cont` 等
        }
    }

    // 构建摘要
    let total_packages = packages.len();
    let total_pass: usize = packages.values().map(|p| p.pass).sum();
    let total_fail: usize = packages.values().map(|p| p.fail).sum();
    let total_skip: usize = packages.values().map(|p| p.skip).sum();
    let total_build_fail: usize = packages.values().filter(|p| p.build_failed).count();

    let has_failures = total_fail > 0 || total_build_fail > 0;

    if !has_failures && total_pass == 0 {
        return "Go test：未找到测试".to_string();
    }

    if !has_failures {
        return format!("✓ Go test：{total_pass} 通过，共 {total_packages} 个包");
    }

    let mut result = String::new();
    result.push_str(&format!(
        "Go test：{} 通过，{} 失败",
        total_pass,
        total_fail + total_build_fail
    ));
    if total_skip > 0 {
        result.push_str(&format!("，{total_skip} 跳过"));
    }
    result.push_str(&format!("，共 {total_packages} 个包\n"));
    result.push_str("═══════════════════════════════════════\n");

    // 先展示构建失败
    for (package, pkg_result) in packages.iter() {
        if !pkg_result.build_failed {
            continue;
        }

        result.push_str(&format!(
            "\n📦 {} [构建失败]\n",
            compact_package_name(package)
        ));

        for line in &pkg_result.build_errors {
            let trimmed = line.trim();
            // 跳过 `# package` 头部行
            if !trimmed.starts_with('#') && !trimmed.is_empty() {
                result.push_str(&format!("  {}\n", truncate(trimmed, /*max_len*/ 120)));
            }
        }
    }

    // 按包分组展示失败测试
    for (package, pkg_result) in packages.iter() {
        if pkg_result.fail == 0 {
            continue;
        }

        result.push_str(&format!(
            "\n📦 {}（{} 通过，{} 失败）\n",
            compact_package_name(package),
            pkg_result.pass,
            pkg_result.fail
        ));

        for (test, outputs) in &pkg_result.failed_tests {
            result.push_str(&format!("  ❌ {test}\n"));

            // 展示失败输出（只保留关键行）
            let relevant_lines: Vec<&String> = outputs
                .iter()
                .filter(|line| {
                    let lower = line.to_lowercase();
                    !line.trim().is_empty()
                        && !line.starts_with("=== RUN")
                        && !line.starts_with("--- FAIL")
                        && (lower.contains("error")
                            || lower.contains("expected")
                            || lower.contains("got")
                            || lower.contains("panic")
                            || line.trim().starts_with("at "))
                })
                .take(5)
                .collect();

            for line in relevant_lines {
                result.push_str(&format!("     {}\n", truncate(line, /*max_len*/ 100)));
            }
        }
    }

    result.trim().to_string()
}

/// 过滤 `go build` 输出，只显示错误
fn filter_go_build(output: &str) -> String {
    let mut errors: Vec<String> = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();
        let lower = trimmed.to_lowercase();

        // 跳过包标记（无错误的 `# package/name` 行）
        if trimmed.starts_with('#') && !lower.contains("error") {
            continue;
        }

        // 收集错误行（`file:line:col` 格式或包含错误关键字）
        if !trimmed.is_empty()
            && (lower.contains("error")
                || trimmed.contains(".go:")
                || lower.contains("undefined")
                || lower.contains("cannot"))
        {
            errors.push(trimmed.to_string());
        }
    }

    if errors.is_empty() {
        return "✓ Go build：成功".to_string();
    }

    let mut result = String::new();
    result.push_str(&format!("Go build：{} 个错误\n", errors.len()));
    result.push_str("═══════════════════════════════════════\n");

    for (i, error) in errors.iter().take(20).enumerate() {
        result.push_str(&format!(
            "{}. {}\n",
            i + 1,
            truncate(error, /*max_len*/ 120)
        ));
    }

    if errors.len() > 20 {
        result.push_str(&format!("\n... +{} 个错误\n", errors.len() - 20));
    }

    result.trim().to_string()
}

/// 过滤 `go vet` 输出，显示问题
fn filter_go_vet(output: &str) -> String {
    let mut issues: Vec<String> = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();

        // 收集问题行（vet 通常以 `file:line:col` 格式报告）
        if !trimmed.is_empty() && !trimmed.starts_with('#') && trimmed.contains(".go:") {
            issues.push(trimmed.to_string());
        }
    }

    if issues.is_empty() {
        return "✓ Go vet：未发现问题".to_string();
    }

    let mut result = String::new();
    result.push_str(&format!("Go vet：{} 个问题\n", issues.len()));
    result.push_str("═══════════════════════════════════════\n");

    for (i, issue) in issues.iter().take(20).enumerate() {
        result.push_str(&format!(
            "{}. {}\n",
            i + 1,
            truncate(issue, /*max_len*/ 120)
        ));
    }

    if issues.len() > 20 {
        result.push_str(&format!("\n... +{} 个问题\n", issues.len() - 20));
    }

    result.trim().to_string()
}

/// 压缩包名（移除过长路径）
fn compact_package_name(package: &str) -> String {
    // 移除常见模块前缀，如 `github.com/user/repo/`
    if let Some(pos) = package.rfind('/') {
        package[pos + 1..].to_string()
    } else {
        package.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_go_test_all_pass() {
        let output = r#"{"Time":"2024-01-01T10:00:00Z","Action":"run","Package":"example.com/foo","Test":"TestBar"}
{"Time":"2024-01-01T10:00:01Z","Action":"output","Package":"example.com/foo","Test":"TestBar","Output":"=== RUN   TestBar\n"}
{"Time":"2024-01-01T10:00:02Z","Action":"pass","Package":"example.com/foo","Test":"TestBar","Elapsed":0.5}
{"Time":"2024-01-01T10:00:02Z","Action":"pass","Package":"example.com/foo","Elapsed":0.5}"#;

        let result = filter_go_test_json(output);
        assert!(result.contains("✓ Go test"));
        assert!(result.contains("1 通过"));
        assert!(result.contains("1 个包"));
    }

    #[test]
    fn test_filter_go_test_with_failures() {
        let output = r#"{"Time":"2024-01-01T10:00:00Z","Action":"run","Package":"example.com/foo","Test":"TestFail"}
{"Time":"2024-01-01T10:00:01Z","Action":"output","Package":"example.com/foo","Test":"TestFail","Output":"=== RUN   TestFail\n"}
{"Time":"2024-01-01T10:00:02Z","Action":"output","Package":"example.com/foo","Test":"TestFail","Output":"    Error: expected 5, got 3\n"}
{"Time":"2024-01-01T10:00:03Z","Action":"fail","Package":"example.com/foo","Test":"TestFail","Elapsed":0.5}
{"Time":"2024-01-01T10:00:03Z","Action":"fail","Package":"example.com/foo","Elapsed":0.5}"#;

        let result = filter_go_test_json(output);
        assert!(result.contains("1 失败"));
        assert!(result.contains("TestFail"));
        assert!(result.contains("expected 5, got 3"));
    }

    #[test]
    fn test_filter_go_build_success() {
        let output = "";
        let result = filter_go_build(output);
        assert!(result.contains("✓ Go build"));
        assert!(result.contains("成功"));
    }

    #[test]
    fn test_filter_go_build_errors() {
        let output = r#"# example.com/foo
main.go:10:5: undefined: missingFunc
main.go:15:2: cannot use x (type int) as type string"#;

        let result = filter_go_build(output);
        assert!(result.contains("2 个错误"));
        assert!(result.contains("undefined: missingFunc"));
        assert!(result.contains("cannot use x"));
    }

    #[test]
    fn test_filter_go_vet_no_issues() {
        let output = "";
        let result = filter_go_vet(output);
        assert!(result.contains("✓ Go vet"));
        assert!(result.contains("未发现问题"));
    }

    #[test]
    fn test_filter_go_vet_with_issues() {
        let output = r#"main.go:42:2: Printf format %d has arg x of wrong type string
utils.go:15:5: unreachable code"#;

        let result = filter_go_vet(output);
        assert!(result.contains("2 个问题"));
        assert!(result.contains("Printf format"));
        assert!(result.contains("unreachable code"));
    }

    #[test]
    fn test_compact_package_name() {
        assert_eq!(compact_package_name("github.com/user/repo/pkg"), "pkg");
        assert_eq!(compact_package_name("example.com/foo"), "foo");
        assert_eq!(compact_package_name("simple"), "simple");
    }
}
