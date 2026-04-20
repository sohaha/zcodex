use crate::compression::CompressionResult;
use crate::compression::ContentKind;
use crate::compression::ExplicitFallbackReason;
use crate::compression::LogRenderMode;
use lazy_static::lazy_static;
use regex::Regex;
use std::collections::HashMap;

lazy_static! {
    static ref TIMESTAMP_RE: Regex =
        crate::utils::compile_regex(r"^\d{4}[-/]\d{2}[-/]\d{2}[T ]\d{2}:\d{2}:\d{2}[.,]?\d*\s*");
    static ref UUID_RE: Regex = crate::utils::compile_regex(
        r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}"
    );
    static ref HEX_RE: Regex = crate::utils::compile_regex(r"0x[0-9a-fA-F]+");
    static ref NUM_RE: Regex = crate::utils::compile_regex(r"\b\d{4,}\b");
    static ref PATH_RE: Regex = crate::utils::compile_regex(r"/[\w./\-]+");
}

pub(crate) fn looks_like_log(content: &str) -> bool {
    let sample: Vec<&str> = content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .take(6)
        .collect();

    if sample.is_empty() {
        return false;
    }

    let signal_count = sample
        .iter()
        .filter(|line| {
            let lower = line.to_ascii_lowercase();
            TIMESTAMP_RE.is_match(line)
                || lower.contains("error")
                || lower.contains("warn")
                || lower.contains("info")
                || lower.contains("fatal")
                || lower.contains("panic")
        })
        .count();

    signal_count * 2 >= sample.len()
}

pub(crate) fn compress_log(
    content: &str,
    content_kind: ContentKind,
    mode: LogRenderMode,
) -> CompressionResult {
    if content_kind != ContentKind::Log {
        return CompressionResult::fallback_full(
            content_kind,
            content.to_string(),
            ExplicitFallbackReason::StrategyUnavailable,
        );
    }

    let stats = analyze_logs(content);
    let output = match mode {
        LogRenderMode::Detailed => render_detailed_log_summary(&stats),
        LogRenderMode::Summary => render_brief_log_summary(&stats),
    };

    CompressionResult::full(content_kind, output)
}

#[derive(Debug, Default)]
struct LogStats {
    total_errors: usize,
    total_warnings: usize,
    total_info: usize,
    error_counts: HashMap<String, usize>,
    warn_counts: HashMap<String, usize>,
    unique_errors: Vec<String>,
    unique_warnings: Vec<String>,
}

fn analyze_logs(content: &str) -> LogStats {
    let mut stats = LogStats::default();

    for line in content.lines() {
        let line_lower = line.to_ascii_lowercase();
        let normalized = normalize_log_line(line);

        if line_lower.contains("error")
            || line_lower.contains("fatal")
            || line_lower.contains("panic")
        {
            let count = stats.error_counts.entry(normalized.clone()).or_insert(0);
            if *count == 0 {
                stats.unique_errors.push(line.to_string());
            }
            *count += 1;
            stats.total_errors += 1;
        } else if line_lower.contains("warn") {
            let count = stats.warn_counts.entry(normalized.clone()).or_insert(0);
            if *count == 0 {
                stats.unique_warnings.push(line.to_string());
            }
            *count += 1;
            stats.total_warnings += 1;
        } else if line_lower.contains("info") {
            stats.total_info += 1;
        }
    }

    stats
}

fn normalize_log_line(line: &str) -> String {
    let mut normalized = TIMESTAMP_RE.replace_all(line, "").to_string();
    normalized = UUID_RE.replace_all(&normalized, "<UUID>").to_string();
    normalized = HEX_RE.replace_all(&normalized, "<HEX>").to_string();
    normalized = NUM_RE.replace_all(&normalized, "<NUM>").to_string();
    normalized = PATH_RE.replace_all(&normalized, "<PATH>").to_string();
    normalized.trim().to_string()
}

fn render_detailed_log_summary(stats: &LogStats) -> String {
    let mut result = Vec::new();
    result.push("📊 日志摘要".to_string());
    result.push(format!(
        "   ❌ {} 个错误（{} 个唯一）",
        stats.total_errors,
        stats.error_counts.len()
    ));
    result.push(format!(
        "   ⚠️  {} 个警告（{} 个唯一）",
        stats.total_warnings,
        stats.warn_counts.len()
    ));
    result.push(format!("   ℹ️  {} 条信息", stats.total_info));
    result.push(String::new());

    if !stats.unique_errors.is_empty() {
        result.push("❌ 错误：".to_string());
        let mut error_list: Vec<_> = stats.error_counts.iter().collect();
        error_list.sort_by(|left, right| right.1.cmp(left.1));

        for (normalized, count) in error_list.iter().take(10) {
            let original = stats
                .unique_errors
                .iter()
                .find(|line| normalize_log_line(line) == **normalized)
                .map(String::as_str)
                .unwrap_or(normalized);
            let truncated = if original.len() > 100 {
                let text = original.chars().take(97).collect::<String>();
                format!("{text}...")
            } else {
                original.to_string()
            };

            if **count > 1 {
                result.push(format!("   [×{count}] {truncated}"));
            } else {
                result.push(format!("   {truncated}"));
            }
        }

        if error_list.len() > 10 {
            result.push(format!("   ... +{} 条唯一错误", error_list.len() - 10));
        }
        result.push(String::new());
    }

    if !stats.unique_warnings.is_empty() {
        result.push("⚠️  警告：".to_string());
        let mut warn_list: Vec<_> = stats.warn_counts.iter().collect();
        warn_list.sort_by(|left, right| right.1.cmp(left.1));

        for (normalized, count) in warn_list.iter().take(5) {
            let original = stats
                .unique_warnings
                .iter()
                .find(|line| normalize_log_line(line) == **normalized)
                .map(String::as_str)
                .unwrap_or(normalized);
            let truncated = if original.len() > 100 {
                let text = original.chars().take(97).collect::<String>();
                format!("{text}...")
            } else {
                original.to_string()
            };

            if **count > 1 {
                result.push(format!("   [×{count}] {truncated}"));
            } else {
                result.push(format!("   {truncated}"));
            }
        }

        if warn_list.len() > 5 {
            result.push(format!("   ... +{} 条唯一警告", warn_list.len() - 5));
        }
    }

    result.join("\n")
}

fn render_brief_log_summary(stats: &LogStats) -> String {
    let mut result = vec!["日志摘要：".to_string()];
    result.push(format!("   {} 个错误", stats.total_errors));
    result.push(format!("   {} 个警告", stats.total_warnings));
    result.push(format!("   {} 条信息", stats.total_info));
    result.join("\n")
}

#[cfg(test)]
mod tests {
    use crate::compression::CompressionHint;
    use crate::compression::CompressionIntent;
    use crate::compression::CompressionRequest;

    #[test]
    fn log_summary_deduplicates_repeated_errors() {
        let result = crate::compression::compress(CompressionRequest {
            source_name: "server.log",
            content: "2024-01-01 10:00:00 ERROR: boom\n2024-01-01 10:00:01 ERROR: boom\n",
            hint: CompressionHint::Log,
            intent: CompressionIntent::Log {
                mode: crate::compression::LogRenderMode::Detailed,
            },
        })
        .expect("log compression should succeed");

        assert!(result.output.contains("[×2]"));
        assert!(result.output.contains("错误"));
    }
}
