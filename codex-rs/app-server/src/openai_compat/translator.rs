use super::ApiError;
use super::adapter::CompatEndpoint;

#[derive(Clone, Copy)]
pub(super) enum UpstreamTranslator {
    Passthrough(PassthroughTranslator),
}

#[derive(Clone, Copy)]
pub(super) struct PassthroughTranslator;

pub(super) struct TranslatedRequest {
    pub(super) body: Option<String>,
}

impl UpstreamTranslator {
    pub(super) fn passthrough() -> Self {
        Self::Passthrough(PassthroughTranslator)
    }

    pub(super) fn translate_request(
        &self,
        endpoint: CompatEndpoint,
        body: Option<String>,
    ) -> Result<TranslatedRequest, ApiError> {
        match self {
            Self::Passthrough(translator) => translator.translate_request(endpoint, body),
        }
    }
}

impl PassthroughTranslator {
    fn translate_request(
        &self,
        _endpoint: CompatEndpoint,
        body: Option<String>,
    ) -> Result<TranslatedRequest, ApiError> {
        Ok(TranslatedRequest { body })
    }
}
