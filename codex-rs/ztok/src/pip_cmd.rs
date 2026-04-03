use crate::tracking;
use crate::utils::resolved_command;
use crate::utils::tool_exists;
use anyhow::Context;
use anyhow::Result;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Package {
    name: String,
    version: String,
    #[serde(default)]
    latest_version: Option<String>,
}

pub fn run(args: &[String], verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    // 自动识别使用 uv 还是 pip
    let use_uv = tool_exists("uv");
    let base_cmd = if use_uv { "uv" } else { "pip" };

    if verbose > 0 && use_uv {
        eprintln!("使用 uv (兼容 pip)");
    }

    // 识别子命令
    let subcommand = args.first().map(std::string::String::as_str).unwrap_or("");

    let (cmd_str, filtered) = match subcommand {
        "list" => run_list(base_cmd, &args[1..], verbose)?,
        "outdated" => run_outdated(base_cmd, &args[1..], verbose)?,
        "install" | "uninstall" | "show" => {
            // 写操作直接透传
            run_passthrough(base_cmd, args, verbose)?
        }
        _ => {
            anyhow::bail!(
                "ztok pip: 不支持的子命令 '{subcommand}'\n支持：list、outdated、install、uninstall、show"
            );
        }
    };

    timer.track(
        &format!("{} {}", base_cmd, args.join(" ")),
        &format!("ztok {} {}", base_cmd, args.join(" ")),
        &cmd_str,
        &filtered,
    );

    Ok(())
}

fn run_list(base_cmd: &str, args: &[String], verbose: u8) -> Result<(String, String)> {
    let mut cmd = resolved_command(base_cmd);

    if base_cmd == "uv" {
        cmd.arg("pip");
    }

    cmd.arg("list").arg("--format=json");

    for arg in args {
        cmd.arg(arg);
    }

    if verbose > 0 {
        eprintln!("运行：{base_cmd} pip list --format=json");
    }

    let output = cmd
        .output()
        .with_context(|| format!("运行 {base_cmd} pip list 失败"))?;

    let stdout = crate::utils::decode_output(&output.stdout);
    let stderr = crate::utils::decode_output(&output.stderr);
    let raw = format!("{stdout}\n{stderr}");

    let filtered = filter_pip_list(&stdout);
    println!("{filtered}");

    if !output.status.success() {
        std::process::exit(output.status.code().unwrap_or(1));
    }

    Ok((raw, filtered))
}

fn run_outdated(base_cmd: &str, args: &[String], verbose: u8) -> Result<(String, String)> {
    let mut cmd = resolved_command(base_cmd);

    if base_cmd == "uv" {
        cmd.arg("pip");
    }

    cmd.arg("list").arg("--outdated").arg("--format=json");

    for arg in args {
        cmd.arg(arg);
    }

    if verbose > 0 {
        eprintln!("运行：{base_cmd} pip list --outdated --format=json");
    }

    let output = cmd
        .output()
        .with_context(|| format!("运行 {base_cmd} pip list --outdated 失败"))?;

    let stdout = crate::utils::decode_output(&output.stdout);
    let stderr = crate::utils::decode_output(&output.stderr);
    let raw = format!("{stdout}\n{stderr}");

    let filtered = filter_pip_outdated(&stdout);
    println!("{filtered}");

    if !output.status.success() {
        std::process::exit(output.status.code().unwrap_or(1));
    }

    Ok((raw, filtered))
}

fn run_passthrough(base_cmd: &str, args: &[String], verbose: u8) -> Result<(String, String)> {
    let mut cmd = resolved_command(base_cmd);

    if base_cmd == "uv" {
        cmd.arg("pip");
    }

    for arg in args {
        cmd.arg(arg);
    }

    if verbose > 0 {
        eprintln!("运行：{} pip {}", base_cmd, args.join(" "));
    }

    let output = cmd
        .output()
        .with_context(|| format!("运行 {} pip {} 失败", base_cmd, args.join(" ")))?;

    let stdout = crate::utils::decode_output(&output.stdout);
    let stderr = crate::utils::decode_output(&output.stderr);
    let raw = format!("{stdout}\n{stderr}");

    print!("{stdout}");
    eprint!("{stderr}");

    if !output.status.success() {
        std::process::exit(output.status.code().unwrap_or(1));
    }

    Ok((raw.clone(), raw))
}

/// 过滤 `pip list` 的 JSON 输出
fn filter_pip_list(output: &str) -> String {
    let packages: Vec<Package> = match serde_json::from_str(output) {
        Ok(p) => p,
        Err(e) => {
            return format!("pip list (JSON 解析失败: {e})");
        }
    };

    if packages.is_empty() {
        return "pip list: 未安装任何包".to_string();
    }

    let mut result = String::new();
    result.push_str(&format!("pip list: {} 个包\n", packages.len()));
    result.push_str("═══════════════════════════════════════\n");

    // 按首字母分组，便于快速浏览
    let mut by_letter: std::collections::HashMap<char, Vec<&Package>> =
        std::collections::HashMap::new();

    for pkg in &packages {
        let first_char = pkg.name.chars().next().unwrap_or('?').to_ascii_lowercase();
        by_letter.entry(first_char).or_default().push(pkg);
    }

    let mut letters: Vec<_> = by_letter.keys().collect();
    letters.sort();

    for letter in letters {
        let Some(pkgs) = by_letter.get(letter) else {
            continue;
        };
        result.push_str(&format!("\n[{}]\n", letter.to_uppercase()));

        for pkg in pkgs.iter().take(10) {
            result.push_str(&format!("  {} ({})\n", pkg.name, pkg.version));
        }

        if pkgs.len() > 10 {
            result.push_str(&format!("  ... +{} 个\n", pkgs.len() - 10));
        }
    }

    result.trim().to_string()
}

/// 过滤 `pip outdated` 的 JSON 输出
fn filter_pip_outdated(output: &str) -> String {
    let packages: Vec<Package> = match serde_json::from_str(output) {
        Ok(p) => p,
        Err(e) => {
            return format!("pip outdated (JSON 解析失败: {e})");
        }
    };

    if packages.is_empty() {
        return "✓ pip outdated: 所有包已是最新".to_string();
    }

    let mut result = String::new();
    result.push_str(&format!("pip outdated: {} 个包\n", packages.len()));
    result.push_str("═══════════════════════════════════════\n");

    for (i, pkg) in packages.iter().take(20).enumerate() {
        let latest = pkg.latest_version.as_deref().unwrap_or("unknown");
        result.push_str(&format!(
            "{}. {} ({} → {})\n",
            i + 1,
            pkg.name,
            pkg.version,
            latest
        ));
    }

    if packages.len() > 20 {
        result.push_str(&format!("\n... +{} 个包\n", packages.len() - 20));
    }

    result.push_str("\n💡 运行 `pip install --upgrade <package>` 进行更新\n");

    result.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_pip_list() {
        let output = r#"[
  {"name": "requests", "version": "2.31.0"},
  {"name": "pytest", "version": "7.4.0"},
  {"name": "rich", "version": "13.0.0"}
]"#;

        let result = filter_pip_list(output);
        assert!(result.contains("3 个包"));
        assert!(result.contains("requests"));
        assert!(result.contains("2.31.0"));
        assert!(result.contains("pytest"));
    }

    #[test]
    fn test_filter_pip_list_empty() {
        let output = "[]";
        let result = filter_pip_list(output);
        assert!(result.contains("未安装任何包"));
    }

    #[test]
    fn test_filter_pip_outdated_none() {
        let output = "[]";
        let result = filter_pip_outdated(output);
        assert!(result.contains("✓"));
        assert!(result.contains("所有包已是最新"));
    }

    #[test]
    fn test_filter_pip_outdated_some() {
        let output = r#"[
  {"name": "requests", "version": "2.31.0", "latest_version": "2.32.0"},
  {"name": "pytest", "version": "7.4.0", "latest_version": "8.0.0"}
]"#;

        let result = filter_pip_outdated(output);
        assert!(result.contains("2 个包"));
        assert!(result.contains("requests"));
        assert!(result.contains("2.31.0 → 2.32.0"));
        assert!(result.contains("pytest"));
        assert!(result.contains("7.4.0 → 8.0.0"));
    }
}
