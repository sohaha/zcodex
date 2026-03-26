use anyhow::Result;
use anyhow::bail;
use codex_core::WireApi;

use super::ApiError;

#[derive(Clone)]
pub(super) enum UpstreamAdapter {
    Responses(ResponsesUpstreamAdapter),
    Chat(ChatUpstreamAdapter),
}

#[derive(Clone)]
pub(super) struct ResponsesUpstreamAdapter;

#[derive(Clone)]
pub(super) struct ChatUpstreamAdapter;

#[derive(Clone, Copy, Debug)]
pub(super) enum CompatEndpoint {
    Models,
    Responses,
    ChatCompletions,
}

impl UpstreamAdapter {
    pub(super) fn responses() -> Self {
        Self::Responses(ResponsesUpstreamAdapter)
    }

    pub(super) fn chat() -> Self {
        Self::Chat(ChatUpstreamAdapter)
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
        match self {
            Self::Responses(_) => "responses",
            Self::Chat(_) => "chat",
        }
    }

    pub(super) fn upstream_path(&self, endpoint: CompatEndpoint) -> Result<&'static str, ApiError> {
        match self {
            Self::Responses(adapter) => adapter.upstream_path(endpoint),
            Self::Chat(adapter) => adapter.upstream_path(endpoint),
        }
    }
}

impl ResponsesUpstreamAdapter {
    fn upstream_path(&self, endpoint: CompatEndpoint) -> Result<&'static str, ApiError> {
        match endpoint {
            CompatEndpoint::Models => Ok("/models"),
            CompatEndpoint::Responses => Ok("/responses"),
            CompatEndpoint::ChatCompletions => Ok("/chat/completions"),
        }
    }
}

impl ChatUpstreamAdapter {
    fn upstream_path(&self, endpoint: CompatEndpoint) -> Result<&'static str, ApiError> {
        match endpoint {
            CompatEndpoint::Models => Ok("/models"),
            CompatEndpoint::Responses => Err(ApiError::bad_request(
                "current upstream adapter does not support /v1/responses",
            )),
            CompatEndpoint::ChatCompletions => Ok("/chat/completions"),
        }
    }
}
