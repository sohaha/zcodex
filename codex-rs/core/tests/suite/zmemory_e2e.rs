#![cfg(not(target_os = "windows"))]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use anyhow::Result;
use codex_features::Feature;
use codex_zmemory::tool_api::ZmemoryToolAction;
use codex_zmemory::tool_api::ZmemoryToolCallParam;
use codex_zmemory::tool_api::run_zmemory_tool;
use core_test_support::responses::ev_assistant_message;
use core_test_support::responses::ev_completed;
use core_test_support::responses::ev_function_call;
use core_test_support::responses::ev_response_created;
use core_test_support::responses::mount_sse_once;
use core_test_support::responses::sse;
use core_test_support::responses::start_mock_server;
use core_test_support::skip_if_no_network;
use core_test_support::test_codex::test_codex;
use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;

const ZMEMORY_JSON_BEGIN: &str = "---BEGIN_ZMEMORY_JSON---";
const ZMEMORY_JSON_END: &str = "---END_ZMEMORY_JSON---";

fn extract_zmemory_json_block(text: &str) -> Value {
    let (_, json_and_suffix) = text
        .split_once(&format!("\n{ZMEMORY_JSON_BEGIN}\n"))
        .expect("zmemory output should include a begin marker on its own line");
    let json = json_and_suffix
        .strip_suffix(&format!("\n{ZMEMORY_JSON_END}"))
        .expect("zmemory output should include the closing marker");
    serde_json::from_str(json).expect("zmemory json block should parse")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn zmemory_function_output_exposes_bounded_json_and_persists_memory() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let mut builder = test_codex().with_config(|config| {
        config
            .features
            .enable(Feature::MemoryTool)
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

    let read_back = run_zmemory_tool(
        test.home.path(),
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
async fn zmemory_function_create_accepts_parent_uri_and_title() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let mut builder = test_codex().with_config(|config| {
        config
            .features
            .enable(Feature::MemoryTool)
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

    let read_back = run_zmemory_tool(
        test.home.path(),
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
            .enable(Feature::MemoryTool)
            .expect("test config should allow feature update");
    });
    let test = builder.build(&server).await?;

    run_zmemory_tool(
        test.home.path(),
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
            .enable(Feature::MemoryTool)
            .expect("test config should allow feature update");
    });
    let test = builder.build(&server).await?;

    run_zmemory_tool(
        test.home.path(),
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
async fn zmemory_function_boot_view_reports_missing_configured_anchors() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let mut builder = test_codex().with_config(|config| {
        config
            .features
            .enable(Feature::MemoryTool)
            .expect("test config should allow feature update");
    });
    let test = builder.build(&server).await?;

    run_zmemory_tool(
        test.home.path(),
        ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://agent".to_string()),
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
    assert_eq!(payload["result"]["view"]["entryCount"], 1);
    assert_eq!(
        payload["result"]["view"]["entries"][0]["uri"],
        "core://agent"
    );
    assert_eq!(
        payload["result"]["view"]["missingUris"][0],
        "core://my_user"
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
            .enable(Feature::MemoryTool)
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
