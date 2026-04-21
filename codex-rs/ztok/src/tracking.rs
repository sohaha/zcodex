use crate::behavior::ZtokBehavior;
use crate::compression::CompressionOutputKind;
use crate::compression::CompressionResult;
use crate::compression::ContentKind;
use crate::compression::ExplicitFallbackReason;
use crate::settings;
use serde::Serialize;
use std::ffi::OsString;
use std::time::Instant;

/// 为运行期命令包装器保留的最小时序统计适配层。
///
/// Codex 将 ZTOK 作为轻量命令过滤层嵌入，因此不会包含上游的分析、
/// 持久化或遥测功能。
pub struct TimedExecution {
    started_at: Instant,
    trace_enabled: bool,
}

impl TimedExecution {
    #[must_use]
    pub fn start() -> Self {
        Self {
            started_at: Instant::now(),
            trace_enabled: settings::runtime_settings().decision_trace.enabled,
        }
    }

    pub fn track(&self, _original_cmd: &str, _ztok_cmd: &str, _input: &str, _output: &str) {
        let _ = self.trace_enabled;
        let _ = self.started_at.elapsed();
    }

    pub fn track_passthrough(&self, _original_cmd: &str, _ztok_cmd: &str) {
        let _ = self.trace_enabled;
        let _ = self.started_at.elapsed();
    }

    pub fn track_compression_decision(
        &self,
        command: &str,
        source: &str,
        behavior: ZtokBehavior,
        input_bytes: usize,
        result: &CompressionResult,
    ) {
        if !self.trace_enabled {
            return;
        }

        let event = CompressionDecisionEvent {
            kind: "compression_decision",
            command,
            source,
            behavior: behavior_label(behavior),
            content_kind: content_kind_label(result.content_kind),
            output_kind: output_kind_label(result.output_kind),
            decision: decision_label(result),
            fallback: result.fallback.map(fallback_label),
            input_bytes,
            output_bytes: result.output.len(),
            elapsed_ms: self.started_at.elapsed().as_millis(),
        };

        if let Ok(serialized) = serde_json::to_string(&event) {
            eprintln!("{serialized}");
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CompressionDecisionEvent<'a> {
    kind: &'static str,
    command: &'a str,
    source: &'a str,
    behavior: &'static str,
    content_kind: &'static str,
    output_kind: &'static str,
    decision: &'static str,
    fallback: Option<&'static str>,
    input_bytes: usize,
    output_bytes: usize,
    elapsed_ms: u128,
}

const fn behavior_label(behavior: ZtokBehavior) -> &'static str {
    match behavior {
        ZtokBehavior::Enhanced => "enhanced",
        ZtokBehavior::Basic => "basic",
    }
}

const fn content_kind_label(content_kind: ContentKind) -> &'static str {
    match content_kind {
        ContentKind::Code => "code",
        ContentKind::Json => "json",
        ContentKind::Log => "log",
        ContentKind::Text => "text",
    }
}

const fn output_kind_label(output_kind: CompressionOutputKind) -> &'static str {
    match output_kind {
        CompressionOutputKind::Full => "full",
        CompressionOutputKind::ShortReference => "short_reference",
        CompressionOutputKind::Diff => "diff",
    }
}

const fn decision_label(result: &CompressionResult) -> &'static str {
    match (result.output_kind, result.fallback) {
        (CompressionOutputKind::ShortReference, _) => "short_reference",
        (CompressionOutputKind::Diff, _) => "diff",
        (CompressionOutputKind::Full, Some(_)) => "full_fallback",
        (CompressionOutputKind::Full, None) => "full",
    }
}

const fn fallback_label(reason: ExplicitFallbackReason) -> &'static str {
    match reason {
        ExplicitFallbackReason::EmptySpecializedOutput => "empty_specialized_output",
        ExplicitFallbackReason::DedupDisabledNoSessionId => "dedup_disabled_no_session_id",
        ExplicitFallbackReason::DedupCacheUnavailable => "dedup_cache_unavailable",
        ExplicitFallbackReason::NearDuplicateLowConfidence => "near_duplicate_low_confidence",
        ExplicitFallbackReason::NearDuplicateCandidateConflict => {
            "near_duplicate_candidate_conflict"
        }
        ExplicitFallbackReason::NearDuplicateSnapshotUnavailable => {
            "near_duplicate_snapshot_unavailable"
        }
        ExplicitFallbackReason::NearDuplicateDiffUnreadable => "near_duplicate_diff_unreadable",
        ExplicitFallbackReason::ShortReferenceUnavailable => "short_reference_unavailable",
        ExplicitFallbackReason::DiffUnavailable => "diff_unavailable",
        ExplicitFallbackReason::StrategyUnavailable => "strategy_unavailable",
    }
}

#[must_use]
pub fn args_display(args: &[OsString]) -> String {
    args.iter()
        .map(|arg| arg.to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compression::CompressionResult;
    use crate::compression::ContentKind;

    #[test]
    fn formats_args_for_passthrough_labels() {
        let args = vec![OsString::from("status"), OsString::from("--short")];
        assert_eq!(args_display(&args), "status --short");
    }

    #[test]
    fn tracking_shim_is_noop() {
        let timer = TimedExecution::start();
        timer.track("git status", "ztok git status", "raw", "filtered");
        timer.track_passthrough("git tag", "ztok 回退：git tag");
    }

    #[test]
    fn compression_decision_labels_match_contract() {
        let result = CompressionResult::fallback_full(
            ContentKind::Text,
            "output".to_string(),
            ExplicitFallbackReason::DedupDisabledNoSessionId,
        );

        assert_eq!(output_kind_label(result.output_kind), "full");
        assert_eq!(decision_label(&result), "full_fallback");
        assert_eq!(
            result.fallback.map(fallback_label),
            Some("dedup_disabled_no_session_id")
        );
    }
}
