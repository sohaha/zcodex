use super::cache::ModelsCacheManager;
use crate::collaboration_mode_presets::CollaborationModesConfig;
use crate::collaboration_mode_presets::builtin_collaboration_mode_presets;
use crate::config::ModelsManagerConfig;
use crate::model_info;
use codex_api::ModelsClient;
use codex_api::RequestTelemetry;
use codex_api::ReqwestTransport;
use codex_api::auth_header_telemetry;
use codex_api::TransportError;
use codex_api::map_api_error;
use codex_app_server_protocol::AuthMode;
use codex_feedback::FeedbackRequestTags;
use codex_feedback::emit_feedback_request_tags_with_auth_env;
use codex_login::AuthEnvTelemetry;
use codex_login::AuthManager;
use codex_model_provider::SharedModelProvider;
use codex_model_provider::create_model_provider;
use codex_login::CodexAuth;
use codex_login::collect_auth_env_telemetry;
use codex_login::default_client::build_reqwest_client;
use codex_model_provider_info::ModelProviderInfo;
use codex_model_provider_info::WireApi;
use codex_otel::TelemetryAuthMode;
use codex_protocol::config_types::CollaborationModeMask;
use codex_protocol::error::CodexErr;
use codex_protocol::error::Result as CoreResult;
use codex_protocol::openai_models::ModelInfo;
use codex_protocol::openai_models::ModelPreset;
use codex_protocol::openai_models::ModelsResponse;
use codex_response_debug_context::extract_response_debug_context;
use codex_response_debug_context::telemetry_transport_error_message;
use http::HeaderMap;
use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::sync::TryLockError;
use tokio::time::timeout;
use tracing::error;
use tracing::info;
use tracing::instrument;

const MODEL_CACHE_FILE: &str = "models_cache.json";
const DEFAULT_MODEL_CACHE_TTL: Duration = Duration::from_secs(300);
const MODELS_REFRESH_TIMEOUT: Duration = Duration::from_secs(5);
const MODELS_ENDPOINT: &str = "/models";

fn provider_cache_key(provider: &ModelProviderInfo, api_provider: &codex_api::Provider) -> String {
    let mut parts = vec![
        format!("name={:?}", provider.name),
        format!("base_url={}", api_provider.base_url),
        format!("wire_api={:?}", api_provider.wire_api),
    ];
    if let Some(params) = &api_provider.query_params
        && !params.is_empty()
    {
        let mut entries: Vec<_> = params.iter().collect();
        entries.sort_by(|(lhs, _), (rhs, _)| lhs.cmp(rhs));
        for (key, value) in entries {
            parts.push(format!("query:{key}={value}"));
        }
    }
    parts.join("|")
}
#[derive(Clone)]
struct ModelsRequestTelemetry {
    auth_mode: Option<String>,
    auth_header_attached: bool,
    auth_header_name: Option<&'static str>,
    auth_env: AuthEnvTelemetry,
}

impl RequestTelemetry for ModelsRequestTelemetry {
    fn on_request(
        &self,
        attempt: u64,
        status: Option<http::StatusCode>,
        error: Option<&TransportError>,
        duration: Duration,
    ) {
        let success = status.is_some_and(|code| code.is_success()) && error.is_none();
        let error_message = error.map(telemetry_transport_error_message);
        let response_debug = error
            .map(extract_response_debug_context)
            .unwrap_or_default();
        let status = status.map(|status| status.as_u16());
        tracing::event!(
            target: "codex_otel.log_only",
            tracing::Level::INFO,
            event.name = "codex.api_request",
            duration_ms = %duration.as_millis(),
            http.response.status_code = status,
            success = success,
            error.message = error_message.as_deref(),
            attempt = attempt,
            endpoint = MODELS_ENDPOINT,
            auth.header_attached = self.auth_header_attached,
            auth.header_name = self.auth_header_name,
            auth.env_openai_api_key_present = self.auth_env.openai_api_key_env_present,
            auth.env_codex_api_key_present = self.auth_env.codex_api_key_env_present,
            auth.env_codex_api_key_enabled = self.auth_env.codex_api_key_env_enabled,
            auth.env_provider_key_name = self.auth_env.provider_env_key_name.as_deref(),
            auth.env_provider_key_present = self.auth_env.provider_env_key_present,
            auth.env_refresh_token_url_override_present = self.auth_env.refresh_token_url_override_present,
            auth.request_id = response_debug.request_id.as_deref(),
            auth.cf_ray = response_debug.cf_ray.as_deref(),
            auth.error = response_debug.auth_error.as_deref(),
            auth.error_code = response_debug.auth_error_code.as_deref(),
            auth.mode = self.auth_mode.as_deref(),
        );
        tracing::event!(
            target: "codex_otel.trace_safe",
            tracing::Level::INFO,
            event.name = "codex.api_request",
            duration_ms = %duration.as_millis(),
            http.response.status_code = status,
            success = success,
            error.message = error_message.as_deref(),
            attempt = attempt,
            endpoint = MODELS_ENDPOINT,
            auth.header_attached = self.auth_header_attached,
            auth.header_name = self.auth_header_name,
            auth.env_openai_api_key_present = self.auth_env.openai_api_key_env_present,
            auth.env_codex_api_key_present = self.auth_env.codex_api_key_env_present,
            auth.env_codex_api_key_enabled = self.auth_env.codex_api_key_env_enabled,
            auth.env_provider_key_name = self.auth_env.provider_env_key_name.as_deref(),
            auth.env_provider_key_present = self.auth_env.provider_env_key_present,
            auth.env_refresh_token_url_override_present = self.auth_env.refresh_token_url_override_present,
            auth.request_id = response_debug.request_id.as_deref(),
            auth.cf_ray = response_debug.cf_ray.as_deref(),
            auth.error = response_debug.auth_error.as_deref(),
            auth.error_code = response_debug.auth_error_code.as_deref(),
            auth.mode = self.auth_mode.as_deref(),
        );
        emit_feedback_request_tags_with_auth_env(
            &FeedbackRequestTags {
                endpoint: MODELS_ENDPOINT,
                auth_header_attached: self.auth_header_attached,
                auth_header_name: self.auth_header_name,
                auth_mode: self.auth_mode.as_deref(),
                auth_retry_after_unauthorized: None,
                auth_recovery_mode: None,
                auth_recovery_phase: None,
                auth_connection_reused: None,
                auth_request_id: response_debug.request_id.as_deref(),
                auth_cf_ray: response_debug.cf_ray.as_deref(),
                auth_error: response_debug.auth_error.as_deref(),
                auth_error_code: response_debug.auth_error_code.as_deref(),
                auth_recovery_followup_success: None,
                auth_recovery_followup_status: None,
            },
            &self.auth_env,
        );
    }
}

/// Strategy for refreshing available models.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshStrategy {
    /// 始终从网络获取，忽略缓存.
    Online,
    /// Only use cached data, never fetch from the network.
    Offline,
    /// 如果缓存可用且新鲜则使用，否则从网络获取.
    OnlineIfUncached,
}

impl RefreshStrategy {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Online => "online",
            Self::Offline => "offline",
            Self::OnlineIfUncached => "online_if_uncached",
        }
    }
}

impl fmt::Display for RefreshStrategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// How the manager's base catalog is sourced for the lifetime of the process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CatalogMode {
    /// 从绑定的 models.json 开始，允许缓存/网络刷新更新.
    Default,
    /// 使用调用者提供的目录作为权威目录，不通过刷新变更.
    Custom,
}

/// 协调远程模型发现及磁盘上的缓存元数据.
#[derive(Debug)]
pub struct ModelsManager {
    remote_models: RwLock<Vec<ModelInfo>>,
    catalog_mode: CatalogMode,
    collaboration_modes_config: CollaborationModesConfig,
    auth_manager: Arc<AuthManager>,
    etag: RwLock<Option<String>>,
    cache_manager: ModelsCacheManager,
    provider: ModelProviderInfo,
}

impl ModelsManager {
    /// 使用提供的 AuthManager 构造管理器.
    ///
    /// 使用 codex_home 存储缓存的模型元数据，并用绑定的目录初始化
    /// 当提供 model_catalog 时，它成为权威的远程模型列表
    /// 且禁用从 /models 的后台刷新.
    pub fn new(
        codex_home: PathBuf,
        auth_manager: Arc<AuthManager>,
        model_catalog: Option<ModelsResponse>,
        collaboration_modes_config: CollaborationModesConfig,
    ) -> Self {
        Self::new_with_provider(
            codex_home,
            auth_manager,
            model_catalog,
            collaboration_modes_config,
            ModelProviderInfo::create_openai_provider(/*base_url*/ None),
        )
    }

    /// 使用用于远程模型刷新的显式提供商构造管理器.
    pub fn new_with_provider(
        codex_home: PathBuf,
        auth_manager: Arc<AuthManager>,
        model_catalog: Option<ModelsResponse>,
        collaboration_modes_config: CollaborationModesConfig,
        provider: ModelProviderInfo,
    ) -> Self {
        let cache_path = codex_home.join(MODEL_CACHE_FILE);
        let cache_manager = ModelsCacheManager::new(cache_path, DEFAULT_MODEL_CACHE_TTL);
        let catalog_mode = if model_catalog.is_some() {
            CatalogMode::Custom
        } else {
            CatalogMode::Default
        };
        // 无论全局 model_catalog 如何，始终应用按提供商的 model_catalog 过滤
        let base_models = model_catalog
            .map(|catalog| catalog.models)
            .unwrap_or_else(|| Self::default_remote_models_for_provider(&provider));

        let remote_models = if let Some(ref catalog_slugs) = provider.model_catalog {
            tracing::warn!(
                "MODEL_CATALOG_DEBUG: Filtering {} models by provider catalog: {:?}",
                base_models.len(),
                catalog_slugs
            );
            base_models
                .into_iter()
                .filter(|model| catalog_slugs.contains(&model.slug))
                .collect()
        } else {
            base_models
        };
        Self {
            catalog_mode,
            remote_models: RwLock::new(remote_models),
            collaboration_modes_config,
            auth_manager,
            etag: RwLock::new(None),
            cache_manager,
            provider,
        }
    }

    /// 列出所有可用模型，按指定策略刷新.
    ///
    /// 返回按优先级排序并按认证模式和可见性过滤的模型预设.
    #[instrument(
        level = "info",
        skip(self),
        fields(refresh_strategy = %refresh_strategy)
    )]
    pub async fn list_models(&self, refresh_strategy: RefreshStrategy) -> Vec<ModelPreset> {
        if let Err(err) = self.refresh_available_models(refresh_strategy).await {
            error!("failed to refresh available models: {err}");
        }
        let remote_models = self.get_remote_models().await;
        self.build_available_models(remote_models)
    }

    /// List collaboration mode presets.
    ///
    /// Returns a static set of presets seeded with the configured model.
    pub fn list_collaboration_modes(&self) -> Vec<CollaborationModeMask> {
        self.list_collaboration_modes_for_config(self.collaboration_modes_config)
    }

    pub fn list_collaboration_modes_for_config(
        &self,
        collaboration_modes_config: CollaborationModesConfig,
    ) -> Vec<CollaborationModeMask> {
        builtin_collaboration_mode_presets(collaboration_modes_config)
    }

    /// 尝试非阻塞列出模型，使用当前缓存状态.
    ///
    /// Returns an error if the internal lock cannot be acquired.
    pub fn try_list_models(&self) -> Result<Vec<ModelPreset>, TryLockError> {
        let remote_models = self.try_get_remote_models()?;
        Ok(self.build_available_models(remote_models))
    }

    // 应该在 core 可见并在 session_configured 事件上发送
    /// Get the model identifier to use, refreshing according to the specified strategy.
    ///
    /// 如果提供了 model，则直接返回。否则根据
    /// auth mode and available models.
    #[instrument(
        level = "info",
        skip(self, model),
        fields(
            model.provided = model.is_some(),
            refresh_strategy = %refresh_strategy
        )
    )]
    pub async fn get_default_model(
        &self,
        model: &Option<String>,
        refresh_strategy: RefreshStrategy,
    ) -> String {
        if let Some(model) = model.as_ref() {
            return model.to_string();
        }
        if let Err(err) = self.refresh_available_models(refresh_strategy).await {
            error!("failed to refresh available models: {err}");
        }
        let remote_models = self.get_remote_models().await;
        let available = self.build_available_models(remote_models);
        available
            .iter()
            .find(|model| model.is_default)
            .or_else(|| available.first())
            .map(|model| model.model.clone())
            .unwrap_or_default()
    }

    // todo(aibrahim): look if we can tighten it to pub(crate)
    /// 查找模型元数据，应用远程覆盖和配置调整.
    #[instrument(level = "info", skip(self, config), fields(model = model))]
    pub async fn get_model_info(&self, model: &str, config: &ModelsManagerConfig) -> ModelInfo {
        let remote_models = self.get_remote_models().await;
        Self::construct_model_info_from_candidates(model, &remote_models, config)
    }

    fn find_model_by_longest_prefix(model: &str, candidates: &[ModelInfo]) -> Option<ModelInfo> {
        let mut best: Option<ModelInfo> = None;
        for candidate in candidates {
            if !model.starts_with(&candidate.slug) {
                continue;
            }
            let is_better_match = if let Some(current) = best.as_ref() {
                candidate.slug.len() > current.slug.len()
            } else {
                true
            };
            if is_better_match {
                best = Some(candidate.clone());
            }
        }
        best
    }

    /// 重试单个带命名空间 slug（如 namespace/model-name）的元数据查找.
    ///
    /// 仅剥离一个前导命名空间段，且仅当命名空间是 ASCII
    /// alphanumeric/underscore (`\\w+`) to avoid broadly matching arbitrary aliases.
    fn find_model_by_namespaced_suffix(model: &str, candidates: &[ModelInfo]) -> Option<ModelInfo> {
        let (namespace, suffix) = model.split_once('/')?;
        if suffix.contains('/') {
            return None;
        }
        if !namespace
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return None;
        }
        Self::find_model_by_longest_prefix(suffix, candidates)
    }

    fn construct_model_info_from_candidates(
        model: &str,
        candidates: &[ModelInfo],
        config: &ModelsManagerConfig,
    ) -> ModelInfo {
        // 首先使用正常的最长前缀匹配。如果未命中，允许狭义范围
        // retry for namespaced slugs like `custom/gpt-5.3-codex`.
        let remote = Self::find_model_by_longest_prefix(model, candidates)
            .or_else(|| Self::find_model_by_namespaced_suffix(model, candidates));
        let model_info = if let Some(remote) = remote {
            ModelInfo {
                slug: model.to_string(),
                used_fallback_model_metadata: false,
                ..remote
            }
        } else {
            model_info::model_info_from_slug(model)
        };
        model_info::with_config_overrides(model_info, config)
    }

    /// 如果提供的 ETag 与缓存的 ETag 不同则刷新模型.
    ///
    /// Uses `Online` strategy to fetch latest models when ETags differ.
    pub async fn refresh_if_new_etag(&self, etag: String) {
        let current_etag = self.get_etag().await;
        if current_etag.clone().is_some() && current_etag.as_deref() == Some(etag.as_str()) {
            if let Err(err) = self.cache_manager.renew_cache_ttl().await {
                error!("failed to renew cache TTL: {err}");
            }
            return;
        }
        if let Err(err) = self.refresh_available_models(RefreshStrategy::Online).await {
            error!("failed to refresh available models: {err}");
        }
    }

    /// Refresh available models according to the specified strategy.
    async fn refresh_available_models(&self, refresh_strategy: RefreshStrategy) -> CoreResult<()> {
        // don't override the custom model catalog if one was provided by the user
        if matches!(self.catalog_mode, CatalogMode::Custom) {
            return Ok(());
        }

        if self.auth_manager.auth_mode() != Some(AuthMode::Chatgpt)
            && !self.provider.has_command_auth()
        {
            if matches!(
                refresh_strategy,
                RefreshStrategy::Offline | RefreshStrategy::OnlineIfUncached
            ) {
                self.try_load_cache().await;
            }
            return Ok(());
        }

        match refresh_strategy {
            RefreshStrategy::Offline => {
                // Only try to load from cache, never fetch
                self.try_load_cache().await;
                Ok(())
            }
            RefreshStrategy::OnlineIfUncached => {
                // 优先尝试缓存，不可用时回退到在线
                if self.try_load_cache().await {
                    info!("模型缓存：为 OnlineIfUncached 使用缓存模型");
                    return Ok(());
                }
                info!("模型缓存：缓存未命中，获取远程模型");
                self.fetch_and_update_models().await
            }
            RefreshStrategy::Online => {
                // Always fetch from network
                self.fetch_and_update_models().await
            }
        }
    }

    async fn fetch_and_update_models(&self) -> CoreResult<()> {
        let _timer =
            codex_otel::start_global_timer("codex.remote_models.fetch_update.duration_ms", &[]);
        let auth = self.auth_manager.auth().await;
        let auth_mode = auth.as_ref().map(CodexAuth::auth_mode);
        let api_provider = self.provider.to_api_provider(auth_mode)?;
        let api_auth = self.provider.api_auth().await?;
        let auth_env = collect_auth_env_telemetry(
            &self.provider,
            self.auth_manager.codex_api_key_env_enabled(),
        );
        let transport = ReqwestTransport::new(build_reqwest_client());
        let auth_telemetry = auth_header_telemetry(api_auth.as_ref());
        let request_telemetry: Arc<dyn RequestTelemetry> = Arc::new(ModelsRequestTelemetry {
            auth_mode: auth_mode.map(|mode| TelemetryAuthMode::from(mode).to_string()),
            auth_header_attached: auth_telemetry.attached,
            auth_header_name: auth_telemetry.name,
            auth_env,
        });
        let client = ModelsClient::new(transport, api_provider, api_auth)
            .with_telemetry(Some(request_telemetry));

        let client_version = crate::client_version_to_whole();
        let (models, etag) = timeout(
            MODELS_REFRESH_TIMEOUT,
            client.list_models(&client_version, HeaderMap::new()),
        )
        .await
        .map_err(|_| CodexErr::Timeout)?
        .map_err(map_api_error)?;

        self.apply_remote_models(models.clone()).await;
        *self.etag.write().await = etag.clone();
        self.cache_manager
            .persist_cache(&models, etag, client_version)
            .await;
        Ok(())
    }

    async fn get_etag(&self) -> Option<String> {
        self.etag.read().await.clone()
    }

    /// Replace the cached remote models and rebuild the derived presets list.
    async fn apply_remote_models(&self, models: Vec<ModelInfo>) {
        let mut existing_models = Self::default_remote_models_for_provider(&self.provider);
        for model in models {
            if let Some(existing_index) = existing_models
                .iter()
                .position(|existing| existing.slug == model.slug)
            {
                existing_models[existing_index] = model;
            } else {
                existing_models.push(model);
            }
        }
        *self.remote_models.write().await = existing_models;
    }

    fn default_remote_models_for_provider(provider: &ModelProviderInfo) -> Vec<ModelInfo> {
        let models = match provider.wire_api {
            WireApi::Anthropic => model_info::anthropic_model_catalog(),
            _ => Self::load_remote_models_from_file().unwrap_or_default(),
        };

        // Apply model_catalog filtering for all wire_api types
        if let Some(ref catalog_slugs) = provider.model_catalog {
            tracing::warn!(
                "MODEL_CATALOG_DEBUG: Filtering {} models by catalog: {:?}",
                models.len(),
                catalog_slugs
            );

            // Try to find matching models in default list
            let matching_models: Vec<_> = models
                .iter()
                .filter(|model| catalog_slugs.contains(&model.slug))
                .cloned()
                .collect();

            if !matching_models.is_empty() {
                tracing::warn!(
                    "MODEL_CATALOG_DEBUG: Found {} matching models in default list",
                    matching_models.len()
                );
                return matching_models;
            }

            // 如果没有匹配，则从目录 slug 创建模型
            tracing::warn!(
                "MODEL_CATALOG_DEBUG: No matches found, creating {} models from catalog slugs",
                catalog_slugs.len()
            );

            // Use first model as template or create fallback
            let template = models.first().cloned().unwrap_or_else(|| ModelInfo {
                slug: String::from("fallback"),
                display_name: String::from("Fallback Model"),
                description: None,
                default_reasoning_level: None,
                supported_reasoning_levels: Vec::new(),
                shell_type: codex_protocol::openai_models::ConfigShellToolType::Default,
                visibility: codex_protocol::openai_models::ModelVisibility::None,
                supported_in_api: true,
                priority: 999,
                additional_speed_tiers: Vec::new(),
                availability_nux: None,
                upgrade: None,
                base_instructions: String::new(),
                model_messages: None,
                supports_reasoning_summaries: false,
                default_reasoning_summary: codex_protocol::config_types::ReasoningSummary::Auto,
                support_verbosity: false,
                default_verbosity: None,
                apply_patch_tool_type: None,
                web_search_tool_type: codex_protocol::openai_models::WebSearchToolType::Text,
                supports_search_tool: false,
                truncation_policy: codex_protocol::openai_models::TruncationPolicyConfig::bytes(
                    10000,
                ),
                supports_parallel_tool_calls: false,
                supports_image_detail_original: false,
                context_window: None,
                auto_compact_token_limit: None,
                effective_context_window_percent: 90,
                experimental_supported_tools: Vec::new(),
                input_modalities: Vec::new(),
                used_fallback_model_metadata: true,
                skip_reasoning_popup: false,
            });

            // 为每个目录 slug 创建模型
            let custom_models: Vec<ModelInfo> = catalog_slugs
                .iter()
                .enumerate()
                .map(|(i, slug)| {
                    let mut model = template.clone();
                    model.slug = slug.clone();
                    model.display_name = slug.clone();
                    model.priority = i as i32;
                    model
                })
                .collect();

            return custom_models;
        } else {
            models
        }
    }

    fn load_remote_models_from_file() -> Result<Vec<ModelInfo>, std::io::Error> {
        Ok(crate::bundled_models_response()?.models)
    }

    /// Attempt to satisfy the refresh from the cache when it matches the provider and TTL.
    async fn try_load_cache(&self) -> bool {
        let _timer =
            codex_otel::start_global_timer("codex.remote_models.load_cache.duration_ms", &[]);
        let client_version = crate::client_version_to_whole();
        let auth_mode = self.auth_manager.auth_mode();
        let api_provider = match self.provider.to_api_provider(auth_mode) {
            Ok(provider) => provider,
            Err(err) => {
                error!("models cache: failed to build provider config: {err}");
                return false;
            }
        };
        let auth_telemetry = auth_header_telemetry(api_auth.as_ref());
        let allow_legacy_without_provider_cache_key = self.provider.is_openai();
        info!(client_version, "models cache: evaluating cache eligibility");
        let cache = match self
            .cache_manager
            .load_fresh(
                &client_version,
                &provider_cache_key,
                allow_legacy_without_provider_cache_key,
            )
            .await
        {
            Some(cache) => cache,
            None => {
                info!("模型缓存：没有可用的缓存条目");
                return false;
            }
        };
        let models = cache.models.clone();
        *self.etag.write().await = cache.etag.clone();
        self.apply_remote_models(models.clone()).await;
        info!(
            models_count = models.len(),
            etag = ?cache.etag,
            "models cache: cache entry applied"
        );
        true
    }

    /// 从活动目录快照构建选择器就绪的预设.
    fn build_available_models(&self, mut remote_models: Vec<ModelInfo>) -> Vec<ModelPreset> {
        remote_models.sort_by(|a, b| a.priority.cmp(&b.priority));

        // Apply provider-level config overrides (e.g. skip_reasoning_popup) before
        // converting to presets so the setting reaches the TUI model picker.
        if self.provider.skip_reasoning_popup {
            for model in &mut remote_models {
                model.skip_reasoning_popup = true;
            }
        }

        let mut presets: Vec<ModelPreset> = remote_models.into_iter().map(Into::into).collect();
        // Filter models by provider-specific model_catalog if configured
        tracing::warn!(
            "MODEL_CATALOG_DEBUG: provider.model_catalog = {:?}, remote_models count = {}",
            self.provider.model_catalog,
            presets.len()
        );
        if let Some(ref catalog_slugs) = self.provider.model_catalog {
            presets.retain(|preset| catalog_slugs.contains(&preset.model));
        }
        let chatgpt_mode = matches!(self.auth_manager.auth_mode(), Some(AuthMode::Chatgpt));
        presets = ModelPreset::filter_by_auth(presets, chatgpt_mode);

        ModelPreset::mark_default_by_picker_visibility(&mut presets);

        presets
    }
    async fn get_remote_models(&self) -> Vec<ModelInfo> {
        self.remote_models.read().await.clone()
    }

    fn try_get_remote_models(&self) -> Result<Vec<ModelInfo>, TryLockError> {
        Ok(self.remote_models.try_read()?.clone())
    }

    /// 使用特定提供商为测试构造管理器.
    pub fn with_provider_for_tests(
        codex_home: PathBuf,
        auth_manager: Arc<AuthManager>,
        provider: ModelProviderInfo,
    ) -> Self {
        Self::new_with_provider(
            codex_home,
            auth_manager,
            /*model_catalog*/ None,
            CollaborationModesConfig::default(),
            provider,
        )
    }

    /// 在不查询远程状态或缓存的情况下获取模型标识符.
    pub fn get_model_offline_for_tests(model: Option<&str>) -> String {
        if let Some(model) = model {
            return model.to_string();
        }
        let mut models = Self::default_remote_models_for_provider(
            &ModelProviderInfo::create_openai_provider(/*base_url*/ None),
        );
        models.sort_by(|a, b| a.priority.cmp(&b.priority));
        let presets: Vec<ModelPreset> = models.into_iter().map(Into::into).collect();
        presets
            .iter()
            .find(|preset| preset.show_in_picker)
            .or_else(|| presets.first())
            .map(|preset| preset.model.clone())
            .unwrap_or_default()
    }

    /// 在不查询远程状态或缓存的情况下构建 ModelInfo.
    pub fn construct_model_info_offline_for_tests(
        model: &str,
        config: &ModelsManagerConfig,
    ) -> ModelInfo {
        let candidates: &[ModelInfo] = if let Some(model_catalog) = config.model_catalog.as_ref() {
            &model_catalog.models
        } else {
            &[]
        };
        Self::construct_model_info_from_candidates(model, candidates, config)
    }
}

#[cfg(test)]
#[path = "manager_tests.rs"]
mod tests;
