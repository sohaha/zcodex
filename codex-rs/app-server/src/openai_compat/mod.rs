use std::collections::HashMap;
use std::collections::HashSet;
use std::io::Error as IoError;
use std::io::ErrorKind;
use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use axum::Router;
use axum::body::Body;
use axum::extract::RawQuery;
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

mod adapter;
#[cfg(test)]
mod tests;

use self::adapter::CompatEndpoint;
use self::adapter::UpstreamAdapter;

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

const REQUEST_HEADER_ALLOWLIST: &[&str] = &[
    "accept",
    "accept-encoding",
    "content-encoding",
    "content-type",
    "idempotency-key",
    "openai-beta",
    "openai-organization",
    "openai-project",
    "user-agent",
];

const RESPONSE_HEADER_ALLOWLIST: &[&str] = &[
    "cache-control",
    "content-type",
    "openai-model",
    "openai-processing-ms",
    "request-id",
    "retry-after",
    "x-request-id",
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
    adapter: UpstreamAdapter,
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

    let listener = tokio::net::TcpListener::bind(args.listen).await?;
    info!(listen = %args.listen, "openai-compatible HTTP proxy listening");
    axum::serve(listener, app_router(state).into_make_service()).await
}

fn app_router(state: OpenAiCompatState) -> Router {
    Router::new()
        .route("/v1/models", get(get_models))
        .route("/v1/responses", post(post_responses))
        .route("/v1/chat/completions", post(post_chat_completions))
        .with_state(state)
}

async fn get_models(
    State(state): State<OpenAiCompatState>,
    RawQuery(raw_query): RawQuery,
    headers: HeaderMap,
) -> Response<Body> {
    proxy_request(
        state,
        Method::GET,
        CompatEndpoint::Models,
        raw_query,
        headers,
        None,
    )
    .await
}

async fn post_responses(
    State(state): State<OpenAiCompatState>,
    RawQuery(raw_query): RawQuery,
    headers: HeaderMap,
    body: String,
) -> Response<Body> {
    proxy_request(
        state,
        Method::POST,
        CompatEndpoint::Responses,
        raw_query,
        headers,
        Some(body),
    )
    .await
}

async fn post_chat_completions(
    State(state): State<OpenAiCompatState>,
    RawQuery(raw_query): RawQuery,
    headers: HeaderMap,
    body: String,
) -> Response<Body> {
    proxy_request(
        state,
        Method::POST,
        CompatEndpoint::ChatCompletions,
        raw_query,
        headers,
        Some(body),
    )
    .await
}

async fn proxy_request(
    state: OpenAiCompatState,
    method: Method,
    endpoint: CompatEndpoint,
    raw_query: Option<String>,
    incoming_headers: HeaderMap,
    body: Option<String>,
) -> Response<Body> {
    if let Err(err) = authorize(&state, &incoming_headers) {
        return err.to_response();
    }

    let resolved_request = match state.upstream.adapter.resolve_request(endpoint) {
        Ok(request) => request,
        Err(err) => return err.to_response(),
    };
    let upstream_url = match build_upstream_url(
        &state.upstream.provider,
        resolved_request.path,
        raw_query.as_deref(),
    ) {
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
        if should_forward_request_header(name) {
            headers.append(name.clone(), value.clone());
        }
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
    if let Some(api_key) = provider_env_api_key(&upstream.provider)? {
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
    if let Some(token) = auth
        .map(|resolved| resolved.get_token())
        .transpose()
        .map_err(|err| {
            ApiError::bad_gateway(format!("failed to load upstream auth token: {err}"))
        })?
    {
        return Ok(Some(token));
    }

    if let Some(env_key) = &upstream.provider.env_key {
        return Err(ApiError::bad_gateway(format!(
            "missing upstream API key in environment variable {env_key} and no fallback bearer token is configured"
        )));
    }

    Ok(None)
}

fn provider_env_api_key(provider: &ModelProviderInfo) -> Result<Option<String>, ApiError> {
    let Some(env_key) = &provider.env_key else {
        return Ok(None);
    };

    Ok(std::env::var(env_key)
        .ok()
        .filter(|value| !value.trim().is_empty()))
}

fn build_upstream_url(
    provider: &ModelProviderInfo,
    path: &str,
    raw_query: Option<&str>,
) -> Result<Url, ApiError> {
    let base_url = provider
        .base_url
        .clone()
        .ok_or_else(|| ApiError::bad_request("current provider has no configured base_url"))?;
    let trimmed_base = base_url.trim_end_matches('/');
    let trimmed_path = path.trim_start_matches('/');
    let mut url = Url::parse(&format!("{trimmed_base}/{trimmed_path}"))
        .map_err(|err| ApiError::internal(format!("invalid upstream URL: {err}")))?;

    let query_pairs = merge_query_pairs(raw_query, provider.query_params.as_ref())?;
    if !query_pairs.is_empty() {
        let mut pairs = url.query_pairs_mut();
        for (key, value) in query_pairs {
            pairs.append_pair(&key, &value);
        }
    }

    Ok(url)
}

fn merge_query_pairs(
    raw_query: Option<&str>,
    provider_query: Option<&HashMap<String, String>>,
) -> Result<Vec<(String, String)>, ApiError> {
    let mut merged = parse_query_pairs(raw_query)?;
    let mut caller_keys = merged
        .iter()
        .map(|(key, _)| key.clone())
        .collect::<HashSet<_>>();

    if let Some(provider_query) = provider_query {
        for (key, value) in provider_query {
            if !caller_keys.contains(key) {
                merged.push((key.clone(), value.clone()));
                caller_keys.insert(key.clone());
            }
        }
    }

    Ok(merged)
}

fn parse_query_pairs(raw_query: Option<&str>) -> Result<Vec<(String, String)>, ApiError> {
    let Some(query) = raw_query.filter(|query| !query.is_empty()) else {
        return Ok(Vec::new());
    };

    validate_query_string(query)?;
    Url::parse(&format!("http://localhost/?{query}"))
        .map(|url| {
            url.query_pairs()
                .map(|(key, value)| (key.into_owned(), value.into_owned()))
                .collect()
        })
        .map_err(|err| ApiError::bad_request(format!("invalid query string: {err}")))
}

fn validate_query_string(query: &str) -> Result<(), ApiError> {
    let bytes = query.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let Some(first) = bytes.get(index + 1) else {
                return Err(ApiError::bad_request(
                    "invalid query string: incomplete percent-encoding",
                ));
            };
            let Some(second) = bytes.get(index + 2) else {
                return Err(ApiError::bad_request(
                    "invalid query string: incomplete percent-encoding",
                ));
            };
            if !first.is_ascii_hexdigit() || !second.is_ascii_hexdigit() {
                return Err(ApiError::bad_request(
                    "invalid query string: invalid percent-encoding",
                ));
            }
            index += 3;
            continue;
        }
        index += 1;
    }
    Ok(())
}

fn should_forward_request_header(name: &HeaderName) -> bool {
    let lower = name.as_str().to_ascii_lowercase();
    !is_hop_by_hop_header(name)
        && !matches!(
            lower.as_str(),
            "authorization"
                | "cookie"
                | "forwarded"
                | "x-forwarded-for"
                | "x-forwarded-host"
                | "x-forwarded-port"
                | "x-forwarded-proto"
                | "x-real-ip"
        )
        && (REQUEST_HEADER_ALLOWLIST
            .iter()
            .any(|candidate| lower.eq_ignore_ascii_case(candidate))
            || lower.starts_with("openai-")
            || lower.starts_with("x-stainless-"))
}

fn should_forward_response_header(name: &HeaderName) -> bool {
    let lower = name.as_str().to_ascii_lowercase();
    !is_hop_by_hop_header(name)
        && !lower.eq_ignore_ascii_case("set-cookie")
        && (RESPONSE_HEADER_ALLOWLIST
            .iter()
            .any(|candidate| lower.eq_ignore_ascii_case(candidate))
            || lower.starts_with("openai-")
            || lower.starts_with("x-request-"))
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
        if should_forward_response_header(name) {
            response = response.header(name, value);
        }
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
    let adapter = UpstreamAdapter::from_wire_api(config.model_provider.wire_api)?;

    let auth_manager = AuthManager::shared(
        config.codex_home.clone(),
        false,
        config.cli_auth_credentials_store_mode,
    );

    Ok(UpstreamConfig {
        provider_id: config.model_provider_id.clone(),
        provider: config.model_provider,
        adapter,
        auth_manager,
    })
}

fn ensure_supported_provider(config: &Config) -> Result<()> {
    ensure_supported_provider_info(&config.model_provider_id, &config.model_provider)
}

fn ensure_supported_provider_info(provider_id: &str, provider: &ModelProviderInfo) -> Result<()> {
    let adapter = UpstreamAdapter::from_wire_api(provider.wire_api)?;
    if provider.base_url.is_none() {
        bail!(
            "current provider `{provider_id}` has no base_url; openai-compatible proxy requires an explicit upstream base_url",
        );
    }

    if matches!(provider.wire_api, WireApi::Anthropic) {
        bail!(
            "`codex app-server openai-compat` does not support providers with wire_api = \"{}\"",
            adapter.wire_api_name(),
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
