use crate::tracking;
use crate::utils::resolved_command;
use anyhow::Context;
use anyhow::Result;

/// 在 LLM 上下文中通常会排除的噪音目录
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

    // 将标志位和路径分开
    let show_all = args
        .iter()
        .any(|a| (a.starts_with('-') && !a.starts_with("--") && a.contains('a')) || a == "--all");

    let flags: Vec<&str> = args
        .iter()
        .filter(|a| a.starts_with('-'))
        .map(std::string::String::as_str)
        .collect();
    let paths: Vec<&str> = args
        .iter()
        .filter(|a| !a.starts_with('-'))
        .map(std::string::String::as_str)
        .collect();

    // 构建 `ls -la`，并附加用户传入的额外标志（如 `-R`）
    // 去掉 `-l`、`-a`、`-h`（这些由我们自己处理）
    let mut cmd = resolved_command("ls");
    cmd.arg("-la");
    for flag in &flags {
        if flag.starts_with("--") {
            // 长参数：跳过 `--all`（已自行处理）
            if *flag != "--all" {
                cmd.arg(flag);
            }
        } else {
            let stripped = flag.trim_start_matches('-');
            let extra: String = stripped
                .chars()
                .filter(|c| *c != 'l' && *c != 'a' && *c != 'h')
                .collect();
            if !extra.is_empty() {
                cmd.arg(format!("-{extra}"));
            }
        }
    }

    // 添加路径（如果没有则默认用 "."）
    if paths.is_empty() {
        cmd.arg(".");
    } else {
        for p in &paths {
            cmd.arg(p);
        }
    }

    let output = cmd.output().context("运行 ls 失败")?;

    if !output.status.success() {
        let stderr = crate::utils::decode_output(&output.stderr);
        eprint!("{stderr}");
        std::process::exit(output.status.code().unwrap_or(1));
    }

    let raw = crate::utils::decode_output(&output.stdout).to_string();
    let filtered = compact_ls(&raw, show_all);

    if verbose > 0 {
        eprintln!(
            "字符数：{} → {}（减少 {}%）",
            raw.len(),
            filtered.len(),
            if !raw.is_empty() {
                100 - (filtered.len() * 100 / raw.len())
            } else {
                0
            }
        );
    }

    let target_display = if paths.is_empty() {
        ".".to_string()
    } else {
        paths.join(" ")
    };
    print!("{filtered}");
    timer.track(
        &format!("ls -la {target_display}"),
        "ztok ls",
        &raw,
        &filtered,
    );

    Ok(())
}

/// 将字节数格式化为易读的大小
fn human_size(bytes: u64) -> String {
    if bytes >= 1_048_576 {
        format!("{:.1}M", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.1}K", bytes as f64 / 1024.0)
    } else {
        format!("{bytes}B")
    }
}

/// 将 `ls -la` 输出解析为紧凑格式：
///   `name/`（目录）
///   `name  size`（文件）
fn compact_ls(raw: &str, show_all: bool) -> String {
    use std::collections::HashMap;

    let mut dirs: Vec<String> = Vec::new();
    let mut files: Vec<(String, String)> = Vec::new(); // （名称，大小）
    let mut by_ext: HashMap<String, usize> = HashMap::new();

    for line in raw.lines() {
        // 跳过 total、空行、`.` 和 `..`
        if line.starts_with("total ") || line.is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 9 {
            continue;
        }

        // 文件名从第 9 列开始（可处理空格）
        let name = parts[8..].join(" ");

        // 跳过 `.` 和 `..`
        if name == "." || name == ".." {
            continue;
        }

        // 除非传了 `-a`，否则过滤噪音目录
        if !show_all && NOISE_DIRS.iter().any(|noise| name == *noise) {
            continue;
        }

        let is_dir = parts[0].starts_with('d');

        if is_dir {
            dirs.push(name);
        } else if parts[0].starts_with('-') || parts[0].starts_with('l') {
            let size: u64 = parts[4].parse().unwrap_or(0);
            let ext = if let Some(pos) = name.rfind('.') {
                name[pos..].to_string()
            } else {
                "no ext".to_string()
            };
            *by_ext.entry(ext).or_insert(0) += 1;
            files.push((name, human_size(size)));
        }
    }

    if dirs.is_empty() && files.is_empty() {
        return "（空）\n".to_string();
    }

    let mut out = String::new();

    // 先输出目录，保持紧凑
    for d in &dirs {
        out.push_str(d);
        out.push_str("/\n");
    }

    // 输出文件及大小
    for (name, size) in &files {
        out.push_str(name);
        out.push_str("  ");
        out.push_str(size);
        out.push('\n');
    }

    // 摘要行
    out.push('\n');
    let mut summary = format!("{} 个文件，{} 个目录", files.len(), dirs.len());
    if !by_ext.is_empty() {
        let mut ext_counts: Vec<_> = by_ext.iter().collect();
        ext_counts.sort_by(|a, b| b.1.cmp(a.1));
        let ext_parts: Vec<String> = ext_counts
            .iter()
            .take(5)
            .map(|(ext, count)| format!("{count} 个 {ext}"))
            .collect();
        summary.push_str(" (");
        summary.push_str(&ext_parts.join(", "));
        if ext_counts.len() > 5 {
            summary.push_str(&format!(", +{} 个更多类型", ext_counts.len() - 5));
        }
        summary.push(')');
    }
    out.push_str(&summary);
    out.push('\n');

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compact_basic() {
        let input = "total 48\n\
                     drwxr-xr-x  2 user  staff    64 Jan  1 12:00 .\n\
                     drwxr-xr-x  2 user  staff    64 Jan  1 12:00 ..\n\
                     drwxr-xr-x  2 user  staff    64 Jan  1 12:00 src\n\
                     -rw-r--r--  1 user  staff  1234 Jan  1 12:00 Cargo.toml\n\
                     -rw-r--r--  1 user  staff  5678 Jan  1 12:00 README.md\n";
        let output = compact_ls(input, false);
        assert!(output.contains("src/"));
        assert!(output.contains("Cargo.toml"));
        assert!(output.contains("README.md"));
        assert!(output.contains("1.2K")); // 1234 bytes
        assert!(output.contains("5.5K")); // 5678 bytes
        assert!(!output.contains("drwx")); // no permissions
        assert!(!output.contains("staff")); // no group
        assert!(!output.contains("total")); // no total
        assert!(!output.contains("\n.\n")); // no . entry
        assert!(!output.contains("\n..\n")); // no .. entry
    }

    #[test]
    fn test_compact_filters_noise() {
        let input = "total 8\n\
                     drwxr-xr-x  2 user  staff  64 Jan  1 12:00 node_modules\n\
                     drwxr-xr-x  2 user  staff  64 Jan  1 12:00 .git\n\
                     drwxr-xr-x  2 user  staff  64 Jan  1 12:00 target\n\
                     drwxr-xr-x  2 user  staff  64 Jan  1 12:00 src\n\
                     -rw-r--r--  1 user  staff  100 Jan  1 12:00 main.rs\n";
        let output = compact_ls(input, false);
        assert!(!output.contains("node_modules"));
        assert!(!output.contains(".git"));
        assert!(!output.contains("target"));
        assert!(output.contains("src/"));
        assert!(output.contains("main.rs"));
    }

    #[test]
    fn test_compact_show_all() {
        let input = "total 8\n\
                     drwxr-xr-x  2 user  staff  64 Jan  1 12:00 .git\n\
                     drwxr-xr-x  2 user  staff  64 Jan  1 12:00 src\n";
        let output = compact_ls(input, true);
        assert!(output.contains(".git/"));
        assert!(output.contains("src/"));
    }

    #[test]
    fn test_compact_empty() {
        let input = "total 0\n";
        let output = compact_ls(input, false);
        assert_eq!(output, "（空）\n");
    }

    #[test]
    fn test_compact_summary() {
        let input = "total 48\n\
                     drwxr-xr-x  2 user  staff    64 Jan  1 12:00 src\n\
                     -rw-r--r--  1 user  staff  1234 Jan  1 12:00 main.rs\n\
                     -rw-r--r--  1 user  staff  5678 Jan  1 12:00 lib.rs\n\
                     -rw-r--r--  1 user  staff   100 Jan  1 12:00 Cargo.toml\n";
        let output = compact_ls(input, false);
        assert!(output.contains("3 个文件，1 个目录"));
        assert!(output.contains(".rs"));
        assert!(output.contains(".toml"));
    }

    #[test]
    fn test_human_size() {
        assert_eq!(human_size(0), "0B");
        assert_eq!(human_size(500), "500B");
        assert_eq!(human_size(1024), "1.0K");
        assert_eq!(human_size(1234), "1.2K");
        assert_eq!(human_size(1_048_576), "1.0M");
        assert_eq!(human_size(2_500_000), "2.4M");
    }

    #[test]
    fn test_compact_handles_filenames_with_spaces() {
        let input = "total 8\n\
                     -rw-r--r--  1 user  staff  1234 Jan  1 12:00 my file.txt\n";
        let output = compact_ls(input, false);
        assert!(output.contains("my file.txt"));
    }

    #[test]
    fn test_compact_symlinks() {
        let input = "total 8\n\
                     lrwxr-xr-x  1 user  staff  10 Jan  1 12:00 link -> target\n";
        let output = compact_ls(input, false);
        assert!(output.contains("link -> target"));
    }
}
