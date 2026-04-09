use anyhow::Result;
use anyhow::bail;
use codex_core::WireApi;

use super::ApiError;
use super::translator::UpstreamTranslator;

#[derive(Clone, Copy)]
pub(super) struct UpstreamAdapter {
    spec: &'static UpstreamAdapterSpec,
}

struct UpstreamAdapterSpec {
    wire_api_name: &'static str,
    responses_path: Option<&'static str>,
    responses_translator: Option<UpstreamTranslator>,
}

const RESPONSES_ADAPTER: UpstreamAdapterSpec = UpstreamAdapterSpec {
    wire_api_name: "responses",
    responses_path: Some("/responses"),
    responses_translator: Some(UpstreamTranslator::Passthrough),
};

const CHAT_ADAPTER: UpstreamAdapterSpec = UpstreamAdapterSpec {
    wire_api_name: "chat",
    responses_path: Some("/chat/completions"),
    responses_translator: Some(UpstreamTranslator::ResponsesToChat),
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

    pub(super) fn wire_api_name(self) -> &'static str {
        self.spec.wire_api_name
    }

    pub(super) fn resolve_request(
        self,
        endpoint: CompatEndpoint,
    ) -> Result<ResolvedUpstreamRequest, ApiError> {
        let path = endpoint.resolve_path(self.spec)?;
        let translator = endpoint.resolve_translator(self.spec)?;
        Ok(ResolvedUpstreamRequest { path, translator })
    }
}

pub(super) struct ResolvedUpstreamRequest {
    pub(super) path: &'static str,
    pub(super) translator: UpstreamTranslator,
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

    fn resolve_translator(
        self,
        spec: &UpstreamAdapterSpec,
    ) -> Result<UpstreamTranslator, ApiError> {
        match self {
            Self::Models | Self::ChatCompletions => Ok(UpstreamTranslator::passthrough()),
            Self::Responses => spec
                .responses_translator
                .clone()
                .ok_or_else(|| ApiError::bad_request(self.unsupported_message())),
        }
    }

    fn unsupported_message(self) -> &'static str {
        match self {
            Self::Models => "current upstream adapter does not support /v1/models",
            Self::Responses => {
                "current upstream provider uses wire_api = \"chat\"; /v1/responses is not available yet, use /v1/chat/completions instead"
            }
            Self::ChatCompletions => {
                "current upstream adapter does not support /v1/chat/completions"
            }
        }
    }
}
