use std::sync::Arc;
use std::sync::Mutex;

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
use codex_protocol::protocol::SessionSource;
use core_test_support::load_default_config_for_test;
use core_test_support::skip_if_no_network;
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
        serde_json::from_slice(&requests[0].body).expect("request body json")
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
