use std::sync::Arc;

use crate::codex::make_session_and_context;
use crate::tools::context::ToolPayload;
use crate::tools::router::ToolCall;
use crate::tools::router::ToolCallSource;
use crate::tools::router::ToolRouter;
use crate::tools::router::ToolRouterParams;
use crate::turn_diff_tracker::TurnDiffTracker;
use codex_native_tldr::tool_api::TldrToolAction;
use codex_native_tldr::tool_api::TldrToolCallParam;
use pretty_assertions::assert_eq;

use super::ToolCallRuntime;

#[tokio::test]
async fn runtime_seam_rewrites_grep_files_to_tldr_before_dispatch() {
    let (session, turn) = make_session_and_context().await;
    *turn.tool_routing_directives.write().await = crate::tools::rewrite::ToolRoutingDirectives {
        prefer_context_search: true,
        ..Default::default()
    };

    let session = Arc::new(session);
    let turn = Arc::new(turn);
    let mcp_tools = session
        .services
        .mcp_connection_manager
        .read()
        .await
        .list_all_tools()
        .await;
    let app_tools = Some(mcp_tools.clone());
    let router = Arc::new(ToolRouter::from_config(
        &turn.tools_config,
        ToolRouterParams {
            mcp_tools: Some(
                mcp_tools
                    .into_iter()
                    .map(|(name, tool)| (name, tool.tool))
                    .collect(),
            ),
            app_tools,
            discoverable_tools: None,
            dynamic_tools: turn.dynamic_tools.as_slice(),
        },
    ));
    let tracker = Arc::new(tokio::sync::Mutex::new(TurnDiffTracker::new()));
    let _runtime = ToolCallRuntime::new(router, Arc::clone(&session), Arc::clone(&turn), tracker);

    let rewritten = ToolCallRuntime::rewrite_call_for_dispatch(
        turn.as_ref(),
        ToolCall {
            tool_name: "grep_files".to_string(),
            tool_namespace: None,
            call_id: "call-runtime-rewrite".to_string(),
            payload: ToolPayload::Function {
                arguments: r#"{"pattern":"create_tldr_tool","include":"*.rs"}"#.to_string(),
            },
        },
        ToolCallSource::Direct,
    )
    .await;

    assert_eq!(rewritten.tool_name, "tldr");
    let ToolPayload::Function { arguments } = rewritten.payload else {
        panic!("expected function payload");
    };
    let args: TldrToolCallParam =
        serde_json::from_str(&arguments).expect("parse rewritten tldr args");
    assert_eq!(args.action, TldrToolAction::Context);
}
