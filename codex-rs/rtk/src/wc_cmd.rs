/// `wc` 的紧凑过滤器：移除冗余路径和对齐填充。
///
/// 压缩示例：
/// - `wc file.py`     → `30L 96W 978B`
/// - `wc -l file.py`  → `30`
/// - `wc -w file.py`  → `96`
/// - `wc -c file.py`  → `978`
/// - `wc -l *.py`     → table with common path prefix stripped
use crate::tracking;
use crate::utils::resolved_command;
use anyhow::Context;
use anyhow::Result;

pub fn run(args: &[String], verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    let mut cmd = resolved_command("wc");
    for arg in args {
        cmd.arg(arg);
    }

    if verbose > 0 {
        eprintln!("运行：wc {}", args.join(" "));
    }

    let output = cmd.output().context("运行 wc 失败")?;
    let stdout = crate::utils::decode_output(&output.stdout);
    let stderr = crate::utils::decode_output(&output.stderr);

    if !output.status.success() {
        let msg = if stderr.trim().is_empty() {
            stdout.trim().to_string()
        } else {
            stderr.trim().to_string()
        };
        eprintln!("失败：wc {msg}");
        std::process::exit(output.status.code().unwrap_or(1));
    }

    let raw = stdout.to_string();

    // 检测用户请求了哪些列
    let mode = detect_mode(args);
    let filtered = filter_wc_output(&raw, &mode);
    println!("{filtered}");

    timer.track(
        &format!("wc {}", args.join(" ")),
        &format!("rtk wc {}", args.join(" ")),
        &raw,
        &filtered,
    );

    Ok(())
}

/// 用户请求的列类型
#[derive(Debug, PartialEq)]
enum WcMode {
    /// 默认：行数、词数、字节数（3 列）
    Full,
    /// 仅行数（-l）
    Lines,
    /// 仅词数（-w）
    Words,
    /// 仅字节数（-c）
    Bytes,
    /// 仅字符数（-m）
    Chars,
    /// 组合 flag —— 保持紧凑格式
    Mixed,
}

fn detect_mode(args: &[String]) -> WcMode {
    let flags: Vec<&str> = args
        .iter()
        .filter(|a| a.starts_with('-'))
        .map(std::string::String::as_str)
        .collect();

    if flags.is_empty() {
        return WcMode::Full;
    }

    // 收集所有单字符 flag（支持 `-lw` 这类组合写法）
    let mut has_l = false;
    let mut has_w = false;
    let mut has_c = false;
    let mut has_m = false;
    let mut flag_count = 0;

    for flag in &flags {
        for ch in flag.chars().skip(1) {
            match ch {
                'l' => {
                    has_l = true;
                    flag_count += 1;
                }
                'w' => {
                    has_w = true;
                    flag_count += 1;
                }
                'c' => {
                    has_c = true;
                    flag_count += 1;
                }
                'm' => {
                    has_m = true;
                    flag_count += 1;
                }
                _ => {}
            }
        }
    }

    if flag_count == 0 {
        return WcMode::Full;
    }
    if flag_count > 1 {
        return WcMode::Mixed;
    }

    if has_l {
        WcMode::Lines
    } else if has_w {
        WcMode::Words
    } else if has_c {
        WcMode::Bytes
    } else if has_m {
        WcMode::Chars
    } else {
        WcMode::Full
    }
}

fn filter_wc_output(raw: &str, mode: &WcMode) -> String {
    let lines: Vec<&str> = raw.trim().lines().collect();

    if lines.is_empty() {
        return String::new();
    }

    // 单文件（只有一行输出，且不含 `total`）
    if lines.len() == 1 {
        return format_single_line(lines[0], mode);
    }

    // 多文件 —— 使用紧凑表格
    format_multi_line(&lines, mode)
}

/// 格式化单行 `wc` 输出（单文件或 stdin）
fn format_single_line(line: &str, mode: &WcMode) -> String {
    let parts: Vec<&str> = line.split_whitespace().collect();

    match mode {
        WcMode::Lines | WcMode::Words | WcMode::Bytes | WcMode::Chars => {
            // 第一个数字就是用户请求的唯一列
            parts
                .first()
                .map(std::string::ToString::to_string)
                .unwrap_or_default()
        }
        WcMode::Full => {
            if parts.len() >= 3 {
                format!("{}L {}W {}B", parts[0], parts[1], parts[2])
            } else {
                line.trim().to_string()
            }
        }
        WcMode::Mixed => {
            // 去掉文件路径，只保留数字
            if parts.len() >= 2 {
                let last_is_path = parts.last().is_some_and(|p| p.parse::<u64>().is_err());
                if last_is_path {
                    parts[..parts.len() - 1].join(" ")
                } else {
                    parts.join(" ")
                }
            } else {
                line.trim().to_string()
            }
        }
    }
}

/// 将多文件输出格式化为紧凑表格
fn format_multi_line(lines: &[&str], mode: &WcMode) -> String {
    let mut result = Vec::new();

    // 找出公共目录前缀，缩短路径显示
    let paths: Vec<&str> = lines
        .iter()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            parts.last().copied()
        })
        .filter(|p| *p != "total")
        .collect();

    let common_prefix = find_common_prefix(&paths);

    for line in lines {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        let is_total = parts.last().is_some_and(|p| *p == "total");

        match mode {
            WcMode::Lines | WcMode::Words | WcMode::Bytes | WcMode::Chars => {
                if is_total {
                    result.push(format!("Σ {}", parts.first().unwrap_or(&"0")));
                } else {
                    let name = strip_prefix(parts.last().unwrap_or(&""), &common_prefix);
                    result.push(format!("{} {}", parts.first().unwrap_or(&"0"), name));
                }
            }
            WcMode::Full => {
                if is_total {
                    result.push(format!(
                        "Σ {}L {}W {}B",
                        parts.first().unwrap_or(&"0"),
                        parts.get(1).unwrap_or(&"0"),
                        parts.get(2).unwrap_or(&"0"),
                    ));
                } else if parts.len() >= 4 {
                    let name = strip_prefix(parts[3], &common_prefix);
                    result.push(format!(
                        "{}L {}W {}B {}",
                        parts[0], parts[1], parts[2], name
                    ));
                } else {
                    result.push(line.trim().to_string());
                }
            }
            WcMode::Mixed => {
                if is_total {
                    let nums: Vec<&str> = parts[..parts.len() - 1].to_vec();
                    result.push(format!("Σ {}", nums.join(" ")));
                } else if parts.len() >= 2 {
                    let last_is_path = parts.last().is_some_and(|p| p.parse::<u64>().is_err());
                    if last_is_path {
                        let name = strip_prefix(parts.last().unwrap_or(&""), &common_prefix);
                        let nums: Vec<&str> = parts[..parts.len() - 1].to_vec();
                        result.push(format!("{} {}", nums.join(" "), name));
                    } else {
                        result.push(parts.join(" "));
                    }
                } else {
                    result.push(line.trim().to_string());
                }
            }
        }
    }

    result.join("\n")
}

/// 找出路径之间的公共目录前缀
fn find_common_prefix(paths: &[&str]) -> String {
    if paths.len() <= 1 {
        return String::new();
    }

    let first = paths[0];
    let prefix = if let Some(pos) = first.rfind('/') {
        &first[..=pos]
    } else {
        return String::new();
    };

    if paths.iter().all(|p| p.starts_with(prefix)) {
        return prefix.to_string();
    }

    // 逐步移除最右侧路径段，尝试更短前缀
    let mut candidate = prefix.to_string();
    while !candidate.is_empty() {
        if paths.iter().all(|p| p.starts_with(&candidate)) {
            return candidate;
        }
        if let Some(pos) = candidate[..candidate.len() - 1].rfind('/') {
            candidate.truncate(pos + 1);
        } else {
            return String::new();
        }
    }
    String::new()
}

/// 从路径中去掉公共前缀
fn strip_prefix<'a>(path: &'a str, prefix: &str) -> &'a str {
    if prefix.is_empty() {
        return path;
    }
    path.strip_prefix(prefix).unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_file_full() {
        let raw = "      30      96     978 scripts/find_duplicate_attrs.py\n";
        let result = filter_wc_output(raw, &WcMode::Full);
        assert_eq!(result, "30L 96W 978B");
    }

    #[test]
    fn test_single_file_lines_only() {
        let raw = "      30 scripts/find_duplicate_attrs.py\n";
        let result = filter_wc_output(raw, &WcMode::Lines);
        assert_eq!(result, "30");
    }

    #[test]
    fn test_single_file_words_only() {
        let raw = "      96 scripts/find_duplicate_attrs.py\n";
        let result = filter_wc_output(raw, &WcMode::Words);
        assert_eq!(result, "96");
    }

    #[test]
    fn test_stdin_full() {
        let raw = "      30      96     978\n";
        let result = filter_wc_output(raw, &WcMode::Full);
        assert_eq!(result, "30L 96W 978B");
    }

    #[test]
    fn test_stdin_lines() {
        let raw = "      30\n";
        let result = filter_wc_output(raw, &WcMode::Lines);
        assert_eq!(result, "30");
    }

    #[test]
    fn test_multi_file_lines() {
        let raw = "      30 src/main.rs\n      50 src/lib.rs\n      80 total\n";
        let result = filter_wc_output(raw, &WcMode::Lines);
        assert_eq!(result, "30 main.rs\n50 lib.rs\nΣ 80");
    }

    #[test]
    fn test_multi_file_full() {
        let raw = "      30      96     978 src/main.rs\n      50     120    1500 src/lib.rs\n      80     216    2478 total\n";
        let result = filter_wc_output(raw, &WcMode::Full);
        assert_eq!(
            result,
            "30L 96W 978B main.rs\n50L 120W 1500B lib.rs\nΣ 80L 216W 2478B"
        );
    }

    #[test]
    fn test_detect_mode_full() {
        let args: Vec<String> = vec!["file.py".into()];
        assert_eq!(detect_mode(&args), WcMode::Full);
    }

    #[test]
    fn test_detect_mode_lines() {
        let args: Vec<String> = vec!["-l".into(), "file.py".into()];
        assert_eq!(detect_mode(&args), WcMode::Lines);
    }

    #[test]
    fn test_detect_mode_mixed() {
        let args: Vec<String> = vec!["-lw".into(), "file.py".into()];
        assert_eq!(detect_mode(&args), WcMode::Mixed);
    }

    #[test]
    fn test_detect_mode_separate_flags() {
        let args: Vec<String> = vec!["-l".into(), "-w".into(), "file.py".into()];
        assert_eq!(detect_mode(&args), WcMode::Mixed);
    }

    #[test]
    fn test_common_prefix() {
        let paths = vec!["src/main.rs", "src/lib.rs", "src/utils.rs"];
        assert_eq!(find_common_prefix(&paths), "src/");
    }

    #[test]
    fn test_no_common_prefix() {
        let paths = vec!["main.rs", "lib.rs"];
        assert_eq!(find_common_prefix(&paths), "");
    }

    #[test]
    fn test_deep_common_prefix() {
        let paths = vec!["src/cmd/wc.rs", "src/cmd/ls.rs"];
        assert_eq!(find_common_prefix(&paths), "src/cmd/");
    }

    #[test]
    fn test_empty() {
        let raw = "";
        let result = filter_wc_output(raw, &WcMode::Full);
        assert_eq!(result, "");
    }
}
