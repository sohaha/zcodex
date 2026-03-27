//! `tree` 命令代理，输出经过 token 优化。
//!
//! 本模块代理原生 `tree` 命令，并在保留目录结构可读性的前提下
//! 过滤输出，减少 token 消耗。
//!
//! Token 优化策略：默认通过 `-I` 自动排除噪音目录；若用户显式
//! 传入 `-a`，则尊重用户意图，不做自动排除。

use crate::tracking;
use crate::utils::resolved_command;
use crate::utils::tool_exists;
use anyhow::Context;
use anyhow::Result;

/// 在 LLM 上下文中通常应排除的噪音目录
const NOISE_DIRS: &[&str] = &[
    "node_modules",
    ".git",
    "target",
    "__pycache__",
    ".next",
    "dist",
    "build",
    ".cache",
    ".turbo",
    ".vercel",
    ".pytest_cache",
    ".mypy_cache",
    ".tox",
    ".venv",
    "venv",
    "env",
    ".env",
    "coverage",
    ".nyc_output",
    ".DS_Store",
    "Thumbs.db",
    ".idea",
    ".vscode",
    ".vs",
    "*.egg-info",
    ".eggs",
];

pub fn run(args: &[String], verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    // 检查是否已安装 `tree`
    if !tool_exists("tree") {
        anyhow::bail!(
            "未找到 tree 命令，请先安装：\n\
             - macOS: brew install tree\n\
             - Ubuntu/Debian: sudo apt install tree\n\
             - Fedora/RHEL: sudo dnf install tree\n\
             - Arch: sudo pacman -S tree"
        );
    }

    let mut cmd = resolved_command("tree");

    // 判断用户是否希望显示全部文件，还是沿用默认行为
    let show_all = args.iter().any(|a| a == "-a" || a == "--all");
    let has_ignore = args.iter().any(|a| a == "-I" || a.starts_with("--ignore="));

    // 除非用户要求显示全部内容，或已手动指定 -I，否则自动注入忽略模式
    if !show_all && !has_ignore {
        let ignore_pattern = NOISE_DIRS.join("|");
        cmd.arg("-I").arg(&ignore_pattern);
    }

    // 透传所有用户参数
    for arg in args {
        cmd.arg(arg);
    }

    let output = cmd.output().context("运行 tree 失败")?;

    if !output.status.success() {
        let stderr = crate::utils::decode_output(&output.stderr);
        eprint!("{stderr}");
        std::process::exit(output.status.code().unwrap_or(1));
    }

    let raw = crate::utils::decode_output(&output.stdout).to_string();
    let filtered = filter_tree_output(&raw);

    if verbose > 0 {
        eprintln!(
            "行数：{} → {}（减少 {}%）",
            raw.lines().count(),
            filtered.lines().count(),
            if raw.lines().count() > 0 {
                100 - (filtered.lines().count() * 100 / raw.lines().count())
            } else {
                0
            }
        );
    }

    print!("{filtered}");
    timer.track("tree", "rtk tree", &raw, &filtered);

    Ok(())
}

fn filter_tree_output(raw: &str) -> String {
    let lines: Vec<&str> = raw.lines().collect();

    if lines.is_empty() {
        return "\n".to_string();
    }

    let mut filtered_lines = Vec::new();

    for line in lines {
        // 跳过末尾摘要行（例如 `"5 directories, 23 files"`）
        if line.contains("director") && line.contains("file") {
            continue;
        }

        // 跳过开头多余空行
        if line.trim().is_empty() && filtered_lines.is_empty() {
            continue;
        }

        filtered_lines.push(line);
    }

    // 移除末尾空行
    while filtered_lines.last().is_some_and(|l| l.trim().is_empty()) {
        filtered_lines.pop();
    }

    filtered_lines.join("\n") + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_removes_summary() {
        let input = ".\n├── src\n│   └── main.rs\n└── Cargo.toml\n\n2 directories, 3 files\n";
        let output = filter_tree_output(input);
        assert!(!output.contains("directories"));
        assert!(!output.contains("files"));
        assert!(output.contains("main.rs"));
        assert!(output.contains("Cargo.toml"));
    }

    #[test]
    fn test_filter_preserves_structure() {
        let input = ".\n├── src\n│   ├── main.rs\n│   └── lib.rs\n└── tests\n    └── test.rs\n";
        let output = filter_tree_output(input);
        assert!(output.contains("├──"));
        assert!(output.contains("│"));
        assert!(output.contains("└──"));
        assert!(output.contains("main.rs"));
        assert!(output.contains("test.rs"));
    }

    #[test]
    fn test_filter_handles_empty() {
        let input = "";
        let output = filter_tree_output(input);
        assert_eq!(output, "\n");
    }

    #[test]
    fn test_filter_removes_trailing_empty_lines() {
        let input = ".\n├── file.txt\n\n\n";
        let output = filter_tree_output(input);
        assert_eq!(output.matches('\n').count(), 2); // Root + file.txt + final newline
    }

    #[test]
    fn test_filter_summary_variations() {
        // 测试不同摘要格式
        let inputs = vec![
            (".\n└── file.txt\n\n0 directories, 1 file\n", "1 file"),
            (".\n└── file.txt\n\n1 directory, 0 files\n", "1 directory"),
            (".\n└── file.txt\n\n10 directories, 25 files\n", "25 files"),
        ];

        for (input, summary_fragment) in inputs {
            let output = filter_tree_output(input);
            assert!(
                !output.contains(summary_fragment),
                "应从输出中移除摘要 `{summary_fragment}`"
            );
            assert!(output.contains("file.txt"), "应保留输出中的 file.txt");
        }
    }

    #[test]
    fn test_noise_dirs_constant() {
        // 验证 `NOISE_DIRS` 是否包含预期模式
        assert!(NOISE_DIRS.contains(&"node_modules"));
        assert!(NOISE_DIRS.contains(&".git"));
        assert!(NOISE_DIRS.contains(&"target"));
        assert!(NOISE_DIRS.contains(&"__pycache__"));
        assert!(NOISE_DIRS.contains(&".next"));
        assert!(NOISE_DIRS.contains(&"dist"));
        assert!(NOISE_DIRS.contains(&"build"));
    }
}
