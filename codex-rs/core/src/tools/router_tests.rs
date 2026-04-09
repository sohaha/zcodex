use std::sync::Arc;

use crate::codex::make_session_and_context;
use crate::function_tool::FunctionCallError;
use crate::tools::context::FunctionToolOutput;
use crate::tools::context::ToolPayload;
use crate::tools::parallel::ToolCallRuntime;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::ToolKind;
use crate::tools::registry::ToolRegistryBuilder;
use crate::tools::rewrite::ToolRoutingDirectives;
use crate::turn_diff_tracker::TurnDiffTracker;
use codex_protocol::models::ResponseInputItem;
use codex_protocol::models::ResponseItem;
use codex_tools::JsonSchema;
use codex_tools::ResponsesApiTool;
use codex_tools::ToolSpec;
use tokio_util::sync::CancellationToken;

use super::ToolCall;
use super::ToolCallSource;
use super::ToolRouter;
use super::ToolRouterParams;

#[tokio::test]
async fn js_repl_tools_only_blocks_direct_tool_calls() -> anyhow::Result<()> {
    let (session, mut turn) = make_session_and_context().await;
    turn.tools_config.js_repl_tools_only = true;

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
    let router = ToolRouter::from_config(
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
    );

    let call = ToolCall {
        tool_name: "shell".to_string(),
        tool_namespace: None,
        call_id: "call-1".to_string(),
        payload: ToolPayload::Function {
            arguments: "{}".to_string(),
        },
    };
    let tracker = Arc::new(tokio::sync::Mutex::new(TurnDiffTracker::new()));
    let err = router
        .dispatch_tool_call_with_code_mode_result(
            session,
            turn,
            tracker,
            call,
            ToolCallSource::Direct,
        )
        .await
        .err()
        .expect("direct tool calls should be blocked");
    let FunctionCallError::RespondToModel(message) = err else {
        panic!("expected RespondToModel, got {err:?}");
    };
    assert!(message.contains("direct tool calls are disabled"));

    Ok(())
}

#[tokio::test]
async fn js_repl_tools_only_allows_js_repl_source_calls() -> anyhow::Result<()> {
    let (session, mut turn) = make_session_and_context().await;
    turn.tools_config.js_repl_tools_only = true;

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
    let router = ToolRouter::from_config(
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
    );

    let call = ToolCall {
        tool_name: "shell".to_string(),
        tool_namespace: None,
        call_id: "call-2".to_string(),
        payload: ToolPayload::Function {
            arguments: "{}".to_string(),
        },
    };
    let tracker = Arc::new(tokio::sync::Mutex::new(TurnDiffTracker::new()));
    let err = router
        .dispatch_tool_call_with_code_mode_result(
            session,
            turn,
            tracker,
            call,
            ToolCallSource::JsRepl,
        )
        .await
        .err()
        .expect("shell call with empty args should fail");
    let message = err.to_string();
    assert!(
        !message.contains("direct tool calls are disabled"),
        "js_repl source should bypass direct-call policy gate"
    );

    Ok(())
}

#[tokio::test]
async fn build_tool_call_uses_namespace_for_registry_name() -> anyhow::Result<()> {
    let (session, _) = make_session_and_context().await;
    let session = Arc::new(session);
    let tool_name = "create_event".to_string();

    let call = ToolRouter::build_tool_call(
        &session,
        ResponseItem::FunctionCall {
            id: None,
            name: tool_name.clone(),
            namespace: Some("mcp__codex_apps__calendar".to_string()),
            arguments: "{}".to_string(),
            call_id: "call-namespace".to_string(),
        },
    )
    .await?
    .expect("function_call should produce a tool call");

    assert_eq!(call.tool_name, tool_name);
    assert_eq!(
        call.tool_namespace,
        Some("mcp__codex_apps__calendar".to_string())
    );
    assert_eq!(call.call_id, "call-namespace");
    match call.payload {
        ToolPayload::Function { arguments } => {
            assert_eq!(arguments, "{}");
        }
        other => panic!("expected function payload, got {other:?}"),
    }

    Ok(())
}

#[tokio::test]
async fn shell_aliases_inherit_parallel_support() -> anyhow::Result<()> {
    let (session, turn) = make_session_and_context().await;
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
    let router = ToolRouter::from_config(
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
    );

    assert!(router.tool_supports_parallel("shell"));
    assert!(router.tool_supports_parallel("container.exec"));
    assert!(router.tool_supports_parallel("local_shell"));
    assert!(router.tool_supports_parallel("shell_command"));

    Ok(())
}

#[derive(Debug)]
struct FakeFunctionHandler {
    label: &'static str,
}

impl ToolHandler for FakeFunctionHandler {
    type Output = FunctionToolOutput;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    async fn handle(
        &self,
        invocation: crate::tools::context::ToolInvocation,
    ) -> Result<Self::Output, FunctionCallError> {
        let ToolPayload::Function { arguments } = invocation.payload else {
            return Err(FunctionCallError::Fatal(
                "fake handler expected function payload".to_string(),
            ));
        };
        Ok(FunctionToolOutput::from_text(
            format!("{}:{arguments}", self.label),
            Some(true),
        ))
    }
}

fn fake_responses_tool(name: &str) -> ToolSpec {
    ToolSpec::Function(ResponsesApiTool {
        name: name.to_string(),
        description: format!("fake {name}"),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::Object {
            properties: Default::default(),
            required: None,
            additional_properties: Some(false.into()),
        },
        output_schema: None,
    })
}

fn fake_router() -> Arc<ToolRouter> {
    let mut builder = ToolRegistryBuilder::new();
    builder.push_spec_with_parallel_support(fake_responses_tool("grep_files"), true);
    builder.push_spec_with_parallel_support(fake_responses_tool("ztldr"), true);
    builder.register_handler(
        "grep_files",
        Arc::new(FakeFunctionHandler { label: "grep" }),
    );
    builder.register_handler("ztldr", Arc::new(FakeFunctionHandler { label: "ztldr" }));

    let (specs, registry) = builder.build();
    let model_visible_specs = specs.iter().map(|spec| spec.spec.clone()).collect();
    Arc::new(ToolRouter {
        registry,
        specs,
        model_visible_specs,
    })
}

fn output_text(response: ResponseInputItem) -> String {
    match response {
        ResponseInputItem::FunctionCallOutput { output, .. } => {
            output.body.to_text().unwrap_or_default()
        }
        other => panic!("expected FunctionCallOutput, got {other:?}"),
    }
}

#[tokio::test]
async fn runtime_dispatch_routes_rewritten_grep_files_to_tldr_handler() {
    let (session, turn) = make_session_and_context().await;

    let runtime = ToolCallRuntime::new(
        fake_router(),
        Arc::new(session),
        Arc::new(turn),
        Arc::new(tokio::sync::Mutex::new(TurnDiffTracker::new())),
    );

    let result = runtime
        .handle_tool_call_with_source(
            ToolCall {
                tool_name: "grep_files".to_string(),
                tool_namespace: None,
                call_id: "call-runtime-dispatch".to_string(),
                payload: ToolPayload::Function {
                    arguments: r#"{"pattern":"create_tldr_tool","include":"*.rs"}"#.to_string(),
                },
            },
            ToolCallSource::Direct,
            CancellationToken::new(),
        )
        .await
        .expect("rewrite+dispatch should succeed")
        .into_response();

    let text = output_text(result);
    assert!(text.starts_with("ztldr:"), "unexpected response: {text}");
    assert!(
        text.contains(r#""action":"context""#),
        "unexpected rewritten args: {text}"
    );
}

#[tokio::test]
async fn runtime_dispatch_keeps_force_raw_grep_on_original_handler() {
    let (session, turn) = make_session_and_context().await;
    *turn.tool_routing_directives.write().await = ToolRoutingDirectives {
        force_raw_grep: true,
        ..Default::default()
    };

    let runtime = ToolCallRuntime::new(
        fake_router(),
        Arc::new(session),
        Arc::new(turn),
        Arc::new(tokio::sync::Mutex::new(TurnDiffTracker::new())),
    );

    let result = runtime
        .handle_tool_call_with_source(
            ToolCall {
                tool_name: "grep_files".to_string(),
                tool_namespace: None,
                call_id: "call-runtime-raw".to_string(),
                payload: ToolPayload::Function {
                    arguments: r#"{"pattern":"create_tldr_tool","include":"*.rs"}"#.to_string(),
                },
            },
            ToolCallSource::Direct,
            CancellationToken::new(),
        )
        .await
        .expect("raw grep dispatch should succeed")
        .into_response();

    let text = output_text(result);
    assert!(text.starts_with("grep:"), "unexpected response: {text}");
    assert!(
        text.contains(r#""pattern":"create_tldr_tool""#),
        "unexpected original args: {text}"
    );
}
