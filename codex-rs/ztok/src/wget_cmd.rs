use crate::fetcher_output;
use crate::settings;
use crate::tracking;
use crate::utils::resolved_command;
use anyhow::Context;
use anyhow::Result;

/// 紧凑版 `wget`：去掉进度条，只保留结果摘要
pub fn run(url: &str, args: &[String], verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    if verbose > 0 {
        eprintln!("wget: {url}");
    }

    // 正常运行 `wget`，但先捕获输出后再解析
    let mut cmd_args: Vec<&str> = vec![];

    // 追加用户参数
    for arg in args {
        cmd_args.push(arg);
    }
    cmd_args.push(url);

    let output = resolved_command("wget")
        .args(&cmd_args)
        .output()
        .context("运行 wget 失败")?;

    let stderr = crate::utils::decode_output(&output.stderr);
    let stdout = crate::utils::decode_output(&output.stdout);

    let raw_output = format!("{stderr}\n{stdout}");

    if output.status.success() {
        let filename = extract_filename_from_output(&stderr, url, args);
        let size = get_file_size(&filename);
        let msg = format!(
            "⬇️ {} 成功 | {} | {}",
            compact_url(url),
            filename,
            format_size(size)
        );
        println!("{msg}");
        timer.track(&format!("wget {url}"), "ztok wget", &raw_output, &msg);
    } else {
        let error = parse_error(&stderr, &stdout);
        let msg = format!("⬇️ {} 失败：{}", compact_url(url), error);
        println!("{msg}");
        timer.track(&format!("wget {url}"), "ztok wget", &raw_output, &msg);
    }

    Ok(())
}

/// 运行 `wget` 并输出到标准输出（便于管道传递）
pub fn run_stdout(url: &str, args: &[String], verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();
    let behavior = settings::runtime_settings().behavior;

    if verbose > 0 {
        eprintln!("wget: {url} -> stdout");
    }

    let mut cmd_args = vec!["-q", "-O", "-"];
    for arg in args {
        cmd_args.push(arg);
    }
    cmd_args.push(url);

    let output = resolved_command("wget")
        .args(&cmd_args)
        .output()
        .context("运行 wget 失败")?;

    if output.status.success() {
        let content = crate::utils::decode_output(&output.stdout);
        let raw_output = content.to_string();
        let source_name = fetcher_output::url_source_label(url);
        let preserve_json_output = is_internal_url(url);
        let compressed = fetcher_output::compress_fetcher_output(
            &source_name,
            &raw_output,
            behavior,
            Some(20),
            preserve_json_output,
        )?;
        fetcher_output::print_fetcher_output(
            &timer,
            "ztok wget",
            &source_name,
            &raw_output,
            "wget:stdout",
            behavior,
            compressed,
        );
    } else {
        let stderr = crate::utils::decode_output(&output.stderr);
        let error = parse_error(&stderr, "");
        let msg = format!("⬇️ {} 失败：{}", compact_url(url), error);
        println!("{msg}");
        timer.track(&format!("wget -O - {url}"), "ztok wget -o", &stderr, &msg);
    }

    Ok(())
}

fn is_internal_url(url: &str) -> bool {
    let lower = url.to_lowercase();
    lower.starts_with("http://localhost")
        || lower.starts_with("http://127.0.0.1")
        || lower.starts_with("http://[::1]")
        || lower.starts_with("https://localhost")
        || lower.starts_with("https://127.0.0.1")
}

fn extract_filename_from_output(stderr: &str, url: &str, args: &[String]) -> String {
    // 优先检查 `-O` 参数
    for (i, arg) in args.iter().enumerate() {
        if (arg == "-O" || arg == "--output-document")
            && let Some(name) = args.get(i + 1)
        {
            return name.clone();
        }
        if let Some(name) = arg.strip_prefix("-O") {
            return name.to_string();
        }
    }

    // 解析 `wget` 输出中的 `"Sauvegarde en"` / `"Saving to"` 提示
    for line in stderr.lines() {
        // 法语示例：`Sauvegarde en : « filename »`
        if line.contains("Sauvegarde en") || line.contains("Saving to") {
            // 使用基于字符的解析，避免 Unicode 被错误切分
            let chars: Vec<char> = line.chars().collect();
            let mut start_idx = None;
            let mut end_idx = None;

            for (i, c) in chars.iter().enumerate() {
                if *c == '«' || (*c == '\'' && start_idx.is_none()) {
                    start_idx = Some(i);
                }
                if *c == '»' || (*c == '\'' && start_idx.is_some()) {
                    end_idx = Some(i);
                }
            }

            if let (Some(s), Some(e)) = (start_idx, end_idx)
                && e > s + 1
            {
                let filename: String = chars[s + 1..e].iter().collect();
                return filename.trim().to_string();
            }
        }
    }

    // 回退方案：直接从 URL 中提取文件名
    let path = url.rsplit("://").next().unwrap_or(url);
    let filename = path
        .rsplit('/')
        .next()
        .unwrap_or("index.html")
        .split('?')
        .next()
        .unwrap_or("index.html");

    if filename.is_empty() || !filename.contains('.') {
        "index.html".to_string()
    } else {
        filename.to_string()
    }
}

fn get_file_size(filename: &str) -> u64 {
    std::fs::metadata(filename).map(|m| m.len()).unwrap_or(0)
}

fn format_size(bytes: u64) -> String {
    if bytes == 0 {
        return "?".to_string();
    }
    if bytes < 1024 {
        format!("{bytes}B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1}KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1}MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1}GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

fn compact_url(url: &str) -> String {
    // 去掉协议头
    let without_proto = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);

    // 过长时进行截断
    let chars: Vec<char> = without_proto.chars().collect();
    if chars.len() <= 50 {
        without_proto.to_string()
    } else {
        let prefix: String = chars[..25].iter().collect();
        let suffix: String = chars[chars.len() - 20..].iter().collect();
        format!("{prefix}...{suffix}")
    }
}

fn parse_error(stderr: &str, stdout: &str) -> String {
    // 常见 `wget` 错误模式
    let combined = format!("{stderr}\n{stdout}");

    if combined.contains("404") {
        return "404 未找到".to_string();
    }
    if combined.contains("403") {
        return "403 禁止访问".to_string();
    }
    if combined.contains("401") {
        return "401 未授权".to_string();
    }
    if combined.contains("500") {
        return "500 服务器错误".to_string();
    }
    if combined.contains("Connection refused") {
        return "连接被拒绝".to_string();
    }
    if combined.contains("unable to resolve") || combined.contains("Name or service not known") {
        return "DNS 解析失败".to_string();
    }
    if combined.contains("timed out") {
        return "连接超时".to_string();
    }
    if combined.contains("SSL") || combined.contains("certificate") {
        return "SSL/TLS 错误".to_string();
    }

    // 返回第一条有意义的报错行
    for line in stderr.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() && !trimmed.starts_with("--") {
            if trimmed.len() > 60 {
                let t: String = trimmed.chars().take(60).collect();
                return format!("{t}...");
            }
            return trimmed.to_string();
        }
    }

    "未知错误".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behavior::ZtokBehavior;

    #[test]
    fn extract_filename_from_output_prefers_output_flag() {
        let stderr = "";
        let args = vec!["-O".to_string(), "custom.txt".to_string()];
        assert_eq!(
            extract_filename_from_output(stderr, "https://example.com/file.txt", &args),
            "custom.txt"
        );
    }

    #[test]
    fn compact_url_truncates_long_urls() {
        let url = "https://example.com/very/long/path/that/keeps/going/and/going/file.txt";
        let compact = compact_url(url);
        assert_eq!(compact, "example.com/very/long/pat...g/and/going/file.txt");
    }

    #[test]
    fn run_stdout_uses_shared_text_compression() {
        let output = (0..25)
            .map(|index| format!("line-{index}"))
            .collect::<Vec<_>>()
            .join("\n");
        let result = fetcher_output::compress_fetcher_output(
            "example.com/archive.txt",
            &output,
            ZtokBehavior::Enhanced,
            Some(20),
            /*preserve_json_output*/ false,
        )
        .expect("compress fetcher output");
        assert!(result.output.contains("line-0"));
        assert!(result.output.contains("省略"));
    }

    #[test]
    fn run_stdout_small_json_returns_raw_in_basic_mode() {
        let output = r#"{"status":"ok","ready":true}"#;
        let result = fetcher_output::compress_fetcher_output(
            "example.com/status",
            output,
            ZtokBehavior::Basic,
            Some(20),
            /*preserve_json_output*/ false,
        )
        .expect("compress fetcher output");
        assert_eq!(result.output, output);
    }

    #[test]
    fn internal_url_detection_matches_local_hosts() {
        assert!(is_internal_url("http://localhost:3000/api/status"));
        assert!(is_internal_url("http://127.0.0.1:8080/api"));
        assert!(!is_internal_url("https://example.com/archive.json"));
    }
}
