#![cfg(not(target_os = "windows"))]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use anyhow::Result;
use codex_core::ZMEMORY_JSON_BEGIN;
use codex_core::ZMEMORY_JSON_END;
use codex_core::config::types::ZmemoryConfig;
use codex_core::config::types::ZmemoryToml;
use codex_features::Feature;
use codex_protocol::protocol::AskForApproval;
use codex_protocol::protocol::EventMsg;
use codex_protocol::protocol::Op;
use codex_protocol::protocol::SandboxPolicy;
use codex_protocol::user_input::UserInput;
use codex_zmemory::tool_api::ZmemoryToolAction;
use codex_zmemory::tool_api::ZmemoryToolCallParam;
use codex_zmemory::tool_api::run_zmemory_tool_with_context;
use core_test_support::responses::ev_assistant_message;
use core_test_support::responses::ev_completed;
use core_test_support::responses::ev_function_call;
use core_test_support::responses::ev_response_created;
use core_test_support::responses::mount_sse_once;
use core_test_support::responses::sse;
use core_test_support::responses::start_mock_server;
use core_test_support::skip_if_no_network;
use core_test_support::test_codex::test_codex;
use core_test_support::wait_for_event;
use core_test_support::wait_for_event_match;
use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;
use std::fs;
use std::sync::Arc;
use tempfile::TempDir;

fn extract_zmemory_json_block(text: &str) -> Value {
    let (_, json_and_suffix) = text
        .split_once(&format!("\n{ZMEMORY_JSON_BEGIN}\n"))
        .expect("zmemory output should include a begin marker on its own line");
    let json = json_and_suffix
        .strip_suffix(&format!("\n{ZMEMORY_JSON_END}"))
        .expect("zmemory output should include the closing marker");
    serde_json::from_str(json).expect("zmemory json block should parse")
}

fn sorted_object_keys(value: &Value) -> Vec<&str> {
    let mut keys = value
        .as_object()
        .expect("value should be an object")
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();
    keys.sort_unstable();
    keys
}

fn tool_parameter_description(
    body: &Value,
    tool_name: &str,
    parameter_name: &str,
) -> Option<String> {
    body.get("tools")
        .and_then(Value::as_array)
        .and_then(|tools| {
            tools.iter().find_map(|tool| {
                if tool.get("name").and_then(Value::as_str) == Some(tool_name) {
                    let parameters = tool.get("parameters")?;
                    parameters
                        .get("properties")
                        .into_iter()
                        .chain(
                            parameters
                                .get("oneOf")
                                .and_then(Value::as_array)
                                .into_iter()
                                .flatten()
                                .filter_map(|variant| variant.get("properties")),
                        )
                        .find_map(|properties| {
                            properties
                                .get(parameter_name)
                                .and_then(|parameter| parameter.get("description"))
                                .and_then(Value::as_str)
                                .map(str::to_owned)
                        })
                } else {
                    None
                }
            })
        })
}

fn tool_names(body: &Value) -> Vec<String> {
    body.get("tools")
        .and_then(Value::as_array)
        .map(|tools| {
            tools
                .iter()
                .filter_map(|tool| tool.get("name").and_then(Value::as_str))
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn zmemory_function_output_exposes_bounded_json_and_persists_memory() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let mut builder = test_codex().with_config(|config| {
        config
            .features
            .enable(Feature::Zmemory)
            .expect("test config should allow feature update");
    });
    let test = builder.build(&server).await?;

    mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-1"),
            ev_function_call(
                "call-1",
                "zmemory",
                &serde_json::to_string(&json!({
                    "action": "create",
                    "uri": "core://agent-profile",
                    "content": "Salem profile memory",
                    "priority": 7
                }))?,
            ),
            ev_completed("resp-1"),
        ]),
    )
    .await;
    let follow_up = mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-1", "done"),
            ev_completed("resp-2"),
        ]),
    )
    .await;

    test.submit_turn("store a long-term memory").await?;

    let output = follow_up
        .single_request()
        .function_call_output_text("call-1")
        .expect("function tool output should be present");
    assert!(output.contains("created core://agent-profile"));
    assert!(output.contains(ZMEMORY_JSON_BEGIN));
    assert!(output.contains(ZMEMORY_JSON_END));

    let payload = extract_zmemory_json_block(&output);
    assert_eq!(payload["action"], "create");
    assert_eq!(payload["result"]["uri"], "core://agent-profile");
    assert_eq!(payload["result"]["priority"], 7);
    let read_back = run_zmemory_tool_with_context(
        test.home.path(),
        test.cwd_path(),
        None,
        None,
        ZmemoryToolCallParam {
            action: ZmemoryToolAction::Read,
            uri: Some("core://agent-profile".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )?;
    assert_eq!(
        read_back.structured_content["result"]["content"],
        "Salem profile memory"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn zmemory_tool_request_documents_defaults_and_workspace_views() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let resp_mock = mount_sse_once(
        &server,
        sse(vec![ev_response_created("resp-1"), ev_completed("resp-1")]),
    )
    .await;
    let mut builder = test_codex().with_config(|config| {
        config
            .features
            .enable(Feature::Zmemory)
            .expect("test config should allow feature update");
    });
    let test = builder.build(&server).await?;

    test.submit_turn("describe the available tools").await?;

    let body = resp_mock.single_request().body_json();
    let tool_names = tool_names(&body);
    let uri_description = tool_parameter_description(&body, "read_memory", "uri")
        .expect("read_memory uri description should be present");

    assert!(uri_description.contains("core://agent"));
    assert!(uri_description.contains("system://boot"));
    assert!(uri_description.contains("system://paths"));
    assert!(tool_names.contains(&"read_memory".to_string()));
    assert!(tool_names.contains(&"search_memory".to_string()));

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn zmemory_function_stats_exposes_strict_path_resolution_shape() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let mut builder = test_codex().with_config(|config| {
        config
            .features
            .enable(Feature::Zmemory)
            .expect("test config should allow feature update");
    });
    let test = builder.build(&server).await?;

    mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-1"),
            ev_function_call(
                "call-stats",
                "zmemory",
                &serde_json::to_string(&json!({
                    "action": "stats"
                }))?,
            ),
            ev_completed("resp-1"),
        ]),
    )
    .await;
    let follow_up = mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-1", "done"),
            ev_completed("resp-2"),
        ]),
    )
    .await;

    test.submit_turn("inspect memory stats").await?;

    let output = follow_up
        .single_request()
        .function_call_output_text("call-stats")
        .expect("function tool output should be present");
    let payload = extract_zmemory_json_block(&output);
    assert_eq!(payload["action"], "stats");
    assert_eq!(
        sorted_object_keys(&payload["result"]["pathResolution"]),
        vec![
            "dbPath",
            "namespace",
            "namespaceSource",
            "reason",
            "source",
            "supportsNamespaceSelection",
            "workspaceKey",
        ]
    );
    assert_eq!(
        payload["result"]["dbPath"],
        payload["result"]["pathResolution"]["dbPath"]
    );
    assert_eq!(
        payload["result"]["reason"],
        payload["result"]["pathResolution"]["reason"]
    );
    assert_eq!(
        payload["result"]["pathResolution"]["source"],
        "projectScoped"
    );
    assert_ne!(
        payload["result"]["pathResolution"]["workspaceKey"],
        Value::Null
    );
    assert_eq!(payload["result"]["pathResolution"]["namespace"], "");
    assert_eq!(
        payload["result"]["pathResolution"]["namespaceSource"],
        "implicitDefault"
    );
    assert_eq!(
        payload["result"]["pathResolution"]["supportsNamespaceSelection"],
        true
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn zmemory_function_audit_exposes_recent_entries() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let mut builder = test_codex().with_config(|config| {
        config
            .features
            .enable(Feature::Zmemory)
            .expect("test config should allow feature update");
    });
    let test = builder.build(&server).await?;

    run_zmemory_tool_with_context(
        test.home.path(),
        test.cwd_path(),
        None,
        None,
        ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://audit-entry".to_string()),
            content: Some("Initial audit content".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )?;
    run_zmemory_tool_with_context(
        test.home.path(),
        test.cwd_path(),
        None,
        None,
        ZmemoryToolCallParam {
            action: ZmemoryToolAction::Update,
            uri: Some("core://audit-entry".to_string()),
            append: Some(" updated".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )?;
    run_zmemory_tool_with_context(
        test.home.path(),
        test.cwd_path(),
        None,
        None,
        ZmemoryToolCallParam {
            action: ZmemoryToolAction::AddAlias,
            new_uri: Some("core://audit-entry-alias".to_string()),
            target_uri: Some("core://audit-entry".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )?;

    mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-1"),
            ev_function_call(
                "call-audit",
                "zmemory",
                &serde_json::to_string(&json!({
                    "action": "audit",
                    "limit": 2
                }))?,
            ),
            ev_completed("resp-1"),
        ]),
    )
    .await;
    let follow_up = mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-1", "done"),
            ev_completed("resp-2"),
        ]),
    )
    .await;

    test.submit_turn("inspect recent memory audit entries")
        .await?;

    let output = follow_up
        .single_request()
        .function_call_output_text("call-audit")
        .expect("function tool output should be present");
    assert!(output.contains("audit: 2 entries"));
    assert!(output.contains(ZMEMORY_JSON_BEGIN));
    assert!(output.contains(ZMEMORY_JSON_END));

    let payload = extract_zmemory_json_block(&output);
    assert_eq!(payload["action"], "audit");
    assert_eq!(payload["result"]["count"], 2);
    assert_eq!(payload["result"]["limit"], 2);
    let entries = payload["result"]["entries"]
        .as_array()
        .expect("entries should be an array");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0]["action"], "add-alias");
    assert_eq!(entries[0]["uri"], "core://audit-entry-alias");
    assert_eq!(entries[1]["action"], "update");
    assert_eq!(entries[1]["uri"], "core://audit-entry");
    assert!(entries[0]["details"].is_object());
    assert!(entries[0]["createdAt"].is_string());

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn zmemory_function_history_exposes_version_chain() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let mut builder = test_codex().with_config(|config| {
        config
            .features
            .enable(Feature::Zmemory)
            .expect("test config should allow feature update");
    });
    let test = builder.build(&server).await?;

    run_zmemory_tool_with_context(
        test.home.path(),
        test.cwd_path(),
        None,
        None,
        ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://history-entry".to_string()),
            content: Some("Initial version".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )?;
    run_zmemory_tool_with_context(
        test.home.path(),
        test.cwd_path(),
        None,
        None,
        ZmemoryToolCallParam {
            action: ZmemoryToolAction::Update,
            uri: Some("core://history-entry".to_string()),
            append: Some(" updated".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )?;

    mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-1"),
            ev_function_call(
                "call-history",
                "zmemory",
                &serde_json::to_string(&json!({
                    "action": "history",
                    "uri": "core://history-entry"
                }))?,
            ),
            ev_completed("resp-1"),
        ]),
    )
    .await;
    let follow_up = mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-1", "done"),
            ev_completed("resp-2"),
        ]),
    )
    .await;

    test.submit_turn("inspect memory history").await?;

    let output = follow_up
        .single_request()
        .function_call_output_text("call-history")
        .expect("function tool output should be present");
    assert!(output.contains("history core://history-entry: 2 versions"));

    let payload = extract_zmemory_json_block(&output);
    assert_eq!(payload["action"], "history");
    assert_eq!(payload["result"]["uri"], "core://history-entry");
    let versions = payload["result"]["versions"]
        .as_array()
        .expect("versions should be an array");
    assert_eq!(versions.len(), 2);
    assert_eq!(versions[0]["content"], "Initial version updated");
    assert_eq!(versions[0]["deprecated"], false);
    assert_eq!(versions[1]["content"], "Initial version");
    assert_eq!(versions[1]["deprecated"], true);

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn zmemory_function_batch_create_returns_results() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let mut builder = test_codex().with_config(|config| {
        config
            .features
            .enable(Feature::Zmemory)
            .expect("test config should allow feature update");
    });
    let test = builder.build(&server).await?;

    let items = vec![
        json!({
            "parentUri": "core://",
            "title": "batch-user-1",
            "content": "batch entry one",
            "priority": 5
        }),
        json!({
            "parentUri": "core://",
            "title": "batch-user-2",
            "content": "batch entry two",
            "priority": 6
        }),
    ];

    mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-1"),
            ev_function_call(
                "call-batch-create",
                "zmemory",
                &serde_json::to_string(&json!({
                    "action": "batch-create",
                    "items": items,
                }))?,
            ),
            ev_completed("resp-1"),
        ]),
    )
    .await;

    let follow_up = mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-1", "done"),
            ev_completed("resp-2"),
        ]),
    )
    .await;

    test.submit_turn("batch create two memories").await?;

    let output = follow_up
        .single_request()
        .function_call_output_text("call-batch-create")
        .expect("function tool output should be present");
    assert!(output.contains("batch created 2 memories"));
    assert!(output.contains(ZMEMORY_JSON_BEGIN));
    assert!(output.contains(ZMEMORY_JSON_END));

    let payload = extract_zmemory_json_block(&output);
    assert_eq!(payload["action"], "batch-create");
    assert_eq!(payload["result"]["count"], 2);
    let entries = payload["result"]["results"]
        .as_array()
        .expect("results should be an array");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0]["uri"], "core://batch-user-1");
    assert_eq!(entries[1]["uri"], "core://batch-user-2");

    let read_back = run_zmemory_tool_with_context(
        test.home.path(),
        test.cwd_path(),
        None,
        None,
        ZmemoryToolCallParam {
            action: ZmemoryToolAction::Read,
            uri: Some("core://batch-user-2".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )?;
    assert_eq!(
        read_back.structured_content["result"]["content"],
        "batch entry two"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn zmemory_function_create_accepts_parent_uri_and_title() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let mut builder = test_codex().with_config(|config| {
        config
            .features
            .enable(Feature::Zmemory)
            .expect("test config should allow feature update");
    });
    let test = builder.build(&server).await?;

    mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-1"),
            ev_function_call(
                "call-parent",
                "zmemory",
                &serde_json::to_string(&json!({
                    "action": "create",
                    "parentUri": "core://",
                    "title": "team-salem",
                    "content": "Parent-based memory",
                    "priority": 3
                }))?,
            ),
            ev_completed("resp-1"),
        ]),
    )
    .await;

    let follow_up = mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-1", "done"),
            ev_completed("resp-2"),
        ]),
    )
    .await;

    test.submit_turn("store memory via parent path").await?;

    let output = follow_up
        .single_request()
        .function_call_output_text("call-parent")
        .expect("function tool output should be present");
    assert!(
        output.contains(ZMEMORY_JSON_BEGIN),
        "expected bounded JSON output, got: {output}"
    );
    let payload = extract_zmemory_json_block(&output);
    assert_eq!(payload["action"], "create");
    assert_eq!(payload["result"]["uri"], "core://team-salem");
    assert!(payload["result"]["documentCount"].is_i64());

    let read_back = run_zmemory_tool_with_context(
        test.home.path(),
        test.cwd_path(),
        None,
        None,
        ZmemoryToolCallParam {
            action: ZmemoryToolAction::Read,
            uri: Some("core://team-salem".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )?;
    assert_eq!(
        read_back.structured_content["result"]["content"],
        "Parent-based memory"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn zmemory_function_search_returns_bounded_json_for_existing_memory() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let mut builder = test_codex().with_config(|config| {
        config
            .features
            .enable(Feature::Zmemory)
            .expect("test config should allow feature update");
    });
    let test = builder.build(&server).await?;

    run_zmemory_tool_with_context(
        test.home.path(),
        test.cwd_path(),
        None,
        None,
        ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://agent-profile".to_string()),
            content: Some("Salem profile memory".to_string()),
            priority: Some(6),
            ..ZmemoryToolCallParam::default()
        },
    )?;

    mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-1"),
            ev_function_call(
                "call-1",
                "zmemory",
                &serde_json::to_string(&json!({
                    "action": "search",
                    "query": "profile",
                    "limit": 5
                }))?,
            ),
            ev_completed("resp-1"),
        ]),
    )
    .await;
    let follow_up = mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-1", "done"),
            ev_completed("resp-2"),
        ]),
    )
    .await;

    test.submit_turn("search long-term memory").await?;

    let output = follow_up
        .single_request()
        .function_call_output_text("call-1")
        .expect("function tool output should be present");
    assert!(output.contains("search profile: 1 matches"));
    assert!(output.contains(ZMEMORY_JSON_BEGIN));
    assert!(output.contains(ZMEMORY_JSON_END));

    let payload = extract_zmemory_json_block(&output);
    assert_eq!(payload["action"], "search");
    assert_eq!(payload["result"]["matchCount"], 1);
    assert_eq!(
        payload["result"]["matches"][0]["uri"],
        "core://agent-profile"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn zmemory_function_read_supports_export_style_system_views() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let mut builder = test_codex().with_config(|config| {
        config
            .features
            .enable(Feature::Zmemory)
            .expect("test config should allow feature update");
    });
    let test = builder.build(&server).await?;

    run_zmemory_tool_with_context(
        test.home.path(),
        test.cwd_path(),
        None,
        None,
        ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://agent".to_string()),
            content: Some("Salem profile memory".to_string()),
            priority: Some(6),
            ..ZmemoryToolCallParam::default()
        },
    )?;

    mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-1"),
            ev_function_call(
                "call-export",
                "zmemory",
                &serde_json::to_string(&json!({
                    "action": "read",
                    "uri": "system://index/core",
                    "limit": 5
                }))?,
            ),
            ev_completed("resp-1"),
        ]),
    )
    .await;
    let follow_up = mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-1", "done"),
            ev_completed("resp-2"),
        ]),
    )
    .await;

    test.submit_turn("export memory index").await?;

    let output = follow_up
        .single_request()
        .function_call_output_text("call-export")
        .expect("function tool output should be present");
    let payload = extract_zmemory_json_block(&output);
    assert_eq!(payload["action"], "read");
    assert_eq!(payload["result"]["uri"], "system://index/core");
    assert_eq!(payload["result"]["view"]["view"], "index");
    assert_eq!(payload["result"]["view"]["domain"], "core");
    assert_eq!(payload["result"]["view"]["entryCount"], 1);

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn zmemory_mcp_read_memory_tool_maps_to_read_action() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let mut builder = test_codex().with_config(|config| {
        config
            .features
            .enable(Feature::Zmemory)
            .expect("test config should allow feature update");
    });
    let test = builder.build(&server).await?;

    mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-1"),
            ev_function_call(
                "call-read",
                "read_memory",
                &serde_json::to_string(&json!({
                    "uri": "system://defaults"
                }))?,
            ),
            ev_completed("resp-1"),
        ]),
    )
    .await;
    let follow_up = mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-1", "done"),
            ev_completed("resp-2"),
        ]),
    )
    .await;

    test.submit_turn("read defaults").await?;

    let output = follow_up
        .single_request()
        .function_call_output_text("call-read")
        .expect("function tool output should be present");
    let payload = extract_zmemory_json_block(&output);
    assert_eq!(payload["action"], "read");
    assert_eq!(payload["result"]["uri"], "system://defaults");

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn zmemory_mcp_read_memory_tool_supports_paths_view() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let mut builder = test_codex().with_config(|config| {
        config
            .features
            .enable(Feature::Zmemory)
            .expect("test config should allow feature update");
    });
    let test = builder.build(&server).await?;

    run_zmemory_tool_with_context(
        test.home.path(),
        test.cwd_path(),
        None,
        None,
        ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://agent".to_string()),
            content: Some("Agent memory".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )?;

    mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-1"),
            ev_function_call(
                "call-read-paths",
                "read_memory",
                &serde_json::to_string(&json!({
                    "uri": "system://paths"
                }))?,
            ),
            ev_completed("resp-1"),
        ]),
    )
    .await;
    let follow_up = mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-1", "done"),
            ev_completed("resp-2"),
        ]),
    )
    .await;

    test.submit_turn("read paths").await?;

    let output = follow_up
        .single_request()
        .function_call_output_text("call-read-paths")
        .expect("function tool output should be present");
    let payload = extract_zmemory_json_block(&output);
    assert_eq!(payload["action"], "read");
    assert_eq!(payload["result"]["uri"], "system://paths");
    assert_eq!(payload["result"]["view"]["view"], "paths");
    assert_eq!(payload["result"]["view"]["entryCount"], 1);

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn zmemory_mcp_create_memory_tool_maps_to_create_action() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let mut builder = test_codex().with_config(|config| {
        config
            .features
            .enable(Feature::Zmemory)
            .expect("test config should allow feature update");
    });
    let test = builder.build(&server).await?;

    mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-1"),
            ev_function_call(
                "call-create-memory",
                "create_memory",
                &serde_json::to_string(&json!({
                    "parent_uri": "core://",
                    "title": "agent-profile",
                    "content": "Created via MCP alias",
                    "priority": 4
                }))?,
            ),
            ev_completed("resp-1"),
        ]),
    )
    .await;
    let follow_up = mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-1", "done"),
            ev_completed("resp-2"),
        ]),
    )
    .await;

    test.submit_turn("create memory through MCP alias").await?;

    let output = follow_up
        .single_request()
        .function_call_output_text("call-create-memory")
        .expect("function tool output should be present");
    let payload = extract_zmemory_json_block(&output);
    assert_eq!(payload["action"], "create");
    assert_eq!(payload["result"]["uri"], "core://agent-profile");

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn zmemory_mcp_update_memory_tool_maps_to_update_action() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let mut builder = test_codex().with_config(|config| {
        config
            .features
            .enable(Feature::Zmemory)
            .expect("test config should allow feature update");
    });
    let test = builder.build(&server).await?;

    run_zmemory_tool_with_context(
        test.home.path(),
        test.cwd_path(),
        None,
        None,
        ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://agent-profile".to_string()),
            content: Some("Original profile".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )?;

    mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-1"),
            ev_function_call(
                "call-update-memory",
                "update_memory",
                &serde_json::to_string(&json!({
                    "uri": "core://agent-profile",
                    "append": " updated"
                }))?,
            ),
            ev_completed("resp-1"),
        ]),
    )
    .await;
    let follow_up = mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-1", "done"),
            ev_completed("resp-2"),
        ]),
    )
    .await;

    test.submit_turn("update memory through MCP alias").await?;

    let output = follow_up
        .single_request()
        .function_call_output_text("call-update-memory")
        .expect("function tool output should be present");
    let payload = extract_zmemory_json_block(&output);
    assert_eq!(payload["action"], "update");
    assert_eq!(payload["result"]["uri"], "core://agent-profile");

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn zmemory_mcp_search_memory_tool_maps_to_search_action() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let mut builder = test_codex().with_config(|config| {
        config
            .features
            .enable(Feature::Zmemory)
            .expect("test config should allow feature update");
    });
    let test = builder.build(&server).await?;

    run_zmemory_tool_with_context(
        test.home.path(),
        test.cwd_path(),
        None,
        None,
        ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://agent-profile".to_string()),
            content: Some("Searchable profile memory".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )?;

    mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-1"),
            ev_function_call(
                "call-search-memory",
                "search_memory",
                &serde_json::to_string(&json!({
                    "query": "profile"
                }))?,
            ),
            ev_completed("resp-1"),
        ]),
    )
    .await;
    let follow_up = mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-1", "done"),
            ev_completed("resp-2"),
        ]),
    )
    .await;

    test.submit_turn("search memory through MCP alias").await?;

    let output = follow_up
        .single_request()
        .function_call_output_text("call-search-memory")
        .expect("function tool output should be present");
    let payload = extract_zmemory_json_block(&output);
    assert_eq!(payload["action"], "search");
    assert_eq!(payload["result"]["query"], "profile");
    assert_eq!(payload["result"]["matchCount"], 1);

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn zmemory_mcp_search_memory_tool_accepts_uri_scope_and_legacy_domain() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let mut builder = test_codex().with_config(|config| {
        config
            .features
            .enable(Feature::Zmemory)
            .expect("test config should allow feature update");
    });
    let test = builder.build(&server).await?;

    run_zmemory_tool_with_context(
        test.home.path(),
        test.cwd_path(),
        None,
        None,
        ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://team".to_string()),
            content: Some("Team root memory".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )?;
    run_zmemory_tool_with_context(
        test.home.path(),
        test.cwd_path(),
        None,
        None,
        ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://team/profile".to_string()),
            content: Some("Scoped profile memory".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )?;

    mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-1"),
            ev_function_call(
                "call-search-domain",
                "search_memory",
                &serde_json::to_string(&json!({
                    "query": "profile",
                    "domain": "core"
                }))?,
            ),
            ev_function_call(
                "call-search-uri",
                "search_memory",
                &serde_json::to_string(&json!({
                    "query": "profile",
                    "uri": "core://team"
                }))?,
            ),
            ev_function_call(
                "call-search-both",
                "search_memory",
                &serde_json::to_string(&json!({
                    "query": "profile",
                    "uri": "core://team",
                    "domain": "missing"
                }))?,
            ),
            ev_completed("resp-1"),
        ]),
    )
    .await;
    let follow_up = mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-1", "done"),
            ev_completed("resp-2"),
        ]),
    )
    .await;

    test.submit_turn("search scoped memory through MCP alias")
        .await?;

    let request = follow_up.single_request();
    let domain_output = request
        .function_call_output_text("call-search-domain")
        .expect("domain search output should be present");
    let domain_payload = extract_zmemory_json_block(&domain_output);
    assert_eq!(domain_payload["result"]["matchCount"], 1);

    let uri_output = request
        .function_call_output_text("call-search-uri")
        .expect("uri search output should be present");
    let uri_payload = extract_zmemory_json_block(&uri_output);
    assert_eq!(uri_payload["result"]["matchCount"], 1);
    assert_eq!(
        uri_payload["result"]["matches"][0]["uri"],
        "core://team/profile"
    );

    let both_output = request
        .function_call_output_text("call-search-both")
        .expect("combined search output should be present");
    let both_payload = extract_zmemory_json_block(&both_output);
    assert_eq!(both_payload["result"]["matchCount"], 1);
    assert_eq!(
        both_payload["result"]["matches"][0]["uri"],
        "core://team/profile"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn zmemory_mcp_delete_memory_tool_maps_to_delete_path_action() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let mut builder = test_codex().with_config(|config| {
        config
            .features
            .enable(Feature::Zmemory)
            .expect("test config should allow feature update");
    });
    let test = builder.build(&server).await?;

    run_zmemory_tool_with_context(
        test.home.path(),
        test.cwd_path(),
        None,
        None,
        ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://agent-profile".to_string()),
            content: Some("Delete me".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )?;

    mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-1"),
            ev_function_call(
                "call-delete-memory",
                "delete_memory",
                &serde_json::to_string(&json!({
                    "uri": "core://agent-profile"
                }))?,
            ),
            ev_completed("resp-1"),
        ]),
    )
    .await;
    let follow_up = mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-1", "done"),
            ev_completed("resp-2"),
        ]),
    )
    .await;

    test.submit_turn("delete memory through MCP alias").await?;

    let output = follow_up
        .single_request()
        .function_call_output_text("call-delete-memory")
        .expect("function tool output should be present");
    let payload = extract_zmemory_json_block(&output);
    assert_eq!(payload["action"], "delete-path");
    assert_eq!(payload["result"]["uri"], "core://agent-profile");

    let err = run_zmemory_tool_with_context(
        test.home.path(),
        test.cwd_path(),
        None,
        None,
        ZmemoryToolCallParam {
            action: ZmemoryToolAction::Read,
            uri: Some("core://agent-profile".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect_err("deleted memory should not be readable");
    assert_eq!(err.to_string(), "memory not found: core://agent-profile");

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn zmemory_mcp_delete_memory_tool_preserves_other_paths_for_same_node() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let mut builder = test_codex().with_config(|config| {
        config
            .features
            .enable(Feature::Zmemory)
            .expect("test config should allow feature update");
    });
    let test = builder.build(&server).await?;

    run_zmemory_tool_with_context(
        test.home.path(),
        test.cwd_path(),
        None,
        None,
        ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://agent-profile".to_string()),
            content: Some("Delete alias only".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )?;
    run_zmemory_tool_with_context(
        test.home.path(),
        test.cwd_path(),
        None,
        None,
        ZmemoryToolCallParam {
            action: ZmemoryToolAction::AddAlias,
            new_uri: Some("core://profile-mirror".to_string()),
            target_uri: Some("core://agent-profile".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )?;

    mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-1"),
            ev_function_call(
                "call-delete-memory",
                "delete_memory",
                &serde_json::to_string(&json!({
                    "uri": "core://profile-mirror"
                }))?,
            ),
            ev_completed("resp-1"),
        ]),
    )
    .await;
    let follow_up = mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-1", "done"),
            ev_completed("resp-2"),
        ]),
    )
    .await;

    test.submit_turn("delete alias memory through MCP alias")
        .await?;

    let output = follow_up
        .single_request()
        .function_call_output_text("call-delete-memory")
        .expect("function tool output should be present");
    let payload = extract_zmemory_json_block(&output);
    assert_eq!(payload["action"], "delete-path");
    assert_eq!(payload["result"]["uri"], "core://profile-mirror");

    let read = run_zmemory_tool_with_context(
        test.home.path(),
        test.cwd_path(),
        None,
        None,
        ZmemoryToolCallParam {
            action: ZmemoryToolAction::Read,
            uri: Some("core://agent-profile".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )?;
    assert_eq!(
        read.structured_content["result"]["content"],
        "Delete alias only"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn zmemory_function_add_alias_returns_bounded_json() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let mut builder = test_codex().with_config(|config| {
        config
            .features
            .enable(Feature::Zmemory)
            .expect("test config should allow feature update");
    });
    let test = builder.build(&server).await?;

    run_zmemory_tool_with_context(
        test.home.path(),
        test.cwd_path(),
        None,
        None,
        ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://agent-profile".to_string()),
            content: Some("Alias target".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )?;

    mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-1"),
            ev_function_call(
                "call-add-alias",
                "zmemory",
                &serde_json::to_string(&json!({
                    "action": "add-alias",
                    "new_uri": "alias://agent-profile-copy",
                    "target_uri": "core://agent-profile",
                    "priority": 3
                }))?,
            ),
            ev_completed("resp-1"),
        ]),
    )
    .await;
    let follow_up = mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-1", "done"),
            ev_completed("resp-2"),
        ]),
    )
    .await;

    test.submit_turn("add alias through MCP alias").await?;

    let output = follow_up
        .single_request()
        .function_call_output_text("call-add-alias")
        .expect("function tool output should be present");
    let payload = extract_zmemory_json_block(&output);
    assert_eq!(payload["action"], "add-alias");
    assert_eq!(payload["result"]["uri"], "alias://agent-profile-copy");
    assert_eq!(payload["result"]["targetUri"], "core://agent-profile");

    let read_back = run_zmemory_tool_with_context(
        test.home.path(),
        test.cwd_path(),
        None,
        None,
        ZmemoryToolCallParam {
            action: ZmemoryToolAction::Read,
            uri: Some("alias://agent-profile-copy".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )?;
    assert_eq!(
        read_back.structured_content["result"]["content"],
        "Alias target"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn zmemory_mcp_manage_triggers_tool_maps_to_manage_triggers_action() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let mut builder = test_codex().with_config(|config| {
        config
            .features
            .enable(Feature::Zmemory)
            .expect("test config should allow feature update");
    });
    let test = builder.build(&server).await?;

    run_zmemory_tool_with_context(
        test.home.path(),
        test.cwd_path(),
        None,
        None,
        ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://agent-profile".to_string()),
            content: Some("Trigger target".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )?;

    mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-1"),
            ev_function_call(
                "call-manage-triggers",
                "manage_triggers",
                &serde_json::to_string(&json!({
                    "uri": "core://agent-profile",
                    "add": ["profile", "agent"]
                }))?,
            ),
            ev_completed("resp-1"),
        ]),
    )
    .await;
    let follow_up = mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-1", "done"),
            ev_completed("resp-2"),
        ]),
    )
    .await;

    test.submit_turn("manage triggers through MCP alias")
        .await?;

    let output = follow_up
        .single_request()
        .function_call_output_text("call-manage-triggers")
        .expect("function tool output should be present");
    let payload = extract_zmemory_json_block(&output);
    assert_eq!(payload["action"], "manage-triggers");
    assert_eq!(payload["result"]["uri"], "core://agent-profile");
    let mut added = payload["result"]["added"]
        .as_array()
        .expect("added should be an array")
        .iter()
        .map(|value| value.as_str().expect("added keyword should be a string"))
        .collect::<Vec<_>>();
    added.sort_unstable();
    assert_eq!(added, vec!["agent", "profile"]);

    let read_back = run_zmemory_tool_with_context(
        test.home.path(),
        test.cwd_path(),
        None,
        None,
        ZmemoryToolCallParam {
            action: ZmemoryToolAction::Read,
            uri: Some("core://agent-profile".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )?;
    let mut keywords = read_back.structured_content["result"]["keywords"]
        .as_array()
        .expect("keywords should be an array")
        .iter()
        .map(|value| value.as_str().expect("keyword should be a string"))
        .collect::<Vec<_>>();
    keywords.sort_unstable();
    assert_eq!(keywords, vec!["agent", "profile"]);

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn zmemory_function_boot_view_reports_missing_configured_anchors() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let mut builder = test_codex().with_config(|config| {
        config
            .features
            .enable(Feature::Zmemory)
            .expect("test config should allow feature update");
    });
    let test = builder.build(&server).await?;

    run_zmemory_tool_with_context(
        test.home.path(),
        test.cwd_path(),
        None,
        None,
        ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://agent".to_string()),
            content: Some("Agent root".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )?;
    run_zmemory_tool_with_context(
        test.home.path(),
        test.cwd_path(),
        None,
        None,
        ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://agent/coding_operating_manual".to_string()),
            content: Some("Boot anchor".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )?;

    mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-1"),
            ev_function_call(
                "call-boot",
                "zmemory",
                &serde_json::to_string(&json!({
                    "action": "read",
                    "uri": "system://boot",
                    "limit": 5
                }))?,
            ),
            ev_completed("resp-1"),
        ]),
    )
    .await;
    let follow_up = mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-1", "done"),
            ev_completed("resp-2"),
        ]),
    )
    .await;

    test.submit_turn("export boot anchors").await?;

    let output = follow_up
        .single_request()
        .function_call_output_text("call-boot")
        .expect("function tool output should be present");
    let payload = extract_zmemory_json_block(&output);
    assert_eq!(payload["action"], "read");
    assert_eq!(payload["result"]["view"]["view"], "boot");
    assert_eq!(payload["result"]["view"]["bootHealthy"], false);
    assert_eq!(payload["result"]["view"]["entryCount"], 1);
    assert_eq!(
        payload["result"]["view"]["presentUris"][0],
        "core://agent/coding_operating_manual"
    );
    assert_eq!(
        payload["result"]["view"]["bootRoles"][0]["role"],
        "agent_operating_manual"
    );
    assert_eq!(payload["result"]["view"]["missingUriCount"], 2);
    assert_eq!(payload["result"]["view"]["unassignedUris"], json!([]));
    assert_eq!(
        payload["result"]["view"]["entries"][0]["uri"],
        "core://agent/coding_operating_manual"
    );
    assert_eq!(
        payload["result"]["view"]["anchors"][0]["role"],
        "agent_operating_manual"
    );
    assert_eq!(
        payload["result"]["view"]["missingUris"][0],
        "core://my_user/coding_preferences"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn zmemory_function_workspace_view_distinguishes_defaults_from_explicit_runtime() -> Result<()>
{
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let home = Arc::new(TempDir::new()?);
    let explicit_db_path = home.path().join("custom-zmemory").join("memory.db");
    std::fs::create_dir_all(
        explicit_db_path
            .parent()
            .expect("explicit zmemory path should have a parent"),
    )?;
    let mut builder = test_codex().with_home(Arc::clone(&home)).with_config({
        let explicit_db_path = explicit_db_path.clone();
        move |config| {
            config
                .features
                .enable(Feature::Zmemory)
                .expect("test config should allow feature update");
            config.zmemory.path = Some(explicit_db_path);
        }
    });
    let test = builder.build(&server).await?;

    mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-1"),
            ev_function_call(
                "call-workspace",
                "zmemory",
                &serde_json::to_string(&json!({
                    "action": "read",
                    "uri": "system://workspace"
                }))?,
            ),
            ev_completed("resp-1"),
        ]),
    )
    .await;
    let follow_up = mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-1", "done"),
            ev_completed("resp-2"),
        ]),
    )
    .await;

    test.submit_turn("inspect workspace memory runtime facts")
        .await?;

    let output = follow_up
        .single_request()
        .function_call_output_text("call-workspace")
        .expect("function tool output should be present");
    assert!(output.contains("read system://workspace: workspace view"));
    let payload = extract_zmemory_json_block(&output);
    assert_eq!(payload["action"], "read");
    assert_eq!(payload["result"]["uri"], "system://workspace");
    assert_eq!(payload["result"]["view"]["view"], "workspace");
    assert_eq!(payload["result"]["view"]["hasExplicitZmemoryPath"], true);
    assert_eq!(payload["result"]["view"]["source"], "explicit");
    assert_eq!(
        payload["result"]["view"]["workspaceBase"],
        json!(test.cwd_path().display().to_string())
    );
    assert_eq!(payload["result"]["view"]["dbPathDiffers"], true);
    assert_eq!(
        payload["result"]["view"]["defaultDbPath"],
        json!(
            home.path()
                .join("zmemory")
                .join("projects")
                .join(
                    payload["result"]["view"]["defaultWorkspaceKey"]
                        .as_str()
                        .expect("default workspace key")
                )
                .join("zmemory.db")
                .display()
                .to_string()
        )
    );
    assert_ne!(
        payload["result"]["view"]["dbPath"],
        payload["result"]["view"]["defaultDbPath"]
    );
    assert_eq!(
        payload["result"]["view"]["workspaceBase"],
        json!(test.cwd_path().display().to_string())
    );
    assert_eq!(payload["result"]["view"]["boot"]["view"], "boot");
    assert_eq!(payload["result"]["view"]["bootHealthy"], false);

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn zmemory_function_workspace_view_reflects_configured_runtime_profile() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let mut builder = test_codex().with_config(|config| {
        config
            .features
            .enable(Feature::Zmemory)
            .expect("test config should allow feature update");
        config.zmemory = ZmemoryConfig::from_toml(Some(ZmemoryToml {
            path: None,
            valid_domains: Some(vec![
                "core".to_string(),
                "project".to_string(),
                "notes".to_string(),
            ]),
            core_memory_uris: Some(vec![
                "core://agent/coding_operating_manual".to_string(),
                "core://my_user/coding_preferences".to_string(),
                "core://agent/my_user/collaboration_contract".to_string(),
            ]),
            namespace: Some("team-alpha".to_string()),
        }));
    });
    let test = builder.build(&server).await?;

    mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-1"),
            ev_function_call(
                "call-workspace-profile",
                "zmemory",
                &serde_json::to_string(&json!({
                    "action": "read",
                    "uri": "system://workspace"
                }))?,
            ),
            ev_completed("resp-1"),
        ]),
    )
    .await;
    let follow_up = mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-1", "done"),
            ev_completed("resp-2"),
        ]),
    )
    .await;

    test.submit_turn("inspect configured zmemory runtime profile")
        .await?;

    let output = follow_up
        .single_request()
        .function_call_output_text("call-workspace-profile")
        .expect("function tool output should be present");
    let payload = extract_zmemory_json_block(&output);
    assert_eq!(
        payload["result"]["view"]["validDomains"],
        json!(["core", "project", "notes"])
    );
    assert_eq!(
        payload["result"]["view"]["coreMemoryUris"],
        json!([
            "core://agent/coding_operating_manual",
            "core://my_user/coding_preferences",
            "core://agent/my_user/collaboration_contract"
        ])
    );
    assert_eq!(payload["result"]["view"]["namespace"], "team-alpha");
    assert_eq!(payload["result"]["view"]["namespaceSource"], "config");
    assert_eq!(
        payload["result"]["view"]["supportsNamespaceSelection"],
        true
    );
    assert_eq!(
        payload["result"]["view"]["bootRoles"],
        json!([
            {
                "role": "agent_operating_manual",
                "uri": "core://agent/coding_operating_manual",
                "configured": true,
                "description": "The assistant's coding operating manual."
            },
            {
                "role": "user_preferences",
                "uri": "core://my_user/coding_preferences",
                "configured": true,
                "description": "Stable user coding preferences for this runtime profile."
            },
            {
                "role": "collaboration_contract",
                "uri": "core://agent/my_user/collaboration_contract",
                "configured": true,
                "description": "Shared long-term collaboration rules for coding tasks."
            }
        ])
    );
    assert_eq!(payload["result"]["view"]["unassignedUris"], json!([]));

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn zmemory_function_workspace_view_reloads_project_scoped_runtime_profile_after_turn_cwd_override()
-> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let home = Arc::new(TempDir::new()?);
    let global_db_path = home.path().join("global-zmemory").join("memory.db");
    let workspace = TempDir::new()?;
    let nested = workspace.path().join("nested");
    let dot_codex = workspace.path().join(".codex");
    std::fs::create_dir_all(
        global_db_path
            .parent()
            .expect("global zmemory path should have a parent"),
    )?;
    std::fs::create_dir_all(workspace.path().join(".git"))?;
    fs::create_dir_all(&nested)?;
    fs::create_dir_all(&dot_codex)?;
    fs::write(
        home.path().join("config.toml"),
        format!(
            "[zmemory]\npath = \"{}\"\n\n[projects.\"{}\"]\ntrust_level = \"trusted\"\n",
            global_db_path.display(),
            workspace.path().display()
        ),
    )?;
    fs::write(
        dot_codex.join("config.toml"),
        r#"[zmemory]
namespace = "project-alpha"
valid_domains = ["core", "project"]
core_memory_uris = [
  "core://agent/project_manual",
  "core://my_user/project_preferences",
  "core://agent/my_user/project_contract",
]
"#,
    )?;
    let mut builder = test_codex()
        .with_home(Arc::clone(&home))
        .with_config(|config| {
            config
                .features
                .enable(Feature::Zmemory)
                .expect("test config should allow feature update");
        });
    let test = builder.build(&server).await?;

    mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-1"),
            ev_function_call(
                "call-workspace-profile-override",
                "zmemory",
                &serde_json::to_string(&json!({
                    "action": "read",
                    "uri": "system://workspace"
                }))?,
            ),
            ev_completed("resp-1"),
        ]),
    )
    .await;
    let follow_up = mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-1", "done"),
            ev_completed("resp-2"),
        ]),
    )
    .await;

    test.codex
        .submit(Op::OverrideTurnContext {
            cwd: Some(nested.clone()),
            approval_policy: Some(AskForApproval::Never),
            approvals_reviewer: None,
            sandbox_policy: Some(SandboxPolicy::DangerFullAccess),
            windows_sandbox_level: None,
            model: None,
            effort: None,
            summary: None,
            service_tier: None,
            collaboration_mode: None,
            personality: None,
        })
        .await?;
    test.codex
        .submit(Op::UserInput {
            items: vec![UserInput::Text {
                text: "inspect overridden zmemory runtime profile".into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
        })
        .await?;

    let turn_id = wait_for_event_match(&test.codex, |event| match event {
        EventMsg::TurnStarted(event) => Some(event.turn_id.clone()),
        _ => None,
    })
    .await;
    wait_for_event(&test.codex, |event| match event {
        EventMsg::TurnComplete(event) => event.turn_id == turn_id,
        _ => false,
    })
    .await;

    let output = follow_up
        .single_request()
        .function_call_output_text("call-workspace-profile-override")
        .expect("function tool output should be present");
    let payload = extract_zmemory_json_block(&output);
    assert_eq!(
        payload["result"]["view"]["dbPath"],
        json!(global_db_path.display().to_string())
    );
    assert_eq!(payload["result"]["view"]["hasExplicitZmemoryPath"], true);
    assert_eq!(payload["result"]["view"]["source"], "explicit");
    assert_eq!(
        payload["result"]["view"]["workspaceBase"],
        json!(workspace.path().display().to_string())
    );
    assert_eq!(
        payload["result"]["view"]["validDomains"],
        json!(["core", "project"])
    );
    assert_eq!(
        payload["result"]["view"]["coreMemoryUris"],
        json!([
            "core://agent/project_manual",
            "core://my_user/project_preferences",
            "core://agent/my_user/project_contract",
        ])
    );
    assert_eq!(payload["result"]["view"]["namespace"], "project-alpha");
    assert_eq!(payload["result"]["view"]["namespaceSource"], "config");
    assert_eq!(
        payload["result"]["view"]["supportsNamespaceSelection"],
        true
    );
    assert_eq!(
        payload["result"]["view"]["bootRoles"],
        json!([
            {
                "role": "agent_operating_manual",
                "uri": "core://agent/project_manual",
                "configured": true,
                "description": "The assistant's coding operating manual."
            },
            {
                "role": "user_preferences",
                "uri": "core://my_user/project_preferences",
                "configured": true,
                "description": "Stable user coding preferences for this runtime profile."
            },
            {
                "role": "collaboration_contract",
                "uri": "core://agent/my_user/project_contract",
                "configured": true,
                "description": "Shared long-term collaboration rules for coding tasks."
            }
        ])
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn zmemory_function_workspace_view_uses_project_path_after_turn_cwd_override() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let mut builder = test_codex().with_config(|config| {
        config
            .features
            .enable(Feature::Zmemory)
            .expect("test config should allow feature update");
    });
    let test = builder.build(&server).await?;

    let workspace = TempDir::new()?;
    let nested = workspace.path().join("nested");
    let dot_codex = workspace.path().join(".codex");
    let configured_db_path = workspace.path().join(".agents").join("memory.db");
    fs::write(workspace.path().join(".git"), "gitdir: here")?;
    fs::create_dir_all(&nested)?;
    fs::create_dir_all(&dot_codex)?;
    fs::create_dir_all(
        configured_db_path
            .parent()
            .expect("configured zmemory path should have parent"),
    )?;
    fs::write(
        dot_codex.join("config.toml"),
        format!("[zmemory]\npath = \"{}\"\n", configured_db_path.display()),
    )?;
    fs::write(
        test.home.path().join("config.toml"),
        format!(
            "[projects.\"{}\"]\ntrust_level = \"trusted\"\n",
            workspace.path().display()
        ),
    )?;

    mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-1"),
            ev_function_call(
                "call-workspace-override",
                "zmemory",
                &serde_json::to_string(&json!({
                    "action": "read",
                    "uri": "system://workspace"
                }))?,
            ),
            ev_completed("resp-1"),
        ]),
    )
    .await;
    let follow_up = mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-1", "done"),
            ev_completed("resp-2"),
        ]),
    )
    .await;

    test.codex
        .submit(Op::OverrideTurnContext {
            cwd: Some(nested.clone()),
            approval_policy: Some(AskForApproval::Never),
            approvals_reviewer: None,
            sandbox_policy: Some(SandboxPolicy::DangerFullAccess),
            windows_sandbox_level: None,
            model: None,
            effort: None,
            summary: None,
            service_tier: None,
            collaboration_mode: None,
            personality: None,
        })
        .await?;
    test.codex
        .submit(Op::UserInput {
            items: vec![UserInput::Text {
                text: "inspect workspace memory runtime facts".into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
        })
        .await?;

    let turn_id = wait_for_event_match(&test.codex, |event| match event {
        EventMsg::TurnStarted(event) => Some(event.turn_id.clone()),
        _ => None,
    })
    .await;
    wait_for_event(&test.codex, |event| match event {
        EventMsg::TurnComplete(event) => event.turn_id == turn_id,
        _ => false,
    })
    .await;

    let output = follow_up
        .single_request()
        .function_call_output_text("call-workspace-override")
        .expect("function tool output should be present");
    let payload = extract_zmemory_json_block(&output);
    assert_eq!(
        payload["result"]["view"]["dbPath"],
        json!(configured_db_path.display().to_string())
    );
    assert_eq!(payload["result"]["view"]["hasExplicitZmemoryPath"], true);
    assert_eq!(payload["result"]["view"]["source"], "explicit");
    assert_eq!(
        payload["result"]["view"]["workspaceBase"],
        json!(nested.display().to_string())
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn zmemory_function_error_returns_failure_without_json_block() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let mut builder = test_codex().with_config(|config| {
        config
            .features
            .enable(Feature::Zmemory)
            .expect("test config should allow feature update");
    });
    let test = builder.build(&server).await?;

    mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-1"),
            ev_function_call(
                "call-1",
                "zmemory",
                &serde_json::to_string(&json!({
                    "action": "read",
                    "uri": "core://missing"
                }))?,
            ),
            ev_completed("resp-1"),
        ]),
    )
    .await;
    let follow_up = mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-1", "done"),
            ev_completed("resp-2"),
        ]),
    )
    .await;

    test.submit_turn("read a missing memory").await?;

    let output = follow_up
        .single_request()
        .function_call_output_text("call-1")
        .expect("function tool output should be present");
    assert_eq!(output, "memory not found: core://missing");
    assert!(!output.contains(ZMEMORY_JSON_BEGIN));

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn zmemory_proactively_captures_explicit_naming_preferences() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let mut builder = test_codex().with_config(|config| {
        config
            .features
            .enable(Feature::Zmemory)
            .expect("test config should allow feature update");
    });
    let test = builder.build(&server).await?;

    mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-1"),
            ev_assistant_message("msg-1", "收到，指挥官。"),
            ev_completed("resp-1"),
        ]),
    )
    .await;

    test.submit_turn("你现在开始称呼我\"指挥官\",你的名字是\"小白\"")
        .await?;

    let user_memory = run_zmemory_tool_with_context(
        test.home.path(),
        test.cwd_path(),
        None,
        None,
        ZmemoryToolCallParam {
            action: ZmemoryToolAction::Read,
            uri: Some("core://my_user".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )?;
    assert_eq!(
        user_memory.structured_content["result"]["content"],
        "The user prefers to be addressed as \"指挥官\"."
    );

    let agent_memory = run_zmemory_tool_with_context(
        test.home.path(),
        test.cwd_path(),
        None,
        None,
        ZmemoryToolCallParam {
            action: ZmemoryToolAction::Read,
            uri: Some("core://agent".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )?;
    assert_eq!(
        agent_memory.structured_content["result"]["content"],
        "The assistant should refer to itself as \"小白\"."
    );

    let contract_memory = run_zmemory_tool_with_context(
        test.home.path(),
        test.cwd_path(),
        None,
        None,
        ZmemoryToolCallParam {
            action: ZmemoryToolAction::Read,
            uri: Some("core://agent/my_user".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )?;
    assert_eq!(
        contract_memory.structured_content["result"]["content"],
        "Shared collaboration contract:\n- Use \"小白\" for the assistant and \"指挥官\" for the user in future interactions."
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn zmemory_proactively_captures_durable_collaboration_preferences() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let mut builder = test_codex().with_config(|config| {
        config
            .features
            .enable(Feature::Zmemory)
            .expect("test config should allow feature update");
    });
    let test = builder.build(&server).await?;

    mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-1"),
            ev_assistant_message("msg-1", "收到，我之后默认用中文并保持简洁。"),
            ev_completed("resp-1"),
        ]),
    )
    .await;

    test.submit_turn("以后默认用中文回答，尽量简洁一点。")
        .await?;

    let contract_memory = run_zmemory_tool_with_context(
        test.home.path(),
        test.cwd_path(),
        None,
        None,
        ZmemoryToolCallParam {
            action: ZmemoryToolAction::Read,
            uri: Some("core://agent/my_user".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )?;
    assert_eq!(
        contract_memory.structured_content["result"]["content"],
        "Shared collaboration contract:\n- Respond in Chinese by default.\n- Keep responses concise by default."
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn zmemory_proactively_captures_explicit_preferences_even_in_high_load_turn() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let mut builder = test_codex().with_config(|config| {
        config
            .features
            .enable(Feature::Zmemory)
            .expect("test config should allow feature update");
    });
    let test = builder.build(&server).await?;

    mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-1"),
            ev_assistant_message("msg-1", "收到，我先处理这批复杂请求。"),
            ev_completed("resp-1"),
        ]),
    )
    .await;

    let high_load_input = format!(
        "{}{}",
        "以后默认用中文回答，尽量简洁一点。".repeat(80),
        "另外这里还有很多一次性任务细节。".repeat(40)
    );
    test.submit_turn(&high_load_input).await?;

    let read_result = run_zmemory_tool_with_context(
        test.home.path(),
        test.cwd_path(),
        None,
        None,
        ZmemoryToolCallParam {
            action: ZmemoryToolAction::Read,
            uri: Some("core://agent/my_user".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )?;
    assert_eq!(
        read_result.structured_content["result"]["content"],
        "Shared collaboration contract:\n- Respond in Chinese by default.\n- Keep responses concise by default."
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn zmemory_proactive_capture_uses_project_path_after_turn_cwd_override() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let mut builder = test_codex().with_config(|config| {
        config
            .features
            .enable(Feature::Zmemory)
            .expect("test config should allow feature update");
    });
    let test = builder.build(&server).await?;

    let workspace = TempDir::new()?;
    let nested = workspace.path().join("nested");
    let dot_codex = workspace.path().join(".codex");
    let configured_db_path = workspace.path().join(".agents").join("memory.db");
    fs::write(workspace.path().join(".git"), "gitdir: here")?;
    fs::create_dir_all(&nested)?;
    fs::create_dir_all(&dot_codex)?;
    fs::create_dir_all(
        configured_db_path
            .parent()
            .expect("configured zmemory path should have parent"),
    )?;
    fs::write(
        dot_codex.join("config.toml"),
        format!("[zmemory]\npath = \"{}\"\n", configured_db_path.display()),
    )?;
    fs::write(
        test.home.path().join("config.toml"),
        format!(
            "[projects.\"{}\"]\ntrust_level = \"trusted\"\n",
            workspace.path().display()
        ),
    )?;

    mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-1"),
            ev_assistant_message("msg-1", "收到，指挥官。"),
            ev_completed("resp-1"),
        ]),
    )
    .await;

    test.codex
        .submit(Op::OverrideTurnContext {
            cwd: Some(nested.clone()),
            approval_policy: Some(AskForApproval::Never),
            approvals_reviewer: None,
            sandbox_policy: Some(SandboxPolicy::DangerFullAccess),
            windows_sandbox_level: None,
            model: None,
            effort: None,
            summary: None,
            service_tier: None,
            collaboration_mode: None,
            personality: None,
        })
        .await?;
    test.codex
        .submit(Op::UserInput {
            items: vec![UserInput::Text {
                text: "你现在开始称呼我\"指挥官\",你的名字是\"小白\"".into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
        })
        .await?;

    let turn_id = wait_for_event_match(&test.codex, |event| match event {
        EventMsg::TurnStarted(event) => Some(event.turn_id.clone()),
        _ => None,
    })
    .await;
    wait_for_event(&test.codex, |event| match event {
        EventMsg::TurnComplete(event) => event.turn_id == turn_id,
        _ => false,
    })
    .await;

    let user_memory = run_zmemory_tool_with_context(
        test.home.path(),
        nested.as_path(),
        Some(configured_db_path.as_path()),
        None,
        ZmemoryToolCallParam {
            action: ZmemoryToolAction::Read,
            uri: Some("core://my_user".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )?;
    assert_eq!(
        user_memory.structured_content["result"]["content"],
        "The user prefers to be addressed as \"指挥官\"."
    );

    let agent_memory = run_zmemory_tool_with_context(
        test.home.path(),
        nested.as_path(),
        Some(configured_db_path.as_path()),
        None,
        ZmemoryToolCallParam {
            action: ZmemoryToolAction::Read,
            uri: Some("core://agent".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )?;
    assert_eq!(
        agent_memory.structured_content["result"]["content"],
        "The assistant should refer to itself as \"小白\"."
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn zmemory_does_not_proactively_capture_preferences_without_feature_flag() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let mut builder = test_codex().with_config(|config| {
        config
            .features
            .disable(Feature::Zmemory)
            .expect("test config should allow feature update");
    });
    let test = builder.build(&server).await?;

    mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-1"),
            ev_assistant_message("msg-1", "收到。"),
            ev_completed("resp-1"),
        ]),
    )
    .await;

    test.submit_turn("你现在开始称呼我\"指挥官\",你的名字是\"小白\"")
        .await?;

    let read_result = run_zmemory_tool_with_context(
        test.home.path(),
        test.cwd_path(),
        None,
        None,
        ZmemoryToolCallParam {
            action: ZmemoryToolAction::Read,
            uri: Some("core://my_user".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    );
    assert_eq!(
        read_result
            .expect_err("zmemory should not proactively persist preferences without the feature")
            .to_string(),
        "memory not found: core://my_user"
    );

    Ok(())
}
