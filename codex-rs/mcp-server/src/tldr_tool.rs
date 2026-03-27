use codex_native_tldr::daemon::TldrDaemonCommand;
use codex_native_tldr::daemon::query_daemon;
use codex_native_tldr::tool_api::TldrToolCallParam;
use codex_native_tldr::tool_api::run_tldr_tool_with_hooks;
use codex_native_tldr::tool_api::tldr_tool_output_schema;
use codex_native_tldr::tool_api::wait_for_external_daemon;
use rmcp::model::CallToolResult;
use rmcp::model::Content;
use rmcp::model::JsonObject;
use rmcp::model::Tool;
use schemars::r#gen::SchemaSettings;
use std::sync::Arc;

pub(crate) fn create_tool_for_tldr_tool_call_param() -> Tool {
    let schema = SchemaSettings::draft2019_09()
        .with(|settings| {
            settings.inline_subschemas = true;
            settings.option_add_null_type = false;
        })
        .into_generator()
        .into_root_schema_for::<TldrToolCallParam>();
    let input_schema = create_tool_input_schema(schema, "TLDR tool schema should serialize");

    Tool {
        name: "tldr".into(),
        title: Some("Native TLDR".to_string()),
        description: Some(
            "Structured code context analysis via native-tldr with daemon-first execution.".into(),
        ),
        input_schema,
        output_schema: Some(match tldr_tool_output_schema() {
            serde_json::Value::Object(map) => Arc::new(map),
            _ => unreachable!("json literal must be an object"),
        }),
        annotations: None,
        execution: None,
        icons: None,
        meta: None,
    }
}

pub(crate) async fn run_tldr_tool(arguments: Option<JsonObject>) -> CallToolResult {
    let args = match arguments.map(serde_json::Value::Object) {
        Some(json_val) => match serde_json::from_value::<TldrToolCallParam>(json_val) {
            Ok(args) => args,
            Err(err) => return error_result(format!("Failed to parse tldr tool arguments: {err}")),
        },
        None => return error_result("Missing arguments for tldr tool-call.".to_string()),
    };

    run_tldr_tool_with_mcp_hooks(
        args,
        |project_root, command| Box::pin(query_daemon(project_root, command)),
        |project_root| Box::pin(wait_for_external_daemon(project_root)),
    )
    .await
}

async fn run_tldr_tool_with_mcp_hooks<Q, E>(
    args: TldrToolCallParam,
    query: Q,
    ensure_running: E,
) -> CallToolResult
where
    Q: for<'a> Fn(
        &'a std::path::Path,
        &'a TldrDaemonCommand,
    ) -> codex_native_tldr::tool_api::QueryDaemonFuture<'a>,
    E: for<'a> Fn(&'a std::path::Path) -> codex_native_tldr::tool_api::EnsureDaemonFuture<'a>,
{
    match run_tldr_tool_with_hooks(args, query, ensure_running).await {
        Ok(result) => success_result(result.text, result.structured_content),
        Err(err) => error_result(format!("tldr tool failed: {err}")),
    }
}

fn success_result(text: String, structured_content: serde_json::Value) -> CallToolResult {
    CallToolResult {
        content: vec![Content::text(text)],
        structured_content: Some(structured_content),
        is_error: Some(false),
        meta: None,
    }
}

fn error_result(text: String) -> CallToolResult {
    CallToolResult {
        content: vec![Content::text(text)],
        structured_content: None,
        is_error: Some(true),
        meta: None,
    }
}

fn create_tool_input_schema(
    schema: schemars::schema::RootSchema,
    panic_message: &str,
) -> Arc<JsonObject> {
    #[expect(clippy::expect_used)]
    let schema_value = serde_json::to_value(&schema).expect(panic_message);
    let mut schema_object = match schema_value {
        serde_json::Value::Object(object) => object,
        _ => panic!("tool schema should serialize to a JSON object"),
    };

    let mut input_schema = JsonObject::new();
    for key in ["properties", "required", "type", "$defs", "definitions"] {
        if let Some(value) = schema_object.remove(key) {
            input_schema.insert(key.to_string(), value);
        }
    }

    Arc::new(input_schema)
}

#[cfg(test)]
mod tests {
    use super::create_tool_for_tldr_tool_call_param;
    use super::run_tldr_tool_with_mcp_hooks;
    use codex_native_tldr::daemon::TldrDaemonCommand;
    use codex_native_tldr::daemon::TldrDaemonResponse;
    use codex_native_tldr::tool_api::TldrToolAction;
    use codex_native_tldr::tool_api::TldrToolCallParam;
    use codex_native_tldr::tool_api::TldrToolLanguage;
    use codex_native_tldr::tool_api::query_daemon_with_hooks;
    use codex_native_tldr::tool_api::tldr_tool_output_schema;
    use pretty_assertions::assert_eq;
    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;
    use tempfile::tempdir;

    #[test]
    fn verify_tldr_tool_json_schema() {
        let tool = create_tool_for_tldr_tool_call_param();
        let tool_json = serde_json::to_value(&tool).expect("tool serializes");
        assert_eq!(tool_json["name"], "tldr");
        assert_eq!(tool_json["title"], "Native TLDR");
        assert_eq!(
            tool_json["description"],
            "Structured code context analysis via native-tldr with daemon-first execution."
        );
        assert_eq!(tool_json["inputSchema"]["type"], "object");
        assert_eq!(
            tool_json["inputSchema"]["required"],
            serde_json::json!(["action"])
        );
        assert_eq!(
            tool_json["inputSchema"]["properties"]["action"]["enum"],
            serde_json::json!([
                "tree", "context", "impact", "semantic", "ping", "warm", "snapshot", "status",
                "notify"
            ])
        );
        assert_eq!(tool_json["outputSchema"], tldr_tool_output_schema());
    }

    #[test]
    fn tldr_tool_param_serializes_camel_case_fields() {
        let value = serde_json::to_value(TldrToolCallParam {
            action: TldrToolAction::Semantic,
            project: Some("/tmp/project".to_string()),
            language: Some(TldrToolLanguage::Typescript),
            symbol: None,
            query: Some("where is auth".to_string()),
            path: None,
        })
        .expect("tool call should serialize");

        assert_eq!(
            value,
            serde_json::json!({
                "action": "semantic",
                "project": "/tmp/project",
                "language": "typescript",
                "query": "where is auth"
            })
        );
    }

    #[tokio::test]
    async fn run_tldr_tool_with_mcp_hooks_preserves_impact_summary_text_contract() {
        let tempdir = tempdir().expect("tempdir should exist");
        let summary =
            "impact summary: 1 symbols across 1 files (1 touched paths); dependency edges=1";
        let result = run_tldr_tool_with_mcp_hooks(
            TldrToolCallParam {
                action: TldrToolAction::Impact,
                project: Some(tempdir.path().display().to_string()),
                language: Some(TldrToolLanguage::Rust),
                symbol: Some("AuthService".to_string()),
                query: None,
                path: None,
            },
            |_project_root, _command| {
                Box::pin(async move {
                    Ok(Some(TldrDaemonResponse {
                        status: "ok".to_string(),
                        message: "impact ready".to_string(),
                        analysis: Some(codex_native_tldr::api::AnalysisResponse {
                            kind: codex_native_tldr::api::AnalysisKind::Pdg,
                            summary: summary.to_string(),
                            details: Some(codex_native_tldr::api::AnalysisDetail {
                                indexed_files: 1,
                                total_symbols: 1,
                                symbol_query: Some("AuthService".to_string()),
                                truncated: false,
                                overview: codex_native_tldr::api::AnalysisOverviewDetail::default(),
                                files: Vec::new(),
                                nodes: Vec::new(),
                                edges: vec![codex_native_tldr::api::AnalysisEdgeDetail {
                                    from: "AuthService".to_string(),
                                    to: "auth::audit".to_string(),
                                    kind: "depends_on".to_string(),
                                }],
                                symbol_index: Vec::new(),
                                units: Vec::new(),
                            }),
                        }),
                        semantic: None,
                        snapshot: None,
                        daemon_status: None,
                        reindex_report: None,
                    }))
                })
            },
            |_project_root| Box::pin(async move { Ok(false) }),
        )
        .await;

        let result_json = serde_json::to_value(&result).expect("call tool result should serialize");
        let structured = result
            .structured_content
            .as_ref()
            .expect("structured content should be present");

        assert_eq!(result.is_error, Some(false));
        assert_eq!(result_json["content"][0]["type"], "text");
        assert_eq!(
            result_json["content"][0]["text"],
            format!("impact rust via daemon: {summary}")
        );
        assert_eq!(structured["action"], "impact");
        assert_eq!(structured["summary"], summary);
        assert_eq!(structured["analysis"]["summary"], summary);
        assert_eq!(structured["analysis"]["kind"], "pdg");
        assert_eq!(
            structured["analysis"]["details"]["symbol_query"],
            "AuthService"
        );
    }

    #[tokio::test]
    async fn query_daemon_with_hooks_retries_when_external_daemon_becomes_ready() {
        let tempdir = tempdir().expect("tempdir should exist");
        let command = TldrDaemonCommand::Ping;
        let query_calls = Arc::new(AtomicUsize::new(0));
        let ensure_calls = Arc::new(AtomicUsize::new(0));
        let query_response = TldrDaemonResponse {
            status: "ok".to_string(),
            message: "pong".to_string(),
            analysis: None,
            semantic: None,
            snapshot: None,
            daemon_status: None,
            reindex_report: None,
        };

        let response = query_daemon_with_hooks(
            tempdir.path(),
            &command,
            &{
                let query_calls = Arc::clone(&query_calls);
                let query_response = query_response.clone();
                move |_project_root, _command| {
                    let query_calls = Arc::clone(&query_calls);
                    let query_response = query_response.clone();
                    Box::pin(async move {
                        let call_index = query_calls.fetch_add(1, Ordering::SeqCst);
                        Ok(if call_index == 0 {
                            None
                        } else {
                            Some(query_response)
                        })
                    })
                }
            },
            &{
                let ensure_calls = Arc::clone(&ensure_calls);
                move |_project_root| {
                    let ensure_calls = Arc::clone(&ensure_calls);
                    Box::pin(async move {
                        ensure_calls.fetch_add(1, Ordering::SeqCst);
                        Ok(true)
                    })
                }
            },
        )
        .await
        .expect("query_daemon_with_hooks should succeed");

        assert_eq!(response, Some(query_response));
        assert_eq!(query_calls.load(Ordering::SeqCst), 2);
        assert_eq!(ensure_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn query_daemon_with_hooks_skips_retry_when_no_external_daemon_is_starting() {
        let tempdir = tempdir().expect("tempdir should exist");
        let command = TldrDaemonCommand::Ping;
        let query_calls = Arc::new(AtomicUsize::new(0));
        let ensure_calls = Arc::new(AtomicUsize::new(0));

        let response = query_daemon_with_hooks(
            tempdir.path(),
            &command,
            &{
                let query_calls = Arc::clone(&query_calls);
                move |_project_root, _command| {
                    let query_calls = Arc::clone(&query_calls);
                    Box::pin(async move {
                        query_calls.fetch_add(1, Ordering::SeqCst);
                        Ok(None)
                    })
                }
            },
            &{
                let ensure_calls = Arc::clone(&ensure_calls);
                move |_project_root| {
                    let ensure_calls = Arc::clone(&ensure_calls);
                    Box::pin(async move {
                        ensure_calls.fetch_add(1, Ordering::SeqCst);
                        Ok(false)
                    })
                }
            },
        )
        .await
        .expect("query_daemon_with_hooks should succeed");

        assert_eq!(response, None);
        assert_eq!(query_calls.load(Ordering::SeqCst), 1);
        assert_eq!(ensure_calls.load(Ordering::SeqCst), 1);
    }
}
