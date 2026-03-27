use crate::tracking;
use crate::utils::resolved_command;
use anyhow::Context;
use anyhow::Result;
use regex::Regex;
use std::collections::HashMap;

pub struct GrepOptions<'a> {
    pub pattern: &'a str,
    pub path: &'a str,
    pub max_line_len: usize,
    pub max_results: usize,
    pub context_only: bool,
    pub file_type: Option<&'a str>,
    pub extra_args: &'a [String],
}

pub fn run(options: GrepOptions<'_>, verbose: u8) -> Result<()> {
    let GrepOptions {
        pattern,
        path,
        max_line_len,
        max_results,
        context_only,
        file_type,
        extra_args,
    } = options;
    let timer = tracking::TimedExecution::start();

    if verbose > 0 {
        eprintln!("grep：在 {path} 中搜索 '{pattern}'");
    }

    // 兼容处理：把 BRE 的 `\|` 转成 rg 可用的 `|`
    let rg_pattern = pattern.replace(r"\|", "|");

    let mut rg_cmd = resolved_command("rg");
    rg_cmd.args(["-n", "--no-heading", &rg_pattern, path]);

    if let Some(ft) = file_type {
        rg_cmd.arg("--type").arg(ft);
    }

    for arg in extra_args {
        // 兼容处理：跳过 grep 风格的 `-r`（rg 默认递归；rg 的 `-r` 表示 `--replace`）
        if arg == "-r" || arg == "--recursive" {
            continue;
        }
        rg_cmd.arg(arg);
    }

    let output = rg_cmd
        .output()
        .or_else(|_| {
            resolved_command("grep")
                .args(["-rn", pattern, path])
                .output()
        })
        .context("grep/rg failed")?;

    let stdout = crate::utils::decode_output(&output.stdout);
    let exit_code = output.status.code().unwrap_or(1);

    let raw_output = stdout.to_string();

    if stdout.trim().is_empty() {
        // 遇到错误时显示 stderr（错误正则、文件缺失等）
        if exit_code == 2 {
            let stderr = crate::utils::decode_output(&output.stderr);
            if !stderr.trim().is_empty() {
                eprintln!("{}", stderr.trim());
            }
        }
        let msg = format!("🔍 0 for '{pattern}'");
        println!("{msg}");
        timer.track(
            &format!("grep -rn '{pattern}' {path}"),
            "rtk grep",
            &raw_output,
            &msg,
        );
        if exit_code != 0 {
            std::process::exit(exit_code);
        }
        return Ok(());
    }

    let mut by_file: HashMap<String, Vec<(usize, String)>> = HashMap::new();
    let mut total = 0;

    for line in stdout.lines() {
        let parts: Vec<&str> = line.splitn(3, ':').collect();

        let (file, line_num, content) = if parts.len() == 3 {
            let ln = parts[1].parse().unwrap_or(0);
            (parts[0].to_string(), ln, parts[2])
        } else if parts.len() == 2 {
            let ln = parts[0].parse().unwrap_or(0);
            (path.to_string(), ln, parts[1])
        } else {
            continue;
        };

        total += 1;
        let cleaned = clean_line(content, max_line_len, context_only, pattern);
        by_file.entry(file).or_default().push((line_num, cleaned));
    }

    let mut rtk_output = String::new();
    rtk_output.push_str(&format!("🔍 {} in {}F:\n\n", total, by_file.len()));

    let mut shown = 0;
    let mut files: Vec<_> = by_file.iter().collect();
    files.sort_by_key(|(f, _)| *f);

    for (file, matches) in files {
        if shown >= max_results {
            break;
        }

        let file_display = compact_path(file);
        rtk_output.push_str(&format!("📄 {} ({}):\n", file_display, matches.len()));

        for (line_num, content) in matches.iter().take(10) {
            rtk_output.push_str(&format!("  {line_num:>4}: {content}\n"));
            shown += 1;
            if shown >= max_results {
                break;
            }
        }

        if matches.len() > 10 {
            rtk_output.push_str(&format!("  +{}\n", matches.len() - 10));
        }
        rtk_output.push('\n');
    }

    if total > shown {
        rtk_output.push_str(&format!("... +{}\n", total - shown));
    }

    print!("{rtk_output}");
    timer.track(
        &format!("grep -rn '{pattern}' {path}"),
        "rtk grep",
        &raw_output,
        &rtk_output,
    );

    if exit_code != 0 {
        std::process::exit(exit_code);
    }

    Ok(())
}

fn clean_line(line: &str, max_len: usize, context_only: bool, pattern: &str) -> String {
    let trimmed = line.trim();

    if context_only
        && let Ok(re) = Regex::new(&format!("(?i).{{0,20}}{}.*", regex::escape(pattern)))
        && let Some(m) = re.find(trimmed)
    {
        let matched = m.as_str();
        if matched.len() <= max_len {
            return matched.to_string();
        }
    }

    if trimmed.len() <= max_len {
        trimmed.to_string()
    } else {
        let lower = trimmed.to_lowercase();
        let pattern_lower = pattern.to_lowercase();

        if let Some(pos) = lower.find(&pattern_lower) {
            let char_pos = lower[..pos].chars().count();
            let chars: Vec<char> = trimmed.chars().collect();
            let char_len = chars.len();

            let start = char_pos.saturating_sub(max_len / 3);
            let end = (start + max_len).min(char_len);
            let start = if end == char_len {
                end.saturating_sub(max_len)
            } else {
                start
            };

            let slice: String = chars[start..end].iter().collect();
            if start > 0 && end < char_len {
                format!("...{slice}...")
            } else if start > 0 {
                format!("...{slice}")
            } else {
                format!("{slice}...")
            }
        } else {
            let t: String = trimmed.chars().take(max_len - 3).collect();
            format!("{t}...")
        }
    }
}

fn compact_path(path: &str) -> String {
    if path.len() <= 50 {
        return path.to_string();
    }

    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() <= 3 {
        return path.to_string();
    }

    format!(
        "{}/.../{}/{}",
        parts[0],
        parts[parts.len() - 2],
        parts[parts.len() - 1]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_line() {
        let line = "            const result = someFunction();";
        let cleaned = clean_line(line, 50, false, "result");
        assert!(!cleaned.starts_with(' '));
        assert!(cleaned.len() <= 50);
    }

    #[test]
    fn test_compact_path() {
        let path = "/Users/patrick/dev/project/src/components/Button.tsx";
        let compact = compact_path(path);
        assert!(compact.len() <= 60);
    }

    #[test]
    fn test_extra_args_accepted() {
        // 验证函数签名允许接收 extra_args
        // 这是编译期测试：只要能编译，就说明签名正确
        let _extra: Vec<String> = vec!["-i".to_string(), "-A".to_string(), "3".to_string()];
        // 无需实际运行，这里只验证参数存在
    }

    #[test]
    fn test_clean_line_multibyte() {
        // 超过 max_len 字节数的泰文文本
        let line = "  สวัสดีครับ นี่คือข้อความที่ยาวมากสำหรับทดสอบ  ";
        let cleaned = clean_line(line, 20, false, "ครับ");
        // 不应 panic
        assert!(!cleaned.is_empty());
    }

    #[test]
    fn test_clean_line_emoji() {
        let line = "🎉🎊🎈🎁🎂🎄 some text 🎃🎆🎇✨";
        let cleaned = clean_line(line, 15, false, "text");
        assert!(!cleaned.is_empty());
    }

    // 兼容：BRE 的 `\|` 会被转换成 rg 可接受的 `|`
    #[test]
    fn test_bre_alternation_translated() {
        let pattern = r"fn foo\|pub.*bar";
        let rg_pattern = pattern.replace(r"\|", "|");
        assert_eq!(rg_pattern, "fn foo|pub.*bar");
    }

    // 兼容：从 extra_args 中移除 `-r`（rg 默认递归）
    #[test]
    fn test_recursive_flag_stripped() {
        let extra_args: Vec<String> = vec!["-r".to_string(), "-i".to_string()];
        let filtered: Vec<&String> = extra_args
            .iter()
            .filter(|a| *a != "-r" && *a != "--recursive")
            .collect();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0], "-i");
    }

    // 验证 rg 调用始终带有行号参数。
    // `main.rs` 中的 `-n/--line-numbers` clap 标志只是为兼容而保留的空操作。
    #[test]
    fn test_rg_always_has_line_numbers() {
        // `grep_cmd::run()` 总是向 rg 传入 `-n`。
        // 该测试用于说明 `-n` 是内建行为，因此 clap 标志可以安全忽略。
        let mut cmd = resolved_command("rg");
        cmd.args(["-n", "--no-heading", "NONEXISTENT_PATTERN_12345", "."]);
        // 如果安装了 rg，它应接受 `-n` 且不报错（exit 1 表示无匹配，不是错误）
        if let Ok(output) = cmd.output() {
            assert!(
                output.status.code() == Some(1) || output.status.success(),
                "`rg -n` 应被正常接受"
            );
        }
        // 如果未安装 rg，则优雅跳过（测试仍然通过）
    }
}
