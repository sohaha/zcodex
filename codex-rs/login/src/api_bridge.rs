use codex_api::CoreAuthProvider;
use codex_model_provider_info::ModelProviderInfo;

use crate::CodexAuth;

pub fn auth_provider_from_auth(
    auth: Option<CodexAuth>,
    provider: &ModelProviderInfo,
) -> codex_protocol::error::Result<CoreAuthProvider> {
    if let Some(api_key) = provider.api_key()? {
        return Ok(CoreAuthProvider {
            token: Some(api_key),
            account_id: None,
        });
    }

    if let Some(token) = provider.configured_bearer_token() {
        return Ok(CoreAuthProvider {
            token: Some(token.to_string()),
            account_id: None,
        });
    }

    if let Some(auth) = auth {
        let token = auth.get_token()?;
        Ok(CoreAuthProvider {
            token: Some(token),
            account_id: auth.get_account_id(),
        })
    } else {
        Ok(CoreAuthProvider {
            token: None,
            account_id: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::auth_provider_from_auth;
    use crate::CodexAuth;
    use codex_model_provider_info::ModelProviderInfo;
    use codex_model_provider_info::WireApi;
    use pretty_assertions::assert_eq;

    fn test_provider() -> ModelProviderInfo {
        ModelProviderInfo {
            name: "OpenAI compatible".to_string(),
            model: None,
            base_url: Some("https://example.com/v1".to_string()),
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
            websocket_connect_timeout_ms: None,
            requires_openai_auth: true,
            supports_websockets: false,
        }
    }

    #[test]
    fn empty_configured_bearer_token_falls_back_to_auth() {
        let mut provider = test_provider();
        provider.experimental_bearer_token = Some(String::new());

        let auth_provider =
            auth_provider_from_auth(Some(CodexAuth::from_api_key("auth-json-key")), &provider)
                .expect("auth provider should resolve");

        assert_eq!(auth_provider.token.as_deref(), Some("auth-json-key"));
        assert_eq!(auth_provider.account_id, None);
    }

    #[test]
    fn non_empty_configured_bearer_token_still_wins() {
        let mut provider = test_provider();
        provider.experimental_bearer_token = Some("provider-token".to_string());

        let auth_provider =
            auth_provider_from_auth(Some(CodexAuth::from_api_key("auth-json-key")), &provider)
                .expect("auth provider should resolve");

        assert_eq!(auth_provider.token.as_deref(), Some("provider-token"));
        assert_eq!(auth_provider.account_id, None);
    }
}
