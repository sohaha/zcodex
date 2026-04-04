#![cfg(not(target_os = "windows"))]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use anyhow::Result;
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
use codex_core::ZMEMORY_JSON_BEGIN;
use codex_core::ZMEMORY_JSON_END;
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
                    tool.get("parameters")
                        .and_then(|parameters| parameters.get("properties"))
                        .and_then(|properties| properties.get(parameter_name))
                        .and_then(|parameter| parameter.get("description"))
                        .and_then(Value::as_str)
                        .map(str::to_owned)
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
    let uri_description = tool_parameter_description(&body, "zmemory", "uri")
        .expect("zmemory uri description should be present");
    let limit_description = tool_parameter_description(&body, "zmemory", "limit")
        .expect("zmemory limit description should be present");

    assert!(
        uri_description.contains(
            "system://boot|defaults|workspace|index|index/<domain>|recent|recent/<n>|glossary|alias|alias/<n>"
        )
    );
    assert!(uri_description.contains("product defaults"));
    assert!(uri_description.contains("current workspace runtime facts"));
    assert!(limit_description.contains("system://defaults"));
    assert!(limit_description.contains("system://workspace"));
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
        vec!["dbPath", "reason", "source", "workspaceKey"]
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
        "Use \"小白\" for the assistant and \"指挥官\" for the user in future interactions."
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
