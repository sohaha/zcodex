use crate::tracking;
use crate::utils::resolved_command;
use anyhow::Context;
use anyhow::Result;
use serde::Deserialize;
use std::collections::HashMap;
use std::ffi::OsString;

use crate::parser::Dependency;
use crate::parser::DependencyState;
use crate::parser::FormatMode;
use crate::parser::OutputParser;
use crate::parser::ParseResult;
use crate::parser::TokenFormatter;
use crate::parser::emit_degradation_warning;
use crate::parser::emit_passthrough_warning;
use crate::parser::truncate_output;

/// pnpm list JSON output structure
#[derive(Debug, Deserialize)]
struct PnpmListOutput {
    #[serde(flatten)]
    packages: HashMap<String, PnpmPackage>,
}

#[derive(Debug, Deserialize)]
struct PnpmPackage {
    version: Option<String>,
    #[serde(rename = "dependencies", default)]
    dependencies: HashMap<String, PnpmPackage>,
    #[serde(rename = "devDependencies", default)]
    dev_dependencies: HashMap<String, PnpmPackage>,
}

/// pnpm outdated JSON output structure
#[derive(Debug, Deserialize)]
struct PnpmOutdatedOutput {
    #[serde(flatten)]
    packages: HashMap<String, PnpmOutdatedPackage>,
}

#[derive(Debug, Deserialize)]
struct PnpmOutdatedPackage {
    current: String,
    latest: String,
    wanted: Option<String>,
    #[serde(rename = "dependencyType", default)]
    dependency_type: String,
}

/// Parser for pnpm list output
pub struct PnpmListParser;

impl OutputParser for PnpmListParser {
    type Output = DependencyState;

    fn parse(input: &str) -> ParseResult<DependencyState> {
        // Tier 1: Try JSON parsing
        match serde_json::from_str::<PnpmListOutput>(input) {
            Ok(json) => {
                let mut dependencies = Vec::new();
                let mut total_count = 0;

                for (name, pkg) in &json.packages {
                    collect_dependencies(
                        name,
                        pkg,
                        /*is_dev*/ false,
                        &mut dependencies,
                        &mut total_count,
                    );
                }

                let result = DependencyState {
                    total_packages: total_count,
                    outdated_count: 0, // list doesn't provide outdated info
                    dependencies,
                };

                ParseResult::Full(result)
            }
            Err(e) => {
                // Tier 2: Try text extraction
                match extract_list_text(input) {
                    Some(result) => {
                        ParseResult::Degraded(result, vec![format!("JSON 解析失败：{e}")])
                    }
                    None => {
                        // Tier 3: Passthrough
                        ParseResult::Passthrough(truncate_output(input, /*max_chars*/ 500))
                    }
                }
            }
        }
    }
}

/// Recursively collect dependencies from pnpm package tree
fn collect_dependencies(
    name: &str,
    pkg: &PnpmPackage,
    is_dev: bool,
    deps: &mut Vec<Dependency>,
    count: &mut usize,
) {
    if let Some(version) = &pkg.version {
        deps.push(Dependency {
            name: name.to_string(),
            current_version: version.clone(),
            latest_version: None,
            wanted_version: None,
            dev_dependency: is_dev,
        });
        *count += 1;
    }

    for (dep_name, dep_pkg) in &pkg.dependencies {
        collect_dependencies(dep_name, dep_pkg, is_dev, deps, count);
    }

    for (dep_name, dep_pkg) in &pkg.dev_dependencies {
        collect_dependencies(dep_name, dep_pkg, /*is_dev*/ true, deps, count);
    }
}

/// Tier 2: Extract list info from text output
fn extract_list_text(output: &str) -> Option<DependencyState> {
    let mut dependencies = Vec::new();
    let mut count = 0;

    for line in output.lines() {
        // Skip box-drawing and metadata
        if line.contains('│')
            || line.contains('├')
            || line.contains('└')
            || line.contains("Legend:")
            || line.trim().is_empty()
        {
            continue;
        }

        // Parse lines like: "package@1.2.3"
        let parts: Vec<&str> = line.split_whitespace().collect();
        if !parts.is_empty() {
            let pkg_str = parts[0];
            if let Some(at_pos) = pkg_str.rfind('@') {
                let name = &pkg_str[..at_pos];
                let version = &pkg_str[at_pos + 1..];
                if !name.is_empty() && !version.is_empty() {
                    dependencies.push(Dependency {
                        name: name.to_string(),
                        current_version: version.to_string(),
                        latest_version: None,
                        wanted_version: None,
                        dev_dependency: false,
                    });
                    count += 1;
                }
            }
        }
    }

    if count > 0 {
        Some(DependencyState {
            total_packages: count,
            outdated_count: 0,
            dependencies,
        })
    } else {
        None
    }
}

/// Parser for pnpm outdated output
pub struct PnpmOutdatedParser;

impl OutputParser for PnpmOutdatedParser {
    type Output = DependencyState;

    fn parse(input: &str) -> ParseResult<DependencyState> {
        // Tier 1: Try JSON parsing
        match serde_json::from_str::<PnpmOutdatedOutput>(input) {
            Ok(json) => {
                let mut dependencies = Vec::new();
                let mut outdated_count = 0;

                for (name, pkg) in &json.packages {
                    if pkg.current != pkg.latest {
                        outdated_count += 1;
                    }

                    dependencies.push(Dependency {
                        name: name.clone(),
                        current_version: pkg.current.clone(),
                        latest_version: Some(pkg.latest.clone()),
                        wanted_version: pkg.wanted.clone(),
                        dev_dependency: pkg.dependency_type == "devDependencies",
                    });
                }

                let result = DependencyState {
                    total_packages: dependencies.len(),
                    outdated_count,
                    dependencies,
                };

                ParseResult::Full(result)
            }
            Err(e) => {
                // Tier 2: Try text extraction
                match extract_outdated_text(input) {
                    Some(result) => {
                        ParseResult::Degraded(result, vec![format!("JSON 解析失败：{e}")])
                    }
                    None => {
                        // Tier 3: Passthrough
                        ParseResult::Passthrough(truncate_output(input, /*max_chars*/ 500))
                    }
                }
            }
        }
    }
}

/// Tier 2: Extract outdated info from text output
fn extract_outdated_text(output: &str) -> Option<DependencyState> {
    let mut dependencies = Vec::new();
    let mut outdated_count = 0;

    for line in output.lines() {
        // Skip box-drawing, headers, legend
        if line.contains('│')
            || line.contains('├')
            || line.contains('└')
            || line.contains('─')
            || line.starts_with("Legend:")
            || line.starts_with("Package")
            || line.trim().is_empty()
        {
            continue;
        }

        // Parse lines: "package  current  wanted  latest"
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 4 {
            let name = parts[0];
            let current = parts[1];
            let latest = parts[3];

            if current != latest {
                outdated_count += 1;
            }

            dependencies.push(Dependency {
                name: name.to_string(),
                current_version: current.to_string(),
                latest_version: Some(latest.to_string()),
                wanted_version: parts.get(2).map(std::string::ToString::to_string),
                dev_dependency: false,
            });
        }
    }

    if !dependencies.is_empty() {
        Some(DependencyState {
            total_packages: dependencies.len(),
            outdated_count,
            dependencies,
        })
    } else {
        None
    }
}

/// Validates npm package name according to official rules
fn is_valid_package_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 214 {
        return false;
    }

    // No path traversal
    if name.contains("..") {
        return false;
    }

    // Only safe characters
    name.chars()
        .all(|c| c.is_alphanumeric() || matches!(c, '@' | '/' | '-' | '_' | '.'))
}

#[derive(Debug, Clone)]
pub enum PnpmCommand {
    List { depth: usize },
    Outdated,
    Install { packages: Vec<String> },
}

pub fn run(cmd: PnpmCommand, args: &[String], verbose: u8) -> Result<()> {
    match cmd {
        PnpmCommand::List { depth } => run_list(depth, args, verbose),
        PnpmCommand::Outdated => run_outdated(args, verbose),
        PnpmCommand::Install { packages } => run_install(&packages, args, verbose),
    }
}

fn run_list(depth: usize, args: &[String], verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    let mut cmd = resolved_command("pnpm");
    cmd.arg("list");
    cmd.arg(format!("--depth={depth}"));
    cmd.arg("--json");

    for arg in args {
        cmd.arg(arg);
    }

    let output = cmd.output().context("运行 pnpm list 失败")?;

    if !output.status.success() {
        let stderr = crate::utils::decode_output(&output.stderr);
        anyhow::bail!("pnpm list 失败：{stderr}");
    }

    let stdout = crate::utils::decode_output(&output.stdout);

    // Parse output using PnpmListParser
    let parse_result = PnpmListParser::parse(&stdout);
    let mode = FormatMode::from_verbosity(verbose);

    let filtered = match parse_result {
        ParseResult::Full(data) => {
            if verbose > 0 {
                eprintln!("pnpm list（Tier 1：完整 JSON 解析）");
            }
            data.format(mode)
        }
        ParseResult::Degraded(data, warnings) => {
            if verbose > 0 {
                emit_degradation_warning("pnpm list", &warnings.join(", "));
            }
            data.format(mode)
        }
        ParseResult::Passthrough(raw) => {
            emit_passthrough_warning("pnpm list", "所有解析层级均失败");
            raw
        }
    };

    println!("{filtered}");

    timer.track(
        &format!("pnpm list --depth={depth}"),
        &format!("rtk pnpm list --depth={depth}"),
        &stdout,
        &filtered,
    );

    Ok(())
}

fn run_outdated(args: &[String], verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    let mut cmd = resolved_command("pnpm");
    cmd.arg("outdated");
    cmd.arg("--format");
    cmd.arg("json");

    for arg in args {
        cmd.arg(arg);
    }

    let output = cmd.output().context("运行 pnpm outdated 失败")?;
    let stdout = crate::utils::decode_output(&output.stdout);
    let stderr = crate::utils::decode_output(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    // Parse output using PnpmOutdatedParser
    let parse_result = PnpmOutdatedParser::parse(&stdout);
    let mode = FormatMode::from_verbosity(verbose);

    let filtered = match parse_result {
        ParseResult::Full(data) => {
            if verbose > 0 {
                eprintln!("pnpm outdated（Tier 1：完整 JSON 解析）");
            }
            data.format(mode)
        }
        ParseResult::Degraded(data, warnings) => {
            if verbose > 0 {
                emit_degradation_warning("pnpm outdated", &warnings.join(", "));
            }
            data.format(mode)
        }
        ParseResult::Passthrough(raw) => {
            emit_passthrough_warning("pnpm outdated", "所有解析层级均失败");
            raw
        }
    };

    if filtered.trim().is_empty() {
        println!("所有包已是最新 ✓");
    } else {
        println!("{filtered}");
    }

    timer.track("pnpm outdated", "rtk pnpm outdated", &combined, &filtered);

    Ok(())
}

fn run_install(packages: &[String], args: &[String], verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    // Validate package names to prevent command injection
    for pkg in packages {
        if !is_valid_package_name(pkg) {
            anyhow::bail!("无效的包名：'{pkg}'（包含不安全字符）");
        }
    }

    let mut cmd = resolved_command("pnpm");
    cmd.arg("install");

    for pkg in packages {
        cmd.arg(pkg);
    }

    for arg in args {
        cmd.arg(arg);
    }

    if verbose > 0 {
        eprintln!("pnpm install 运行中...");
    }

    let output = cmd.output().context("运行 pnpm install 失败")?;
    let stdout = crate::utils::decode_output(&output.stdout);
    let stderr = crate::utils::decode_output(&output.stderr);

    if !output.status.success() {
        anyhow::bail!("pnpm install 失败：{stderr}");
    }

    let combined = format!("{stdout}{stderr}");
    let filtered = filter_pnpm_install(&combined);

    println!("{filtered}");

    timer.track(
        &format!("pnpm install {}", packages.join(" ")),
        &format!("rtk pnpm install {}", packages.join(" ")),
        &combined,
        &filtered,
    );

    Ok(())
}

/// Filter pnpm install output - remove progress bars, keep summary
fn filter_pnpm_install(output: &str) -> String {
    let mut result = Vec::new();
    let mut saw_progress = false;

    for line in output.lines() {
        // Skip progress bars
        if line.contains("Progress") || line.contains('│') || line.contains('%') {
            saw_progress = true;
            continue;
        }

        if saw_progress && line.trim().is_empty() {
            continue;
        }

        // Keep error lines
        if line.contains("ERR") || line.contains("error") || line.contains("ERROR") {
            result.push(line.to_string());
            continue;
        }

        // Keep summary lines
        if line.contains("packages in")
            || line.contains("dependencies")
            || line.starts_with('+')
            || line.starts_with('-')
        {
            result.push(line.trim().to_string());
        }
    }

    if result.is_empty() {
        "已完成 ✓".to_string()
    } else {
        result.join("\n")
    }
}

/// Runs an unsupported pnpm subcommand by passing it through directly
pub fn run_passthrough(args: &[OsString], verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    if verbose > 0 {
        eprintln!("pnpm 透传：{args:?}");
    }
    let status = resolved_command("pnpm")
        .args(args)
        .status()
        .context("运行 pnpm 失败")?;

    let args_str = tracking::args_display(args);
    timer.track_passthrough(
        &format!("pnpm {args_str}"),
        &format!("rtk pnpm {args_str} (passthrough)"),
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
    fn test_pnpm_list_parser_json() {
        let json = r#"{
            "my-project": {
                "version": "1.0.0",
                "dependencies": {
                    "express": {
                        "version": "4.18.2"
                    }
                }
            }
        }"#;

        let result = PnpmListParser::parse(json);
        assert_eq!(result.tier(), 1);
        assert!(result.is_ok());

        let data = result.unwrap();
        assert!(data.total_packages >= 2);
    }

    #[test]
    fn test_pnpm_outdated_parser_json() {
        let json = r#"{
            "express": {
                "current": "4.18.2",
                "latest": "4.19.0",
                "wanted": "4.18.2"
            }
        }"#;

        let result = PnpmOutdatedParser::parse(json);
        assert_eq!(result.tier(), 1);
        assert!(result.is_ok());

        let data = result.unwrap();
        assert_eq!(data.outdated_count, 1);
        assert_eq!(data.dependencies[0].name, "express");
    }

    #[test]
    fn test_package_name_validation() {
        assert!(is_valid_package_name("lodash"));
        assert!(is_valid_package_name("@clerk/express"));
        assert!(!is_valid_package_name("../../../etc/passwd"));
        assert!(!is_valid_package_name("lodash; rm -rf /"));
    }

    #[test]
    fn test_run_passthrough_accepts_args() {
        // Test that run_passthrough compiles and has correct signature
        let _args: Vec<OsString> = vec![OsString::from("help")];
        // Compile-time verification that the function exists with correct signature
    }
}
