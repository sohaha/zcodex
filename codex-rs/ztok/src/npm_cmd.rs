use crate::tracking;
use crate::utils::resolved_command;
use anyhow::Context;
use anyhow::Result;

/// 已知 `npm` 子命令：这些命令前不应自动注入 `run`。
/// 生产代码与测试共享这份列表，避免两边行为漂移。
const NPM_SUBCOMMANDS: &[&str] = &[
    "install",
    "i",
    "ci",
    "uninstall",
    "remove",
    "rm",
    "update",
    "up",
    "list",
    "ls",
    "outdated",
    "init",
    "create",
    "publish",
    "pack",
    "link",
    "audit",
    "fund",
    "exec",
    "explain",
    "why",
    "search",
    "view",
    "info",
    "show",
    "config",
    "set",
    "get",
    "cache",
    "prune",
    "dedupe",
    "doctor",
    "help",
    "version",
    "prefix",
    "root",
    "bin",
    "bugs",
    "docs",
    "home",
    "repo",
    "ping",
    "whoami",
    "token",
    "profile",
    "team",
    "access",
    "owner",
    "deprecate",
    "dist-tag",
    "star",
    "stars",
    "login",
    "logout",
    "adduser",
    "unpublish",
    "pkg",
    "diff",
    "rebuild",
    "test",
    "t",
    "start",
    "stop",
    "restart",
];

pub fn run(args: &[String], verbose: u8, skip_env: bool) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    let mut cmd = resolved_command("npm");

    // 判断这是 `npm run <script>` 还是其他 `npm` 子命令（如 `install`、`list`）。
    // 只有当参数看起来像脚本名时才注入 `run`，已知子命令不注入。
    let first_arg = args.first().map(std::string::String::as_str);
    let is_run_explicit = first_arg == Some("run");
    let is_npm_subcommand = first_arg
        .map(|a| NPM_SUBCOMMANDS.contains(&a) || a.starts_with('-'))
        .unwrap_or(false);

    let effective_args = if is_run_explicit {
        // `ztok npm run build` → `npm run build`
        cmd.arg("run");
        &args[1..]
    } else if is_npm_subcommand {
        // `ztok npm install express` → `npm install express`
        args
    } else {
        // `ztok npm build` → `npm run build`（视为脚本名）
        cmd.arg("run");
        args
    };

    for arg in effective_args {
        cmd.arg(arg);
    }

    if skip_env {
        cmd.env("SKIP_ENV_VALIDATION", "1");
    }

    if verbose > 0 {
        eprintln!("运行：npm {}", args.join(" "));
    }

    let output = cmd.output().context("运行 npm 失败")?;
    let stdout = crate::utils::decode_output(&output.stdout);
    let stderr = crate::utils::decode_output(&output.stderr);
    let raw = format!("{stdout}\n{stderr}");

    let filtered = filter_npm_output(&raw);
    println!("{filtered}");

    timer.track(
        &format!("npm {}", args.join(" ")),
        &format!("ztok npm {}", args.join(" ")),
        &raw,
        &filtered,
    );

    if !output.status.success() {
        std::process::exit(output.status.code().unwrap_or(1));
    }

    Ok(())
}

/// 过滤 `npm` 输出：去掉样板信息、进度条和 `npm WARN`
fn filter_npm_output(output: &str) -> String {
    let mut result = Vec::new();

    for line in output.lines() {
        // 跳过 `npm` 样板输出
        if line.starts_with('>') && line.contains('@') {
            continue;
        }
        // 跳过 `npm` 生命周期脚本提示
        if line.trim_start().starts_with("npm WARN") {
            continue;
        }
        if line.trim_start().starts_with("npm notice") {
            continue;
        }
        // 跳过进度指示
        if line.contains("⸩") || line.contains("⸨") || line.contains("...") && line.len() < 10 {
            continue;
        }
        // 跳过空行
        if line.trim().is_empty() {
            continue;
        }

        result.push(line.to_string());
    }

    if result.is_empty() {
        "已完成 ✓".to_string()
    } else {
        result.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_npm_output() {
        let output = r#"
> project@1.0.0 build
> next build

npm WARN deprecated inflight@1.0.6: This module is not supported
npm notice

   Creating an optimized production build...
   ✓ Build completed
"#;
        let result = filter_npm_output(output);
        assert!(!result.contains("npm WARN"));
        assert!(!result.contains("npm notice"));
        assert!(!result.contains("> project@"));
        assert!(result.contains("Build completed"));
    }

    #[test]
    fn test_npm_subcommand_routing() {
        // 使用共享的 NPM_SUBCOMMANDS 常量，确保生产代码与测试不漂移。
        fn needs_run_injection(args: &[&str]) -> bool {
            let first = args.first().copied();
            let is_run_explicit = first == Some("run");
            let is_subcommand = first
                .map(|a| NPM_SUBCOMMANDS.contains(&a) || a.starts_with('-'))
                .unwrap_or(false);
            !is_run_explicit && !is_subcommand
        }

        // 已知子命令不应注入 `run`
        for subcmd in NPM_SUBCOMMANDS {
            assert!(
                !needs_run_injection(&[subcmd]),
                "`npm {subcmd}` 不应注入 `run`"
            );
        }

        // 脚本名应注入 `run`
        for script in &["build", "dev", "lint", "typecheck", "deploy"] {
            assert!(
                needs_run_injection(&[script]),
                "`npm {script}` 应注入 `run`"
            );
        }

        // 纯 flag 不应注入 `run`
        assert!(!needs_run_injection(&["--version"]));
        assert!(!needs_run_injection(&["-h"]));

        // 已显式写出 `run` 时，不应再额外注入
        assert!(!needs_run_injection(&["run", "build"]));
    }

    #[test]
    fn test_filter_npm_output_empty() {
        let output = "\n\n\n";
        let result = filter_npm_output(output);
        assert_eq!(result, "已完成 ✓");
    }
}
