use super::RealtimeHandoffState;
use super::RealtimeSessionKind;
use super::realtime_api_key;
use super::realtime_text_from_handoff_request;
use crate::CodexAuth;
use crate::ModelProviderInfo;
use crate::WireApi;
use async_channel::bounded;
use codex_protocol::protocol::RealtimeHandoffRequested;
use codex_protocol::protocol::RealtimeTranscriptEntry;
use pretty_assertions::assert_eq;

#[test]
fn extracts_text_from_handoff_request_active_transcript() {
    let handoff = RealtimeHandoffRequested {
        handoff_id: "handoff_1".to_string(),
        item_id: "item_1".to_string(),
        input_transcript: "ignored".to_string(),
        active_transcript: vec![
            RealtimeTranscriptEntry {
                role: "user".to_string(),
                text: "hello".to_string(),
            },
            RealtimeTranscriptEntry {
                role: "assistant".to_string(),
                text: "hi there".to_string(),
            },
        ],
    };
    assert_eq!(
        realtime_text_from_handoff_request(&handoff),
        Some("user: hello\nassistant: hi there".to_string())
    );
}

#[test]
fn extracts_text_from_handoff_request_input_transcript_if_messages_missing() {
    let handoff = RealtimeHandoffRequested {
        handoff_id: "handoff_1".to_string(),
        item_id: "item_1".to_string(),
        input_transcript: "ignored".to_string(),
        active_transcript: vec![],
    };
    assert_eq!(
        realtime_text_from_handoff_request(&handoff),
        Some("ignored".to_string())
    );
}

#[test]
fn ignores_empty_handoff_request_input_transcript() {
    let handoff = RealtimeHandoffRequested {
        handoff_id: "handoff_1".to_string(),
        item_id: "item_1".to_string(),
        input_transcript: String::new(),
        active_transcript: vec![],
    };
    assert_eq!(realtime_text_from_handoff_request(&handoff), None);
}

#[test]
fn realtime_api_key_ignores_empty_configured_bearer_token() {
    let provider = ModelProviderInfo {
        name: "OpenAI compatible".to_string(),
        model: None,
        base_url: Some("https://example.com/v1".to_string()),
        env_key: None,
        env_key_instructions: None,
        experimental_bearer_token: Some(String::new()),
        wire_api: WireApi::Responses,
        query_params: None,
        http_headers: None,
        env_http_headers: None,
        request_max_retries: None,
        stream_max_retries: None,
        stream_idle_timeout_ms: None,
        websocket_connect_timeout_ms: None,
        requires_openai_auth: false,
        supports_websockets: false,
    };

    let api_key = realtime_api_key(Some(&CodexAuth::from_api_key("auth-json-key")), &provider)
        .expect("realtime api key should fall back to auth");
    assert_eq!(api_key, "auth-json-key");
}

#[tokio::test]
async fn clears_active_handoff_explicitly() {
    let (tx, _rx) = bounded(1);
    let state = RealtimeHandoffState::new(tx, RealtimeSessionKind::V1);

    *state.active_handoff.lock().await = Some("handoff_1".to_string());
    assert_eq!(
        state.active_handoff.lock().await.clone(),
        Some("handoff_1".to_string())
    );

    *state.active_handoff.lock().await = None;
    assert_eq!(state.active_handoff.lock().await.clone(), None);
}
