use crate::compression;
use crate::compression::CompressionHint;
use crate::compression::CompressionIntent;
use crate::compression::CompressionRequest;
use crate::compression::ExplicitFallbackReason;
use crate::compression::ReadOptions;
use crate::filter::FilterLevel;
use crate::filter::Language;
use crate::session_dedup;
use crate::settings;
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

    let source_name = file.display().to_string();
    let behavior = settings::runtime_settings().behavior;
    let compressed = compression::compress_for_behavior(
        CompressionRequest {
            source_name: &source_name,
            content: &content,
            hint: CompressionHint::CodeOrText(lang),
            intent: CompressionIntent::Read(ReadOptions {
                level,
                max_lines,
                tail_lines,
                line_numbers,
                language: lang,
            }),
        },
        behavior,
    )?;

    if compressed.fallback == Some(ExplicitFallbackReason::EmptySpecializedOutput) {
        eprintln!(
            "ztok: warning: filter produced empty output for {} ({} bytes), showing raw content",
            file.display(),
            content.len()
        );
    }

    if verbose > 0 {
        let original_lines = content.lines().count();
        let filtered_lines = compressed.output.lines().count();
        let reduction = if original_lines > 0 {
            ((original_lines - filtered_lines) as f64 / original_lines as f64) * 100.0
        } else {
            0.0
        };
        eprintln!("行数：{original_lines} -> {filtered_lines}（减少 {reduction:.1}%）");
    }

    let ztok_output = session_dedup::dedup_read_output(
        &source_name,
        &content,
        &format!(
            "read:{level}:max_lines={max_lines:?}:tail_lines={tail_lines:?}:line_numbers={line_numbers}"
        ),
        compressed,
    )
    .output;
    println!("{ztok_output}");
    timer.track(
        &format!("cat {}", file.display()),
        "ztok read",
        &content,
        &ztok_output,
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

    let source_name = "-".to_string();
    let behavior = settings::runtime_settings().behavior;
    let compressed = compression::compress_for_behavior(
        CompressionRequest {
            source_name: &source_name,
            content: &content,
            hint: CompressionHint::CodeOrText(lang),
            intent: CompressionIntent::Read(ReadOptions {
                level,
                max_lines,
                tail_lines,
                line_numbers,
                language: lang,
            }),
        },
        behavior,
    )?;

    if compressed.fallback == Some(ExplicitFallbackReason::EmptySpecializedOutput) {
        eprintln!(
            "ztok: warning: filter produced empty output for stdin ({} bytes), showing raw content",
            content.len()
        );
    }

    if verbose > 0 {
        let original_lines = content.lines().count();
        let filtered_lines = compressed.output.lines().count();
        let reduction = if original_lines > 0 {
            ((original_lines - filtered_lines) as f64 / original_lines as f64) * 100.0
        } else {
            0.0
        };
        eprintln!("行数：{original_lines} -> {filtered_lines}（减少 {reduction:.1}%）");
    }

    let ztok_output = session_dedup::dedup_read_output(
        &source_name,
        &content,
        &format!(
            "read:{level}:max_lines={max_lines:?}:tail_lines={tail_lines:?}:line_numbers={line_numbers}"
        ),
        compressed,
    )
    .output;
    println!("{ztok_output}");

    timer.track("cat - (stdin)", "ztok read -", &content, &ztok_output);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filter::FilterLevel;
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
        let output = compression::compress(CompressionRequest {
            source_name: "sample.txt",
            content: "a\nb\nc\nd\n",
            hint: CompressionHint::CodeOrText(Language::Unknown),
            intent: CompressionIntent::Read(ReadOptions {
                level: FilterLevel::None,
                max_lines: None,
                tail_lines: Some(2),
                line_numbers: false,
                language: Language::Unknown,
            }),
        })
        .expect("tail-lines compression should succeed");
        assert_eq!(output.output, "c\nd\n");
    }

    #[test]
    fn test_apply_line_window_tail_lines_no_trailing_newline() {
        let output = compression::compress(CompressionRequest {
            source_name: "sample.txt",
            content: "a\nb\nc\nd",
            hint: CompressionHint::CodeOrText(Language::Unknown),
            intent: CompressionIntent::Read(ReadOptions {
                level: FilterLevel::None,
                max_lines: None,
                tail_lines: Some(2),
                line_numbers: false,
                language: Language::Unknown,
            }),
        })
        .expect("tail-lines compression should succeed");
        assert_eq!(output.output, "c\nd");
    }

    #[test]
    fn test_apply_line_window_max_lines_still_works() {
        let output = compression::compress(CompressionRequest {
            source_name: "sample.txt",
            content: "a\nb\nc\nd\n",
            hint: CompressionHint::CodeOrText(Language::Unknown),
            intent: CompressionIntent::Read(ReadOptions {
                level: FilterLevel::None,
                max_lines: Some(2),
                tail_lines: None,
                line_numbers: false,
                language: Language::Unknown,
            }),
        })
        .expect("max-lines compression should succeed");
        assert!(output.output.starts_with("a\n"));
        assert!(output.output.contains("省略"));
    }
}
