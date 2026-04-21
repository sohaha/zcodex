use crate::behavior::ZtokBehavior;
use crate::compression;
use crate::compression::CompressionHint;
use crate::compression::CompressionIntent;
use crate::compression::CompressionRequest;
use crate::compression::LogRenderMode;
use crate::session_dedup;
use crate::tracking;
use anyhow::Result;
use std::fs;
use std::io::BufRead;
use std::io::{self};
use std::path::Path;

/// 过滤并去重日志输出
pub fn run_file(file: &Path, verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    if verbose > 0 {
        eprintln!("分析日志：{}", file.display());
    }

    let content = fs::read_to_string(file)?;
    let source_name = file.display().to_string();
    let behavior = ZtokBehavior::from_env();
    let result = session_dedup::dedup_output(
        &source_name,
        &content,
        "log:mode=detailed",
        compression::compress_for_behavior(
            CompressionRequest {
                source_name: &source_name,
                content: &content,
                hint: CompressionHint::Log,
                intent: CompressionIntent::Log {
                    mode: LogRenderMode::Detailed,
                },
            },
            behavior,
        )?,
    )
    .output;
    println!("{result}");
    timer.track(
        &format!("cat {}", file.display()),
        "ztok log",
        &content,
        &result,
    );
    Ok(())
}

/// 过滤来自 stdin 的日志
pub fn run_stdin(_verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();
    let behavior = ZtokBehavior::from_env();

    let mut content = String::new();
    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        content.push_str(&line?);
        content.push('\n');
    }

    let result = session_dedup::dedup_output(
        "-",
        &content,
        "log:mode=detailed",
        compression::compress_for_behavior(
            CompressionRequest {
                source_name: "-",
                content: &content,
                hint: CompressionHint::Log,
                intent: CompressionIntent::Log {
                    mode: LogRenderMode::Detailed,
                },
            },
            behavior,
        )?,
    )
    .output;
    println!("{result}");

    timer.track("log (stdin)", "ztok log (stdin)", &content, &result);

    Ok(())
}

/// 供其他模块调用
pub fn run_stdin_str(content: &str) -> String {
    compression::compress(CompressionRequest {
        source_name: "-",
        content,
        hint: CompressionHint::Log,
        intent: CompressionIntent::Log {
            mode: LogRenderMode::Detailed,
        },
    })
    .map(|result| result.output)
    .unwrap_or_else(|_| content.to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_analyze_logs() {
        let logs = r#"
2024-01-01 10:00:00 ERROR: Connection failed to /api/server
2024-01-01 10:00:01 ERROR: Connection failed to /api/server
2024-01-01 10:00:02 ERROR: Connection failed to /api/server
2024-01-01 10:00:03 WARN: Retrying connection
2024-01-01 10:00:04 INFO: Connected
"#;
        let result = super::run_stdin_str(logs);
        assert!(result.contains("×3"));
        assert!(result.contains("错误"));
    }

    #[test]
    fn test_analyze_logs_multibyte() {
        let logs = format!(
            "2024-01-01 10:00:00 ERROR: {} connection failed\n\
             2024-01-01 10:00:01 WARN: {} retry attempt\n",
            "ข้อผิดพลาด".repeat(15),
            "คำเตือน".repeat(15)
        );
        let result = super::run_stdin_str(&logs);
        // 即使遇到超长多字节消息也不应 panic
        assert!(result.contains("错误"));
    }
}
