use crate::session::turn_context::TurnContext;
use crate::tools::context::ToolCallSource;
use crate::tools::rewrite::ToolRoutingDirectives;
use crate::tools::rewrite::auto_tldr::rewrite_grep_files_to_tldr;
use crate::tools::rewrite::decision::ToolRewriteDecision;
use crate::tools::rewrite::read_gate::rewrite_read_file_to_tldr;
use crate::tools::rewrite::tldr_routing::SearchSignal;
use crate::tools::router::ToolCall;
use codex_native_tldr::tool_api::TldrToolAction;
use codex_otel::SessionTelemetry;
use codex_otel::TOOL_ROUTE_METRIC;
use tracing::info;

pub(crate) async fn rewrite_tool_call(
    turn: &TurnContext,
    call: ToolCall,
    source: ToolCallSource,
) -> ToolRewriteDecision {
    let mode = turn.tools_config.auto_tldr_routing;
    let from_tool = call.tool_name.display();
    let call_id = call.call_id.clone();

    let decision = if matches!(source, ToolCallSource::JsRepl | ToolCallSource::CodeMode) {
        ToolRewriteDecision::Passthrough {
            call,
            reason: "unknown_passthrough",
            signal: None,
        }
    } else if mode.is_off() {
        ToolRewriteDecision::Passthrough {
            call,
            reason: "mode_off",
            signal: None,
        }
    } else {
        let directives = turn.tool_routing_directives.read().await.clone();
        route_auto_tldr(turn, call, directives, mode).await
    };

    log_tool_route(
        mode.as_str(),
        source,
        from_tool.as_str(),
        &call_id,
        &decision,
    );
    emit_tool_route_metric(
        &turn.session_telemetry,
        mode.as_str(),
        source,
        &decision,
        from_tool.as_str(),
    );
    decision
}

async fn route_auto_tldr(
    turn: &TurnContext,
    call: ToolCall,
    directives: ToolRoutingDirectives,
    mode: crate::config::AutoTldrRoutingMode,
) -> ToolRewriteDecision {
    match call.tool_name.as_str() {
        "grep_files" => rewrite_grep_files_to_tldr(turn, call, directives, mode).await,
        "read_file" => rewrite_read_file_to_tldr(turn, call, directives, mode).await,
        _ => ToolRewriteDecision::Passthrough {
            call,
            reason: "unknown_passthrough",
            signal: None,
        },
    }
}

fn log_tool_route(
    mode: &'static str,
    source: ToolCallSource,
    from_tool: &str,
    call_id: &str,
    decision: &ToolRewriteDecision,
) {
    let to_tool = decision.call().tool_name.as_str();
    let action = decision.action().map(tldr_action_name).unwrap_or_default();
    let signal = decision
        .signal()
        .map(search_signal_name)
        .unwrap_or_default();

    match decision {
        ToolRewriteDecision::Passthrough { .. } => info!(
            target: "codex_core::tool_route",
            decision = "passthrough",
            mode,
            from_tool,
            to_tool,
            reason = decision.reason(),
            call_id,
            source = ?source,
            signal,
            "tool route decision"
        ),
        ToolRewriteDecision::Rewrite { .. } => info!(
            target: "codex_core::tool_route",
            decision = "rewrite",
            mode,
            from_tool,
            to_tool,
            reason = decision.reason(),
            call_id,
            source = ?source,
            action,
            signal,
            "tool route decision"
        ),
    }
}

fn emit_tool_route_metric(
    session_telemetry: &SessionTelemetry,
    mode: &'static str,
    source: ToolCallSource,
    decision: &ToolRewriteDecision,
    from_tool: &str,
) {
    let route_decision = match decision {
        ToolRewriteDecision::Passthrough { .. } => "passthrough",
        ToolRewriteDecision::Rewrite { .. } => "rewrite",
    };
    let to_tool = decision.call().tool_name.as_str();
    let action = decision.action().map(tldr_action_name).unwrap_or("none");
    let signal = decision.signal().map(search_signal_name).unwrap_or("none");
    let source = tool_call_source_name(source);

    session_telemetry.counter(
        TOOL_ROUTE_METRIC,
        /*inc*/ 1,
        &[
            ("decision", route_decision),
            ("mode", mode),
            ("source", source),
            ("from_tool", from_tool),
            ("to_tool", to_tool),
            ("reason", decision.reason()),
            ("action", action),
            ("signal", signal),
        ],
    );
}

fn tldr_action_name(action: &TldrToolAction) -> &'static str {
    match action {
        TldrToolAction::Structure => "structure",
        TldrToolAction::Search => "search",
        TldrToolAction::Extract => "extract",
        TldrToolAction::Imports => "imports",
        TldrToolAction::Importers => "importers",
        TldrToolAction::Context => "context",
        TldrToolAction::Impact => "impact",
        TldrToolAction::Calls => "calls",
        TldrToolAction::Dead => "dead",
        TldrToolAction::Arch => "arch",
        TldrToolAction::ChangeImpact => "change-impact",
        TldrToolAction::Cfg => "cfg",
        TldrToolAction::Dfg => "dfg",
        TldrToolAction::Slice => "slice",
        TldrToolAction::Semantic => "semantic",
        TldrToolAction::Diagnostics => "diagnostics",
        TldrToolAction::Doctor => "doctor",
        TldrToolAction::Ping => "ping",
        TldrToolAction::Warm => "warm",
        TldrToolAction::Snapshot => "snapshot",
        TldrToolAction::Status => "status",
        TldrToolAction::Notify => "notify",
    }
}

fn search_signal_name(signal: SearchSignal) -> &'static str {
    match signal {
        SearchSignal::BareSymbol => "bare_symbol",
        SearchSignal::WrappedSymbol => "wrapped_symbol",
        SearchSignal::MemberSymbol => "member_symbol",
        SearchSignal::NaturalLanguage => "natural_language",
        SearchSignal::PathLike => "path_like",
        SearchSignal::GenericSemantic => "generic_semantic",
    }
}

fn tool_call_source_name(source: ToolCallSource) -> &'static str {
    match source {
        ToolCallSource::Direct => "direct",
        ToolCallSource::JsRepl => "js_repl",
        ToolCallSource::CodeMode => "code_mode",
    }
}

#[cfg(test)]
mod tests {
    use super::TOOL_ROUTE_METRIC;
    use super::emit_tool_route_metric;
    use crate::tools::context::ToolCallSource;
    use crate::tools::rewrite::decision::ToolRewriteDecision;
    use crate::tools::rewrite::tldr_routing::SearchSignal;
    use crate::tools::router::ToolCall;
    use codex_native_tldr::tool_api::TldrToolAction;
    use codex_otel::MetricsClient;
    use codex_otel::MetricsConfig;
    use codex_otel::SessionTelemetry;
    use codex_protocol::ThreadId;
    use codex_protocol::protocol::SessionSource;
    use opentelemetry::KeyValue;
    use opentelemetry_sdk::metrics::InMemoryMetricExporter;
    use opentelemetry_sdk::metrics::data::AggregatedMetrics;
    use opentelemetry_sdk::metrics::data::Metric;
    use opentelemetry_sdk::metrics::data::MetricData;
    use opentelemetry_sdk::metrics::data::ResourceMetrics;
    use pretty_assertions::assert_eq;
    use std::collections::BTreeMap;

    fn test_session_telemetry() -> SessionTelemetry {
        let exporter = InMemoryMetricExporter::default();
        let metrics = MetricsClient::new(
            MetricsConfig::in_memory("test", "codex-core", env!("CARGO_PKG_VERSION"), exporter)
                .with_runtime_reader(),
        )
        .expect("in-memory metrics client");
        SessionTelemetry::new(
            ThreadId::new(),
            "gpt-5.1",
            "gpt-5.1",
            /*account_id*/ None,
            /*account_email*/ None,
            /*auth_mode*/ None,
            "test_originator".to_string(),
            /*log_user_prompts*/ false,
            "tty".to_string(),
            SessionSource::Cli,
        )
        .with_metrics_without_metadata_tags(metrics)
    }

    fn find_metric<'a>(resource_metrics: &'a ResourceMetrics, name: &str) -> &'a Metric {
        for scope_metrics in resource_metrics.scope_metrics() {
            for metric in scope_metrics.metrics() {
                if metric.name() == name {
                    return metric;
                }
            }
        }
        panic!("metric {name} missing");
    }

    fn attributes_to_map<'a>(
        attributes: impl Iterator<Item = &'a KeyValue>,
    ) -> BTreeMap<String, String> {
        attributes
            .map(|kv| (kv.key.as_str().to_string(), kv.value.as_str().to_string()))
            .collect()
    }

    fn metric_point(resource_metrics: &ResourceMetrics) -> (BTreeMap<String, String>, u64) {
        let metric = find_metric(resource_metrics, TOOL_ROUTE_METRIC);
        match metric.data() {
            AggregatedMetrics::U64(data) => match data {
                MetricData::Sum(sum) => {
                    let points: Vec<_> = sum.data_points().collect();
                    assert_eq!(points.len(), 1);
                    let point = points[0];
                    (attributes_to_map(point.attributes()), point.value())
                }
                _ => panic!("unexpected counter aggregation"),
            },
            _ => panic!("unexpected counter data type"),
        }
    }

    #[test]
    fn emits_rewrite_metric_with_action_and_signal_tags() {
        let session_telemetry = test_session_telemetry();
        let decision = ToolRewriteDecision::Rewrite {
            call: ToolCall {
                tool_name: "ztldr".into(),
                call_id: "call-1".to_string(),
                payload: crate::tools::context::ToolPayload::Function {
                    arguments: "{}".to_string(),
                },
            },
            reason: "structural_member_symbol_query",
            action: Some(TldrToolAction::Context),
            signal: Some(SearchSignal::MemberSymbol),
        };

        emit_tool_route_metric(
            &session_telemetry,
            "safe",
            ToolCallSource::Direct,
            &decision,
            "grep_files",
        );

        let snapshot = session_telemetry
            .snapshot_metrics()
            .expect("runtime metrics snapshot");
        let (attrs, value) = metric_point(&snapshot);

        assert_eq!(value, 1);
        assert_eq!(
            attrs,
            BTreeMap::from([
                ("action".to_string(), "context".to_string()),
                ("decision".to_string(), "rewrite".to_string()),
                ("from_tool".to_string(), "grep_files".to_string()),
                ("mode".to_string(), "safe".to_string()),
                (
                    "reason".to_string(),
                    "structural_member_symbol_query".to_string(),
                ),
                ("signal".to_string(), "member_symbol".to_string()),
                ("source".to_string(), "direct".to_string()),
                ("to_tool".to_string(), "ztldr".to_string()),
            ])
        );
    }

    #[test]
    fn emits_passthrough_metric_with_none_placeholders() {
        let session_telemetry = test_session_telemetry();
        let decision = ToolRewriteDecision::Passthrough {
            call: ToolCall {
                tool_name: "grep_files".into(),
                call_id: "call-2".to_string(),
                payload: crate::tools::context::ToolPayload::Function {
                    arguments: "{}".to_string(),
                },
            },
            reason: "raw_pattern_regex",
            signal: None,
        };

        emit_tool_route_metric(
            &session_telemetry,
            "safe",
            ToolCallSource::Direct,
            &decision,
            "grep_files",
        );

        let snapshot = session_telemetry
            .snapshot_metrics()
            .expect("runtime metrics snapshot");
        let (attrs, value) = metric_point(&snapshot);

        assert_eq!(value, 1);
        assert_eq!(
            attrs,
            BTreeMap::from([
                ("action".to_string(), "none".to_string()),
                ("decision".to_string(), "passthrough".to_string()),
                ("from_tool".to_string(), "grep_files".to_string()),
                ("mode".to_string(), "safe".to_string()),
                ("reason".to_string(), "raw_pattern_regex".to_string()),
                ("signal".to_string(), "none".to_string()),
                ("source".to_string(), "direct".to_string()),
                ("to_tool".to_string(), "grep_files".to_string()),
            ])
        );
    }
}
