use crate::codex::TurnContext;
use crate::tools::context::ToolCallSource;
use crate::tools::rewrite::ToolRoutingDirectives;
use crate::tools::rewrite::auto_tldr::rewrite_grep_files_to_tldr;
use crate::tools::rewrite::decision::ToolRewriteDecision;
use crate::tools::rewrite::read_gate::rewrite_read_file_to_tldr;
use crate::tools::router::ToolCall;
use codex_native_tldr::tool_api::TldrToolAction;
use tracing::info;

pub(crate) async fn rewrite_tool_call(
    turn: &TurnContext,
    call: ToolCall,
    source: ToolCallSource,
) -> ToolRewriteDecision {
    let mode = turn.tools_config.auto_tldr_routing;
    let from_tool = call.tool_name.clone();
    let call_id = call.call_id.clone();

    let decision = if matches!(source, ToolCallSource::JsRepl | ToolCallSource::CodeMode) {
        ToolRewriteDecision::Passthrough {
            call,
            reason: "unknown_passthrough",
        }
    } else if mode.is_off() {
        ToolRewriteDecision::Passthrough {
            call,
            reason: "mode_off",
        }
    } else {
        let directives = turn.tool_routing_directives.read().await.clone();
        route_auto_tldr(turn, call, directives, mode).await
    };

    log_tool_route(mode.as_str(), source, &from_tool, &call_id, &decision);
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
            "tool route decision"
        ),
    }
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
