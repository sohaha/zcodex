use super::*;
use codex_utils_absolute_path::AbsolutePathBuf;
use codex_utils_absolute_path::AbsolutePathBufGuard;
use pretty_assertions::assert_eq;
use std::num::NonZeroU64;
use tempfile::tempdir;

#[test]
fn test_deserialize_ollama_model_provider_toml() {
    let azure_provider_toml = r#"
name = "Ollama"
base_url = "http://localhost:11434/v1"
        "#;
    let expected_provider = ModelProviderInfo {
        name: "Ollama".into(),
        model: None,
        base_url: Some("http://localhost:11434/v1".into()),
        env_key: None,
        env_key_instructions: None,
        experimental_bearer_token: None,
        auth: None,
        wire_api: WireApi::Responses,
        query_params: None,
        http_headers: None,
        env_http_headers: None,
        request_max_retries: None,
        stream_max_retries: None,
        stream_idle_timeout_ms: None,
        retry_base_delay_ms: None,
        websocket_connect_timeout_ms: None,
        requires_openai_auth: false,
        supports_websockets: false,
        model_context_window: None,
        model_auto_compact_token_limit: None,
    };

    let provider: ModelProviderInfo = toml::from_str(azure_provider_toml).unwrap();
    assert_eq!(expected_provider, provider);
}

#[test]
fn test_deserialize_azure_model_provider_toml() {
    let azure_provider_toml = r#"
name = "Azure"
base_url = "https://xxxxx.openai.azure.com/openai"
env_key = "AZURE_OPENAI_API_KEY"
query_params = { api-version = "2025-04-01-preview" }
        "#;
    let expected_provider = ModelProviderInfo {
        name: "Azure".into(),
        model: None,
        base_url: Some("https://xxxxx.openai.azure.com/openai".into()),
        env_key: Some("AZURE_OPENAI_API_KEY".into()),
        env_key_instructions: None,
        experimental_bearer_token: None,
        auth: None,
        wire_api: WireApi::Responses,
        query_params: Some(maplit::hashmap! {
            "api-version".to_string() => "2025-04-01-preview".to_string(),
        }),
        http_headers: None,
        env_http_headers: None,
        request_max_retries: None,
        stream_max_retries: None,
        stream_idle_timeout_ms: None,
        retry_base_delay_ms: None,
        websocket_connect_timeout_ms: None,
        requires_openai_auth: false,
        supports_websockets: false,
        model_context_window: None,
        model_auto_compact_token_limit: None,
    };

    let provider: ModelProviderInfo = toml::from_str(azure_provider_toml).unwrap();
    assert_eq!(expected_provider, provider);
}

#[test]
fn test_deserialize_example_model_provider_toml() {
    let azure_provider_toml = r#"
name = "Example"
base_url = "https://example.com"
env_key = "API_KEY"
http_headers = { "X-Example-Header" = "example-value" }
env_http_headers = { "X-Example-Env-Header" = "EXAMPLE_ENV_VAR" }
        "#;
    let expected_provider = ModelProviderInfo {
        name: "Example".into(),
        model: None,
        base_url: Some("https://example.com".into()),
        env_key: Some("API_KEY".into()),
        env_key_instructions: None,
        experimental_bearer_token: None,
        auth: None,
        wire_api: WireApi::Responses,
        query_params: None,
        http_headers: Some(maplit::hashmap! {
            "X-Example-Header".to_string() => "example-value".to_string(),
        }),
        env_http_headers: Some(maplit::hashmap! {
            "X-Example-Env-Header".to_string() => "EXAMPLE_ENV_VAR".to_string(),
        }),
        request_max_retries: None,
        stream_max_retries: None,
        stream_idle_timeout_ms: None,
        retry_base_delay_ms: None,
        websocket_connect_timeout_ms: None,
        requires_openai_auth: false,
        supports_websockets: false,
        model_context_window: None,
        model_auto_compact_token_limit: None,
    };

    let provider: ModelProviderInfo = toml::from_str(azure_provider_toml).unwrap();
    assert_eq!(expected_provider, provider);
}

#[test]
fn test_deserialize_provider_api_key_alias() {
    let provider_toml = r#"
name = "Example"
api_key = "test-token"
        "#;
    let expected_provider = ModelProviderInfo {
        name: "Example".into(),
        model: None,
        base_url: None,
        env_key: None,
        env_key_instructions: None,
        experimental_bearer_token: Some("test-token".into()),
        auth: None,
        wire_api: WireApi::Responses,
        query_params: None,
        http_headers: None,
        env_http_headers: None,
        request_max_retries: None,
        stream_max_retries: None,
        stream_idle_timeout_ms: None,
        retry_base_delay_ms: None,
        websocket_connect_timeout_ms: None,
        requires_openai_auth: false,
        supports_websockets: false,
        model_context_window: None,
        model_auto_compact_token_limit: None,
    };

    let provider: ModelProviderInfo = toml::from_str(provider_toml).unwrap();
    assert_eq!(expected_provider, provider);
}

#[test]
fn log_safe_summary_redacts_sensitive_provider_fields() {
    let provider = ModelProviderInfo {
        name: Some("Example".into()),
        model: Some("gpt-5.4".into()),
        base_url: Some("https://example.com/v1".into()),
        env_key: Some("EXAMPLE_API_KEY".into()),
        env_key_instructions: Some("set EXAMPLE_API_KEY".into()),
        experimental_bearer_token: Some("super-secret-token".into()),
        auth: Some(ModelProviderAuthInfo {
            command: "./print-token".to_string(),
            args: vec!["inline-secret".to_string()],
            timeout_ms: NonZeroU64::new(5_000).unwrap(),
            refresh_interval_ms: 300_000,
            cwd: AbsolutePathBuf::from("/tmp/provider-auth"),
        }),
        wire_api: WireApi::Responses,
        query_params: Some(maplit::hashmap! {
            "api-version".to_string() => "secret-value".to_string(),
        }),
        http_headers: Some(maplit::hashmap! {
            "Authorization".to_string() => "Bearer super-secret-token".to_string(),
            "X-Test".to_string() => "visible".to_string(),
        }),
        env_http_headers: Some(maplit::hashmap! {
            "X-Env-Header".to_string() => "EXAMPLE_HEADER_ENV".to_string(),
        }),
        request_max_retries: Some(2),
        stream_max_retries: Some(3),
        stream_idle_timeout_ms: Some(4_000),
        retry_base_delay_ms: Some(250),
        websocket_connect_timeout_ms: Some(15_000),
        requires_openai_auth: false,
        supports_websockets: true,
        model_context_window: Some(128_000),
        model_auto_compact_token_limit: Some(96_000),
        max_output_tokens: Some(16_000),
        model_catalog: Some(vec!["gpt-5.4".into()]),
        skip_reasoning_popup: true,
    };

    let summary = provider.log_safe_summary();

    assert!(summary.contains("https://example.com/v1"));
    assert!(summary.contains("EXAMPLE_API_KEY"));
    assert!(summary.contains("experimental_bearer_token_configured: true"));
    assert!(summary.contains("auth_configured: true"));
    assert!(summary.contains("http_header_names: [\"Authorization\", \"X-Test\"]"));
    assert!(!summary.contains("super-secret-token"));
    assert!(!summary.contains("inline-secret"));
    assert!(!summary.contains("./print-token"));
    assert!(!summary.contains("/tmp/provider-auth"));
    assert!(!summary.contains("secret-value"));
}

#[test]
fn provider_supplied_auth_detects_configured_authorization_headers() {
    let provider = ModelProviderInfo {
        name: "Example".into(),
        model: None,
        base_url: None,
        env_key: None,
        env_key_instructions: None,
        experimental_bearer_token: None,
        auth: None,
        wire_api: WireApi::Responses,
        query_params: None,
        http_headers: Some(maplit::hashmap! {
            "authorization".to_string() => "Bearer token".to_string(),
        }),
        env_http_headers: None,
        request_max_retries: None,
        stream_max_retries: None,
        stream_idle_timeout_ms: None,
        retry_base_delay_ms: None,
        websocket_connect_timeout_ms: None,
        requires_openai_auth: false,
        supports_websockets: false,
        model_context_window: None,
        model_auto_compact_token_limit: None,
    };

    assert!(provider.uses_provider_supplied_auth());
}

#[test]
fn provider_supplied_auth_ignores_unrelated_headers() {
    let provider = ModelProviderInfo {
        name: "Example".into(),
        model: None,
        base_url: None,
        env_key: None,
        env_key_instructions: None,
        experimental_bearer_token: None,
        auth: None,
        wire_api: WireApi::Responses,
        query_params: None,
        http_headers: Some(maplit::hashmap! {
            "X-Test".to_string() => "value".to_string(),
        }),
        env_http_headers: None,
        request_max_retries: None,
        stream_max_retries: None,
        stream_idle_timeout_ms: None,
        retry_base_delay_ms: None,
        websocket_connect_timeout_ms: None,
        requires_openai_auth: false,
        supports_websockets: false,
        model_context_window: None,
        model_auto_compact_token_limit: None,
    };

    assert!(!provider.uses_provider_supplied_auth());
}

#[test]
fn test_deserialize_chat_wire_api() {
    let provider_toml = r#"
name = "OpenAI using Chat Completions"
base_url = "https://api.openai.com/v1"
env_key = "OPENAI_API_KEY"
wire_api = "chat"
        "#;

    let provider = toml::from_str::<ModelProviderInfo>(provider_toml)
        .expect("chat wire_api should deserialize");
    assert_eq!(provider.wire_api, WireApi::Chat);
}

#[test]
fn official_openai_api_detection_uses_wire_api_and_base_url() {
    let chat_provider = ModelProviderInfo {
        name: "OpenAI Chat".into(),
        model: None,
        base_url: Some(DEFAULT_OPENAI_BASE_URL.into()),
        env_key: None,
        env_key_instructions: None,
        experimental_bearer_token: None,
        auth: None,
        wire_api: WireApi::Chat,
        query_params: None,
        http_headers: None,
        env_http_headers: None,
        request_max_retries: None,
        stream_max_retries: None,
        stream_idle_timeout_ms: None,
        retry_base_delay_ms: None,
        websocket_connect_timeout_ms: None,
        requires_openai_auth: false,
        supports_websockets: false,
        model_context_window: None,
        model_auto_compact_token_limit: None,
    };
    assert!(chat_provider.uses_official_openai_api());
    assert!(!chat_provider.uses_official_openai_responses_api());

    let custom_provider = ModelProviderInfo {
        base_url: Some("https://example.com/v1".into()),
        ..chat_provider
    };
    assert!(!custom_provider.uses_official_openai_api());
}

#[test]
fn anthropic_provider_defaults_to_official_base_url() {
    let provider = ModelProviderInfo {
        name: "Anthropic".into(),
        model: None,
        base_url: None,
        env_key: None,
        env_key_instructions: None,
        experimental_bearer_token: None,
        auth: None,
        wire_api: WireApi::Anthropic,
        query_params: None,
        http_headers: None,
        env_http_headers: None,
        request_max_retries: None,
        stream_max_retries: None,
        stream_idle_timeout_ms: None,
        retry_base_delay_ms: None,
        websocket_connect_timeout_ms: None,
        requires_openai_auth: false,
        supports_websockets: false,
        model_context_window: None,
        model_auto_compact_token_limit: None,
    };

    let api_provider = provider
        .to_api_provider(None)
        .expect("anthropic provider should build");
    assert_eq!(api_provider.base_url, DEFAULT_ANTHROPIC_BASE_URL);
}

#[test]
fn anthropic_provider_honors_configured_base_url() {
    let provider = ModelProviderInfo {
        name: "Anthropic".into(),
        model: None,
        base_url: Some("https://proxy.example/v1".into()),
        env_key: None,
        env_key_instructions: None,
        experimental_bearer_token: None,
        auth: None,
        wire_api: WireApi::Anthropic,
        query_params: None,
        http_headers: None,
        env_http_headers: None,
        request_max_retries: None,
        stream_max_retries: None,
        stream_idle_timeout_ms: None,
        retry_base_delay_ms: None,
        websocket_connect_timeout_ms: None,
        requires_openai_auth: false,
        supports_websockets: false,
        model_context_window: None,
        model_auto_compact_token_limit: None,
    };

    let api_provider = provider
        .to_api_provider(None)
        .expect("anthropic provider should build");
    assert_eq!(api_provider.base_url, "https://proxy.example/v1");
}

#[test]
fn anthropic_provider_uses_api_key_for_authorization_and_x_api_key() {
    const API_KEY_ENV: &str = "CODEX_TEST_ANTHROPIC_API_KEY";
    const SUBPROCESS_ENV: &str = "CODEX_TEST_ANTHROPIC_PROVIDER_SUBPROCESS";

    if std::env::var_os(SUBPROCESS_ENV).is_none() {
        let output = std::process::Command::new(
            std::env::current_exe().expect("test binary path should resolve"),
        )
        .arg("--exact")
        .arg("model_provider_info::tests::anthropic_provider_uses_api_key_for_authorization_and_x_api_key")
        .env(SUBPROCESS_ENV, "1")
        .env(API_KEY_ENV, "test-anthropic-api-key")
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
        name: "Anthropic".into(),
        model: None,
        base_url: None,
        env_key: Some(API_KEY_ENV.to_string()),
        env_key_instructions: None,
        experimental_bearer_token: None,
        auth: None,
        wire_api: WireApi::Anthropic,
        query_params: None,
        http_headers: None,
        env_http_headers: None,
        request_max_retries: None,
        stream_max_retries: None,
        stream_idle_timeout_ms: None,
        retry_base_delay_ms: None,
        websocket_connect_timeout_ms: None,
        requires_openai_auth: false,
        supports_websockets: false,
        model_context_window: None,
        model_auto_compact_token_limit: None,
    };

    let api_provider = provider
        .to_api_provider(None)
        .expect("anthropic provider should build");

    assert_eq!(
        api_provider
            .headers
            .get("anthropic-version")
            .and_then(|value| value.to_str().ok()),
        Some("2023-06-01")
    );
    assert_eq!(
        api_provider
            .headers
            .get("x-api-key")
            .and_then(|value| value.to_str().ok()),
        Some("test-anthropic-api-key")
    );
    assert_eq!(
        api_provider
            .headers
            .get(http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok()),
        Some("Bearer test-anthropic-api-key")
    );
}

#[test]
fn anthropic_provider_preserves_explicit_authorization_header() {
    let provider = ModelProviderInfo {
        name: "Anthropic".into(),
        model: None,
        base_url: None,
        env_key: None,
        env_key_instructions: None,
        experimental_bearer_token: None,
        auth: None,
        wire_api: WireApi::Anthropic,
        query_params: None,
        http_headers: Some(maplit::hashmap! {
            "Authorization".to_string() => "Bearer explicit-token".to_string(),
        }),
        env_http_headers: None,
        request_max_retries: None,
        stream_max_retries: None,
        stream_idle_timeout_ms: None,
        retry_base_delay_ms: None,
        websocket_connect_timeout_ms: None,
        requires_openai_auth: false,
        supports_websockets: false,
        model_context_window: None,
        model_auto_compact_token_limit: None,
    };

    let api_provider = provider
        .to_api_provider(None)
        .expect("anthropic provider should build");

    assert_eq!(
        api_provider
            .headers
            .get(http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok()),
        Some("Bearer explicit-token")
    );
}

#[test]
fn test_deserialize_websocket_connect_timeout() {
    let provider_toml = r#"
name = "OpenAI"
base_url = "https://api.openai.com/v1"
websocket_connect_timeout_ms = 15000
supports_websockets = true
        "#;

    let provider: ModelProviderInfo = toml::from_str(provider_toml).unwrap();
    assert_eq!(provider.websocket_connect_timeout_ms, Some(15_000));
}

#[test]
fn test_supports_remote_compaction_for_openai() {
    let provider = ModelProviderInfo::create_openai_provider(/*base_url*/ None);

    assert!(provider.supports_remote_compaction());
}

#[test]
fn test_supports_remote_compaction_for_azure_name() {
    let provider = ModelProviderInfo {
        name: "Azure".into(),
        base_url: Some("https://example.com/openai".into()),
        env_key: Some("AZURE_OPENAI_API_KEY".into()),
        env_key_instructions: None,
        experimental_bearer_token: None,
        auth: None,
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

    assert!(provider.supports_remote_compaction());
}

#[test]
fn test_supports_remote_compaction_for_non_openai_non_azure_provider() {
    let provider = ModelProviderInfo {
        name: "Example".into(),
        base_url: Some("https://example.com/v1".into()),
        env_key: Some("API_KEY".into()),
        env_key_instructions: None,
        experimental_bearer_token: None,
        auth: None,
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

    assert!(!provider.supports_remote_compaction());
}

#[test]
fn test_deserialize_provider_auth_config_defaults() {
    let base_dir = tempdir().unwrap();
    let provider_toml = r#"
name = "Corp"

[auth]
command = "./scripts/print-token"
args = ["--format=text"]
        "#;

    let provider: ModelProviderInfo = {
        let _guard = AbsolutePathBufGuard::new(base_dir.path());
        toml::from_str(provider_toml).unwrap()
    };

    assert_eq!(
        provider.auth,
        Some(ModelProviderAuthInfo {
            command: "./scripts/print-token".to_string(),
            args: vec!["--format=text".to_string()],
            timeout_ms: NonZeroU64::new(5_000).unwrap(),
            refresh_interval_ms: 300_000,
            cwd: AbsolutePathBuf::resolve_path_against_base(".", base_dir.path()),
        })
    );
}

#[test]
fn test_deserialize_provider_auth_config_allows_zero_refresh_interval() {
    let base_dir = tempdir().unwrap();
    let provider_toml = r#"
name = "Corp"

[auth]
command = "./scripts/print-token"
refresh_interval_ms = 0
        "#;

    let provider: ModelProviderInfo = {
        let _guard = AbsolutePathBufGuard::new(base_dir.path());
        toml::from_str(provider_toml).unwrap()
    };

    let auth = provider.auth.expect("auth config should deserialize");
    assert_eq!(auth.refresh_interval_ms, 0);
    assert_eq!(auth.refresh_interval(), None);
}
