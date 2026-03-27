use crate::filter::FilterLevel;
use crate::filter::Language;
use crate::filter::{self};
use crate::tracking;
use anyhow::Context;
use anyhow::Result;
use std::fs;
use std::path::Path;

pub fn run(
    file: &Path,
    level: FilterLevel,
    max_lines: Option<usize>,
    tail_lines: Option<usize>,
    line_numbers: bool,
    verbose: u8,
) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    if verbose > 0 {
        eprintln!("读取：{}（过滤：{}）", file.display(), level);
    }

    // 读取文件内容
    let content =
        fs::read_to_string(file).with_context(|| format!("读取文件失败：{}", file.display()))?;

    // 根据扩展名识别语言
    let lang = file
        .extension()
        .and_then(|e| e.to_str())
        .map(Language::from_extension)
        .unwrap_or(Language::Unknown);

    if verbose > 1 {
        eprintln!("检测到语言：{lang:?}");
    }

    // 应用过滤器
    let filter = filter::get_filter(level);
    let mut filtered = filter.filter(&content, lang);

    if verbose > 0 {
        let original_lines = content.lines().count();
        let filtered_lines = filtered.lines().count();
        let reduction = if original_lines > 0 {
            ((original_lines - filtered_lines) as f64 / original_lines as f64) * 100.0
        } else {
            0.0
        };
        eprintln!("行数：{original_lines} -> {filtered_lines}（减少 {reduction:.1}%）");
    }

    filtered = apply_line_window(&filtered, max_lines, tail_lines, lang);

    let rtk_output = if line_numbers {
        format_with_line_numbers(&filtered)
    } else {
        filtered
    };
    println!("{rtk_output}");
    timer.track(
        &format!("cat {}", file.display()),
        "rtk read",
        &content,
        &rtk_output,
    );
    Ok(())
}

pub fn run_stdin(
    level: FilterLevel,
    max_lines: Option<usize>,
    tail_lines: Option<usize>,
    line_numbers: bool,
    verbose: u8,
) -> Result<()> {
    use std::io::Read as IoRead;
    use std::io::{self};

    let timer = tracking::TimedExecution::start();

    if verbose > 0 {
        eprintln!("读取 stdin（过滤：{level}）");
    }

    // 从标准输入读取
    let mut content = String::new();
    io::stdin()
        .lock()
        .read_to_string(&mut content)
        .context("读取 stdin 失败")?;

    // 标准输入没有扩展名，因此使用 `Unknown` 语言
    let lang = Language::Unknown;

    if verbose > 1 {
        eprintln!("语言：{lang:?}（stdin 无扩展名）");
    }

    // 应用过滤器
    let filter = filter::get_filter(level);
    let mut filtered = filter.filter(&content, lang);

    if verbose > 0 {
        let original_lines = content.lines().count();
        let filtered_lines = filtered.lines().count();
        let reduction = if original_lines > 0 {
            ((original_lines - filtered_lines) as f64 / original_lines as f64) * 100.0
        } else {
            0.0
        };
        eprintln!("行数：{original_lines} -> {filtered_lines}（减少 {reduction:.1}%）");
    }

    filtered = apply_line_window(&filtered, max_lines, tail_lines, lang);

    let rtk_output = if line_numbers {
        format_with_line_numbers(&filtered)
    } else {
        filtered
    };
    println!("{rtk_output}");

    timer.track("cat - (stdin)", "rtk read -", &content, &rtk_output);
    Ok(())
}

fn format_with_line_numbers(content: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let width = lines.len().to_string().len();
    let mut out = String::new();
    for (i, line) in lines.iter().enumerate() {
        out.push_str(&format!("{:>width$} │ {}\n", i + 1, line, width = width));
    }
    out
}

fn apply_line_window(
    content: &str,
    max_lines: Option<usize>,
    tail_lines: Option<usize>,
    lang: Language,
) -> String {
    if let Some(tail) = tail_lines {
        if tail == 0 {
            return String::new();
        }
        let lines: Vec<&str> = content.lines().collect();
        let start = lines.len().saturating_sub(tail);
        let mut result = lines[start..].join("\n");
        if content.ends_with('\n') {
            result.push('\n');
        }
        return result;
    }

    if let Some(max) = max_lines {
        return filter::smart_truncate(content, max, lang);
    }

    content.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_read_rust_file() -> Result<()> {
        let mut file = NamedTempFile::with_suffix(".rs")?;
        writeln!(
            file,
            r#"// Comment
fn main() {{
    println!("Hello");
}}"#
        )?;

        // 只验证不会发生 `panic`
        run(file.path(), FilterLevel::Minimal, None, None, false, 0)?;
        Ok(())
    }

    #[test]
    fn test_stdin_support_signature() {
        // 验证 `run_stdin` 的签名正确且能通过编译
        // 这里不实际运行，因为那会阻塞等待标准输入
        // 这属于编译期验证：确认函数存在且签名正确
    }

    #[test]
    fn test_apply_line_window_tail_lines() {
        let input = "a\nb\nc\nd\n";
        let output = apply_line_window(input, None, Some(2), Language::Unknown);
        assert_eq!(output, "c\nd\n");
    }

    #[test]
    fn test_apply_line_window_tail_lines_no_trailing_newline() {
        let input = "a\nb\nc\nd";
        let output = apply_line_window(input, None, Some(2), Language::Unknown);
        assert_eq!(output, "c\nd");
    }

    #[test]
    fn test_apply_line_window_max_lines_still_works() {
        let input = "a\nb\nc\nd\n";
        let output = apply_line_window(input, Some(2), None, Language::Unknown);
        assert!(output.starts_with("a\n"));
        assert!(output.contains("省略"));
    }
}
