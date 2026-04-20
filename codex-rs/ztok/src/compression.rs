use crate::compression_json;
use crate::compression_log;
use crate::filter;
use crate::filter::FilterLevel;
use crate::filter::Language;
use anyhow::Result;
use serde_json::Value;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ContentKind {
    Code,
    Json,
    Log,
    Text,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CompressionOutputKind {
    Full,
    ShortReference,
    #[cfg_attr(not(test), allow(dead_code))]
    Diff,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExplicitFallbackReason {
    EmptySpecializedOutput,
    DedupDisabledNoSessionId,
    DedupCacheUnavailable,
    ShortReferenceUnavailable,
    DiffUnavailable,
    StrategyUnavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CompressionHint {
    #[cfg_attr(not(test), allow(dead_code))]
    Auto,
    CodeOrText(Language),
    Json,
    Log,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JsonRenderMode {
    Compact,
    Schema,
    Summary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LogRenderMode {
    Detailed,
    Summary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReadOptions {
    pub level: FilterLevel,
    pub max_lines: Option<usize>,
    pub tail_lines: Option<usize>,
    pub line_numbers: bool,
    pub language: Language,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CompressionIntent {
    Read(ReadOptions),
    Json {
        max_depth: usize,
        mode: JsonRenderMode,
    },
    Log {
        mode: LogRenderMode,
    },
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct CompressionRequest<'a> {
    pub source_name: &'a str,
    pub content: &'a str,
    pub hint: CompressionHint,
    pub intent: CompressionIntent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompressionResult {
    pub content_kind: ContentKind,
    pub output_kind: CompressionOutputKind,
    pub output: String,
    pub fallback: Option<ExplicitFallbackReason>,
}

impl CompressionResult {
    pub(crate) fn full(content_kind: ContentKind, output: String) -> Self {
        Self {
            content_kind,
            output_kind: CompressionOutputKind::Full,
            output,
            fallback: None,
        }
    }

    pub(crate) fn fallback_full(
        content_kind: ContentKind,
        output: String,
        reason: ExplicitFallbackReason,
    ) -> Self {
        Self {
            content_kind,
            output_kind: CompressionOutputKind::Full,
            output,
            fallback: Some(reason),
        }
    }

    #[cfg(test)]
    fn short_reference(content_kind: ContentKind, output: impl Into<String>) -> Self {
        Self {
            content_kind,
            output_kind: CompressionOutputKind::ShortReference,
            output: output.into(),
            fallback: None,
        }
    }

    #[cfg(test)]
    fn diff(content_kind: ContentKind, output: impl Into<String>) -> Self {
        Self {
            content_kind,
            output_kind: CompressionOutputKind::Diff,
            output: output.into(),
            fallback: None,
        }
    }
}

pub(crate) fn compress(request: CompressionRequest<'_>) -> Result<CompressionResult> {
    let content_kind = detect_content_kind(request.source_name, request.content, request.hint);

    match request.intent {
        CompressionIntent::Read(options) => {
            Ok(compress_read(request.content, content_kind, options))
        }
        CompressionIntent::Json { max_depth, mode } => {
            compression_json::compress_json(request.content, content_kind, max_depth, mode)
        }
        CompressionIntent::Log { mode } => Ok(compression_log::compress_log(
            request.content,
            content_kind,
            mode,
        )),
    }
}

fn detect_content_kind(source_name: &str, content: &str, hint: CompressionHint) -> ContentKind {
    match hint {
        CompressionHint::Json => ContentKind::Json,
        CompressionHint::Log => ContentKind::Log,
        CompressionHint::CodeOrText(language) => {
            if language == Language::Unknown || language == Language::Data {
                ContentKind::Text
            } else {
                ContentKind::Code
            }
        }
        CompressionHint::Auto => detect_content_kind_auto(source_name, content),
    }
}

fn detect_content_kind_auto(source_name: &str, content: &str) -> ContentKind {
    let extension = Path::new(source_name)
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase);

    if extension
        .as_deref()
        .is_some_and(|ext| matches!(ext, "json" | "jsonc" | "json5"))
    {
        return ContentKind::Json;
    }

    if extension
        .as_deref()
        .is_some_and(|ext| matches!(ext, "log" | "out" | "err"))
    {
        return ContentKind::Log;
    }

    if looks_like_json(content) {
        return ContentKind::Json;
    }

    if compression_log::looks_like_log(content) {
        return ContentKind::Log;
    }

    if let Some(ext) = extension.as_deref() {
        let language = Language::from_extension(ext);
        if language != Language::Unknown && language != Language::Data {
            return ContentKind::Code;
        }
    }

    ContentKind::Text
}

fn looks_like_json(content: &str) -> bool {
    let trimmed = content.trim_start();
    (trimmed.starts_with('{') || trimmed.starts_with('['))
        && serde_json::from_str::<Value>(trimmed).is_ok()
}

fn compress_read(
    content: &str,
    content_kind: ContentKind,
    options: ReadOptions,
) -> CompressionResult {
    let candidate = match content_kind {
        ContentKind::Code | ContentKind::Text => {
            filter::get_filter(options.level).filter(content, options.language)
        }
        ContentKind::Json | ContentKind::Log => content.to_string(),
    };

    let (windowed, fallback) = if candidate.trim().is_empty() && !content.trim().is_empty() {
        (
            apply_line_window(
                content,
                options.max_lines,
                options.tail_lines,
                options.language,
            ),
            Some(ExplicitFallbackReason::EmptySpecializedOutput),
        )
    } else {
        (
            apply_line_window(
                &candidate,
                options.max_lines,
                options.tail_lines,
                options.language,
            ),
            None,
        )
    };

    let output = if options.line_numbers {
        format_with_line_numbers(&windowed)
    } else {
        windowed
    };

    match fallback {
        Some(reason) => CompressionResult::fallback_full(content_kind, output, reason),
        None => CompressionResult::full(content_kind, output),
    }
}

fn format_with_line_numbers(content: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let width = lines.len().to_string().len();
    let mut output = String::new();
    for (index, line) in lines.iter().enumerate() {
        output.push_str(&format!(
            "{:>width$} │ {}\n",
            index + 1,
            line,
            width = width
        ));
    }
    output
}

fn apply_line_window(
    content: &str,
    max_lines: Option<usize>,
    tail_lines: Option<usize>,
    language: Language,
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
        return filter::smart_truncate(content, max, language);
    }

    content.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_detects_json_by_extension() {
        let kind = detect_content_kind("payload.json", "{\"ok\":true}", CompressionHint::Auto);
        assert_eq!(kind, ContentKind::Json);
    }

    #[test]
    fn auto_detects_logs_by_content() {
        let content = "2026-04-20 10:00:00 ERROR: boom\n2026-04-20 10:00:01 WARN: retry\n";
        let kind = detect_content_kind("stdin", content, CompressionHint::Auto);
        assert_eq!(kind, ContentKind::Log);
    }

    #[test]
    fn read_fallback_is_explicit_when_filter_empties_content() {
        let result = compress(CompressionRequest {
            source_name: "sample.rs",
            content: "// only comment\n",
            hint: CompressionHint::CodeOrText(Language::Rust),
            intent: CompressionIntent::Read(ReadOptions {
                level: FilterLevel::Minimal,
                max_lines: None,
                tail_lines: None,
                line_numbers: false,
                language: Language::Rust,
            }),
        })
        .expect("read compression should succeed");

        assert_eq!(result.output, "// only comment\n");
        assert_eq!(
            result.fallback,
            Some(ExplicitFallbackReason::EmptySpecializedOutput)
        );
    }

    #[test]
    fn short_reference_and_diff_contracts_exist_for_future_issues() {
        let reference = CompressionResult::short_reference(ContentKind::Text, "ref");
        let diff = CompressionResult::diff(ContentKind::Text, "delta");

        assert_eq!(reference.output_kind, CompressionOutputKind::ShortReference);
        assert_eq!(diff.output_kind, CompressionOutputKind::Diff);
    }
}
