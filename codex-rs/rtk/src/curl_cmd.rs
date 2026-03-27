use crate::json_cmd;
use crate::tracking;
use crate::utils::resolved_command;
use crate::utils::truncate;
use anyhow::Context;
use anyhow::Result;

pub fn run(args: &[String], verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();
    let mut cmd = resolved_command("curl");
    cmd.arg("-s"); // Silent mode (no progress bar)

    for arg in args {
        cmd.arg(arg);
    }

    if verbose > 0 {
        eprintln!("运行：curl -s {}", args.join(" "));
    }

    let output = cmd.output().context("运行 curl 失败")?;
    let stdout = crate::utils::decode_output(&output.stdout);
    let stderr = crate::utils::decode_output(&output.stderr);

    if !output.status.success() {
        let msg = if stderr.trim().is_empty() {
            stdout.trim().to_string()
        } else {
            stderr.trim().to_string()
        };
        eprintln!("失败：curl {msg}");
        std::process::exit(output.status.code().unwrap_or(1));
    }

    let raw = stdout.to_string();

    // 自动识别 JSON 并交给过滤器处理
    let filtered = filter_curl_output(&stdout);
    println!("{filtered}");

    timer.track(
        &format!("curl {}", args.join(" ")),
        &format!("rtk curl {}", args.join(" ")),
        &raw,
        &filtered,
    );

    Ok(())
}

fn filter_curl_output(output: &str) -> String {
    let trimmed = output.trim();

    // 尝试识别 JSON：以 { 或 [ 开头
    if (trimmed.starts_with('{') || trimmed.starts_with('['))
        && (trimmed.ends_with('}') || trimmed.ends_with(']'))
        && let Ok(schema) = json_cmd::filter_json_string(trimmed, /*max_depth*/ 5)
    {
        // 仅当 schema 确实比原文更短时才使用（#297）
        if schema.len() <= trimmed.len() {
            return schema;
        }
    }

    // 非 JSON：截断过长输出
    let lines: Vec<&str> = trimmed.lines().collect();
    if lines.len() > 30 {
        let mut result: Vec<&str> = lines[..30].to_vec();
        result.push("");
        let msg = format!(
            "...（剩余 {} 行，共 {} 字节）",
            lines.len() - 30,
            trimmed.len()
        );
        return format!("{}\n{}", result.join("\n"), msg);
    }

    // 输出较短：保留原样，但截断过长行
    lines
        .iter()
        .map(|l| truncate(l, /*max_len*/ 200))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_curl_json() {
        // 大型 JSON：若 schema 比原文更短，应返回 schema
        let output = r#"{"name": "a very long user name here", "count": 42, "items": [1, 2, 3], "description": "a very long description that takes up many characters in the original JSON payload", "status": "active", "url": "https://example.com/api/v1/users/123"}"#;
        let result = filter_curl_output(output);
        assert!(result.contains("name"));
        assert!(result.contains("string"));
        assert!(result.contains("int"));
    }

    #[test]
    fn test_filter_curl_json_array() {
        let output = r#"[{"id": 1}, {"id": 2}]"#;
        let result = filter_curl_output(output);
        assert!(result.contains("id"));
    }

    #[test]
    fn test_filter_curl_non_json() {
        let output = "Hello, World!\nThis is plain text.";
        let result = filter_curl_output(output);
        assert!(result.contains("Hello, World!"));
        assert!(result.contains("plain text"));
    }

    #[test]
    fn test_filter_curl_json_small_returns_original() {
        // 小型 JSON：若结构摘要反而更长（issue #297）
        let output = r#"{"r2Ready":true,"status":"ok"}"#;
        let result = filter_curl_output(output);
        // 结构摘要会是 "{\n  r2Ready: bool,\n  status: string\n}"，长度更长
        // 应保持返回原始 JSON
        assert_eq!(result.trim(), output.trim());
    }

    #[test]
    fn test_filter_curl_long_output() {
        let lines: Vec<String> = (0..50).map(|i| format!("Line {i}")).collect();
        let output = lines.join("\n");
        let result = filter_curl_output(&output);
        assert!(result.contains("Line 0"));
        assert!(result.contains("Line 29"));
        assert!(result.contains("剩余"));
    }
}
