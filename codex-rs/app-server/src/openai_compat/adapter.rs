use anyhow::Result;
use anyhow::bail;
use codex_core::WireApi;

use super::ApiError;

#[derive(Clone, Copy)]
pub(super) struct UpstreamAdapter {
    spec: &'static UpstreamAdapterSpec,
}

struct UpstreamAdapterSpec {
    wire_api_name: &'static str,
    responses_path: Option<&'static str>,
}

const RESPONSES_ADAPTER: UpstreamAdapterSpec = UpstreamAdapterSpec {
    wire_api_name: "responses",
    responses_path: Some("/responses"),
};

const CHAT_ADAPTER: UpstreamAdapterSpec = UpstreamAdapterSpec {
    wire_api_name: "chat",
    responses_path: None,
};

#[derive(Clone, Copy, Debug)]
pub(super) enum CompatEndpoint {
    Models,
    Responses,
    ChatCompletions,
}

impl UpstreamAdapter {
    pub(super) fn responses() -> Self {
        Self {
            spec: &RESPONSES_ADAPTER,
        }
    }

    pub(super) fn chat() -> Self {
        Self {
            spec: &CHAT_ADAPTER,
        }
    }

    pub(super) fn from_wire_api(wire_api: WireApi) -> Result<Self> {
        match wire_api {
            WireApi::Responses => Ok(Self::responses()),
            WireApi::Chat => Ok(Self::chat()),
            WireApi::Anthropic => bail!(
                "`codex app-server openai-compat` does not support providers with wire_api = \"anthropic\""
            ),
        }
    }

    pub(super) fn wire_api_name(&self) -> &'static str {
        self.spec.wire_api_name
    }

    pub(super) fn resolve_request(
        &self,
        endpoint: CompatEndpoint,
    ) -> Result<ResolvedUpstreamRequest, ApiError> {
        let path = endpoint.resolve_path(self.spec)?;
        Ok(ResolvedUpstreamRequest { path })
    }
}

pub(super) struct ResolvedUpstreamRequest {
    pub(super) path: &'static str,
}

impl CompatEndpoint {
    fn resolve_path(self, spec: &UpstreamAdapterSpec) -> Result<&'static str, ApiError> {
        match self {
            Self::Models => Ok("/models"),
            Self::Responses => spec
                .responses_path
                .ok_or_else(|| ApiError::bad_request(self.unsupported_message())),
            Self::ChatCompletions => Ok("/chat/completions"),
        }
    }

    fn unsupported_message(self) -> &'static str {
        match self {
            Self::Models => "current upstream adapter does not support /v1/models",
            Self::Responses => "current upstream adapter does not support /v1/responses",
            Self::ChatCompletions => {
                "current upstream adapter does not support /v1/chat/completions"
            }
        }
    }
}
