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

    match run_tldr_tool_with_hooks(
        args,
        |project_root, command| Box::pin(query_daemon(project_root, command)),
        |project_root| Box::pin(wait_for_external_daemon(project_root)),
    )
    .await
    {
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
    use codex_native_tldr::daemon::TldrDaemonCommand;
    use codex_native_tldr::daemon::TldrDaemonResponse;
    use codex_native_tldr::tool_api::TldrToolAction;
    use codex_native_tldr::tool_api::TldrToolCallParam;
    use codex_native_tldr::tool_api::TldrToolLanguage;
    use codex_native_tldr::tool_api::query_daemon_with_hooks;
    use pretty_assertions::assert_eq;
    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;
    use tempfile::tempdir;

    #[test]
    fn verify_tldr_tool_json_schema() {
        let tool = create_tool_for_tldr_tool_call_param();
        let tool_json = serde_json::to_value(&tool).expect("tool serializes");
        let expected_tool_json: serde_json::Value = serde_json::from_str(
            r##"{
              "description": "Structured code context analysis via native-tldr with daemon-first execution.",
              "inputSchema": {
                "properties": {
                  "action": {
                    "enum": ["tree", "context", "impact", "semantic", "ping", "warm", "snapshot", "status", "notify"],
                    "type": "string"
                  },
                  "language": {
                    "enum": ["rust", "typescript", "javascript", "python", "go", "php", "zig"],
                    "type": "string"
                  },
                  "path": { "type": "string" },
                  "project": { "type": "string" },
                  "query": { "type": "string" },
                  "symbol": { "type": "string" }
                },
                "required": ["action"],
                "type": "object"
              },
              "name": "tldr",
              "outputSchema": {
                "$defs": {
                  "analysis": {
                    "properties": {
                      "kind": { "type": "string" },
                      "summary": { "type": "string" }
                    },
                    "type": "object"
                  },
                  "reindexReport": {
                    "properties": {
                      "embedding_dimensions": { "type": "integer" },
                      "embedding_enabled": { "type": "boolean" },
                      "finished_at": { "type": "string" },
                      "indexed_files": { "type": "integer" },
                      "indexed_units": { "type": "integer" },
                      "languages": {
                        "items": { "type": "string" },
                        "type": "array"
                      },
                      "message": { "type": "string" },
                      "started_at": { "type": "string" },
                      "status": { "type": "string" },
                      "truncated": { "type": "boolean" }
                    },
                    "type": ["object", "null"]
                  },
                  "semantic": {
                    "properties": {
                      "embeddingUsed": { "type": "boolean" },
                      "enabled": { "type": "boolean" },
                      "indexedFiles": { "type": "integer" },
                      "matches": {
                        "items": {
                          "properties": {
                            "embedding_score": { "type": ["number", "null"] },
                            "line": { "type": "integer" },
                            "path": { "type": "string" },
                            "snippet": { "type": "string" }
                          },
                          "type": "object"
                        },
                        "type": "array"
                      },
                      "message": { "type": "string" },
                      "query": { "type": "string" },
                      "truncated": { "type": "boolean" }
                    },
                    "type": "object"
                  }
                },
                "properties": {
                  "action": { "type": "string" },
                  "analysis": { "$ref": "#/$defs/analysis" },
                  "daemonStatus": {
                    "properties": {
                      "config": {
                        "properties": {
                          "auto_start": { "type": "boolean" },
                          "semantic_auto_reindex_threshold": { "type": "integer" },
                          "semantic_enabled": { "type": "boolean" },
                          "session_dirty_file_threshold": { "type": "integer" },
                          "socket_mode": { "type": "string" }
                        },
                        "type": "object"
                      },
                      "health_reason": { "type": ["string", "null"] },
                      "healthy": { "type": "boolean" },
                      "last_query_at": { "type": ["string", "null"] },
                      "lock_is_held": { "type": "boolean" },
                      "lock_path": { "type": "string" },
                      "pid_is_live": { "type": "boolean" },
                      "pid_path": { "type": "string" },
                      "project_root": { "type": "string" },
                      "recovery_hint": { "type": ["string", "null"] },
                      "semantic_reindex_pending": { "type": "boolean" },
                      "socket_exists": { "type": "boolean" },
                      "socket_path": { "type": "string" },
                      "stale_pid": { "type": "boolean" },
                      "stale_socket": { "type": "boolean" }
                    },
                    "type": "object"
                  },
                  "embeddingUsed": { "type": "boolean" },
                  "enabled": { "type": "boolean" },
                  "indexedFiles": { "type": "integer" },
                  "language": { "type": "string" },
                  "message": { "type": "string" },
                  "matches": {
                    "items": {
                      "properties": {
                        "embedding_score": { "type": ["number", "null"] },
                        "line": { "type": "integer" },
                        "path": { "type": "string" },
                        "snippet": { "type": "string" }
                      },
                      "type": "object"
                    },
                    "type": "array"
                  },
                  "project": { "type": "string" },
                  "query": { "type": "string" },
                  "reindexReport": { "$ref": "#/$defs/reindexReport" },
                  "semantic": { "$ref": "#/$defs/semantic" },
                  "snapshot": {
                    "properties": {
                      "cached_entries": { "type": "integer" },
                      "dirty_file_threshold": { "type": "integer" },
                      "dirty_files": { "type": "integer" },
                      "last_query_at": { "type": ["string", "null"] },
                      "last_reindex": { "$ref": "#/$defs/reindexReport" },
                      "last_reindex_attempt": { "$ref": "#/$defs/reindexReport" },
                      "reindex_pending": { "type": "boolean" }
                    },
                    "type": "object"
                  },
                  "source": { "type": "string" },
                  "status": { "type": "string" },
                  "summary": { "type": "string" },
                  "truncated": { "type": "boolean" }
                },
                "type": "object"
              },
              "title": "Native TLDR"
            }"##,
        )
        .expect("expected tool schema should parse");

        assert_eq!(expected_tool_json, tool_json);
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
