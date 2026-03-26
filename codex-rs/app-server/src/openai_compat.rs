use std::collections::HashMap;
use std::io::Error as IoError;
use std::io::ErrorKind;
use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use axum::Router;
use axum::body::Body;
use axum::extract::Query;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::http::HeaderName;
use axum::http::HeaderValue;
use axum::http::Method;
use axum::http::Response;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::routing::post;
use clap::Args;
use codex_arg0::Arg0DispatchPaths;
use codex_core::AuthManager;
use codex_core::ModelProviderInfo;
use codex_core::WireApi;
use codex_core::config::Config;
use codex_core::config::ConfigBuilder;
use codex_core::config_loader::LoaderOverrides;
use codex_utils_cli::CliConfigOverrides;
use reqwest::Client;
use reqwest::Url;
use tracing::info;

const HOP_BY_HOP_HEADERS: &[&str] = &[
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
    "host",
    "content-length",
];

#[derive(Debug, Clone, Args)]
pub struct OpenAiCompatServerArgs {
    /// HTTP 监听地址。默认仅监听本机回环地址。
    #[arg(long = "listen", default_value = "127.0.0.1:8080")]
    pub listen: SocketAddr,

    /// 若设置，则要求请求携带匹配的 `Authorization: Bearer <token>`。
    #[arg(long = "auth-token-env", value_name = "ENV")]
    pub auth_token_env: Option<String>,
}

#[derive(Clone)]
struct OpenAiCompatState {
    upstream: Arc<UpstreamConfig>,
    auth_token: Option<Arc<str>>,
    client: Client,
}

#[derive(Clone)]
struct UpstreamConfig {
    provider_id: String,
    provider: ModelProviderInfo,
    auth_manager: Arc<AuthManager>,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message: message.into(),
        }
    }

    fn bad_gateway(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            message: message.into(),
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.into(),
        }
    }

    fn to_response(&self) -> Response<Body> {
        (
            self.status,
            axum::Json(serde_json::json!({
                "error": {
                    "message": self.message,
                    "type": status_error_type(self.status),
                }
            })),
        )
            .into_response()
    }
}

fn status_error_type(status: StatusCode) -> &'static str {
    match status {
        StatusCode::BAD_REQUEST => "invalid_request_error",
        StatusCode::UNAUTHORIZED => "authentication_error",
        StatusCode::BAD_GATEWAY => "api_connection_error",
        _ => "server_error",
    }
}

pub async fn run_openai_compat_server(
    _arg0_paths: Arg0DispatchPaths,
    cli_config_overrides: CliConfigOverrides,
    loader_overrides: LoaderOverrides,
    args: OpenAiCompatServerArgs,
) -> std::io::Result<()> {
    let upstream = Arc::new(
        build_upstream_config(cli_config_overrides, loader_overrides)
            .await
            .map_err(to_io_error)?,
    );
    let auth_token = read_auth_token(args.auth_token_env.as_deref()).map_err(to_io_error)?;
    let state = OpenAiCompatState {
        upstream,
        auth_token: auth_token.map(Arc::<str>::from),
        client: Client::builder()
            .build()
            .map_err(|err| IoError::other(format!("failed to build reqwest client: {err}")))?,
    };

    let router = Router::new()
        .route("/v1/models", get(get_models))
        .route("/v1/responses", post(post_responses))
        .route("/v1/chat/completions", post(post_chat_completions))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(args.listen).await?;
    info!(listen = %args.listen, "openai-compatible HTTP proxy listening");
    axum::serve(listener, router.into_make_service()).await
}

async fn get_models(
    State(state): State<OpenAiCompatState>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Response<Body> {
    proxy_request(state, Method::GET, "/models", query, headers, None).await
}

async fn post_responses(
    State(state): State<OpenAiCompatState>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
    body: String,
) -> Response<Body> {
    proxy_request(
        state,
        Method::POST,
        "/responses",
        query,
        headers,
        Some(body),
    )
    .await
}

async fn post_chat_completions(
    State(state): State<OpenAiCompatState>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
    body: String,
) -> Response<Body> {
    proxy_request(
        state,
        Method::POST,
        "/chat/completions",
        query,
        headers,
        Some(body),
    )
    .await
}

async fn proxy_request(
    state: OpenAiCompatState,
    method: Method,
    path: &str,
    query: HashMap<String, String>,
    incoming_headers: HeaderMap,
    body: Option<String>,
) -> Response<Body> {
    if let Err(err) = authorize(&state, &incoming_headers) {
        return err.to_response();
    }

    let upstream_url = match build_upstream_url(&state.upstream.provider, path, &query) {
        Ok(url) => url,
        Err(err) => return err.to_response(),
    };

    let upstream_headers = match build_upstream_headers(&state.upstream, &incoming_headers).await {
        Ok(headers) => headers,
        Err(err) => return err.to_response(),
    };

    let mut request = state
        .client
        .request(method.clone(), upstream_url.clone())
        .headers(upstream_headers);
    if let Some(body) = body {
        request = request.body(body);
    }

    let upstream_response = match request.send().await {
        Ok(response) => response,
        Err(err) => {
            return ApiError::bad_gateway(format!(
                "failed to proxy {} {} via provider `{}`: {err}",
                method, upstream_url, state.upstream.provider_id,
            ))
            .to_response();
        }
    };

    response_from_upstream(upstream_response).await
}

fn authorize(state: &OpenAiCompatState, headers: &HeaderMap) -> Result<(), ApiError> {
    let Some(expected) = state.auth_token.as_deref() else {
        return Ok(());
    };
    let actual = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .ok_or_else(|| ApiError::unauthorized("missing Authorization: Bearer token"))?;
    if actual == expected {
        Ok(())
    } else {
        Err(ApiError::unauthorized("invalid bearer token"))
    }
}

async fn build_upstream_headers(
    upstream: &UpstreamConfig,
    incoming_headers: &HeaderMap,
) -> Result<HeaderMap, ApiError> {
    let mut headers = HeaderMap::new();

    for (name, value) in incoming_headers {
        if is_hop_by_hop_header(name) || *name == axum::http::header::AUTHORIZATION {
            continue;
        }
        headers.append(name.clone(), value.clone());
    }

    if let Some(static_headers) = &upstream.provider.http_headers {
        apply_config_headers(&mut headers, static_headers)?;
    }
    if let Some(env_headers) = &upstream.provider.env_http_headers {
        apply_env_headers(&mut headers, env_headers);
    }

    if !headers.contains_key(axum::http::header::AUTHORIZATION)
        && let Some(bearer) = resolve_provider_bearer_token(upstream).await?
    {
        let mut value = HeaderValue::from_str(&format!("Bearer {bearer}"))
            .map_err(|err| ApiError::internal(format!("invalid bearer token header: {err}")))?;
        value.set_sensitive(true);
        headers.insert(axum::http::header::AUTHORIZATION, value);
    }

    Ok(headers)
}

fn apply_config_headers(
    headers: &mut HeaderMap,
    configured: &HashMap<String, String>,
) -> Result<(), ApiError> {
    for (name, value) in configured {
        let name = HeaderName::try_from(name.as_str())
            .map_err(|err| ApiError::internal(format!("invalid configured header name: {err}")))?;
        let value = HeaderValue::from_str(value)
            .map_err(|err| ApiError::internal(format!("invalid configured header value: {err}")))?;
        headers.insert(name, value);
    }
    Ok(())
}

fn apply_env_headers(headers: &mut HeaderMap, configured: &HashMap<String, String>) {
    for (name, env_var) in configured {
        if let Ok(value) = std::env::var(env_var)
            && !value.trim().is_empty()
            && let (Ok(name), Ok(value)) = (
                HeaderName::try_from(name.as_str()),
                HeaderValue::from_str(&value),
            )
        {
            headers.insert(name, value);
        }
    }
}

async fn resolve_provider_bearer_token(
    upstream: &UpstreamConfig,
) -> Result<Option<String>, ApiError> {
    if let Some(api_key) = upstream.provider.api_key().map_err(provider_auth_error)? {
        return Ok(Some(api_key));
    }

    if let Some(token) = upstream
        .provider
        .experimental_bearer_token
        .as_deref()
        .filter(|token| !token.trim().is_empty())
    {
        return Ok(Some(token.to_string()));
    }

    let auth = upstream.auth_manager.auth().await;
    auth.map(|auth| auth.get_token())
        .transpose()
        .map_err(|err| ApiError::bad_gateway(format!("failed to load upstream auth token: {err}")))
}

fn provider_auth_error(err: codex_core::error::CodexErr) -> ApiError {
    ApiError::bad_gateway(format!("failed to resolve provider auth: {err}"))
}

fn build_upstream_url(
    provider: &ModelProviderInfo,
    path: &str,
    incoming_query: &HashMap<String, String>,
) -> Result<Url, ApiError> {
    let base_url = provider
        .base_url
        .clone()
        .ok_or_else(|| ApiError::bad_request("current provider has no configured base_url"))?;
    let trimmed_base = base_url.trim_end_matches('/');
    let trimmed_path = path.trim_start_matches('/');
    let mut url = Url::parse(&format!("{trimmed_base}/{trimmed_path}"))
        .map_err(|err| ApiError::internal(format!("invalid upstream URL: {err}")))?;

    {
        let mut pairs = url.query_pairs_mut();
        for (key, value) in incoming_query {
            pairs.append_pair(key, value);
        }
        if let Some(provider_query) = &provider.query_params {
            for (key, value) in provider_query {
                pairs.append_pair(key, value);
            }
        }
    }

    Ok(url)
}

fn is_hop_by_hop_header(name: &HeaderName) -> bool {
    HOP_BY_HOP_HEADERS
        .iter()
        .any(|candidate| name.as_str().eq_ignore_ascii_case(candidate))
}

async fn response_from_upstream(upstream: reqwest::Response) -> Response<Body> {
    let status = upstream.status();
    let headers = upstream.headers().clone();
    let mut response = Response::builder().status(status.as_u16());
    for (name, value) in &headers {
        if is_hop_by_hop_header(name) {
            continue;
        }
        response = response.header(name, value);
    }

    match response.body(Body::from_stream(upstream.bytes_stream())) {
        Ok(response) => response,
        Err(err) => {
            ApiError::internal(format!("failed to build proxy response: {err}")).to_response()
        }
    }
}

async fn build_upstream_config(
    cli_config_overrides: CliConfigOverrides,
    loader_overrides: LoaderOverrides,
) -> Result<UpstreamConfig> {
    let cli_kv_overrides = cli_config_overrides
        .parse_overrides()
        .map_err(anyhow::Error::msg)?;
    let config = ConfigBuilder::default()
        .cli_overrides(cli_kv_overrides)
        .loader_overrides(loader_overrides)
        .build()
        .await
        .context("failed to load config for openai-compatible proxy")?;

    ensure_supported_provider(&config)?;

    let auth_manager = AuthManager::shared(
        config.codex_home.clone(),
        false,
        config.cli_auth_credentials_store_mode,
    );

    Ok(UpstreamConfig {
        provider_id: config.model_provider_id.clone(),
        provider: config.model_provider,
        auth_manager,
    })
}

fn ensure_supported_provider(config: &Config) -> Result<()> {
    if config.model_provider.wire_api != WireApi::Responses {
        bail!(
            "`codex app-server openai-compat` currently only supports providers with wire_api = \"responses\"; current provider `{}` uses `{}`",
            config.model_provider_id,
            config.model_provider.wire_api,
        );
    }
    if config.model_provider.base_url.is_none() {
        bail!(
            "current provider `{}` has no base_url; openai-compatible proxy requires an explicit upstream base_url",
            config.model_provider_id,
        );
    }
    Ok(())
}

fn read_auth_token(auth_token_env: Option<&str>) -> Result<Option<String>> {
    let Some(env_name) = auth_token_env else {
        return Ok(None);
    };
    let token = std::env::var(env_name).with_context(|| {
        format!("failed to read auth token from environment variable {env_name}")
    })?;
    if token.is_empty() {
        bail!("environment variable {env_name} is empty");
    }
    Ok(Some(token))
}

fn to_io_error(err: anyhow::Error) -> IoError {
    IoError::new(ErrorKind::InvalidData, err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_core::CodexAuth;
    use pretty_assertions::assert_eq;
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

    #[test]
    fn build_upstream_url_appends_provider_query_params() {
        let url = build_upstream_url(
            &provider("https://example.com/v1".to_string()),
            "/chat/completions",
            &HashMap::from([("stream".to_string(), "true".to_string())]),
        )
        .expect("url");

        assert_eq!(
            url.as_str(),
            "https://example.com/v1/chat/completions?stream=true&api-version=2025-04-01-preview"
        );
    }

    #[tokio::test]
    async fn build_upstream_headers_prefers_provider_bearer_token() {
        let upstream = UpstreamConfig {
            provider_id: "test-provider".to_string(),
            provider: provider("https://example.com/v1".to_string()),
            auth_manager: AuthManager::from_auth_for_testing(CodexAuth::from_api_key(
                "ignored-auth",
            )),
        };
        let headers = build_upstream_headers(&upstream, &HeaderMap::new())
            .await
            .expect("headers");
        assert_eq!(headers["authorization"], "Bearer provider-token");
        assert_eq!(headers["x-provider-header"], "present");
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

        let state = OpenAiCompatState {
            upstream: Arc::new(UpstreamConfig {
                provider_id: "test-provider".to_string(),
                provider: provider(format!("{}/v1", server.uri())),
                auth_manager: AuthManager::from_auth_for_testing(CodexAuth::from_api_key(
                    "ignored-auth",
                )),
            }),
            auth_token: None,
            client: Client::builder().build().expect("client"),
        };

        let response = proxy_request(
            state,
            Method::POST,
            "/chat/completions",
            HashMap::new(),
            HeaderMap::new(),
            Some("{}".to_string()),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
    }
}
