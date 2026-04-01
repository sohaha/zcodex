use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use codex_core::ModelClient;
use codex_core::ModelProviderInfo;
use codex_core::Prompt;
use codex_core::ResponseEvent;
use codex_core::WireApi;
use codex_otel::SessionTelemetry;
use codex_protocol::ThreadId;
use codex_protocol::config_types::ReasoningSummary;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ResponseItem;
use codex_protocol::protocol::SandboxPolicy;
use codex_protocol::protocol::SessionSource;
use core_test_support::load_default_config_for_test;
use core_test_support::skip_if_no_network;
use core_test_support::test_codex::test_codex;
use futures::StreamExt;
use pretty_assertions::assert_eq;
use tempfile::TempDir;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::Respond;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;
use wiremock::matchers::path;

#[derive(Clone, Default)]
struct RequestRecorder {
    requests: Arc<Mutex<Vec<wiremock::Request>>>,
}

impl wiremock::Match for RequestRecorder {
    fn matches(&self, request: &wiremock::Request) -> bool {
        self.requests
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(request.clone());
        true
    }
}

impl RequestRecorder {
    fn single_json_body(&self) -> serde_json::Value {
        let requests = self
            .requests
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert_eq!(requests.len(), 1);
        match serde_json::from_slice(&requests[0].body) {
            Ok(body) => body,
            Err(err) => panic!("request body json parse failed: {err}"),
        }
    }

    fn json_bodies(&self) -> Vec<serde_json::Value> {
        self.requests
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .map(|request| match serde_json::from_slice(&request.body) {
                Ok(body) => body,
                Err(err) => panic!("request body json parse failed: {err}"),
            })
            .collect()
    }
}

struct SseResponder {
    body: String,
}

impl Respond for SseResponder {
    fn respond(&self, _: &wiremock::Request) -> ResponseTemplate {
        ResponseTemplate::new(200)
            .insert_header("content-type", "text/event-stream")
            .set_body_string(self.body.clone())
    }
}

struct SseSequenceResponder {
    index: AtomicUsize,
    bodies: Vec<String>,
}

fn chat_test_builder(
    server: &MockServer,
    model: &'static str,
) -> core_test_support::test_codex::TestCodexBuilder {
    test_codex().with_model(model).with_config({
        let base_url = format!("{}/v1", server.uri());
        move |config| {
            config.model_provider.base_url = Some(base_url);
            config.model_provider.wire_api = WireApi::Chat;
            config.model_provider.supports_websockets = false;
            config.model_provider.request_max_retries = Some(0);
            config.model_provider.stream_max_retries = Some(0);
        }
    })
}

impl Respond for SseSequenceResponder {
    fn respond(&self, _: &wiremock::Request) -> ResponseTemplate {
        let index = self.index.fetch_add(1, Ordering::SeqCst);
        ResponseTemplate::new(200)
            .insert_header("content-type", "text/event-stream")
            .set_body_string(
                self.bodies
                    .get(index)
                    .unwrap_or_else(|| panic!("missing response body at index {index}"))
                    .clone(),
            )
    }
}

#[tokio::test]
async fn chat_wire_api_streams_via_chat_completions_endpoint() {
    skip_if_no_network!();

    let server = MockServer::start().await;
    let recorder = RequestRecorder::default();
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(recorder.clone())
        .respond_with(SseResponder {
            body: concat!(
                "data: {\"id\":\"chatcmpl-1\",\"model\":\"gpt-test\",\"choices\":[{\"delta\":{\"content\":\"hello\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":1,\"total_tokens\":4}}\n\n",
                "data: [DONE]\n\n"
            )
            .to_string(),
        })
        .mount(&server)
        .await;

    let provider = ModelProviderInfo {
        name: "mock-chat".into(),
        model: None,
        base_url: Some(format!("{}/v1", server.uri())),
        env_key: None,
        env_key_instructions: None,
        experimental_bearer_token: None,
        auth: None,
        wire_api: WireApi::Chat,
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

    let codex_home = TempDir::new().expect("tempdir");
    let mut config = load_default_config_for_test(&codex_home).await;
    config.model_provider_id = provider.name.clone();
    config.model_provider = provider.clone();
    let effort = config.model_reasoning_effort;
    let summary = config.model_reasoning_summary;
    let model = codex_core::test_support::get_model_offline(config.model.as_deref());
    config.model = Some(model.clone());
    let model_info =
        codex_core::test_support::construct_model_info_offline(model.as_str(), &Arc::new(config));

    let conversation_id = ThreadId::new();
    let session_telemetry = SessionTelemetry::new(
        conversation_id,
        model.as_str(),
        model_info.slug.as_str(),
        None,
        None,
        None,
        "test-originator".to_string(),
        false,
        "test-terminal".to_string(),
        SessionSource::Cli,
    );

    let client = ModelClient::new(
        None,
        conversation_id,
        provider,
        SessionSource::Cli,
        None,
        false,
        false,
        None,
    );
    let mut client_session = client.new_session();

    let mut prompt = Prompt::default();
    prompt.input = vec![ResponseItem::Message {
        id: None,
        role: "user".into(),
        content: vec![ContentItem::InputText {
            text: "hello".into(),
        }],
        end_turn: None,
        phase: None,
    }];

    let mut stream = client_session
        .stream(
            &prompt,
            &model_info,
            &session_telemetry,
            effort,
            summary.unwrap_or(ReasoningSummary::None),
            None,
            None,
        )
        .await
        .expect("stream failed");
    while let Some(event) = stream.next().await {
        if matches!(event, Ok(ResponseEvent::Completed { .. })) {
            break;
        }
    }

    let body = recorder.single_json_body();
    assert_eq!(body["model"], model_info.slug);
    assert_eq!(body["stream"], true);
    assert_eq!(body["messages"][0]["role"], "system");
    assert_eq!(body["messages"][1]["role"], "user");
    assert_eq!(body["messages"][1]["content"], "hello");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn chat_wire_api_replays_function_call_outputs_on_followup_request() -> anyhow::Result<()> {
    skip_if_no_network!(Ok(()));

    let server = MockServer::start().await;
    let recorder = RequestRecorder::default();
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(recorder.clone())
        .respond_with(SseSequenceResponder {
            index: AtomicUsize::new(0),
            bodies: vec![
                concat!(
                    "data: {\"id\":\"chatcmpl-1\",\"model\":\"gpt-5\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call-1\",\"function\":{\"name\":\"shell\",\"arguments\":\"{\\\"command\\\":[\\\"/bin/echo\\\",\\\"hi\\\"]}\"}}]},\"finish_reason\":\"tool_calls\"}],\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":1,\"total_tokens\":4}}\n\n",
                    "data: [DONE]\n\n"
                )
                .to_string(),
                concat!(
                    "data: {\"id\":\"chatcmpl-2\",\"model\":\"gpt-5\",\"choices\":[{\"delta\":{\"content\":\"done\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":8,\"completion_tokens\":1,\"total_tokens\":9}}\n\n",
                    "data: [DONE]\n\n"
                )
                .to_string(),
            ],
        })
        .up_to_n_times(2)
        .expect(2)
        .mount(&server)
        .await;

    let test = chat_test_builder(&server, "gpt-5").build(&server).await?;

    test.submit_turn_with_policy("say hi via shell", SandboxPolicy::DangerFullAccess)
        .await?;

    let bodies = recorder.json_bodies();
    assert_eq!(bodies.len(), 2);
    let first_messages = bodies[0]["messages"].as_array().expect("first messages");
    assert!(
        first_messages
            .iter()
            .any(|message| message["role"] == "user" && message["content"] == "say hi via shell")
    );

    let second_messages = bodies[1]["messages"].as_array().expect("second messages");
    let assistant_tool_call = second_messages
        .iter()
        .find(|message| {
            message["role"] == "assistant"
                && message
                    .get("tool_calls")
                    .and_then(serde_json::Value::as_array)
                    .is_some()
        })
        .expect("assistant tool call message");
    assert_eq!(
        assistant_tool_call["tool_calls"][0]["function"]["name"],
        "shell"
    );

    let tool_message = second_messages
        .iter()
        .find(|message| message["role"] == "tool")
        .expect("tool message");
    let tool_content = tool_message["content"].as_str().unwrap_or_default();
    assert!(
        tool_content.contains("hi"),
        "expected shell output to be replayed, got {tool_content}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn chat_wire_api_replays_custom_tool_outputs_on_followup_request() -> anyhow::Result<()> {
    skip_if_no_network!(Ok(()));

    let server = MockServer::start().await;
    let recorder = RequestRecorder::default();
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(recorder.clone())
        .respond_with(SseSequenceResponder {
            index: AtomicUsize::new(0),
            bodies: vec![
                concat!(
                    "data: {\"id\":\"chatcmpl-1\",\"model\":\"gpt-5.1-codex\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call-custom\",\"function\":{\"name\":\"apply_patch\",\"arguments\":\"{\\\"input\\\":\\\"*** Begin Patch\\\\n*** Add File: hello.txt\\\\n+hello from patch\\\\n*** End Patch\\\"}\"}}]},\"finish_reason\":\"tool_calls\"}],\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":1,\"total_tokens\":4}}\n\n",
                    "data: [DONE]\n\n"
                )
                .to_string(),
                concat!(
                    "data: {\"id\":\"chatcmpl-2\",\"model\":\"gpt-5.1-codex\",\"choices\":[{\"delta\":{\"content\":\"patched\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":8,\"completion_tokens\":1,\"total_tokens\":9}}\n\n",
                    "data: [DONE]\n\n"
                )
                .to_string(),
            ],
        })
        .up_to_n_times(2)
        .expect(2)
        .mount(&server)
        .await;

    let test = chat_test_builder(&server, "gpt-5.1-codex")
        .build(&server)
        .await?;
    test.submit_turn_with_policy("apply a patch", SandboxPolicy::DangerFullAccess)
        .await?;

    let bodies = recorder.json_bodies();
    let second_messages = bodies[1]["messages"].as_array().expect("second messages");
    let assistant_tool_call = second_messages
        .iter()
        .find(|message| {
            message["role"] == "assistant"
                && message["tool_calls"][0]["function"]["name"] == "apply_patch"
        })
        .expect("assistant custom tool call message");
    assert_eq!(
        assistant_tool_call["tool_calls"][0]["function"]["name"],
        "apply_patch"
    );

    let tool_message = second_messages
        .iter()
        .find(|message| message["role"] == "tool")
        .expect("tool message");
    let tool_content = tool_message["content"].as_str().unwrap_or_default();
    assert!(
        tool_content.contains("hello.txt") || tool_content.contains("Done"),
        "expected apply_patch output to be replayed, got {tool_content}"
    );

    assert_eq!(
        std::fs::read_to_string(test.workspace_path("hello.txt"))?,
        "hello from patch\n"
    );

    Ok(())
}

#[tokio::test]
async fn chat_wire_api_replays_tool_search_history_into_chat_messages() {
    skip_if_no_network!();

    let server = MockServer::start().await;
    let recorder = RequestRecorder::default();
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(recorder.clone())
        .respond_with(SseResponder {
            body: concat!(
                "data: {\"id\":\"chatcmpl-1\",\"model\":\"gpt-5.1-codex\",\"choices\":[{\"delta\":{\"content\":\"searched\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":8,\"completion_tokens\":1,\"total_tokens\":9}}\n\n",
                "data: [DONE]\n\n"
            )
            .to_string(),
        })
        .mount(&server)
        .await;

    let provider = ModelProviderInfo {
        name: "mock-chat".into(),
        model: None,
        base_url: Some(format!("{}/v1", server.uri())),
        env_key: None,
        env_key_instructions: None,
        experimental_bearer_token: None,
        auth: None,
        wire_api: WireApi::Chat,
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

    let codex_home = TempDir::new().expect("tempdir");
    let mut config = load_default_config_for_test(&codex_home).await;
    config.model_provider_id = provider.name.clone();
    config.model_provider = provider.clone();
    config.model = Some("gpt-5.1-codex".to_string());
    let model_info =
        codex_core::test_support::construct_model_info_offline("gpt-5.1-codex", &Arc::new(config));

    let conversation_id = ThreadId::new();
    let session_telemetry = SessionTelemetry::new(
        conversation_id,
        "gpt-5.1-codex",
        model_info.slug.as_str(),
        None,
        None,
        None,
        "test-originator".to_string(),
        false,
        "test-terminal".to_string(),
        SessionSource::Cli,
    );

    let client = ModelClient::new(
        None,
        conversation_id,
        provider,
        SessionSource::Cli,
        None,
        false,
        false,
        None,
    );
    let mut client_session = client.new_session();

    let mut prompt = Prompt::default();
    prompt.input = vec![
        ResponseItem::ToolSearchCall {
            id: None,
            call_id: Some("call-search".to_string()),
            status: None,
            execution: "client".to_string(),
            arguments: serde_json::json!({
                "query": "read file",
                "limit": 2,
            }),
        },
        ResponseItem::ToolSearchOutput {
            call_id: Some("call-search".to_string()),
            status: "completed".to_string(),
            execution: "client".to_string(),
            tools: vec![serde_json::json!({
                "type": "function",
                "name": "read_file",
                "description": "Read a file",
            })],
        },
        ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: "search tools for reading files".to_string(),
            }],
            end_turn: None,
            phase: None,
        },
    ];

    let mut stream = client_session
        .stream(
            &prompt,
            &model_info,
            &session_telemetry,
            None,
            ReasoningSummary::None,
            None,
            None,
        )
        .await
        .expect("stream failed");
    while let Some(event) = stream.next().await {
        if matches!(event, Ok(ResponseEvent::Completed { .. })) {
            break;
        }
    }

    let body = recorder.single_json_body();
    let messages = body["messages"].as_array().expect("messages");
    let assistant_tool_call = messages
        .iter()
        .find(|message| {
            message["role"] == "assistant"
                && message["tool_calls"][0]["function"]["name"] == "tool_search"
        })
        .expect("assistant tool search call message");
    assert_eq!(
        assistant_tool_call["tool_calls"][0]["function"]["name"],
        "tool_search"
    );

    let tool_message = messages
        .iter()
        .find(|message| message["role"] == "tool")
        .expect("tool message");
    let tool_content = tool_message["content"].as_str().unwrap_or_default();
    assert!(
        tool_content.contains("\"status\":\"completed\"") && tool_content.contains("read_file"),
        "expected tool_search output to be replayed, got {tool_content}"
    );
}
