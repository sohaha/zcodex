use crate::behavior::ZtokBehavior;
use crate::compression;
use crate::compression::CompressionHint;
use crate::compression::CompressionIntent;
use crate::compression::CompressionRequest;
use crate::compression::CompressionResult;
use crate::compression::ContentKind;
use crate::compression::JsonRenderMode;
use crate::compression::ReadOptions;
use crate::filter::FilterLevel;
use crate::filter::Language;
use crate::session_dedup;
use crate::tracking;
use anyhow::Result;

const JSON_MAX_DEPTH: usize = 5;

pub(crate) fn compress_fetcher_output(
    source_name: &str,
    raw_output: &str,
    behavior: ZtokBehavior,
    max_lines: Option<usize>,
    preserve_json_output: bool,
) -> Result<CompressionResult> {
    let content = raw_output.trim();

    if !preserve_json_output && looks_like_json(content) {
        if behavior.is_basic() {
            return Ok(CompressionResult::full(
                ContentKind::Json,
                content.to_string(),
            ));
        }

        let schema = compression::compress_for_behavior(
            CompressionRequest {
                source_name,
                content,
                hint: CompressionHint::Json,
                intent: CompressionIntent::Json {
                    max_depth: JSON_MAX_DEPTH,
                    mode: JsonRenderMode::Schema,
                },
            },
            behavior,
        )?;

        if schema.output.len() <= content.len() {
            return Ok(schema);
        }

        return Ok(CompressionResult::full(
            ContentKind::Json,
            content.to_string(),
        ));
    }

    compression::compress_for_behavior(
        CompressionRequest {
            source_name,
            content,
            hint: CompressionHint::CodeOrText(Language::Unknown),
            intent: CompressionIntent::Read(ReadOptions {
                level: FilterLevel::None,
                max_lines,
                tail_lines: None,
                line_numbers: false,
                language: Language::Unknown,
            }),
        },
        behavior,
    )
}

pub(crate) fn print_fetcher_output(
    timer: &tracking::TimedExecution,
    command_label: &str,
    source_name: &str,
    raw_output: &str,
    output_signature: &str,
    behavior: ZtokBehavior,
    compressed: CompressionResult,
) {
    let result = session_dedup::dedup_output(source_name, raw_output, output_signature, compressed);
    timer.track_compression_decision(
        command_label,
        source_name,
        behavior,
        raw_output.len(),
        &result,
    );
    println!("{}", result.output);
}

pub(crate) fn url_source_label(url: &str) -> String {
    let without_fragment = url.split('#').next().unwrap_or(url);
    let without_query = without_fragment
        .split('?')
        .next()
        .unwrap_or(without_fragment);
    let without_proto = without_query
        .strip_prefix("https://")
        .or_else(|| without_query.strip_prefix("http://"))
        .unwrap_or(without_query);
    crate::utils::truncate(strip_url_userinfo(without_proto), 60)
}

fn strip_url_userinfo(url: &str) -> &str {
    match url.split_once('@') {
        Some((userinfo, remainder)) if !userinfo.contains('/') => remainder,
        _ => url,
    }
}

fn looks_like_json(content: &str) -> bool {
    let trimmed = content.trim_start();
    (trimmed.starts_with('{') || trimmed.starts_with('['))
        && serde_json::from_str::<serde_json::Value>(trimmed).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_source_label_redacts_query_and_fragment() {
        assert_eq!(
            url_source_label("https://example.com/api/items?token=secret#debug"),
            "example.com/api/items"
        );
    }

    #[test]
    fn url_source_label_redacts_userinfo() {
        assert_eq!(
            url_source_label("https://user:pass@example.com/api/items?token=secret"),
            "example.com/api/items"
        );
    }

    #[test]
    fn basic_mode_keeps_json_raw() {
        let result = compress_fetcher_output(
            "example.com/data",
            "{\"ok\":true}",
            ZtokBehavior::Basic,
            Some(30),
            /*preserve_json_output*/ false,
        )
        .expect("compress fetcher output");

        assert_eq!(result.output, "{\"ok\":true}");
    }
}
