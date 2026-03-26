use super::*;
use crate::openai_compat::adapter::CompatEndpoint;
use crate::openai_compat::adapter::UpstreamAdapter;
use codex_core::CodexAuth;
use futures::StreamExt;
use futures::stream;
use pretty_assertions::assert_eq;
use reqwest::header::CACHE_CONTROL;
use reqwest::header::CONTENT_TYPE;
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
async fn chat_wire_api_rejects_responses_endpoint() {
    let response = proxy_request(
        OpenAiCompatState {
            upstream: Arc::new(UpstreamConfig {
                provider_id: "test-provider".to_string(),
                provider: ModelProviderInfo {
                    wire_api: WireApi::Chat,
                    ..provider("https://example.com/v1".to_string())
                },
                adapter: UpstreamAdapter::chat(),
                auth_manager: AuthManager::from_auth_for_testing(CodexAuth::from_api_key(
                    "ignored-auth",
                )),
            }),
            auth_token: None,
            client: Client::builder().build().expect("client"),
        },
        Method::POST,
        CompatEndpoint::Responses,
        None,
        HeaderMap::new(),
        Some("{}".to_string()),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
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
