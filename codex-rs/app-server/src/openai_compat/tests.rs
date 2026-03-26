use super::*;
use crate::openai_compat::adapter::CompatEndpoint;
use crate::openai_compat::adapter::UpstreamAdapter;
use codex_core::CodexAuth;
use futures::StreamExt;
use futures::stream;
use pretty_assertions::assert_eq;
use reqwest::header::CACHE_CONTROL;
use reqwest::header::CONTENT_TYPE;
use serde_json::Value;
use serde_json::json;
use tokio::sync::Mutex;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio::time::Duration;
use tokio::time::timeout;
use tokio_util::bytes::Bytes;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::header;
use wiremock::matchers::method;
use wiremock::matchers::path;
use wiremock::matchers::query_param;

fn provider(base_url: String) -> ModelProviderInfo {
    ModelProviderInfo {
        name: "Proxy Test".to_string(),
        model: Some("gpt-5.4".to_string()),
        base_url: Some(base_url),
        env_key: None,
        env_key_instructions: None,
        experimental_bearer_token: Some("provider-token".to_string()),
        wire_api: WireApi::Responses,
        query_params: Some(HashMap::from([(
            "api-version".to_string(),
            "2025-04-01-preview".to_string(),
        )])),
        http_headers: Some(HashMap::from([(
            "x-provider-header".to_string(),
            "present".to_string(),
        )])),
        env_http_headers: None,
        request_max_retries: None,
        stream_max_retries: None,
        stream_idle_timeout_ms: None,
        websocket_connect_timeout_ms: None,
        requires_openai_auth: false,
        supports_websockets: false,
    }
}

fn state(base_url: String) -> OpenAiCompatState {
    OpenAiCompatState {
        upstream: Arc::new(UpstreamConfig {
            provider_id: "test-provider".to_string(),
            provider: provider(base_url),
            adapter: UpstreamAdapter::responses(),
            auth_manager: AuthManager::from_auth_for_testing(CodexAuth::from_api_key(
                "ignored-auth",
            )),
        }),
        auth_token: None,
        client: Client::builder().build().expect("client"),
    }
}

fn chat_state(base_url: String) -> OpenAiCompatState {
    OpenAiCompatState {
        upstream: Arc::new(UpstreamConfig {
            provider_id: "test-provider".to_string(),
            provider: ModelProviderInfo {
                wire_api: WireApi::Chat,
                ..provider(base_url)
            },
            adapter: UpstreamAdapter::chat(),
            auth_manager: AuthManager::from_auth_for_testing(CodexAuth::from_api_key(
                "ignored-auth",
            )),
        }),
        auth_token: None,
        client: Client::builder().build().expect("client"),
    }
}

fn state_with_local_auth(base_url: String, auth_token: &str) -> OpenAiCompatState {
    OpenAiCompatState {
        upstream: Arc::new(UpstreamConfig {
            provider_id: "test-provider".to_string(),
            provider: provider(base_url),
            adapter: UpstreamAdapter::responses(),
            auth_manager: AuthManager::from_auth_for_testing(CodexAuth::from_api_key(
                "ignored-auth",
            )),
        }),
        auth_token: Some(Arc::<str>::from(auth_token)),
        client: Client::builder().build().expect("client"),
    }
}

async fn spawn_router(router: Router) -> (SocketAddr, JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener");
    let address = listener.local_addr().expect("local addr");
    let handle = tokio::spawn(async move {
        axum::serve(listener, router.into_make_service())
            .await
            .expect("server should run");
    });
    (address, handle)
}

#[test]
fn build_upstream_url_preserves_duplicate_query_keys_and_caller_precedence() {
    let url = build_upstream_url(
        &provider("https://example.com/v1".to_string()),
        "/chat/completions",
        Some("stream=true&tag=one&tag=two&api-version=caller"),
    )
    .expect("url");

    assert_eq!(
        url.as_str(),
        "https://example.com/v1/chat/completions?stream=true&tag=one&tag=two&api-version=caller"
    );
}

#[test]
fn build_upstream_url_rejects_invalid_query_string() {
    let err = build_upstream_url(
        &provider("https://example.com/v1".to_string()),
        "/chat/completions",
        Some("bad=%ZZ"),
    )
    .expect_err("invalid query should fail");

    assert_eq!(err.status, StatusCode::BAD_REQUEST);
    assert!(err.message.contains("invalid query string"));
}

#[tokio::test]
async fn build_upstream_headers_prefers_provider_bearer_token() {
    let upstream = UpstreamConfig {
        provider_id: "test-provider".to_string(),
        provider: provider("https://example.com/v1".to_string()),
        adapter: UpstreamAdapter::responses(),
        auth_manager: AuthManager::from_auth_for_testing(CodexAuth::from_api_key("ignored-auth")),
    };
    let headers = build_upstream_headers(&upstream, &HeaderMap::new())
        .await
        .expect("headers");
    assert_eq!(headers["authorization"], "Bearer provider-token");
    assert_eq!(headers["x-provider-header"], "present");
}

#[tokio::test]
async fn build_upstream_headers_does_not_forward_sensitive_local_headers() {
    let upstream = UpstreamConfig {
        provider_id: "test-provider".to_string(),
        provider: provider("https://example.com/v1".to_string()),
        adapter: UpstreamAdapter::responses(),
        auth_manager: AuthManager::from_auth_for_testing(CodexAuth::from_api_key("ignored-auth")),
    };
    let mut incoming = HeaderMap::new();
    incoming.insert("content-type", HeaderValue::from_static("application/json"));
    incoming.insert("accept", HeaderValue::from_static("text/event-stream"));
    incoming.insert("cookie", HeaderValue::from_static("session=local"));
    incoming.insert("x-forwarded-for", HeaderValue::from_static("127.0.0.1"));
    incoming.insert(
        "authorization",
        HeaderValue::from_static("Bearer local-proxy"),
    );

    let headers = build_upstream_headers(&upstream, &incoming)
        .await
        .expect("headers");

    assert_eq!(headers["content-type"], "application/json");
    assert_eq!(headers["accept"], "text/event-stream");
    assert!(headers.get("cookie").is_none());
    assert!(headers.get("x-forwarded-for").is_none());
    assert_eq!(headers["authorization"], "Bearer provider-token");
}

#[tokio::test]
async fn resolve_provider_bearer_token_falls_back_when_env_key_is_missing() {
    let mut provider = provider("https://example.com/v1".to_string());
    provider.env_key = Some(format!("CODEX_TEST_MISSING_{}", uuid::Uuid::now_v7()));

    let token = resolve_provider_bearer_token(&UpstreamConfig {
        provider_id: "test-provider".to_string(),
        provider,
        adapter: UpstreamAdapter::responses(),
        auth_manager: AuthManager::from_auth_for_testing(CodexAuth::from_api_key("ignored-auth")),
    })
    .await
    .expect("token");

    assert_eq!(token, Some("provider-token".to_string()));
}

#[tokio::test]
async fn proxy_forwards_chat_completions_to_provider() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(query_param("api-version", "2025-04-01-preview"))
        .and(header("authorization", "Bearer provider-token"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{\"ok\":true}"))
        .mount(&server)
        .await;

    let response = proxy_request(
        state(format!("{}/v1", server.uri())),
        Method::POST,
        CompatEndpoint::ChatCompletions,
        None,
        HeaderMap::new(),
        Some("{}".to_string()),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn chat_wire_api_translates_responses_endpoint_to_chat_upstream() {
    let (request_tx, request_rx) = oneshot::channel::<Value>();
    let request_tx = Arc::new(Mutex::new(Some(request_tx)));
    let upstream_router = Router::new().route(
        "/v1/chat/completions",
        post({
            let request_tx = request_tx.clone();
            move |body: String| {
                let request_tx = request_tx.clone();
                async move {
                    if let Some(tx) = request_tx.lock().await.take() {
                        let parsed = serde_json::from_str::<Value>(&body).expect("body json");
                        let _ = tx.send(parsed);
                    }
                    axum::Json(json!({
                        "id": "chatcmpl-1",
                        "object": "chat.completion",
                        "created": 123,
                        "model": "gpt-chat",
                        "choices": [{
                            "index": 0,
                            "message": {
                                "role": "assistant",
                                "content": "hello from chat"
                            },
                            "finish_reason": "stop"
                        }],
                        "usage": {
                            "prompt_tokens": 11,
                            "prompt_tokens_details": { "cached_tokens": 2 },
                            "completion_tokens": 5,
                            "completion_tokens_details": { "reasoning_tokens": 1 },
                            "total_tokens": 16
                        }
                    }))
                }
            }
        }),
    );
    let (upstream_addr, _upstream_handle) = spawn_router(upstream_router).await;

    let response = proxy_request(
        chat_state(format!("http://{upstream_addr}/v1")),
        Method::POST,
        CompatEndpoint::Responses,
        None,
        HeaderMap::new(),
        Some(
            json!({
                "model": "gpt-chat",
                "instructions": "system",
                "input": [{
                    "type": "message",
                    "role": "user",
                    "content": [{ "type": "input_text", "text": "hello" }]
                }],
                "tools": [],
                "tool_choice": "auto",
                "parallel_tool_calls": false,
                "store": false,
                "stream": false,
                "include": []
            })
            .to_string(),
        ),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let body: Value = serde_json::from_slice(&body).expect("json body");
    assert_eq!(body["object"], "response");
    assert_eq!(body["status"], "completed");
    assert_eq!(body["model"], "gpt-chat");
    assert_eq!(body["output"][0]["type"], "message");
    assert_eq!(body["output"][0]["role"], "assistant");
    assert_eq!(body["output"][0]["content"][0]["text"], "hello from chat");
    assert_eq!(body["usage"]["input_tokens"], 11);
    assert_eq!(body["usage"]["input_tokens_details"]["cached_tokens"], 2);

    let upstream_request = request_rx.await.expect("upstream request");
    assert_eq!(upstream_request["model"], "gpt-chat");
    assert_eq!(upstream_request["messages"][0]["role"], "system");
    assert_eq!(upstream_request["messages"][1]["role"], "user");
    assert_eq!(upstream_request["stream"], Value::Bool(false));
}

#[tokio::test]
async fn chat_wire_api_proxy_forwards_chat_completions_via_running_server() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(query_param("api-version", "2025-04-01-preview"))
        .and(header("authorization", "Bearer provider-token"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{\"chat\":true}"))
        .mount(&server)
        .await;

    let (proxy_addr, _proxy_handle) =
        spawn_router(app_router(chat_state(format!("{}/v1", server.uri())))).await;

    let response = reqwest::Client::new()
        .post(format!("http://{proxy_addr}/v1/chat/completions"))
        .header(CONTENT_TYPE, "application/json")
        .body("{}")
        .send()
        .await
        .expect("proxy request should succeed");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.text().await.expect("body"),
        "{\"chat\":true}".to_string()
    );
}

#[tokio::test]
async fn chat_wire_api_streams_chat_completions_without_buffering() {
    let (release_second_chunk_tx, release_second_chunk_rx) = oneshot::channel::<()>();
    let release_second_chunk_rx = Arc::new(Mutex::new(Some(release_second_chunk_rx)));
    let upstream_router = Router::new().route(
        "/v1/chat/completions",
        post({
            let release_second_chunk_rx = release_second_chunk_rx.clone();
            move || {
                let release_second_chunk_rx = release_second_chunk_rx.clone();
                async move {
                    let body = Body::from_stream(stream::unfold(0usize, move |state| {
                        let release_second_chunk_rx = release_second_chunk_rx.clone();
                        async move {
                            match state {
                                0 => Some((
                                    Ok::<Bytes, std::convert::Infallible>(Bytes::from_static(
                                        b"data: first\n\n",
                                    )),
                                    1,
                                )),
                                1 => {
                                    if let Some(rx) = release_second_chunk_rx.lock().await.take() {
                                        let _ = rx.await;
                                    }
                                    Some((
                                        Ok::<Bytes, std::convert::Infallible>(Bytes::from_static(
                                            b"data: second\n\n",
                                        )),
                                        2,
                                    ))
                                }
                                _ => None,
                            }
                        }
                    }));
                    (
                        [
                            (CONTENT_TYPE, "text/event-stream"),
                            (CACHE_CONTROL, "no-cache"),
                        ],
                        body,
                    )
                }
            }
        }),
    );
    let (upstream_addr, _upstream_handle) = spawn_router(upstream_router).await;

    let (proxy_addr, _proxy_handle) =
        spawn_router(app_router(chat_state(format!("http://{upstream_addr}/v1")))).await;

    let response = reqwest::Client::new()
        .post(format!(
            "http://{proxy_addr}/v1/chat/completions?stream=true"
        ))
        .header(CONTENT_TYPE, "application/json")
        .body("{}")
        .send()
        .await
        .expect("proxy request should succeed");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("text/event-stream")
    );

    let mut bytes_stream = response.bytes_stream();
    let first_chunk = bytes_stream
        .next()
        .await
        .expect("first chunk")
        .expect("first chunk should be readable");
    assert_eq!(first_chunk, Bytes::from_static(b"data: first\n\n"));
    assert!(
        timeout(Duration::from_millis(100), bytes_stream.next())
            .await
            .is_err()
    );

    release_second_chunk_tx
        .send(())
        .expect("second chunk should be released");
    let second_chunk = timeout(Duration::from_secs(1), bytes_stream.next())
        .await
        .expect("second chunk should arrive")
        .expect("second chunk stream item should exist")
        .expect("second chunk should be readable");
    assert_eq!(second_chunk, Bytes::from_static(b"data: second\n\n"));
}

#[tokio::test]
async fn chat_wire_api_running_server_rejects_responses_endpoint() {
    let (proxy_addr, _proxy_handle) =
        spawn_router(app_router(chat_state("https://example.com/v1".to_string()))).await;

    let response = reqwest::Client::new()
        .post(format!("http://{proxy_addr}/v1/responses"))
        .header(CONTENT_TYPE, "application/json")
        .body(
            json!({
                "model": "gpt-chat",
                "instructions": "system",
                "input": [{
                    "type": "message",
                    "role": "user",
                    "content": [{ "type": "input_text", "text": "hello" }]
                }],
                "tools": [],
                "tool_choice": "auto",
                "parallel_tool_calls": false,
                "store": false,
                "stream": true,
                "include": []
            })
            .to_string(),
        )
        .send()
        .await
        .expect("proxy request should succeed");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        response.text().await.expect("body"),
        "{\"error\":{\"message\":\"current upstream provider uses wire_api = \\\"chat\\\"; streamed /v1/responses translation is not available yet, use /v1/chat/completions for streaming\",\"type\":\"invalid_request_error\"}}"
    );
}

#[tokio::test]
async fn chat_wire_api_proxy_forwards_models_via_running_server() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .and(query_param("api-version", "2025-04-01-preview"))
        .and(header("authorization", "Bearer provider-token"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string("{\"data\":[{\"id\":\"gpt-chat\"}]}"),
        )
        .mount(&server)
        .await;

    let (proxy_addr, _proxy_handle) =
        spawn_router(app_router(chat_state(format!("{}/v1", server.uri())))).await;

    let response = reqwest::Client::new()
        .get(format!("http://{proxy_addr}/v1/models"))
        .send()
        .await
        .expect("proxy request should succeed");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.text().await.expect("body"),
        "{\"data\":[{\"id\":\"gpt-chat\"}]}".to_string()
    );
}

#[tokio::test]
async fn running_server_requires_local_bearer_auth_when_configured() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{\"data\":[]}"))
        .mount(&server)
        .await;

    let (proxy_addr, _proxy_handle) = spawn_router(app_router(state_with_local_auth(
        format!("{}/v1", server.uri()),
        "secret",
    )))
    .await;

    let unauthorized = reqwest::Client::new()
        .get(format!("http://{proxy_addr}/v1/models"))
        .send()
        .await
        .expect("request should complete");
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

    let authorized = reqwest::Client::new()
        .get(format!("http://{proxy_addr}/v1/models"))
        .bearer_auth("secret")
        .send()
        .await
        .expect("authorized request should complete");
    assert_eq!(authorized.status(), StatusCode::OK);
}

#[tokio::test]
async fn response_header_filtering_keeps_allowlisted_headers_only() {
    let upstream_router = Router::new().route(
        "/v1/models",
        get(|| async {
            (
                [
                    ("content-type", "application/json"),
                    ("request-id", "req-123"),
                    ("x-request-id", "xreq-456"),
                    ("set-cookie", "secret=cookie"),
                    ("x-internal-debug", "drop-me"),
                ],
                "{\"data\":[]}",
            )
        }),
    );
    let (upstream_addr, _upstream_handle) = spawn_router(upstream_router).await;

    let (proxy_addr, _proxy_handle) =
        spawn_router(app_router(state(format!("http://{upstream_addr}/v1")))).await;

    let response = reqwest::Client::new()
        .get(format!("http://{proxy_addr}/v1/models"))
        .send()
        .await
        .expect("proxy request should succeed");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("request-id")
            .and_then(|value| value.to_str().ok()),
        Some("req-123")
    );
    assert_eq!(
        response
            .headers()
            .get("x-request-id")
            .and_then(|value| value.to_str().ok()),
        Some("xreq-456")
    );
    assert!(response.headers().get("set-cookie").is_none());
    assert!(response.headers().get("x-internal-debug").is_none());
}

#[tokio::test]
async fn proxy_streams_upstream_response_without_buffering_entire_body() {
    let (release_second_chunk_tx, release_second_chunk_rx) = oneshot::channel::<()>();
    let release_second_chunk_rx = Arc::new(Mutex::new(Some(release_second_chunk_rx)));
    let upstream_router = Router::new().route(
        "/v1/responses",
        post({
            let release_second_chunk_rx = release_second_chunk_rx.clone();
            move || {
                let release_second_chunk_rx = release_second_chunk_rx.clone();
                async move {
                    let body = Body::from_stream(stream::unfold(0usize, move |state| {
                        let release_second_chunk_rx = release_second_chunk_rx.clone();
                        async move {
                            match state {
                                0 => Some((
                                    Ok::<Bytes, std::convert::Infallible>(Bytes::from_static(
                                        b"data: first\n\n",
                                    )),
                                    1,
                                )),
                                1 => {
                                    if let Some(rx) = release_second_chunk_rx.lock().await.take() {
                                        let _ = rx.await;
                                    }
                                    Some((
                                        Ok::<Bytes, std::convert::Infallible>(Bytes::from_static(
                                            b"data: second\n\n",
                                        )),
                                        2,
                                    ))
                                }
                                _ => None,
                            }
                        }
                    }));
                    (
                        [
                            (CONTENT_TYPE, "text/event-stream"),
                            (CACHE_CONTROL, "no-cache"),
                        ],
                        body,
                    )
                }
            }
        }),
    );
    let (upstream_addr, _upstream_handle) = spawn_router(upstream_router).await;

    let proxy_state = state(format!("http://{upstream_addr}/v1"));
    let (proxy_addr, _proxy_handle) = spawn_router(app_router(proxy_state)).await;

    let response = reqwest::Client::new()
        .post(format!("http://{proxy_addr}/v1/responses?stream=true"))
        .header(CONTENT_TYPE, "application/json")
        .body("{}")
        .send()
        .await
        .expect("proxy request should succeed");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("text/event-stream")
    );
    assert_eq!(
        response
            .headers()
            .get(CACHE_CONTROL)
            .and_then(|value| value.to_str().ok()),
        Some("no-cache")
    );
    assert!(response.headers().get("set-cookie").is_none());

    let mut bytes_stream = response.bytes_stream();
    let first_chunk = bytes_stream
        .next()
        .await
        .expect("first chunk")
        .expect("first chunk should be readable");
    assert_eq!(first_chunk, Bytes::from_static(b"data: first\n\n"));
    assert!(
        timeout(Duration::from_millis(100), bytes_stream.next())
            .await
            .is_err()
    );

    release_second_chunk_tx
        .send(())
        .expect("second chunk should be released");
    let second_chunk = timeout(Duration::from_secs(1), bytes_stream.next())
        .await
        .expect("second chunk should arrive")
        .expect("second chunk stream item should exist")
        .expect("second chunk should be readable");
    assert_eq!(second_chunk, Bytes::from_static(b"data: second\n\n"));
}

#[test]
fn authorize_rejects_missing_or_invalid_bearer_token() {
    let state = OpenAiCompatState {
        upstream: Arc::new(UpstreamConfig {
            provider_id: "test-provider".to_string(),
            provider: provider("https://example.com/v1".to_string()),
            adapter: UpstreamAdapter::responses(),
            auth_manager: AuthManager::from_auth_for_testing(CodexAuth::from_api_key(
                "ignored-auth",
            )),
        }),
        auth_token: Some(Arc::<str>::from("expected-token")),
        client: Client::builder().build().expect("client"),
    };

    let missing = authorize(&state, &HeaderMap::new()).expect_err("missing auth should fail");
    assert_eq!(missing.status, StatusCode::UNAUTHORIZED);

    let mut invalid_headers = HeaderMap::new();
    invalid_headers.insert(
        axum::http::header::AUTHORIZATION,
        HeaderValue::from_static("Bearer wrong-token"),
    );
    let invalid = authorize(&state, &invalid_headers).expect_err("wrong auth should fail");
    assert_eq!(invalid.status, StatusCode::UNAUTHORIZED);
}

#[test]
fn read_auth_token_requires_non_empty_env_value() {
    let env_name = format!("CODEX_TEST_AUTH_TOKEN_{}", uuid::Uuid::now_v7());
    unsafe {
        std::env::set_var(&env_name, "");
    }

    let err = read_auth_token(Some(&env_name)).expect_err("empty auth token should fail");
    assert!(err.to_string().contains("is empty"));

    unsafe {
        std::env::remove_var(&env_name);
    }
}

#[test]
fn read_auth_token_reads_value_from_env() {
    let env_name = format!("CODEX_TEST_AUTH_TOKEN_{}", uuid::Uuid::now_v7());
    unsafe {
        std::env::set_var(&env_name, "secret-token");
    }

    let token = read_auth_token(Some(&env_name)).expect("env auth token should load");
    assert_eq!(token, Some("secret-token".to_string()));

    unsafe {
        std::env::remove_var(&env_name);
    }
}

#[tokio::test]
async fn build_upstream_headers_falls_back_to_auth_manager_token() {
    let mut provider = provider("https://example.com/v1".to_string());
    provider.experimental_bearer_token = None;

    let headers = build_upstream_headers(
        &UpstreamConfig {
            provider_id: "test-provider".to_string(),
            provider,
            adapter: UpstreamAdapter::responses(),
            auth_manager: AuthManager::from_auth_for_testing(CodexAuth::from_api_key(
                "auth-manager-token",
            )),
        },
        &HeaderMap::new(),
    )
    .await
    .expect("headers");

    assert_eq!(headers["authorization"], "Bearer auth-manager-token");
}

#[test]
fn ensure_supported_provider_rejects_missing_base_url() {
    let err = ensure_supported_provider_info(
        "test-provider",
        &ModelProviderInfo {
            base_url: None,
            ..provider("https://example.com/v1".to_string())
        },
    )
    .expect_err("base_url should be required");
    assert!(err.to_string().contains("has no base_url"));
}

#[test]
fn ensure_supported_provider_accepts_chat_wire_api() {
    let provider = ModelProviderInfo {
        wire_api: WireApi::Chat,
        ..provider("https://example.com/v1".to_string())
    };

    ensure_supported_provider_info("test-provider", &provider)
        .expect("chat wire api should be accepted for openai compat proxy");
}

#[test]
fn chat_adapter_resolves_responses_via_chat_completions_path() {
    let adapter = UpstreamAdapter::chat();
    assert_eq!(
        adapter
            .resolve_request(CompatEndpoint::ChatCompletions)
            .expect("chat request")
            .path,
        "/chat/completions"
    );
    assert_eq!(
        adapter
            .resolve_request(CompatEndpoint::Responses)
            .expect("translated responses request")
            .path,
        "/chat/completions"
    );
}

#[test]
fn responses_adapter_resolves_all_openai_compat_endpoints() {
    let adapter = UpstreamAdapter::responses();
    assert_eq!(
        adapter
            .resolve_request(CompatEndpoint::Models)
            .expect("models request")
            .path,
        "/models"
    );
    assert_eq!(
        adapter
            .resolve_request(CompatEndpoint::Responses)
            .expect("responses request")
            .path,
        "/responses"
    );
    assert_eq!(
        adapter
            .resolve_request(CompatEndpoint::ChatCompletions)
            .expect("chat request")
            .path,
        "/chat/completions"
    );
}

#[test]
fn resolved_request_uses_passthrough_translator_by_default() {
    let resolved = UpstreamAdapter::responses()
        .resolve_request(CompatEndpoint::Responses)
        .expect("responses request");
    let translated = resolved
        .translator
        .translate_request(CompatEndpoint::Responses, Some("{\"ok\":true}".to_string()))
        .expect("translator should pass request through");

    assert_eq!(translated.body, Some("{\"ok\":true}".to_string()));
}
