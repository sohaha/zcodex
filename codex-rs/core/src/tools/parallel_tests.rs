use crate::codex::make_session_and_context;
use crate::config::AutoTldrRoutingMode;
use crate::tools::context::ToolPayload;
use crate::tools::rewrite::ToolRoutingDirectives;
use crate::tools::router::ToolCall;
use crate::tools::router::ToolCallSource;
use codex_native_tldr::tool_api::TldrToolAction;
use codex_native_tldr::tool_api::TldrToolCallParam;
use pretty_assertions::assert_eq;

use super::ToolCallRuntime;

async fn make_turn_context() -> crate::codex::TurnContext {
    let (_session, turn) = make_session_and_context().await;
    turn
}

#[tokio::test]
async fn runtime_seam_rewrites_grep_files_to_tldr_before_dispatch() {
    let turn = make_turn_context().await;

    let rewritten = ToolCallRuntime::rewrite_call_for_dispatch(
        &turn,
        ToolCall {
            tool_name: "grep_files".into(),
            call_id: "call-runtime-rewrite".to_string(),
            payload: ToolPayload::Function {
                arguments: r#"{"pattern":"create_tldr_tool","include":"*.rs"}"#.to_string(),
            },
        },
        ToolCallSource::Direct,
    )
    .await;

    assert_eq!(rewritten.tool_name, "ztldr".into());
    let ToolPayload::Function { arguments } = rewritten.payload else {
        panic!("expected function payload");
    };
    let args: TldrToolCallParam =
        serde_json::from_str(&arguments).expect("parse rewritten tldr args");
    assert_eq!(args.action, TldrToolAction::Context);
    assert_eq!(rewritten.call_id, "call-runtime-rewrite");
}

#[tokio::test]
async fn runtime_seam_keeps_grep_files_raw_when_mode_is_off() {
    let mut turn = make_turn_context().await;
    turn.tools_config.auto_tldr_routing = AutoTldrRoutingMode::Off;

    let original = ToolCall {
        tool_name: "grep_files".into(),
        call_id: "call-mode-off".to_string(),
        payload: ToolPayload::Function {
            arguments: r#"{"pattern":"create_tldr_tool","include":"*.rs"}"#.to_string(),
        },
    };

    let rewritten =
        ToolCallRuntime::rewrite_call_for_dispatch(&turn, original.clone(), ToolCallSource::Direct)
            .await;

    assert_eq!(rewritten.tool_name, original.tool_name);
    assert_eq!(rewritten.call_id, original.call_id);
}

#[tokio::test]
async fn runtime_seam_keeps_grep_files_raw_when_directive_forces_raw_grep() {
    let turn = make_turn_context().await;
    *turn.tool_routing_directives.write().await = ToolRoutingDirectives {
        force_raw_grep: true,
        ..Default::default()
    };

    let original = ToolCall {
        tool_name: "grep_files".into(),
        call_id: "call-force-raw".to_string(),
        payload: ToolPayload::Function {
            arguments: r#"{"pattern":"create_tldr_tool","include":"*.rs"}"#.to_string(),
        },
    };

    let rewritten =
        ToolCallRuntime::rewrite_call_for_dispatch(&turn, original.clone(), ToolCallSource::Direct)
            .await;

    assert_eq!(rewritten.tool_name, original.tool_name);
    assert_eq!(rewritten.call_id, original.call_id);
}

#[tokio::test]
async fn runtime_seam_does_not_rewrite_grep_files_for_code_mode_source() {
    let turn = make_turn_context().await;
    *turn.tool_routing_directives.write().await = ToolRoutingDirectives {
        prefer_context_search: true,
        ..Default::default()
    };

    let original = ToolCall {
        tool_name: "grep_files".into(),
        call_id: "call-code-mode".to_string(),
        payload: ToolPayload::Function {
            arguments: r#"{"pattern":"create_tldr_tool","include":"*.rs"}"#.to_string(),
        },
    };

    let rewritten = ToolCallRuntime::rewrite_call_for_dispatch(
        &turn,
        original.clone(),
        ToolCallSource::CodeMode,
    )
    .await;

    assert_eq!(rewritten.tool_name, original.tool_name);
    assert_eq!(rewritten.call_id, original.call_id);
}
