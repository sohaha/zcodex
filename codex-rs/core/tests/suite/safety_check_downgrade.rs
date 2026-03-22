use anyhow::Result;
use codex_core::ModelProviderInfo;
use codex_core::WireApi;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ResponseItem;
use codex_protocol::protocol::AskForApproval;
use codex_protocol::protocol::EventMsg;
use codex_protocol::protocol::ModelRerouteReason;
use codex_protocol::protocol::Op;
use codex_protocol::protocol::SandboxPolicy;
use codex_protocol::user_input::UserInput;
use core_test_support::responses::ev_assistant_message;
use core_test_support::responses::ev_function_call;
use core_test_support::responses::ev_response_created;
use core_test_support::responses::mount_response_once;
use core_test_support::responses::mount_response_sequence;
use core_test_support::responses::sse;
use core_test_support::responses::sse_completed;
use core_test_support::responses::sse_response;
use core_test_support::responses::start_mock_server;
use core_test_support::skip_if_no_network;
use core_test_support::test_codex::test_codex;
use core_test_support::wait_for_event;
use pretty_assertions::assert_eq;
use tokio::time::Duration;
use tokio::time::timeout;

const SERVER_MODEL: &str = "gpt-5.2";
const REQUESTED_MODEL: &str = "gpt-5.3-codex";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn openai_model_header_mismatch_emits_warning_event_and_warning_item() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let response =
        sse_response(sse_completed("resp-1")).insert_header("OpenAI-Model", SERVER_MODEL);
    let _mock = mount_response_once(&server, response).await;

    let mut builder = test_codex().with_model(REQUESTED_MODEL);
    let test = builder.build(&server).await?;

    test.codex
        .submit(Op::UserTurn {
            items: vec![UserInput::Text {
                text: "trigger safety check".to_string(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            cwd: test.cwd_path().to_path_buf(),
            approval_policy: AskForApproval::Never,
            sandbox_policy: SandboxPolicy::DangerFullAccess,
            model: REQUESTED_MODEL.to_string(),
            effort: test.config.model_reasoning_effort,
            summary: None,
            service_tier: None,
            collaboration_mode: None,
            personality: None,
        })
        .await?;

    let reroute = wait_for_event(&test.codex, |event| {
        matches!(event, EventMsg::ModelReroute(_))
    })
    .await;
    let EventMsg::ModelReroute(reroute) = reroute else {
        panic!("expected model reroute event");
    };
    assert_eq!(reroute.from_model, REQUESTED_MODEL);
    assert_eq!(reroute.to_model, SERVER_MODEL);
    assert_eq!(reroute.reason, ModelRerouteReason::HighRiskCyberActivity);

    let warning = wait_for_event(&test.codex, |event| matches!(event, EventMsg::Warning(_))).await;
    let EventMsg::Warning(warning) = warning else {
        panic!("expected warning event");
    };
    assert!(warning.message.contains(REQUESTED_MODEL));
    assert!(warning.message.contains(SERVER_MODEL));

    let warning_item = wait_for_event(&test.codex, |event| {
        matches!(
            event,
            EventMsg::RawResponseItem(raw)
                if matches!(
                    &raw.item,
                    ResponseItem::Message { content, .. }
                        if content.iter().any(|item| matches!(
                            item,
                            ContentItem::InputText { text } if text.starts_with("Warning: ")
                        ))
                )
        )
    })
    .await;
    let EventMsg::RawResponseItem(raw) = warning_item else {
        panic!("expected raw response item event");
    };
    let ResponseItem::Message { role, content, .. } = raw.item else {
        panic!("expected warning to be recorded as a message item");
    };
    assert_eq!(role, "user");
    let warning_text = content.iter().find_map(|item| match item {
        ContentItem::InputText { text } => Some(text.as_str()),
        _ => None,
    });
    let warning_text = warning_text.expect("warning message should include input_text content");
    assert!(warning_text.contains(REQUESTED_MODEL));
    assert!(warning_text.contains(SERVER_MODEL));

    let _ = wait_for_event(&test.codex, |event| {
        matches!(event, EventMsg::TurnComplete(_))
    })
    .await;

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn response_model_field_mismatch_emits_warning_when_header_matches_requested() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let response = sse_response(sse(vec![
        serde_json::json!({
            "type": "response.created",
            "response": {
                "id": "resp-1",
                "headers": {
                    "OpenAI-Model": SERVER_MODEL
                }
            }
        }),
        core_test_support::responses::ev_completed("resp-1"),
    ]))
    .insert_header("OpenAI-Model", REQUESTED_MODEL);
    let _mock = mount_response_once(&server, response).await;

    let mut builder = test_codex().with_model(REQUESTED_MODEL);
    let test = builder.build(&server).await?;

    test.codex
        .submit(Op::UserTurn {
            items: vec![UserInput::Text {
                text: "trigger response model check".to_string(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            cwd: test.cwd_path().to_path_buf(),
            approval_policy: AskForApproval::Never,
            sandbox_policy: SandboxPolicy::DangerFullAccess,
            model: REQUESTED_MODEL.to_string(),
            effort: test.config.model_reasoning_effort,
            summary: None,
            service_tier: None,
            collaboration_mode: None,
            personality: None,
        })
        .await?;

    let reroute = wait_for_event(&test.codex, |event| {
        matches!(event, EventMsg::ModelReroute(_))
    })
    .await;
    let EventMsg::ModelReroute(reroute) = reroute else {
        panic!("expected model reroute event");
    };
    assert_eq!(reroute.from_model, REQUESTED_MODEL);
    assert_eq!(reroute.to_model, SERVER_MODEL);
    assert_eq!(reroute.reason, ModelRerouteReason::HighRiskCyberActivity);

    let warning = wait_for_event(&test.codex, |event| {
        matches!(
            event,
            EventMsg::Warning(warning)
                if warning
                    .message
                    .contains("flagged for potentially high-risk cyber activity")
        )
    })
    .await;
    let EventMsg::Warning(warning) = warning else {
        panic!("expected warning event");
    };
    assert!(warning.message.contains(REQUESTED_MODEL));
    assert!(warning.message.contains(SERVER_MODEL));

    let _ = wait_for_event(&test.codex, |event| {
        matches!(event, EventMsg::TurnComplete(_))
    })
    .await;

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn openai_model_header_mismatch_only_emits_one_warning_per_turn() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let tool_args = serde_json::json!({
        "command": "echo hello",
        "timeout_ms": 1_000
    });

    let first_response = sse_response(sse(vec![
        ev_response_created("resp-1"),
        ev_function_call(
            "call-1",
            "shell_command",
            &serde_json::to_string(&tool_args)?,
        ),
        core_test_support::responses::ev_completed("resp-1"),
    ]))
    .insert_header("OpenAI-Model", SERVER_MODEL);
    let second_response = sse_response(sse(vec![
        ev_response_created("resp-2"),
        ev_assistant_message("msg-1", "done"),
        core_test_support::responses::ev_completed("resp-2"),
    ]))
    .insert_header("OpenAI-Model", SERVER_MODEL);
    let _mock = mount_response_sequence(&server, vec![first_response, second_response]).await;

    let mut builder = test_codex().with_model(REQUESTED_MODEL);
    let test = builder.build(&server).await?;

    test.codex
        .submit(Op::UserTurn {
            items: vec![UserInput::Text {
                text: "trigger follow-up turn".to_string(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            cwd: test.cwd_path().to_path_buf(),
            approval_policy: AskForApproval::Never,
            sandbox_policy: SandboxPolicy::DangerFullAccess,
            model: REQUESTED_MODEL.to_string(),
            effort: test.config.model_reasoning_effort,
            summary: None,
            service_tier: None,
            collaboration_mode: None,
            personality: None,
        })
        .await?;

    let mut warning_count = 0;
    loop {
        let event = wait_for_event(&test.codex, |_| true).await;
        match event {
            EventMsg::Warning(warning) if warning.message.contains(REQUESTED_MODEL) => {
                warning_count += 1;
            }
            EventMsg::TurnComplete(_) => break,
            _ => {}
        }
    }

    assert_eq!(warning_count, 1);

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn custom_provider_model_mismatch_does_not_emit_openai_safety_warning() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let response =
        sse_response(sse_completed("resp-1")).insert_header("OpenAI-Model", SERVER_MODEL);
    let _mock = mount_response_once(&server, response).await;

    let base_url = format!("{}/v1", server.uri());
    let mut builder = test_codex()
        .with_model(REQUESTED_MODEL)
        .with_config(move |config| {
            config.model_provider_id = "relay".to_string();
            config.model_provider = ModelProviderInfo {
                name: "relay".into(),
                base_url: Some(base_url),
                env_key: None,
                env_key_instructions: None,
                experimental_bearer_token: Some("relay-token".into()),
                wire_api: WireApi::Responses,
                query_params: None,
                http_headers: None,
                env_http_headers: None,
                request_max_retries: Some(0),
                stream_max_retries: Some(0),
                stream_idle_timeout_ms: Some(5_000),
                websocket_connect_timeout_ms: None,
                requires_openai_auth: false,
                supports_websockets: false,
            };
        });
    let test = builder.build(&server).await?;

    test.codex
        .submit(Op::UserTurn {
            items: vec![UserInput::Text {
                text: "trigger custom provider model mismatch".to_string(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            cwd: test.cwd_path().to_path_buf(),
            approval_policy: AskForApproval::Never,
            sandbox_policy: SandboxPolicy::DangerFullAccess,
            model: REQUESTED_MODEL.to_string(),
            effort: test.config.model_reasoning_effort,
            summary: None,
            service_tier: None,
            collaboration_mode: None,
            personality: None,
        })
        .await?;

    loop {
        let event = timeout(Duration::from_secs(10), test.codex.next_event())
            .await
            .expect("timeout waiting for event")
            .expect("stream ended unexpectedly")
            .msg;
        match event {
            EventMsg::ModelReroute(_) => {
                panic!("custom provider mismatch should not emit OpenAI reroute warning");
            }
            EventMsg::Warning(warning)
                if warning
                    .message
                    .contains("flagged for potentially high-risk cyber activity") =>
            {
                panic!("custom provider mismatch should not emit OpenAI cyber-safety warning");
            }
            EventMsg::TurnComplete(_) => break,
            _ => {}
        }
    }

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn openai_model_header_casing_only_mismatch_does_not_warn() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let requested_header = REQUESTED_MODEL.to_ascii_uppercase();
    let response = sse_response(sse_completed("resp-1"))
        .insert_header("OpenAI-Model", requested_header.as_str());
    let _mock = mount_response_once(&server, response).await;

    let mut builder = test_codex().with_model(REQUESTED_MODEL);
    let test = builder.build(&server).await?;

    test.codex
        .submit(Op::UserTurn {
            items: vec![UserInput::Text {
                text: "trigger casing check".to_string(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            cwd: test.cwd_path().to_path_buf(),
            approval_policy: AskForApproval::Never,
            sandbox_policy: SandboxPolicy::DangerFullAccess,
            model: REQUESTED_MODEL.to_string(),
            effort: test.config.model_reasoning_effort,
            summary: None,
            service_tier: None,
            collaboration_mode: None,
            personality: None,
        })
        .await?;

    let mut reroute_count = 0;
    let mut warning_count = 0;
    loop {
        let event = wait_for_event(&test.codex, |_| true).await;
        match event {
            EventMsg::ModelReroute(_) => reroute_count += 1,
            EventMsg::Warning(warning)
                if warning
                    .message
                    .contains("flagged for potentially high-risk cyber activity") =>
            {
                warning_count += 1;
            }
            EventMsg::TurnComplete(_) => break,
            _ => {}
        }
    }

    assert_eq!(reroute_count, 0);
    assert_eq!(warning_count, 0);

    Ok(())
}
