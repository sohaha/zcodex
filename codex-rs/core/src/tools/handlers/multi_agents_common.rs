use crate::agent::AgentStatus;
use crate::codex::Session;
use crate::codex::TurnContext;
use crate::config::Config;
use crate::config::ConfigBuilder;
use crate::function_tool::FunctionCallError;
use crate::tools::context::FunctionToolOutput;
use crate::tools::context::ToolOutput;
use crate::tools::context::ToolPayload;
use codex_app_server_protocol::ConfigLayerSource;
use codex_features::Feature;
use codex_models_manager::collaboration_mode_presets::CollaborationModesConfig;
use codex_models_manager::manager::ModelsManager;
use codex_models_manager::manager::RefreshStrategy;
use codex_protocol::AgentPath;
use codex_protocol::ThreadId;
use codex_protocol::error::CodexErr;
use codex_protocol::models::BaseInstructions;
use codex_protocol::models::ResponseInputItem;
use codex_protocol::openai_models::ReasoningEffort;
use codex_protocol::openai_models::ReasoningEffortPreset;
use codex_protocol::protocol::CollabAgentRef;
use codex_protocol::protocol::CollabAgentStatusEntry;
use codex_protocol::protocol::Op;
use codex_protocol::protocol::SessionSource;
use codex_protocol::protocol::SubAgentSource;
use codex_protocol::user_input::UserInput;
use serde::Serialize;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use tracing::warn;

/// Minimum wait timeout to prevent tight polling loops from burning CPU.
pub(crate) const MIN_WAIT_TIMEOUT_MS: i64 = 10_000;
pub(crate) const DEFAULT_WAIT_TIMEOUT_MS: i64 = 30_000;
pub(crate) const MAX_WAIT_TIMEOUT_MS: i64 = 3600 * 1000;

pub(crate) fn function_arguments(payload: ToolPayload) -> Result<String, FunctionCallError> {
    match payload {
        ToolPayload::Function { arguments } => Ok(arguments),
        _ => Err(FunctionCallError::RespondToModel(
            "collab handler received unsupported payload".to_string(),
        )),
    }
}

pub(crate) fn tool_output_json_text<T>(value: &T, tool_name: &str) -> String
where
    T: Serialize,
{
    serde_json::to_string(value).unwrap_or_else(|err| {
        JsonValue::String(format!("failed to serialize {tool_name} result: {err}")).to_string()
    })
}

pub(crate) fn tool_output_response_item<T>(
    call_id: &str,
    payload: &ToolPayload,
    value: &T,
    success: Option<bool>,
    tool_name: &str,
) -> ResponseInputItem
where
    T: Serialize,
{
    FunctionToolOutput::from_text(tool_output_json_text(value, tool_name), success)
        .to_response_item(call_id, payload)
}

pub(crate) fn tool_output_code_mode_result<T>(value: &T, tool_name: &str) -> JsonValue
where
    T: Serialize,
{
    serde_json::to_value(value).unwrap_or_else(|err| {
        JsonValue::String(format!("failed to serialize {tool_name} result: {err}"))
    })
}

pub(crate) fn build_wait_agent_statuses(
    statuses: &HashMap<ThreadId, AgentStatus>,
    receiver_agents: &[CollabAgentRef],
) -> Vec<CollabAgentStatusEntry> {
    if statuses.is_empty() {
        return Vec::new();
    }

    let mut entries = Vec::with_capacity(statuses.len());
    let mut seen = HashMap::with_capacity(receiver_agents.len());
    for receiver_agent in receiver_agents {
        seen.insert(receiver_agent.thread_id, ());
        if let Some(status) = statuses.get(&receiver_agent.thread_id) {
            entries.push(CollabAgentStatusEntry {
                thread_id: receiver_agent.thread_id,
                agent_nickname: receiver_agent.agent_nickname.clone(),
                agent_role: receiver_agent.agent_role.clone(),
                status: status.clone(),
            });
        }
    }

    let mut extras = statuses
        .iter()
        .filter(|(thread_id, _)| !seen.contains_key(thread_id))
        .map(|(thread_id, status)| CollabAgentStatusEntry {
            thread_id: *thread_id,
            agent_nickname: None,
            agent_role: None,
            status: status.clone(),
        })
        .collect::<Vec<_>>();
    extras.sort_by(|left, right| left.thread_id.to_string().cmp(&right.thread_id.to_string()));
    entries.extend(extras);
    entries
}

pub(crate) fn collab_spawn_error(err: CodexErr) -> FunctionCallError {
    match err {
        CodexErr::UnsupportedOperation(message) if message == "thread manager dropped" => {
            FunctionCallError::RespondToModel("collab manager unavailable".to_string())
        }
        CodexErr::UnsupportedOperation(message) => FunctionCallError::RespondToModel(message),
        err => FunctionCallError::RespondToModel(format!("collab spawn failed: {err}")),
    }
}

pub(crate) fn collab_agent_error(agent_id: ThreadId, err: CodexErr) -> FunctionCallError {
    match err {
        CodexErr::ThreadNotFound(id) => {
            FunctionCallError::RespondToModel(format!("agent with id {id} not found"))
        }
        CodexErr::InternalAgentDied => {
            FunctionCallError::RespondToModel(format!("agent with id {agent_id} is closed"))
        }
        CodexErr::UnsupportedOperation(_) => {
            FunctionCallError::RespondToModel("collab manager unavailable".to_string())
        }
        err => FunctionCallError::RespondToModel(format!("collab tool failed: {err}")),
    }
}

pub(crate) fn thread_spawn_source(
    parent_thread_id: ThreadId,
    parent_session_source: &SessionSource,
    depth: i32,
    parent_model: Option<&str>,
    agent_role: Option<&str>,
    task_name: Option<String>,
) -> Result<SessionSource, FunctionCallError> {
    let agent_path = task_name
        .as_deref()
        .map(|task_name| {
            parent_session_source
                .get_agent_path()
                .unwrap_or_else(AgentPath::root)
                .join(task_name)
                .map_err(FunctionCallError::RespondToModel)
        })
        .transpose()?;
    Ok(SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
        parent_thread_id,
        depth,
        parent_model: parent_model.map(str::to_string),
        agent_path,
        agent_nickname: None,
        agent_role: agent_role.map(str::to_string),
    }))
}

pub(crate) fn parse_collab_input(
    message: Option<String>,
    items: Option<Vec<UserInput>>,
) -> Result<Op, FunctionCallError> {
    match (message, items) {
        (Some(_), Some(_)) => Err(FunctionCallError::RespondToModel(
            "Provide either message or items, but not both".to_string(),
        )),
        (None, None) => Err(FunctionCallError::RespondToModel(
            "Provide one of: message or items".to_string(),
        )),
        (Some(message), None) => {
            if message.trim().is_empty() {
                return Err(FunctionCallError::RespondToModel(
                    "Empty message can't be sent to an agent".to_string(),
                ));
            }
            Ok(vec![UserInput::Text {
                text: message,
                text_elements: Vec::new(),
            }]
            .into())
        }
        (None, Some(items)) => {
            if items.is_empty() {
                return Err(FunctionCallError::RespondToModel(
                    "Items can't be empty".to_string(),
                ));
            }
            Ok(items.into())
        }
    }
}

/// Builds the base config snapshot for a newly spawned sub-agent.
///
/// The returned config starts from the parent's effective config and then refreshes the
/// runtime-owned fields carried on `turn`, including model selection, reasoning settings,
/// approval policy, sandbox, and cwd. Role-specific overrides are layered after this step;
/// skipping this helper and cloning stale config state directly can send the child agent out with
/// the wrong provider or runtime policy.
pub(crate) async fn build_agent_spawn_config(
    base_instructions: &BaseInstructions,
    turn: &TurnContext,
) -> Result<Config, FunctionCallError> {
    let mut config = build_agent_shared_config(turn).await?;
    config.base_instructions = Some(base_instructions.text.clone());
    Ok(config)
}

pub(crate) async fn build_agent_resume_config(
    turn: &TurnContext,
    child_depth: i32,
) -> Result<Config, FunctionCallError> {
    let mut config = build_agent_shared_config(turn).await?;
    apply_spawn_agent_overrides(&mut config, child_depth);
    // For resume, keep base instructions sourced from rollout/session metadata.
    config.base_instructions = None;
    Ok(config)
}

pub(crate) async fn build_agent_shared_config(
    turn: &TurnContext,
) -> Result<Config, FunctionCallError> {
    let base_config = turn.config.clone();
    let mut config = (*base_config).clone();
    config.model = Some(turn.model_info.slug.clone());
    config.model_provider = turn.provider.clone();
    config
        .model_providers
        .insert(config.model_provider_id.clone(), turn.provider.clone());
    config.model_reasoning_effort = turn.reasoning_effort;
    config.model_reasoning_summary = Some(turn.reasoning_summary);
    config.developer_instructions = turn.developer_instructions.clone();
    config.compact_prompt = turn.compact_prompt.clone();
    reload_project_scoped_config(&mut config, turn).await;
    apply_spawn_agent_runtime_overrides(&mut config, turn)?;

    Ok(config)
}

fn project_scoped_origin(config: &Config, key: &str) -> Option<ConfigLayerSource> {
    config
        .config_layer_stack
        .origins()
        .remove(key)
        .map(|metadata| metadata.name)
}

fn should_reload_project_scoped_key(config: &Config, key: &str) -> bool {
    matches!(
        project_scoped_origin(config, key),
        None | Some(ConfigLayerSource::Project { .. })
    )
}

async fn reload_project_scoped_config(config: &mut Config, turn: &TurnContext) {
    if config.cwd.as_path() == turn.cwd.as_path() {
        return;
    }

    let reload_zmemory = should_reload_project_scoped_key(config, "zmemory.path");
    let reload_agent_max_threads = should_reload_project_scoped_key(config, "agents.max_threads");
    let reload_agent_max_depth = should_reload_project_scoped_key(config, "agents.max_depth");
    let reload_agent_job_max_runtime_seconds =
        should_reload_project_scoped_key(config, "agents.job_max_runtime_seconds");
    let reload_agent_roles = true;

    if !(reload_zmemory
        || reload_agent_max_threads
        || reload_agent_max_depth
        || reload_agent_job_max_runtime_seconds
        || reload_agent_roles)
    {
        return;
    }

    match ConfigBuilder::default()
        .codex_home(config.codex_home.clone())
        .fallback_cwd(Some(turn.cwd.to_path_buf()))
        .build()
        .await
    {
        Ok(reloaded_config) => {
            if reload_zmemory {
                config.zmemory = reloaded_config.zmemory;
            }
            if reload_agent_max_threads {
                config.agent_max_threads = reloaded_config.agent_max_threads;
            }
            if reload_agent_max_depth {
                config.agent_max_depth = reloaded_config.agent_max_depth;
            }
            if reload_agent_job_max_runtime_seconds {
                config.agent_job_max_runtime_seconds =
                    reloaded_config.agent_job_max_runtime_seconds;
            }
            if reload_agent_roles {
                config.agent_roles = reloaded_config.agent_roles;
            }
        }
        Err(err) => {
            warn!(
                error = %err,
                cwd = %turn.cwd.display(),
                "failed to reload project-scoped child agent config for current turn cwd; using turn config"
            );
        }
    }
}

/// Copies runtime-only turn state onto a child config before it is handed to `AgentControl`.
///
/// These values are chosen by the live turn rather than persisted config, so leaving them stale
/// can make a child agent disagree with its parent about approval policy, cwd, or sandboxing.
pub(crate) fn apply_spawn_agent_runtime_overrides(
    config: &mut Config,
    turn: &TurnContext,
) -> Result<(), FunctionCallError> {
    config
        .permissions
        .approval_policy
        .set(turn.approval_policy.value())
        .map_err(|err| {
            FunctionCallError::RespondToModel(format!("approval_policy is invalid: {err}"))
        })?;
    config.permissions.shell_environment_policy = turn.shell_environment_policy.clone();
    config.codex_linux_sandbox_exe = turn.codex_linux_sandbox_exe.clone();
    config.cwd = turn.cwd.clone();
    config
        .permissions
        .sandbox_policy
        .set(turn.sandbox_policy.get().clone())
        .map_err(|err| {
            FunctionCallError::RespondToModel(format!("sandbox_policy is invalid: {err}"))
        })?;
    config.permissions.file_system_sandbox_policy = turn.file_system_sandbox_policy.clone();
    config.permissions.network_sandbox_policy = turn.network_sandbox_policy;
    Ok(())
}

pub(crate) fn apply_spawn_agent_overrides(config: &mut Config, child_depth: i32) {
    if child_depth >= config.agent_max_depth && !config.features.enabled(Feature::MultiAgentV2) {
        let _ = config.features.disable(Feature::SpawnCsv);
        let _ = config.features.disable(Feature::Collab);
    }
}

pub(crate) fn apply_requested_spawn_agent_provider_override(
    config: &mut Config,
    requested_provider: Option<&str>,
) -> Result<(), FunctionCallError> {
    let Some(requested_provider) = requested_provider else {
        return Ok(());
    };

    let provider = config
        .model_providers
        .get(requested_provider)
        .cloned()
        .ok_or_else(|| {
            let available = config
                .model_providers
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join(", ");
            FunctionCallError::RespondToModel(format!(
                "Unknown provider `{requested_provider}` for spawn_agent. Available providers: {available}"
            ))
        })?;
    config.model_provider_id = requested_provider.to_string();
    config.model_provider = provider;
    Ok(())
}

pub(crate) fn spawn_agent_models_manager(session: &Session, config: &Config) -> ModelsManager {
    ModelsManager::new_with_provider(
        config.codex_home.clone(),
        session.services.auth_manager.clone(),
        config.model_catalog.clone(),
        CollaborationModesConfig::default(),
        config.model_provider.clone(),
    )
}

pub(crate) fn spawn_agent_models_manager_for_provider(
    session: &Session,
    turn: &TurnContext,
    config: &Config,
) -> std::sync::Arc<ModelsManager> {
    if config.model_provider == turn.provider {
        session.services.models_manager.clone()
    } else {
        std::sync::Arc::new(spawn_agent_models_manager(session, config))
    }
}

pub(crate) async fn apply_requested_spawn_agent_model_overrides(
    session: &Session,
    turn: &TurnContext,
    config: &mut Config,
    requested_provider: Option<&str>,
    requested_model: Option<&str>,
    requested_reasoning_effort: Option<ReasoningEffort>,
) -> Result<(), FunctionCallError> {
    if requested_provider.is_none()
        && requested_model.is_none()
        && requested_reasoning_effort.is_none()
    {
        return Ok(());
    }

    let original_provider_id = config.model_provider_id.clone();
    let original_provider = config.model_provider.clone();
    let original_model = config.model.clone();
    let original_reasoning_effort = config.model_reasoning_effort;
    let restore_original_selection = |config: &mut Config| {
        config.model_provider_id = original_provider_id.clone();
        config.model_provider = original_provider.clone();
        config.model = original_model.clone();
        config.model_reasoning_effort = original_reasoning_effort;
    };

    apply_requested_spawn_agent_provider_override(config, requested_provider)?;

    let models_manager = spawn_agent_models_manager_for_provider(session, turn, config);
    let selected_model_name = if let Some(requested_model) = requested_model {
        match resolve_spawn_agent_model_name(models_manager.as_ref(), requested_model).await {
            Ok(model) => model,
            Err(err) => {
                restore_original_selection(config);
                if requested_provider.is_some() {
                    return Err(err);
                }
                warn!(
                    requested_model,
                    fallback_model = %turn.model_info.slug,
                    "spawn_agent requested model unavailable after refreshing model catalog; falling back to parent model"
                );
                return Ok(());
            }
        }
    } else if requested_provider.is_some() {
        match select_spawn_agent_model_for_provider_switch(models_manager.as_ref(), config).await {
            Ok(model) => model,
            Err(err) => {
                restore_original_selection(config);
                return Err(err);
            }
        }
    } else {
        config
            .model
            .clone()
            .unwrap_or_else(|| turn.model_info.slug.clone())
    };
    let selected_model_info = models_manager
        .get_model_info(&selected_model_name, &config.to_models_manager_config())
        .await;

    config.model = Some(selected_model_name.clone());
    if let Some(reasoning_effort) = requested_reasoning_effort {
        let validation = validate_spawn_agent_reasoning_effort(
            &selected_model_name,
            &selected_model_info.supported_reasoning_levels,
            reasoning_effort,
        );
        if let Err(err) = validation {
            restore_original_selection(config);
            if requested_provider.is_some() || requested_model.is_none() {
                return Err(err);
            }
            warn!(
                requested_model = %selected_model_name,
                requested_reasoning_effort = %reasoning_effort,
                fallback_model = %turn.model_info.slug,
                "spawn_agent requested reasoning effort is unsupported for the requested model; falling back to parent model"
            );
            return Ok(());
        }
        config.model_reasoning_effort = Some(reasoning_effort);
    } else if requested_model.is_some() || requested_provider.is_some() {
        config.model_reasoning_effort = selected_model_info.default_reasoning_level;
    }

    Ok(())
}

pub(crate) async fn resolve_spawn_agent_model_name(
    models_manager: &ModelsManager,
    requested_model: &str,
) -> Result<String, FunctionCallError> {
    let offline_models = models_manager.list_models(RefreshStrategy::Offline).await;
    if let Ok(model) = find_spawn_agent_model_name(&offline_models, requested_model) {
        return Ok(model);
    }

    let refreshed_models = models_manager.list_models(RefreshStrategy::Online).await;
    find_spawn_agent_model_name(&refreshed_models, requested_model)
}

pub(crate) async fn select_spawn_agent_model_for_provider_switch(
    models_manager: &ModelsManager,
    config: &Config,
) -> Result<String, FunctionCallError> {
    if let Some(current_model) = config.model.as_deref()
        && resolve_spawn_agent_model_name(models_manager, current_model)
            .await
            .is_ok()
    {
        return Ok(current_model.to_string());
    }

    if let Some(provider_default_model) = config.model_provider.model.as_deref() {
        return resolve_spawn_agent_model_name(models_manager, provider_default_model).await;
    }

    let available_models = models_manager
        .list_models(RefreshStrategy::OnlineIfUncached)
        .await;
    available_models
        .iter()
        .find(|model| model.is_default)
        .or_else(|| available_models.first())
        .map(|model| model.model.clone())
        .ok_or_else(|| {
            FunctionCallError::RespondToModel(format!(
                "Provider `{}` has no available models for spawn_agent",
                config.model_provider_id
            ))
        })
}

fn find_spawn_agent_model_name(
    available_models: &[codex_protocol::openai_models::ModelPreset],
    requested_model: &str,
) -> Result<String, FunctionCallError> {
    available_models
        .iter()
        .find(|model| model.model == requested_model)
        .map(|model| model.model.clone())
        .ok_or_else(|| {
            let available = available_models
                .iter()
                .map(|model| model.model.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            FunctionCallError::RespondToModel(format!(
                "Unknown model `{requested_model}` for spawn_agent. Available models: {available}"
            ))
        })
}

fn validate_spawn_agent_reasoning_effort(
    model: &str,
    supported_reasoning_levels: &[ReasoningEffortPreset],
    requested_reasoning_effort: ReasoningEffort,
) -> Result<(), FunctionCallError> {
    if supported_reasoning_levels
        .iter()
        .any(|preset| preset.effort == requested_reasoning_effort)
    {
        return Ok(());
    }

    let supported = supported_reasoning_levels
        .iter()
        .map(|preset| preset.effort.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    Err(FunctionCallError::RespondToModel(format!(
        "Reasoning effort `{requested_reasoning_effort}` is not supported for model `{model}`. Supported reasoning efforts: {supported}"
    )))
}
