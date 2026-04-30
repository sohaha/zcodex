use codex_model_provider_info::ModelProviderInfo;
use codex_model_provider_info::WireApi;
use serde::Deserialize;
use serde::Serialize;
use std::io;
use std::io::ErrorKind;
use std::path::PathBuf;
use tokio::fs;
use tracing::error;

pub const MODEL_REFRESH_STATE_FILE: &str = "models_refresh_state.json";

/// Manages provider refresh capabilities persisted across process restarts.
#[derive(Debug)]
pub struct ModelsRefreshStateManager {
    state_path: PathBuf,
}

impl ModelsRefreshStateManager {
    pub fn new(state_path: PathBuf) -> Self {
        Self { state_path }
    }

    pub async fn is_models_endpoint_unsupported(&self, provider: &ModelProviderInfo) -> bool {
        let signature = UnsupportedModelsEndpointProvider::from_provider(provider);
        match self.load().await {
            Ok(Some(state)) => state
                .unsupported_models_endpoint_providers
                .iter()
                .any(|entry| entry == &signature),
            Ok(None) => false,
            Err(err) => {
                error!("failed to load models refresh state: {err}");
                false
            }
        }
    }

    pub async fn mark_models_endpoint_unsupported(
        &self,
        provider: &ModelProviderInfo,
    ) -> io::Result<bool> {
        let signature = UnsupportedModelsEndpointProvider::from_provider(provider);
        let mut state = self.load().await?.unwrap_or_default();
        if state
            .unsupported_models_endpoint_providers
            .iter()
            .any(|entry| entry == &signature)
        {
            return Ok(false);
        }

        state.unsupported_models_endpoint_providers.push(signature);
        self.save(&state).await?;
        Ok(true)
    }

    async fn load(&self) -> io::Result<Option<ModelsRefreshState>> {
        match fs::read(&self.state_path).await {
            Ok(contents) => {
                let state = serde_json::from_slice(&contents)
                    .map_err(|err| io::Error::new(ErrorKind::InvalidData, err.to_string()))?;
                Ok(Some(state))
            }
            Err(err) if err.kind() == ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err),
        }
    }

    async fn save(&self, state: &ModelsRefreshState) -> io::Result<()> {
        if let Some(parent) = self.state_path.parent() {
            fs::create_dir_all(parent).await?;
        }
        let json = serde_json::to_vec_pretty(state)
            .map_err(|err| io::Error::new(ErrorKind::InvalidData, err.to_string()))?;
        fs::write(&self.state_path, json).await
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelsRefreshState {
    #[serde(default)]
    unsupported_models_endpoint_providers: Vec<UnsupportedModelsEndpointProvider>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UnsupportedModelsEndpointProvider {
    base_url: Option<String>,
    wire_api: WireApi,
    model_catalog: Option<Vec<String>>,
}

impl UnsupportedModelsEndpointProvider {
    fn from_provider(provider: &ModelProviderInfo) -> Self {
        let mut model_catalog = provider.model_catalog.clone();
        if let Some(entries) = model_catalog.as_mut() {
            entries.sort_unstable();
            entries.dedup();
            if entries.is_empty() {
                model_catalog = None;
            }
        }

        Self {
            base_url: provider
                .base_url
                .clone()
                .filter(|value| !value.trim().is_empty()),
            wire_api: provider.wire_api,
            model_catalog,
        }
    }
}
