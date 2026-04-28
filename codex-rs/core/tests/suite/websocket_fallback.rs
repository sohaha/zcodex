use anyhow::Result;
use codex_model_provider_info::ModelProviderInfo;
use codex_model_provider_info::WireApi;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ResponseItem;
use codex_protocol::protocol::AskForApproval;
use codex_protocol::protocol::EventMsg;
use codex_protocol::protocol::Op;
use codex_protocol::protocol::SandboxPolicy;
use codex_protocol::user_input::UserInput;
use core_test_support::responses;
use core_test_support::responses::ev_completed;
use core_test_support::responses::ev_response_created;
use core_test_support::responses::mount_sse_once;
use core_test_support::responses::mount_sse_sequence;
use core_test_support::responses::sse;
use core_test_support::skip_if_no_network;
use core_test_support::test_codex::TestCodex;
use core_test_support::test_codex::test_codex;
use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;
use tokio::time::Duration;
use tokio::time::timeout;
use wiremock::Mock;
use wiremock::ResponseTemplate;
use wiremock::http::Method;
use wiremock::matchers::method;
use wiremock::matchers::path_regex;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn websocket_fallback_switches_to_http_on_upgrade_required_connect() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = responses::start_mock_server().await;
    Mock::given(method("GET"))
        .and(path_regex(".*/responses$"))
        .respond_with(ResponseTemplate::new(426))
        .mount(&server)
        .await;

    let response_mock = mount_sse_once(
        &server,
        sse(vec![ev_response_created("resp-1"), ev_completed("resp-1")]),
    )
    .await;

    let mut builder = test_codex().with_config({
        let base_url = format!("{}/v1", server.uri());
        move |config| {
            config.model_provider.base_url = Some(base_url);
            config.model_provider.wire_api = WireApi::Responses;
            config.model_provider.supports_websockets = true;
            // If we don't treat 426 specially, the sampling loop would retry the WebSocket
            // handshake before switching to the HTTP transport.
            config.model_provider.stream_max_retries = Some(2);
            config.model_provider.request_max_retries = Some(0);
        }
    });
    let test = builder.build(&server).await?;

    test.submit_turn("hello").await?;

    let requests = server.received_requests().await.unwrap_or_default();
    let websocket_attempts = requests
        .iter()
        .filter(|req| req.method == Method::GET && req.url.path().ends_with("/responses"))
        .count();
    let http_attempts = requests
        .iter()
        .filter(|req| req.method == Method::POST && req.url.path().ends_with("/responses"))
        .count();

    // The startup prewarm request sees 426 and immediately switches the session to HTTP fallback,
    // so the first turn goes straight to HTTP with no additional websocket connect attempt.
    assert_eq!(websocket_attempts, 1);
    assert_eq!(http_attempts, 1);
    assert_eq!(response_mock.requests().len(), 1);

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn websocket_fallback_switches_to_http_after_retries_exhausted() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = responses::start_mock_server().await;
    let response_mock = mount_sse_once(
        &server,
        sse(vec![ev_response_created("resp-1"), ev_completed("resp-1")]),
    )
    .await;

    let mut builder = test_codex().with_config({
        let base_url = format!("{}/v1", server.uri());
        move |config| {
            config.model_provider.base_url = Some(base_url);
            config.model_provider.wire_api = WireApi::Responses;
            config.model_provider.supports_websockets = true;
            config.model_provider.stream_max_retries = Some(2);
            config.model_provider.request_max_retries = Some(0);
        }
    });
    let test = builder.build(&server).await?;

    test.submit_turn("hello").await?;

    let requests = server.received_requests().await.unwrap_or_default();
    let websocket_attempts = requests
        .iter()
        .filter(|req| req.method == Method::GET && req.url.path().ends_with("/responses"))
        .count();
    let http_attempts = requests
        .iter()
        .filter(|req| req.method == Method::POST && req.url.path().ends_with("/responses"))
        .count();

    // Deferred request prewarm is attempted at startup.
    // The first turn then makes 3 websocket stream attempts (initial try + 2 retries),
    // after which fallback activates and the request is replayed over HTTP.
    assert_eq!(websocket_attempts, 4);
    assert_eq!(http_attempts, 1);
    assert_eq!(response_mock.requests().len(), 1);

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn websocket_fallback_hides_first_websocket_retry_stream_error() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = responses::start_mock_server().await;
    let response_mock = mount_sse_once(
        &server,
        sse(vec![ev_response_created("resp-1"), ev_completed("resp-1")]),
    )
    .await;

    let mut builder = test_codex().with_config({
        let base_url = format!("{}/v1", server.uri());
        move |config| {
            config.model_provider.base_url = Some(base_url);
            config.model_provider.wire_api = WireApi::Responses;
            config.model_provider.supports_websockets = true;
            config.model_provider.stream_max_retries = Some(2);
            config.model_provider.request_max_retries = Some(0);
        }
    });
    let TestCodex {
        codex,
        session_configured,
        cwd,
        ..
    } = builder.build(&server).await?;

    codex
        .submit(Op::UserTurn {
            environments: None,
            items: vec![UserInput::Text {
                text: "hello".into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            cwd: cwd.path().to_path_buf(),
            approval_policy: AskForApproval::Never,
            approvals_reviewer: None,
            sandbox_policy: SandboxPolicy::DangerFullAccess,
            permission_profile: None,
            model: session_configured.model.clone(),
            effort: None,
            summary: None,
            service_tier: None,
            collaboration_mode: None,
            personality: None,
        })
        .await?;

    let mut stream_error_messages = Vec::new();
    loop {
        let event = timeout(Duration::from_secs(10), codex.next_event())
            .await
            .expect("timeout waiting for event")
            .expect("event stream ended unexpectedly")
            .msg;
        match event {
            EventMsg::StreamError(e) => stream_error_messages.push(e.message),
            EventMsg::TurnComplete(_) => break,
            _ => {}
        }
    }

    let expected_stream_errors = if cfg!(debug_assertions) {
        vec!["正在重新连接... 1/2", "正在重新连接... 2/2"]
    } else {
        vec!["正在重新连接... 2/2"]
    };
    assert_eq!(stream_error_messages, expected_stream_errors);
    assert_eq!(response_mock.requests().len(), 1);

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn websocket_fallback_is_sticky_across_turns() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = responses::start_mock_server().await;
    let response_mock = mount_sse_sequence(
        &server,
        vec![
            sse(vec![ev_response_created("resp-1"), ev_completed("resp-1")]),
            sse(vec![ev_response_created("resp-2"), ev_completed("resp-2")]),
        ],
    )
    .await;

    let mut builder = test_codex().with_config({
        let base_url = format!("{}/v1", server.uri());
        move |config| {
            config.model_provider.base_url = Some(base_url);
            config.model_provider.wire_api = WireApi::Responses;
            config.model_provider.supports_websockets = true;
            config.model_provider.stream_max_retries = Some(2);
            config.model_provider.request_max_retries = Some(0);
        }
    });
    let test = builder.build(&server).await?;

    test.submit_turn("first").await?;
    test.submit_turn("second").await?;

    let requests = server.received_requests().await.unwrap_or_default();
    let websocket_attempts = requests
        .iter()
        .filter(|req| req.method == Method::GET && req.url.path().ends_with("/responses"))
        .count();
    let http_attempts = requests
        .iter()
        .filter(|req| req.method == Method::POST && req.url.path().ends_with("/responses"))
        .count();

    // WebSocket attempts all happen on the first turn:
    // 1 deferred request prewarm attempt (startup) + 3 stream attempts
    // (initial try + 2 retries) before fallback.
    // Fallback is sticky, so the second turn stays on HTTP and adds no websocket attempts.
    assert_eq!(websocket_attempts, 4);
    assert_eq!(http_attempts, 2);
    assert_eq!(response_mock.requests().len(), 2);

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn request_fallback_switches_to_configured_provider_and_model() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let primary_server = responses::start_mock_server().await;
    Mock::given(method("POST"))
        .and(path_regex(".*/responses$"))
        .respond_with(ResponseTemplate::new(500).set_body_string("primary failed"))
        .mount(&primary_server)
        .await;

    let fallback_server = responses::start_mock_server().await;
    let fallback_mock = mount_sse_once(
        &fallback_server,
        sse(vec![ev_response_created("resp-1"), ev_completed("resp-1")]),
    )
    .await;

    let mut builder = test_codex().with_config({
        let fallback_base_url = format!("{}/v1", fallback_server.uri());
        move |config| {
            config.model_provider.request_max_retries = Some(0);
            config.model_provider.stream_max_retries = Some(0);
            config.model_provider.supports_websockets = false;
            config.fallback_provider_id = Some("fallback".to_string());
            config.fallback_provider = Some(ModelProviderInfo {
                name: Some("fallback".to_string()),
                model: None,
                base_url: Some(fallback_base_url),
                env_key: None,
                model_catalog: None,
                env_key_instructions: None,
                experimental_bearer_token: None,
                auth: None,
                aws: None,
                wire_api: WireApi::Responses,
                query_params: None,
                http_headers: None,
                env_http_headers: None,
                request_max_retries: Some(0),
                stream_max_retries: Some(0),
                stream_idle_timeout_ms: None,
                retry_base_delay_ms: None,
                websocket_connect_timeout_ms: None,
                requires_openai_auth: false,
                supports_websockets: false,
                model_context_window: None,
                model_auto_compact_token_limit: None,
                max_output_tokens: None,
                skip_reasoning_popup: false,
                retry_429: true,
            });
            config.fallback_model = Some("fallback-model".to_string());
        }
    });
    let test = builder.build(&primary_server).await?;

    test.submit_turn("hello").await?;

    let primary_requests = primary_server.received_requests().await.unwrap_or_default();
    let primary_http_attempts = primary_requests
        .iter()
        .filter(|req| req.method == Method::POST && req.url.path().ends_with("/responses"))
        .count();
    assert_eq!(primary_http_attempts, 1);

    let fallback_request = fallback_mock.single_request();
    let fallback_body: Value = fallback_request.body_json();
    assert_eq!(fallback_body["model"].as_str(), Some("fallback-model"));

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn request_fallback_handles_primary_usage_limit() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let primary_server = responses::start_mock_server().await;
    Mock::given(method("POST"))
        .and(path_regex(".*/responses$"))
        .respond_with(ResponseTemplate::new(429).set_body_json(json!({
            "error": {
                "type": "usage_limit_reached",
                "message": "limit reached"
            }
        })))
        .mount(&primary_server)
        .await;

    let fallback_server = responses::start_mock_server().await;
    let fallback_mock = mount_sse_once(
        &fallback_server,
        sse(vec![ev_response_created("resp-1"), ev_completed("resp-1")]),
    )
    .await;

    let mut builder = test_codex().with_config({
        let fallback_base_url = format!("{}/v1", fallback_server.uri());
        move |config| {
            config.model_provider.request_max_retries = Some(0);
            config.model_provider.stream_max_retries = Some(0);
            config.model_provider.supports_websockets = false;
            config.fallback_provider_id = Some("fallback".to_string());
            config.fallback_provider = Some(ModelProviderInfo {
                name: Some("fallback".to_string()),
                model: None,
                base_url: Some(fallback_base_url),
                env_key: None,
                model_catalog: None,
                env_key_instructions: None,
                experimental_bearer_token: None,
                auth: None,
                aws: None,
                wire_api: WireApi::Responses,
                query_params: None,
                http_headers: None,
                env_http_headers: None,
                request_max_retries: Some(0),
                stream_max_retries: Some(0),
                stream_idle_timeout_ms: None,
                retry_base_delay_ms: None,
                websocket_connect_timeout_ms: None,
                requires_openai_auth: false,
                supports_websockets: false,
                model_context_window: None,
                model_auto_compact_token_limit: None,
                max_output_tokens: None,
                skip_reasoning_popup: false,
                retry_429: true,
            });
            config.fallback_model = Some("fallback-model".to_string());
        }
    });
    let test = builder.build(&primary_server).await?;

    test.submit_turn("hello").await?;

    let primary_requests = primary_server.received_requests().await.unwrap_or_default();
    let primary_http_attempts = primary_requests
        .iter()
        .filter(|req| req.method == Method::POST && req.url.path().ends_with("/responses"))
        .count();
    assert_eq!(primary_http_attempts, 1);

    let fallback_request = fallback_mock.single_request();
    let fallback_body: Value = fallback_request.body_json();
    assert_eq!(fallback_body["model"].as_str(), Some("fallback-model"));

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn request_fallback_emits_warning_event_without_warning_item() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let primary_server = responses::start_mock_server().await;
    Mock::given(method("POST"))
        .and(path_regex(".*/responses$"))
        .respond_with(ResponseTemplate::new(500).set_body_string("primary failed"))
        .mount(&primary_server)
        .await;

    let fallback_server = responses::start_mock_server().await;
    let fallback_mock = mount_sse_once(
        &fallback_server,
        sse(vec![ev_response_created("resp-1"), ev_completed("resp-1")]),
    )
    .await;

    let mut builder = test_codex().with_config({
        let fallback_base_url = format!("{}/v1", fallback_server.uri());
        move |config| {
            config.model_provider.request_max_retries = Some(0);
            config.model_provider.stream_max_retries = Some(0);
            config.model_provider.supports_websockets = false;
            config.fallback_provider_id = Some("z-ai".to_string());
            config.fallback_provider = Some(ModelProviderInfo {
                name: Some("z-ai".to_string()),
                model: None,
                base_url: Some(fallback_base_url),
                env_key: None,
                model_catalog: None,
                env_key_instructions: None,
                experimental_bearer_token: None,
                auth: None,
                aws: None,
                wire_api: WireApi::Responses,
                query_params: None,
                http_headers: None,
                env_http_headers: None,
                request_max_retries: Some(0),
                stream_max_retries: Some(0),
                stream_idle_timeout_ms: None,
                retry_base_delay_ms: None,
                websocket_connect_timeout_ms: None,
                requires_openai_auth: false,
                supports_websockets: false,
                model_context_window: None,
                model_auto_compact_token_limit: None,
                max_output_tokens: None,
                skip_reasoning_popup: false,
                retry_429: true,
            });
            config.fallback_model = Some("glm4.7".to_string());
        }
    });
    let TestCodex { codex, cwd, .. } = builder.build(&primary_server).await?;

    codex
        .submit(Op::UserTurn {
            environments: None,
            items: vec![UserInput::Text {
                text: "hello".into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            cwd: cwd.path().to_path_buf(),
            approval_policy: AskForApproval::Never,
            approvals_reviewer: None,
            sandbox_policy: SandboxPolicy::DangerFullAccess,
            permission_profile: None,
            model: "gpt-5".to_string(),
            effort: None,
            summary: None,
            service_tier: None,
            collaboration_mode: None,
            personality: None,
        })
        .await?;

    let mut warning_message = None;
    let mut warning_item = None;
    loop {
        let event = timeout(Duration::from_secs(10), codex.next_event())
            .await
            .expect("timeout waiting for event")
            .expect("event stream ended unexpectedly")
            .msg;
        match event {
            EventMsg::Warning(warning) => warning_message = Some(warning.message),
            EventMsg::RawResponseItem(raw) => {
                if let ResponseItem::Message { content, .. } = raw.item
                    && content.iter().any(|item| {
                        matches!(
                            item,
                            ContentItem::InputText { text }
                                if text.contains("此请求已被路由到 z-ai/glm4.7 作为后备方案。")
                        )
                    })
                {
                    warning_item = Some(());
                }
            }
            EventMsg::TurnComplete(_) => break,
            _ => {}
        }
    }

    assert_eq!(
        warning_message.as_deref(),
        Some("此请求已被路由到 z-ai/glm4.7 作为后备方案。")
    );
    assert_eq!(warning_item, None);
    assert_eq!(fallback_mock.requests().len(), 1);

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn custom_tool_rejection_retries_same_provider_before_request_fallback() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let primary_server = responses::start_mock_server().await;
    let primary_mock = mount_sse_sequence(
        &primary_server,
        vec![
            sse(vec![json!({
                "type": "response.failed",
                "response": {
                    "id": "resp-1",
                    "error": {
                        "type": "invalid_request_error",
                        "message": "Failed to deserialize the JSON body into the target type: tools[4].type: unknown variant 'custom', expected 'function'"
                    }
                }
            })]),
            sse(vec![ev_response_created("resp-2"), ev_completed("resp-2")]),
        ],
    )
    .await;

    let fallback_server = responses::start_mock_server().await;
    let fallback_mock = mount_sse_once(
        &fallback_server,
        sse(vec![
            ev_response_created("resp-fallback"),
            ev_completed("resp-fallback"),
        ]),
    )
    .await;

    let mut builder = test_codex().with_config({
        let primary_base_url = format!("{}/v1", primary_server.uri());
        let fallback_base_url = format!("{}/v1", fallback_server.uri());
        move |config| {
            config.include_apply_patch_tool = true;
            config.model_provider.base_url = Some(primary_base_url);
            config.model_provider.wire_api = WireApi::Responses;
            config.model_provider.supports_websockets = false;
            config.model_provider.request_max_retries = Some(0);
            config.model_provider.stream_max_retries = Some(0);
            config.fallback_providers = vec![codex_core::config::FallbackProviderConfig {
                provider_id: "fallback".to_string(),
                provider: ModelProviderInfo {
                    name: Some("fallback".to_string()),
                    model: None,
                    base_url: Some(fallback_base_url),
                    env_key: None,
                    model_catalog: None,
                    env_key_instructions: None,
                    experimental_bearer_token: None,
                    auth: None,
                    aws: None,
                    wire_api: WireApi::Responses,
                    query_params: None,
                    http_headers: None,
                    env_http_headers: None,
                    request_max_retries: Some(0),
                    stream_max_retries: Some(0),
                    stream_idle_timeout_ms: None,
                    retry_base_delay_ms: None,
                    websocket_connect_timeout_ms: None,
                    requires_openai_auth: false,
                    supports_websockets: false,
                    model_context_window: None,
                    model_auto_compact_token_limit: None,
                    max_output_tokens: None,
                    skip_reasoning_popup: false,
                    retry_429: true,
                },
                model: Some("fallback-model".to_string()),
            }];
        }
    });
    let test = builder.build(&primary_server).await?;

    test.submit_turn("hello").await?;

    assert_eq!(primary_mock.requests().len(), 2);
    assert_eq!(fallback_mock.requests().len(), 0);

    let primary_requests = primary_mock.requests();
    let first_body: Value = primary_requests[0].body_json();
    let second_body: Value = primary_requests[1].body_json();
    let first_apply_patch_type = first_body["tools"]
        .as_array()
        .and_then(|tools| {
            tools
                .iter()
                .find(|tool| tool["name"].as_str() == Some("apply_patch"))
        })
        .and_then(|tool| tool["type"].as_str());
    let second_apply_patch_type = second_body["tools"]
        .as_array()
        .and_then(|tools| {
            tools
                .iter()
                .find(|tool| tool["name"].as_str() == Some("apply_patch"))
        })
        .and_then(|tool| tool["type"].as_str());

    assert_eq!(first_apply_patch_type, Some("custom"));
    assert_eq!(second_apply_patch_type, Some("function"));

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn request_fallback_walks_provider_chain_until_success() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let primary_server = responses::start_mock_server().await;
    Mock::given(method("POST"))
        .and(path_regex(".*/responses$"))
        .respond_with(ResponseTemplate::new(500).set_body_string("primary failed"))
        .mount(&primary_server)
        .await;

    let fallback_a_server = responses::start_mock_server().await;
    Mock::given(method("POST"))
        .and(path_regex(".*/responses$"))
        .respond_with(ResponseTemplate::new(502).set_body_string("fallback a failed"))
        .mount(&fallback_a_server)
        .await;

    let fallback_b_server = responses::start_mock_server().await;
    let fallback_b_mock = mount_sse_once(
        &fallback_b_server,
        sse(vec![ev_response_created("resp-1"), ev_completed("resp-1")]),
    )
    .await;

    let mut builder = test_codex().with_config({
        let fallback_a_base_url = format!("{}/v1", fallback_a_server.uri());
        let fallback_b_base_url = format!("{}/v1", fallback_b_server.uri());
        move |config| {
            config.model_provider.request_max_retries = Some(0);
            config.model_provider.stream_max_retries = Some(0);
            config.model_provider.supports_websockets = false;
            config.fallback_providers = vec![
                codex_core::config::FallbackProviderConfig {
                    provider_id: "fallback-a".to_string(),
                    provider: ModelProviderInfo {
                        name: Some("fallback-a".to_string()),
                        model: None,
                        base_url: Some(fallback_a_base_url),
                        env_key: None,
                        model_catalog: None,
                        env_key_instructions: None,
                        experimental_bearer_token: None,
                        auth: None,
                        aws: None,
                        wire_api: WireApi::Responses,
                        query_params: None,
                        http_headers: None,
                        env_http_headers: None,
                        request_max_retries: Some(0),
                        stream_max_retries: Some(0),
                        stream_idle_timeout_ms: None,
                        retry_base_delay_ms: None,
                        websocket_connect_timeout_ms: None,
                        requires_openai_auth: false,
                        supports_websockets: false,
                        model_context_window: None,
                        model_auto_compact_token_limit: None,
                        max_output_tokens: None,
                        skip_reasoning_popup: false,
                        retry_429: true,
                    },
                    model: Some("fallback-model-a".to_string()),
                },
                codex_core::config::FallbackProviderConfig {
                    provider_id: "fallback-b".to_string(),
                    provider: ModelProviderInfo {
                        name: Some("fallback-b".to_string()),
                        model: None,
                        base_url: Some(fallback_b_base_url),
                        env_key: None,
                        model_catalog: None,
                        env_key_instructions: None,
                        experimental_bearer_token: None,
                        auth: None,
                        aws: None,
                        wire_api: WireApi::Responses,
                        query_params: None,
                        http_headers: None,
                        env_http_headers: None,
                        request_max_retries: Some(0),
                        stream_max_retries: Some(0),
                        stream_idle_timeout_ms: None,
                        retry_base_delay_ms: None,
                        websocket_connect_timeout_ms: None,
                        requires_openai_auth: false,
                        supports_websockets: false,
                        model_context_window: None,
                        model_auto_compact_token_limit: None,
                        max_output_tokens: None,
                        skip_reasoning_popup: false,
                        retry_429: true,
                    },
                    model: Some("fallback-model-b".to_string()),
                },
            ];
        }
    });
    let test = builder.build(&primary_server).await?;

    test.submit_turn("hello").await?;

    let primary_http_attempts = primary_server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|req| req.method == Method::POST && req.url.path().ends_with("/responses"))
        .count();
    let fallback_a_http_attempts = fallback_a_server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|req| req.method == Method::POST && req.url.path().ends_with("/responses"))
        .count();
    assert_eq!(primary_http_attempts, 1);
    assert_eq!(fallback_a_http_attempts, 1);

    let fallback_b_request = fallback_b_mock.single_request();
    let fallback_b_body: Value = fallback_b_request.body_json();
    assert_eq!(fallback_b_body["model"].as_str(), Some("fallback-model-b"));

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn request_fallback_chain_preserves_primary_model_for_later_fallbacks() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let primary_server = responses::start_mock_server().await;
    Mock::given(method("POST"))
        .and(path_regex(".*/responses$"))
        .respond_with(ResponseTemplate::new(500).set_body_string("primary failed"))
        .mount(&primary_server)
        .await;

    let fallback_a_server = responses::start_mock_server().await;
    Mock::given(method("POST"))
        .and(path_regex(".*/responses$"))
        .respond_with(ResponseTemplate::new(502).set_body_string("fallback a failed"))
        .mount(&fallback_a_server)
        .await;

    let fallback_b_server = responses::start_mock_server().await;
    let fallback_b_mock = mount_sse_once(
        &fallback_b_server,
        sse(vec![ev_response_created("resp-1"), ev_completed("resp-1")]),
    )
    .await;

    let mut builder = test_codex().with_config({
        let fallback_a_base_url = format!("{}/v1", fallback_a_server.uri());
        let fallback_b_base_url = format!("{}/v1", fallback_b_server.uri());
        move |config| {
            config.model = Some("primary-model".to_string());
            config.model_provider.request_max_retries = Some(0);
            config.model_provider.stream_max_retries = Some(0);
            config.model_provider.supports_websockets = false;
            config.fallback_providers = vec![
                codex_core::config::FallbackProviderConfig {
                    provider_id: "fallback-a".to_string(),
                    provider: ModelProviderInfo {
                        name: Some("fallback-a".to_string()),
                        model: None,
                        base_url: Some(fallback_a_base_url),
                        env_key: None,
                        model_catalog: None,
                        env_key_instructions: None,
                        experimental_bearer_token: None,
                        auth: None,
                        aws: None,
                        wire_api: WireApi::Responses,
                        query_params: None,
                        http_headers: None,
                        env_http_headers: None,
                        request_max_retries: Some(0),
                        stream_max_retries: Some(0),
                        stream_idle_timeout_ms: None,
                        retry_base_delay_ms: None,
                        websocket_connect_timeout_ms: None,
                        requires_openai_auth: false,
                        supports_websockets: false,
                        model_context_window: None,
                        model_auto_compact_token_limit: None,
                        max_output_tokens: None,
                        skip_reasoning_popup: false,
                        retry_429: true,
                    },
                    model: Some("fallback-model-a".to_string()),
                },
                codex_core::config::FallbackProviderConfig {
                    provider_id: "fallback-b".to_string(),
                    provider: ModelProviderInfo {
                        name: Some("fallback-b".to_string()),
                        model: None,
                        base_url: Some(fallback_b_base_url),
                        env_key: None,
                        model_catalog: None,
                        env_key_instructions: None,
                        experimental_bearer_token: None,
                        auth: None,
                        aws: None,
                        wire_api: WireApi::Responses,
                        query_params: None,
                        http_headers: None,
                        env_http_headers: None,
                        request_max_retries: Some(0),
                        stream_max_retries: Some(0),
                        stream_idle_timeout_ms: None,
                        retry_base_delay_ms: None,
                        websocket_connect_timeout_ms: None,
                        requires_openai_auth: false,
                        supports_websockets: false,
                        model_context_window: None,
                        model_auto_compact_token_limit: None,
                        max_output_tokens: None,
                        skip_reasoning_popup: false,
                        retry_429: true,
                    },
                    model: None,
                },
            ];
        }
    });
    let test = builder.build(&primary_server).await?;

    test.submit_turn("hello").await?;

    let fallback_b_request = fallback_b_mock.single_request();
    let fallback_b_body: Value = fallback_b_request.body_json();
    assert_eq!(fallback_b_body["model"].as_str(), Some("primary-model"));

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn request_fallback_chain_uses_provider_default_model_when_unspecified() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let primary_server = responses::start_mock_server().await;
    Mock::given(method("POST"))
        .and(path_regex(".*/responses$"))
        .respond_with(ResponseTemplate::new(500).set_body_string("primary failed"))
        .mount(&primary_server)
        .await;

    let fallback_server = responses::start_mock_server().await;
    let fallback_mock = mount_sse_once(
        &fallback_server,
        sse(vec![ev_response_created("resp-1"), ev_completed("resp-1")]),
    )
    .await;

    let mut builder = test_codex().with_config({
        let fallback_base_url = format!("{}/v1", fallback_server.uri());
        move |config| {
            config.model = Some("primary-model".to_string());
            config.model_provider.request_max_retries = Some(0);
            config.model_provider.stream_max_retries = Some(0);
            config.model_provider.supports_websockets = false;
            config.fallback_providers = vec![codex_core::config::FallbackProviderConfig {
                provider_id: "fallback".to_string(),
                provider: ModelProviderInfo {
                    name: Some("fallback".to_string()),
                    model: Some("provider-default-model".to_string()),
                    base_url: Some(fallback_base_url),
                    env_key: None,
                    model_catalog: None,
                    env_key_instructions: None,
                    experimental_bearer_token: None,
                    auth: None,
                    aws: None,
                    wire_api: WireApi::Responses,
                    query_params: None,
                    http_headers: None,
                    env_http_headers: None,
                    request_max_retries: Some(0),
                    stream_max_retries: Some(0),
                    stream_idle_timeout_ms: None,
                    retry_base_delay_ms: None,
                    websocket_connect_timeout_ms: None,
                    requires_openai_auth: false,
                    supports_websockets: false,
                    model_context_window: None,
                    model_auto_compact_token_limit: None,
                    max_output_tokens: None,
                    skip_reasoning_popup: false,
                    retry_429: true,
                },
                model: None,
            }];
        }
    });
    let test = builder.build(&primary_server).await?;

    test.submit_turn("hello").await?;

    let fallback_request = fallback_mock.single_request();
    let fallback_body: Value = fallback_request.body_json();
    assert_eq!(
        fallback_body["model"].as_str(),
        Some("provider-default-model")
    );

    Ok(())
}
