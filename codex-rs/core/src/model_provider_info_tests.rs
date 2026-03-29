use super::*;
use pretty_assertions::assert_eq;

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
        wire_api: WireApi::Responses,
        query_params: Some(maplit::hashmap! {
            "api-version".to_string() => "2025-04-01-preview".to_string(),
        }),
        http_headers: None,
        env_http_headers: None,
        request_max_retries: None,
        stream_max_retries: None,
        stream_idle_timeout_ms: None,
        websocket_connect_timeout_ms: None,
        requires_openai_auth: false,
        supports_websockets: false,
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
        websocket_connect_timeout_ms: None,
        requires_openai_auth: false,
        supports_websockets: false,
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

    let provider: ModelProviderInfo = toml::from_str(provider_toml).unwrap();
    assert_eq!(expected_provider, provider);
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
        wire_api: WireApi::Responses,
        query_params: None,
        http_headers: Some(maplit::hashmap! {
            "authorization".to_string() => "Bearer token".to_string(),
        }),
        env_http_headers: None,
        request_max_retries: None,
        stream_max_retries: None,
        stream_idle_timeout_ms: None,
        websocket_connect_timeout_ms: None,
        requires_openai_auth: false,
        supports_websockets: false,
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
        wire_api: WireApi::Responses,
        query_params: None,
        http_headers: Some(maplit::hashmap! {
            "X-Test".to_string() => "value".to_string(),
        }),
        env_http_headers: None,
        request_max_retries: None,
        stream_max_retries: None,
        stream_idle_timeout_ms: None,
        websocket_connect_timeout_ms: None,
        requires_openai_auth: false,
        supports_websockets: false,
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
        wire_api: WireApi::Anthropic,
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

    let api_provider = provider
        .to_api_provider(None)
        .expect("anthropic provider should build");
    assert_eq!(api_provider.base_url, "https://api.anthropic.com/v1");
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
        wire_api: WireApi::Anthropic,
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
        wire_api: WireApi::Anthropic,
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
        experimental_bearer_token: Some("test-anthropic-api-key".into()),
        wire_api: WireApi::Anthropic,
        query_params: None,
        http_headers: Some(maplit::hashmap! {
            "Authorization".to_string() => "Bearer explicit-token".to_string(),
        }),
        env_http_headers: None,
        request_max_retries: None,
        stream_max_retries: None,
        stream_idle_timeout_ms: None,
        websocket_connect_timeout_ms: None,
        requires_openai_auth: false,
        supports_websockets: false,
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
