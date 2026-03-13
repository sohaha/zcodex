use super::*;
use pretty_assertions::assert_eq;

#[test]
fn map_api_error_maps_server_overloaded() {
    let err = map_api_error(ApiError::ServerOverloaded);
    assert!(matches!(err, CodexErr::ServerOverloaded));
}

#[test]
fn map_api_error_maps_server_overloaded_from_503_body() {
    let body = serde_json::json!({
        "error": {
            "code": "server_is_overloaded"
        }
    })
    .to_string();
    let err = map_api_error(ApiError::Transport(TransportError::Http {
        status: http::StatusCode::SERVICE_UNAVAILABLE,
        url: Some("http://example.com/v1/responses".to_string()),
        headers: None,
        body: Some(body),
    }));

    assert!(matches!(err, CodexErr::ServerOverloaded));
}

#[test]
fn map_api_error_maps_anthropic_context_window_bad_request() {
    let body = serde_json::json!({
        "type": "error",
        "error": {
            "type": "invalid_request_error",
            "message": "prompt is too long: 220000 tokens > 200000 max"
        }
    })
    .to_string();
    let err = map_api_error(ApiError::Transport(TransportError::Http {
        status: http::StatusCode::BAD_REQUEST,
        url: Some("http://example.com/v1/messages".to_string()),
        headers: None,
        body: Some(body),
    }));

    assert!(matches!(err, CodexErr::ContextWindowExceeded));
}

#[test]
fn map_api_error_keeps_regular_anthropic_invalid_request() {
    let body = serde_json::json!({
        "type": "error",
        "error": {
            "type": "invalid_request_error",
            "message": "prompt is too long for tool name validation"
        }
    })
    .to_string();
    let err = map_api_error(ApiError::Transport(TransportError::Http {
        status: http::StatusCode::BAD_REQUEST,
        url: Some("http://example.com/v1/messages".to_string()),
        headers: None,
        body: Some(body),
    }));

    assert!(
        matches!(err, CodexErr::InvalidRequest(message) if message.contains("tool name validation"))
    );
}

#[test]
fn map_api_error_maps_usage_limit_limit_name_header() {
    let mut headers = HeaderMap::new();
    headers.insert(
        ACTIVE_LIMIT_HEADER,
        http::HeaderValue::from_static("codex_other"),
    );
    headers.insert(
        "x-codex-other-limit-name",
        http::HeaderValue::from_static("codex_other"),
    );
    let body = serde_json::json!({
        "error": {
            "type": "usage_limit_reached",
            "plan_type": "pro",
        }
    })
    .to_string();
    let err = map_api_error(ApiError::Transport(TransportError::Http {
        status: http::StatusCode::TOO_MANY_REQUESTS,
        url: Some("http://example.com/v1/responses".to_string()),
        headers: Some(headers),
        body: Some(body),
    }));

    let CodexErr::UsageLimitReached(usage_limit) = err else {
        panic!("expected CodexErr::UsageLimitReached, got {err:?}");
    };
    assert_eq!(
        usage_limit
            .rate_limits
            .as_ref()
            .and_then(|snapshot| snapshot.limit_name.as_deref()),
        Some("codex_other")
    );
}

#[test]
fn map_api_error_does_not_fallback_limit_name_to_limit_id() {
    let mut headers = HeaderMap::new();
    headers.insert(
        ACTIVE_LIMIT_HEADER,
        http::HeaderValue::from_static("codex_other"),
    );
    let body = serde_json::json!({
        "error": {
            "type": "usage_limit_reached",
            "plan_type": "pro",
        }
    })
    .to_string();
    let err = map_api_error(ApiError::Transport(TransportError::Http {
        status: http::StatusCode::TOO_MANY_REQUESTS,
        url: Some("http://example.com/v1/responses".to_string()),
        headers: Some(headers),
        body: Some(body),
    }));

    let CodexErr::UsageLimitReached(usage_limit) = err else {
        panic!("expected CodexErr::UsageLimitReached, got {err:?}");
    };
    assert_eq!(
        usage_limit
            .rate_limits
            .as_ref()
            .and_then(|snapshot| snapshot.limit_name.as_deref()),
        None
    );
}

#[test]
fn anthropic_auth_provider_does_not_duplicate_api_key_as_bearer() {
    let env_var = if cfg!(windows) { "USERNAME" } else { "USER" };
    let provider = ModelProviderInfo {
        name: "Anthropic".to_string(),
        base_url: Some("https://api.anthropic.com/v1".to_string()),
        env_key: Some(env_var.to_string()),
        env_key_instructions: None,
        experimental_bearer_token: None,
        wire_api: crate::model_provider_info::WireApi::Anthropic,
        query_params: None,
        http_headers: None,
        env_http_headers: None,
        request_max_retries: None,
        stream_max_retries: None,
        stream_idle_timeout_ms: None,
        requires_openai_auth: false,
        supports_websockets: false,
    };

    let auth = auth_provider_from_auth(None, &provider).expect("anthropic auth should build");
    assert_eq!(codex_api::AuthProvider::bearer_token(&auth), None);
}
