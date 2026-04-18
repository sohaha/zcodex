//! Registry of model providers supported by Codex.
//!
//! Providers can be defined in two places:
//!   1. Built-in defaults compiled into the binary so Codex works out-of-the-box.
//!   2. User-defined entries inside `~/.codex/config.toml` under the `model_providers`
//!      key. These override or extend the defaults at runtime.

use codex_api::Provider as ApiProvider;
use codex_api::RetryConfig as ApiRetryConfig;
use codex_api::provider::WireApi as ApiWireApi;
use codex_app_server_protocol::AuthMode;
use codex_protocol::config_types::ModelProviderAuthInfo;
use codex_protocol::error::CodexErr;
use codex_protocol::error::EnvVarError;
use codex_protocol::error::Result as CodexResult;
use http::HeaderMap;
use http::header::HeaderName;
use http::header::HeaderValue;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use std::fmt;
use std::time::Duration;

const DEFAULT_STREAM_IDLE_TIMEOUT_MS: u64 = 300_000;
const DEFAULT_STREAM_MAX_RETRIES: u64 = 5;
const DEFAULT_REQUEST_MAX_RETRIES: u64 = 4;
const DEFAULT_RETRY_BASE_DELAY_MS: u64 = 200;
pub const DEFAULT_WEBSOCKET_CONNECT_TIMEOUT_MS: u64 = 15_000;
/// Hard cap for user-configured `stream_max_retries`.
const MAX_STREAM_MAX_RETRIES: u64 = 100;
/// Hard cap for user-configured `request_max_retries`.
const MAX_REQUEST_MAX_RETRIES: u64 = 100;

const OPENAI_PROVIDER_NAME: &str = "OpenAI";
pub const OPENAI_PROVIDER_ID: &str = "openai";
pub const DEFAULT_OPENAI_BASE_URL: &str = "https://api.openai.com/v1";
pub const DEFAULT_CHATGPT_BASE_URL: &str = "https://chatgpt.com/backend-api/codex";
pub const DEFAULT_ANTHROPIC_BASE_URL: &str = "https://api.anthropic.com/v1";
const DEFAULT_ANTHROPIC_VERSION: &str = "2023-06-01";
pub const LEGACY_OLLAMA_CHAT_PROVIDER_ID: &str = "ollama-chat";
pub const OLLAMA_CHAT_PROVIDER_REMOVED_ERROR: &str = "`ollama-chat` is no longer supported.\nHow to fix: replace `ollama-chat` with `ollama` in `model_provider`, `oss_provider`, or `--local-provider`.\nMore info: https://github.com/openai/codex/discussions/7782";

/// Wire protocol that the provider speaks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum WireApi {
    /// The Responses API exposed by OpenAI at `/v1/responses`.
    #[default]
    Responses,
    /// Legacy Chat Completions API exposed by OpenAI at `/v1/chat/completions`.
    Chat,
    /// Anthropic Messages API exposed at `/v1/messages`.
    Anthropic,
}

impl fmt::Display for WireApi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Responses => "responses",
            Self::Chat => "chat",
            Self::Anthropic => "anthropic",
        };
        f.write_str(value)
    }
}

impl<'de> Deserialize<'de> for WireApi {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        match value.as_str() {
            "responses" => Ok(Self::Responses),
            "chat" => Ok(Self::Chat),
            "anthropic" => Ok(Self::Anthropic),
            _ => Err(serde::de::Error::unknown_variant(
                &value,
                &["responses", "chat", "anthropic"],
            )),
        }
    }
}

/// Serializable representation of a provider definition.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct ModelProviderInfo {
    /// Friendly display name.
    #[serde(default)]
    pub name: Option<String>,
    /// Optional fixed model override used by legacy configs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    /// Base URL for the provider's OpenAI-compatible API.
    pub base_url: Option<String>,
    /// Environment variable that stores the user's API key for this provider.
    pub env_key: Option<String>,
    /// Optional list of model slugs to restrict available models for this provider.
    /// When set, only these models will be shown in the model picker for this provider.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_catalog: Option<Vec<String>>,
    /// Optional instructions to help the user get a valid value for the
    /// variable and set it.
    pub env_key_instructions: Option<String>,
    /// Value to use with `Authorization: Bearer <token>` header. Use of this
    /// config is discouraged in favor of `env_key` for security reasons, but
    /// this may be necessary when using this programmatically.
    #[serde(alias = "api_key")]
    pub experimental_bearer_token: Option<String>,
    /// Command-backed bearer-token configuration for this provider.
    pub auth: Option<ModelProviderAuthInfo>,
    /// Which wire protocol this provider expects.
    #[serde(default)]
    pub wire_api: WireApi,
    /// Optional query parameters to append to the base URL.
    pub query_params: Option<HashMap<String, String>>,
    /// Additional HTTP headers to include in requests to this provider where
    /// the (key, value) pairs are the header name and value.
    pub http_headers: Option<HashMap<String, String>>,
    /// Optional HTTP headers to include in requests to this provider where the
    /// (key, value) pairs are the header name and _environment variable_ whose
    /// value should be used. If the environment variable is not set, or the
    /// value is empty, the header will not be included in the request.
    pub env_http_headers: Option<HashMap<String, String>>,
    /// Maximum number of times to retry a failed HTTP request to this provider.
    pub request_max_retries: Option<u64>,
    /// Number of times to retry reconnecting a dropped streaming response before failing.
    pub stream_max_retries: Option<u64>,
    /// Idle timeout (in milliseconds) to wait for activity on a streaming response before treating
    /// the connection as lost.
    pub stream_idle_timeout_ms: Option<u64>,
    /// Base delay (in milliseconds) for retry backoff. The actual delay between retries will be
    /// this value multiplied by 2^(attempt-1) with jitter.
    pub retry_base_delay_ms: Option<u64>,
    /// Maximum time (in milliseconds) to wait for a websocket connection attempt before treating
    /// it as failed.
    pub websocket_connect_timeout_ms: Option<u64>,
    /// Does this provider require an OpenAI API Key or ChatGPT login token? If true,
    /// user is presented with login screen on first run, and login preference and token/key
    /// are stored in auth.json. If false (which is the default), login screen is skipped,
    /// and API key (if needed) comes from the "env_key" environment variable.
    #[serde(default)]
    pub requires_openai_auth: bool,
    /// Whether this provider supports the Responses API WebSocket transport.
    #[serde(default)]
    pub supports_websockets: bool,
    /// Size of the context window for the model, in tokens.
    #[serde(default)]
    pub model_context_window: Option<i64>,
    /// Token usage threshold triggering auto-compaction of conversation history.
    #[serde(default)]
    pub model_auto_compact_token_limit: Option<i64>,
    /// Maximum number of tokens the model can generate for this provider.
    #[serde(default)]
    pub max_output_tokens: Option<i64>,
    /// When true, selecting this provider's model in the picker skips the
    /// reasoning-effort (thinking level) selection popup and uses the model's
    /// default reasoning effort directly.
    #[serde(default)]
    pub skip_reasoning_popup: bool,
}

impl ModelProviderInfo {
    pub fn log_safe_summary(&self) -> String {
        let sorted_names = |values: Option<&HashMap<String, String>>| {
            let mut names = values
                .into_iter()
                .flat_map(HashMap::keys)
                .cloned()
                .collect::<Vec<_>>();
            names.sort_unstable();
            names
        };

        format!(
            "ModelProviderInfo {{ name: {:?}, model: {:?}, base_url: {:?}, env_key: {:?}, model_catalog: {:?}, env_key_instructions_present: {}, experimental_bearer_token_configured: {}, auth_configured: {}, wire_api: {:?}, query_param_names: {:?}, http_header_names: {:?}, env_http_header_names: {:?}, request_max_retries: {:?}, stream_max_retries: {:?}, stream_idle_timeout_ms: {:?}, retry_base_delay_ms: {:?}, websocket_connect_timeout_ms: {:?}, requires_openai_auth: {}, supports_websockets: {}, model_context_window: {:?}, model_auto_compact_token_limit: {:?}, max_output_tokens: {:?}, skip_reasoning_popup: {} }}",
            self.name,
            self.model,
            self.base_url,
            self.env_key,
            self.model_catalog,
            self.env_key_instructions.is_some(),
            self.configured_bearer_token().is_some(),
            self.auth.is_some(),
            self.wire_api,
            sorted_names(self.query_params.as_ref()),
            sorted_names(self.http_headers.as_ref()),
            sorted_names(self.env_http_headers.as_ref()),
            self.request_max_retries,
            self.stream_max_retries,
            self.stream_idle_timeout_ms,
            self.retry_base_delay_ms,
            self.websocket_connect_timeout_ms,
            self.requires_openai_auth,
            self.supports_websockets,
            self.model_context_window,
            self.model_auto_compact_token_limit,
            self.max_output_tokens,
            self.skip_reasoning_popup
        )
    }

    pub fn validate(&self) -> std::result::Result<(), String> {
        let Some(auth) = self.auth.as_ref() else {
            return Ok(());
        };

        if auth.command.trim().is_empty() {
            return Err("provider auth.command must not be empty".to_string());
        }

        let mut conflicts = Vec::new();
        if self.env_key.is_some() {
            conflicts.push("env_key");
        }
        if self.experimental_bearer_token.is_some() {
            conflicts.push("experimental_bearer_token");
        }
        if self.requires_openai_auth {
            conflicts.push("requires_openai_auth");
        }

        if conflicts.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "provider auth cannot be combined with {}",
                conflicts.join(", ")
            ))
        }
    }

    fn build_header_map(&self) -> CodexResult<HeaderMap> {
        let capacity = self.http_headers.as_ref().map_or(0, HashMap::len)
            + self.env_http_headers.as_ref().map_or(0, HashMap::len);
        let mut headers = HeaderMap::with_capacity(capacity);
        if let Some(extra) = &self.http_headers {
            for (k, v) in extra {
                if let (Ok(name), Ok(value)) = (HeaderName::try_from(k), HeaderValue::try_from(v)) {
                    headers.insert(name, value);
                }
            }
        }

        if let Some(env_headers) = &self.env_http_headers {
            for (header, env_var) in env_headers {
                if let Ok(val) = std::env::var(env_var)
                    && !val.trim().is_empty()
                    && let (Ok(name), Ok(value)) =
                        (HeaderName::try_from(header), HeaderValue::try_from(val))
                {
                    headers.insert(name, value);
                }
            }
        }

        Ok(headers)
    }

    pub fn to_api_provider(&self, auth_mode: Option<AuthMode>) -> CodexResult<ApiProvider> {
        let default_base_url = match self.wire_api {
            WireApi::Anthropic => DEFAULT_ANTHROPIC_BASE_URL,
            _ => {
                if matches!(auth_mode, Some(AuthMode::Chatgpt)) {
                    DEFAULT_CHATGPT_BASE_URL
                } else {
                    DEFAULT_OPENAI_BASE_URL
                }
            }
        };
        let base_url = self
            .base_url
            .clone()
            .unwrap_or_else(|| default_base_url.to_string());

        let mut headers = self.build_header_map()?;
        if self.wire_api == WireApi::Anthropic {
            let _ = headers.insert(
                HeaderName::from_static("anthropic-version"),
                HeaderValue::from_static(DEFAULT_ANTHROPIC_VERSION),
            );
            if let Some(api_key) = self.api_key()? {
                let header_value = HeaderValue::from_str(&api_key).map_err(|err| {
                    CodexErr::InvalidRequest(format!("invalid x-api-key header: {err}"))
                })?;
                let _ = headers
                    .entry(HeaderName::from_static("x-api-key"))
                    .or_insert(header_value);
                let auth_value =
                    HeaderValue::from_str(&format!("Bearer {api_key}")).map_err(|err| {
                        CodexErr::InvalidRequest(format!("invalid Authorization header: {err}"))
                    })?;
                let _ = headers
                    .entry(http::header::AUTHORIZATION)
                    .or_insert(auth_value);
            }
        }
        let retry = ApiRetryConfig {
            max_attempts: self.request_max_retries(),
            base_delay: self.retry_base_delay(),
            retry_429: false,
            retry_5xx: true,
            retry_transport: true,
        };
        let wire_api = match self.wire_api {
            WireApi::Responses => ApiWireApi::Responses,
            WireApi::Chat => ApiWireApi::Chat,
            WireApi::Anthropic => ApiWireApi::Anthropic,
        };

        Ok(ApiProvider {
            name: self.name.clone().unwrap_or_else(|| "unknown".to_string()),
            base_url,
            wire_api,
            query_params: self.query_params.clone(),
            headers,
            retry,
            stream_idle_timeout: self.stream_idle_timeout(),
        })
    }

    /// If `env_key` is Some, returns the API key for this provider if present
    /// (and non-empty) in the environment. If `env_key` is required but
    /// cannot be found, returns an error.
    pub fn api_key(&self) -> CodexResult<Option<String>> {
        match &self.env_key {
            Some(env_key) => {
                let api_key = std::env::var(env_key)
                    .ok()
                    .filter(|v| !v.trim().is_empty())
                    .ok_or_else(|| {
                        CodexErr::EnvVar(EnvVarError {
                            var: env_key.clone(),
                            instructions: self.env_key_instructions.clone(),
                        })
                    })?;
                Ok(Some(api_key))
            }
            None => Ok(None),
        }
    }

    /// Effective maximum number of request retries for this provider.
    pub fn request_max_retries(&self) -> u64 {
        self.request_max_retries
            .unwrap_or(DEFAULT_REQUEST_MAX_RETRIES)
            .min(MAX_REQUEST_MAX_RETRIES)
    }

    /// Effective maximum number of stream reconnection attempts for this provider.
    pub fn stream_max_retries(&self) -> u64 {
        self.stream_max_retries
            .unwrap_or(DEFAULT_STREAM_MAX_RETRIES)
            .min(MAX_STREAM_MAX_RETRIES)
    }

    /// Effective idle timeout for streaming responses.
    pub fn stream_idle_timeout(&self) -> Duration {
        self.stream_idle_timeout_ms
            .map(Duration::from_millis)
            .unwrap_or(Duration::from_millis(DEFAULT_STREAM_IDLE_TIMEOUT_MS))
    }

    /// Effective base delay for retry backoff.
    pub fn retry_base_delay(&self) -> Duration {
        self.retry_base_delay_ms
            .map(Duration::from_millis)
            .unwrap_or(Duration::from_millis(DEFAULT_RETRY_BASE_DELAY_MS))
    }

    /// Effective timeout for websocket connect attempts.
    pub fn websocket_connect_timeout(&self) -> Duration {
        self.websocket_connect_timeout_ms
            .map(Duration::from_millis)
            .unwrap_or(Duration::from_millis(DEFAULT_WEBSOCKET_CONNECT_TIMEOUT_MS))
    }

    pub fn create_openai_provider(base_url: Option<String>) -> ModelProviderInfo {
        ModelProviderInfo {
            name: Some(OPENAI_PROVIDER_NAME.into()),
            model: None,
            base_url,
            env_key: None,
            env_key_instructions: None,
            experimental_bearer_token: None,
            auth: None,
            wire_api: WireApi::Responses,
            query_params: None,
            http_headers: Some(
                [("version".to_string(), env!("CARGO_PKG_VERSION").to_string())]
                    .into_iter()
                    .collect(),
            ),
            env_http_headers: Some(
                [
                    (
                        "OpenAI-Organization".to_string(),
                        "OPENAI_ORGANIZATION".to_string(),
                    ),
                    ("OpenAI-Project".to_string(), "OPENAI_PROJECT".to_string()),
                ]
                .into_iter()
                .collect(),
            ),
            // Use global defaults for retry/timeout unless overridden in config.toml.
            request_max_retries: None,
            stream_max_retries: None,
            stream_idle_timeout_ms: None,
            retry_base_delay_ms: None,
            websocket_connect_timeout_ms: None,
            requires_openai_auth: true,
            supports_websockets: true,
            model_context_window: None,
            model_auto_compact_token_limit: None,
            max_output_tokens: None,
            model_catalog: None,
            skip_reasoning_popup: false,
        }
    }

    pub fn is_openai(&self) -> bool {
        if self.name.as_deref() == Some(OPENAI_PROVIDER_NAME) {
            return true;
        }
        match self.base_url.as_deref() {
            Some(url) => url == DEFAULT_OPENAI_BASE_URL || url == DEFAULT_CHATGPT_BASE_URL,
            None => false,
        }
    }

    pub fn supports_remote_compaction(&self) -> bool {
        self.is_openai()
    }
    pub fn configured_bearer_token(&self) -> Option<&str> {
        self.experimental_bearer_token
            .as_deref()
            .map(str::trim)
            .filter(|token| !token.is_empty())
    }

    pub fn uses_provider_supplied_auth(&self) -> bool {
        if self.auth.is_some() || self.configured_bearer_token().is_some() {
            return true;
        }

        let has_auth_header = |headers: &HashMap<String, String>| {
            headers
                .keys()
                .any(|key| key.eq_ignore_ascii_case("authorization"))
        };

        self.http_headers.as_ref().is_some_and(has_auth_header)
            || self.env_http_headers.as_ref().is_some_and(has_auth_header)
    }

    pub fn uses_official_openai_api(&self) -> bool {
        if !self.is_openai() {
            return false;
        }
        match self.base_url.as_deref() {
            None => true,
            Some(url) => url == DEFAULT_OPENAI_BASE_URL || url == DEFAULT_CHATGPT_BASE_URL,
        }
    }

    pub fn uses_official_openai_responses_api(&self) -> bool {
        if self.wire_api != WireApi::Responses || !self.is_openai() {
            return false;
        }
        match self.base_url.as_deref() {
            None => true,
            Some(url) => url == DEFAULT_OPENAI_BASE_URL,
        }
    }

    pub fn has_command_auth(&self) -> bool {
        self.auth.is_some()
    }
}

pub const DEFAULT_LMSTUDIO_PORT: u16 = 1234;
pub const DEFAULT_OLLAMA_PORT: u16 = 11434;

pub const LMSTUDIO_OSS_PROVIDER_ID: &str = "lmstudio";
pub const OLLAMA_OSS_PROVIDER_ID: &str = "ollama";

/// Built-in default provider list.
pub fn built_in_model_providers(
    openai_base_url: Option<String>,
) -> HashMap<String, ModelProviderInfo> {
    use ModelProviderInfo as P;
    let openai_provider = P::create_openai_provider(openai_base_url);

    // We do not want to be in the business of adjucating which third-party
    // providers are bundled with Codex CLI, so we only include the OpenAI and
    // open source ("oss") providers by default. Users are encouraged to add to
    // `model_providers` in config.toml to add their own providers.
    [
        (OPENAI_PROVIDER_ID, openai_provider),
        (
            OLLAMA_OSS_PROVIDER_ID,
            create_oss_provider(DEFAULT_OLLAMA_PORT, WireApi::Responses),
        ),
        (
            LMSTUDIO_OSS_PROVIDER_ID,
            create_oss_provider(DEFAULT_LMSTUDIO_PORT, WireApi::Responses),
        ),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect()
}

pub fn create_oss_provider(default_provider_port: u16, wire_api: WireApi) -> ModelProviderInfo {
    // These CODEX_OSS_ environment variables are experimental: we may
    // switch to reading values from config.toml instead.
    let default_codex_oss_base_url = format!(
        "http://localhost:{codex_oss_port}/v1",
        codex_oss_port = std::env::var("CODEX_OSS_PORT")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(default_provider_port)
    );

    let codex_oss_base_url = std::env::var("CODEX_OSS_BASE_URL")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or(default_codex_oss_base_url);
    create_oss_provider_with_base_url(&codex_oss_base_url, wire_api)
}

pub fn create_oss_provider_with_base_url(base_url: &str, wire_api: WireApi) -> ModelProviderInfo {
    ModelProviderInfo {
        name: Some("gpt-oss".into()),
        model: None,
        base_url: Some(base_url.into()),
        env_key: None,
        env_key_instructions: None,
        experimental_bearer_token: None,
        auth: None,
        wire_api,
        query_params: None,
        http_headers: None,
        env_http_headers: None,
        request_max_retries: None,
        stream_max_retries: None,
        stream_idle_timeout_ms: None,
        retry_base_delay_ms: None,
        websocket_connect_timeout_ms: None,
        requires_openai_auth: false,
        model_context_window: None,
        model_auto_compact_token_limit: None,
        max_output_tokens: None,
        model_catalog: None,
        supports_websockets: false,
        skip_reasoning_popup: false,
    }
}
