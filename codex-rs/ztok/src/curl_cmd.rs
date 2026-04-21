use crate::fetcher_output;
use crate::settings;
use crate::tracking;
use crate::utils::resolved_command;
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
    let source_name = args
        .iter()
        .find(|arg| arg.starts_with("http://") || arg.starts_with("https://"))
        .map(|arg| fetcher_output::url_source_label(arg))
        .unwrap_or_else(|| "curl".to_string());
    let behavior = settings::runtime_settings().behavior;
    let preserve_json_output = is_internal_url(args);
    let compressed = fetcher_output::compress_fetcher_output(
        &source_name,
        &raw,
        behavior,
        Some(30),
        preserve_json_output,
    )?;
    fetcher_output::print_fetcher_output(
        &timer,
        "ztok curl",
        &source_name,
        &raw,
        &format!("curl:internal={preserve_json_output}"),
        behavior,
        compressed,
    );

    Ok(())
}

fn is_internal_url(args: &[String]) -> bool {
    args.iter().any(|arg| {
        let lower = arg.to_lowercase();
        lower.starts_with("http://localhost")
            || lower.starts_with("http://127.0.0.1")
            || lower.starts_with("http://[::1]")
            || lower.starts_with("https://localhost")
            || lower.starts_with("https://127.0.0.1")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behavior::ZtokBehavior;

    #[test]
    fn filter_curl_json_uses_shared_schema_compression() {
        let output = r#"{"name": "a very long user name here", "count": 42, "items": [1, 2, 3], "description": "a very long description that takes up many characters in the original JSON payload", "status": "active", "url": "https://example.com/api/v1/users/123"}"#;
        let result = fetcher_output::compress_fetcher_output(
            "api.example.com/data",
            output,
            ZtokBehavior::Enhanced,
            Some(30),
            /*preserve_json_output*/ false,
        )
        .expect("compress fetcher output");
        assert!(result.output.contains("name"));
        assert!(result.output.contains("string"));
        assert!(result.output.contains("int"));
    }

    #[test]
    fn filter_curl_json_array_uses_shared_schema_compression() {
        let output = r#"[{"id": 1}, {"id": 2}]"#;
        let result = fetcher_output::compress_fetcher_output(
            "api.example.com/items",
            output,
            ZtokBehavior::Enhanced,
            Some(30),
            /*preserve_json_output*/ false,
        )
        .expect("compress fetcher output");
        assert!(result.output.contains("id"));
    }

    #[test]
    fn filter_curl_non_json_uses_shared_text_compression() {
        let output = "Hello, World!\nThis is plain text.";
        let result = fetcher_output::compress_fetcher_output(
            "example.com/plain",
            output,
            ZtokBehavior::Enhanced,
            Some(30),
            /*preserve_json_output*/ false,
        )
        .expect("compress fetcher output");
        assert_eq!(result.output, output);
    }

    #[test]
    fn filter_curl_json_small_returns_original() {
        let output = r#"{"r2Ready":true,"status":"ok"}"#;
        let result = fetcher_output::compress_fetcher_output(
            "api.example.com/health",
            output,
            ZtokBehavior::Enhanced,
            Some(30),
            /*preserve_json_output*/ false,
        )
        .expect("compress fetcher output");
        assert_eq!(result.output, output);
    }

    #[test]
    fn internal_url_keeps_raw_json_output() {
        let output = r#"{"r2Ready":true,"status":"ok"}"#;
        let result = fetcher_output::compress_fetcher_output(
            "localhost:3000/api",
            output,
            ZtokBehavior::Enhanced,
            Some(30),
            /*preserve_json_output*/ true,
        )
        .expect("compress fetcher output");
        assert_eq!(result.output, output);
    }

    #[test]
    fn is_internal_url_localhost() {
        assert!(is_internal_url(&[
            "http://localhost:9222/json/version".to_string()
        ]));
        assert!(is_internal_url(&["http://127.0.0.1:8080/api".to_string()]));
        assert!(is_internal_url(&[
            "-s".to_string(),
            "http://localhost:3000".to_string()
        ]));
        assert!(!is_internal_url(&[
            "https://api.example.com/data".to_string()
        ]));
        assert!(!is_internal_url(&["https://github.com".to_string()]));
    }
}
