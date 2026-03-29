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

#[test]
fn realtime_api_key_uses_openai_env_fallback_for_official_chat_provider() {
    const SUBPROCESS_ENV: &str = "CODEX_TEST_REALTIME_API_KEY_SUBPROCESS";

    if std::env::var_os(SUBPROCESS_ENV).is_none() {
        let output = std::process::Command::new(
            std::env::current_exe().expect("test binary path should resolve"),
        )
        .arg("--exact")
        .arg("realtime_conversation::tests::realtime_api_key_uses_openai_env_fallback_for_official_chat_provider")
        .env(SUBPROCESS_ENV, "1")
        .env("OPENAI_API_KEY", "openai-env-key")
        .output()
        .expect("subprocess should run");

        assert!(
            output.status.success(),
            "subprocess failed:\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }

    let provider = ModelProviderInfo {
        name: "OpenAI Compatible".to_string(),
        model: None,
        base_url: Some(crate::model_provider_info::DEFAULT_OPENAI_BASE_URL.to_string()),
        env_key: None,
        env_key_instructions: None,
        experimental_bearer_token: None,
        wire_api: WireApi::Chat,
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

    let api_key = realtime_api_key(None, &provider).expect("official endpoint should use env key");
    assert_eq!(api_key, "openai-env-key");
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
