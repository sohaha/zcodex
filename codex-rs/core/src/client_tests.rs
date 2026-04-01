use super::AuthRequestTelemetryContext;
use super::ModelClient;
use super::PendingUnauthorizedRetry;
use super::UnauthorizedRecoveryExecution;
use crate::auth::CodexAuth;
use crate::client_common::tools::ResponsesApiTool;
use crate::client_common::tools::ToolSpec;
use crate::tools::spec::JsonSchema;
use codex_otel::SessionTelemetry;
use codex_protocol::ThreadId;
use codex_protocol::openai_models::ModelInfo;
use codex_protocol::protocol::SessionSource;
use codex_protocol::protocol::SubAgentSource;
use pretty_assertions::assert_eq;
use serde_json::json;

fn test_model_client(session_source: SessionSource) -> ModelClient {
    test_model_client_with_wire_api(
        session_source,
        crate::model_provider_info::WireApi::Responses,
    )
}

fn test_model_client_with_wire_api(
    session_source: SessionSource,
    wire_api: crate::model_provider_info::WireApi,
) -> ModelClient {
    let provider = crate::model_provider_info::create_oss_provider_with_base_url(
        "https://example.com/v1",
        wire_api,
    );
    ModelClient::new(
        /*auth_manager*/ None,
        ThreadId::new(),
        provider,
        session_source,
        /*model_verbosity*/ None,
        /*enable_request_compression*/ false,
        /*include_timing_metrics*/ false,
        /*beta_features_header*/ None,
    )
}

fn test_model_client_with_provider_and_auth(
    provider: crate::model_provider_info::ModelProviderInfo,
    auth: CodexAuth,
) -> ModelClient {
    ModelClient::new(
        Some(crate::test_support::auth_manager_from_auth(auth)),
        ThreadId::new(),
        provider,
        SessionSource::Cli,
        None,
        true,
        false,
        None,
    )
}

fn test_model_info() -> ModelInfo {
    serde_json::from_value(json!({
        "slug": "gpt-test",
        "display_name": "gpt-test",
        "description": "desc",
        "default_reasoning_level": "medium",
        "supported_reasoning_levels": [
            {"effort": "medium", "description": "medium"}
        ],
        "shell_type": "shell_command",
        "visibility": "list",
        "supported_in_api": true,
        "priority": 1,
        "upgrade": null,
        "base_instructions": "base instructions",
        "model_messages": null,
        "supports_reasoning_summaries": false,
        "support_verbosity": false,
        "default_verbosity": null,
        "apply_patch_tool_type": null,
        "truncation_policy": {"mode": "bytes", "limit": 10000},
        "supports_parallel_tool_calls": false,
        "supports_image_detail_original": false,
        "context_window": 272000,
        "auto_compact_token_limit": null,
        "experimental_supported_tools": []
    }))
    .expect("deserialize test model info")
}

fn test_session_telemetry() -> SessionTelemetry {
    SessionTelemetry::new(
        ThreadId::new(),
        "gpt-test",
        "gpt-test",
        /*account_id*/ None,
        /*account_email*/ None,
        /*auth_mode*/ None,
        "test-originator".to_string(),
        /*log_user_prompts*/ false,
        "test-terminal".to_string(),
        SessionSource::Cli,
    )
}

fn test_prompt_with_tools(tools: Vec<ToolSpec>) -> crate::client_common::Prompt {
    crate::client_common::Prompt {
        input: vec![codex_protocol::models::ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![codex_protocol::models::ContentItem::InputText {
                text: "hello".to_string(),
            }],
            end_turn: None,
            phase: None,
        }],
        tools,
        parallel_tool_calls: false,
        base_instructions: codex_protocol::models::BaseInstructions {
            text: "system".to_string(),
        },
        personality: None,
        output_schema: None,
    }
}

#[test]
fn build_subagent_headers_sets_other_subagent_label() {
    let client = test_model_client(SessionSource::SubAgent(SubAgentSource::Other(
        "memory_consolidation".to_string(),
    )));
    let headers = client.build_subagent_headers();
    let value = headers
        .get("x-openai-subagent")
        .and_then(|value| value.to_str().ok());
    assert_eq!(value, Some("memory_consolidation"));
}

#[tokio::test]
async fn summarize_memories_returns_empty_for_empty_input() {
    let client = test_model_client(SessionSource::Cli);
    let model_info = test_model_info();
    let session_telemetry = test_session_telemetry();

    let output = client
        .summarize_memories(
            Vec::new(),
            &model_info,
            /*effort*/ None,
            &session_telemetry,
        )
        .await
        .expect("empty summarize request should succeed");
    assert_eq!(output.len(), 0);
}

#[tokio::test]
async fn summarize_memories_rejects_anthropic_provider() {
    let client = test_model_client_with_wire_api(
        SessionSource::Cli,
        crate::model_provider_info::WireApi::Anthropic,
    );
    let model_info = test_model_info();
    let session_telemetry = test_session_telemetry();

    let err = client
        .summarize_memories(
            vec![codex_api::RawMemory {
                id: "trace-1".to_string(),
                metadata: codex_api::RawMemoryMetadata {
                    source_path: "/tmp/trace.json".to_string(),
                },
                items: vec![json!({"type": "message", "role": "user", "content": []})],
            }],
            &model_info,
            None,
            &session_telemetry,
        )
        .await
        .expect_err("anthropic summarize should be rejected");

    assert_eq!(
        err.to_string(),
        "unsupported operation: memory summarize is not supported for Anthropic providers"
    );
}

#[test]
fn auth_request_telemetry_context_tracks_attached_auth_and_retry_phase() {
    let auth_context = AuthRequestTelemetryContext::new(
        Some(crate::auth::AuthMode::Chatgpt),
        &crate::api_bridge::CoreAuthProvider::for_test(Some("access-token"), Some("workspace-123")),
        PendingUnauthorizedRetry::from_recovery(UnauthorizedRecoveryExecution {
            mode: "managed",
            phase: "refresh_token",
        }),
    );

    assert_eq!(auth_context.auth_mode, Some("Chatgpt"));
    assert!(auth_context.auth_header_attached);
    assert_eq!(auth_context.auth_header_name, Some("authorization"));
    assert!(auth_context.retry_after_unauthorized);
    assert_eq!(auth_context.recovery_mode, Some("managed"));
    assert_eq!(auth_context.recovery_phase, Some("refresh_token"));
}

#[tokio::test]
async fn provider_auth_disables_unauthorized_recovery_and_request_compression() {
    let mut provider = crate::model_provider_info::create_oss_provider_with_base_url(
        "https://example.com/v1",
        crate::model_provider_info::WireApi::Responses,
    );
    provider.experimental_bearer_token = Some("provider-token".to_string());
    let client = test_model_client_with_provider_and_auth(
        provider,
        CodexAuth::create_dummy_chatgpt_auth_for_testing(),
    );

    let client_setup = client
        .current_client_setup()
        .await
        .expect("client setup should resolve");

    assert_eq!(
        client_setup.api_auth.auth_mode(),
        Some(crate::auth::AuthMode::ApiKey)
    );
    assert!(client.unauthorized_recovery().is_none());
    assert_eq!(
        client
            .new_session()
            .responses_request_compression(&client_setup.api_auth),
        codex_api::requests::responses::Compression::None
    );
}

#[tokio::test]
async fn official_openai_endpoint_enables_request_compression_without_openai_name() {
    let provider = crate::ModelProviderInfo {
        name: "OpenAI Chat".to_string(),
        model: None,
        base_url: Some(crate::model_provider_info::DEFAULT_OPENAI_BASE_URL.to_string()),
        env_key: None,
        env_key_instructions: None,
        experimental_bearer_token: None,
        auth: None,
        wire_api: crate::WireApi::Responses,
        query_params: None,
        http_headers: None,
        env_http_headers: None,
        request_max_retries: None,
        stream_max_retries: None,
        stream_idle_timeout_ms: None,
        websocket_connect_timeout_ms: None,
        requires_openai_auth: true,
        supports_websockets: true,
    };
    let client = test_model_client_with_provider_and_auth(
        provider,
        CodexAuth::create_dummy_chatgpt_auth_for_testing(),
    );

    let client_setup = client
        .current_client_setup()
        .await
        .expect("client setup should resolve");

    assert_eq!(
        client
            .new_session()
            .responses_request_compression(&client_setup.api_auth),
        codex_api::requests::responses::Compression::Zstd
    );
}

#[test]
fn filter_tools_for_chat_provider_drops_hosted_only_tools() {
    let filtered = super::filter_tools_for_wire_api(
        &[
            ToolSpec::Function(ResponsesApiTool {
                name: "read_file".to_string(),
                description: "Read".to_string(),
                strict: false,
                defer_loading: None,
                parameters: JsonSchema::Object {
                    properties: Default::default(),
                    required: None,
                    additional_properties: None,
                },
                output_schema: None,
            }),
            ToolSpec::WebSearch {
                external_web_access: Some(true),
                filters: None,
                user_location: None,
                search_context_size: None,
                search_content_types: None,
            },
            ToolSpec::ImageGeneration {
                output_format: "png".to_string(),
            },
        ],
        codex_api::provider::WireApi::Chat,
    );

    assert_eq!(filtered.len(), 1);
    assert!(matches!(&filtered[0], ToolSpec::Function(tool) if tool.name == "read_file"));
}

#[test]
fn build_responses_request_for_chat_provider_omits_hosted_only_tools() {
    let client = test_model_client_with_wire_api(
        SessionSource::Cli,
        crate::model_provider_info::WireApi::Chat,
    );
    let session = client.new_session();
    let prompt = test_prompt_with_tools(vec![
        ToolSpec::WebSearch {
            external_web_access: Some(true),
            filters: None,
            user_location: None,
            search_context_size: None,
            search_content_types: None,
        },
        ToolSpec::ImageGeneration {
            output_format: "png".to_string(),
        },
    ]);
    let provider = codex_api::Provider {
        name: "mock".to_string(),
        base_url: "https://example.com/v1".to_string(),
        wire_api: codex_api::provider::WireApi::Chat,
        query_params: None,
        headers: http::HeaderMap::new(),
        retry: codex_api::provider::RetryConfig {
            max_attempts: 1,
            base_delay: std::time::Duration::from_millis(1),
            retry_429: false,
            retry_5xx: false,
            retry_transport: false,
        },
        stream_idle_timeout: std::time::Duration::from_secs(5),
    };

    let request = session
        .build_responses_request(
            &provider,
            &prompt,
            &test_model_info(),
            None,
            codex_protocol::config_types::ReasoningSummary::None,
            None,
        )
        .expect("build request");

    assert_eq!(request.tools, Vec::<serde_json::Value>::new());
}
