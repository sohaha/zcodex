use crate::agent::AgentStatus;
use crate::config::Config;
use crate::config::ConfigOverrides;
use crate::config::deserialize_config_toml_with_base;
use crate::config_loader::CloudRequirementsLoader;
use crate::config_loader::ConfigLayerStackOrdering;
use crate::config_loader::LoaderOverrides;
use crate::config_loader::load_config_layers_state;
use crate::function_tool::FunctionCallError;
use crate::session::session::Session;
use crate::session::turn_context::TurnContext;
use crate::tools::context::FunctionToolOutput;
use crate::tools::context::ToolOutput;
use crate::tools::context::ToolPayload;
use codex_app_server_protocol::ConfigLayerSource;
use codex_exec_server::LOCAL_FS;
use codex_features::Feature;
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
use toml::Value as TomlValue;

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
    let live_config = turn.config.as_ref();
    let mut config = live_config.clone();
    if live_config.cwd != turn.cwd {
        let codex_home = turn.config.codex_home.clone();
        let turn_cwd = turn.cwd.as_path().display().to_string();
        let empty_cli_overrides: &[(String, TomlValue)] = &[];
        let config_layer_stack = load_config_layers_state(
            LOCAL_FS.as_ref(),
            &codex_home,
            Some(turn.cwd.clone()),
            empty_cli_overrides,
            LoaderOverrides::default(),
            CloudRequirementsLoader::default(),
            &codex_config::NoopThreadConfigLoader,
            /*host_name*/ None,
        )
        .await
        .map_err(|err| {
            FunctionCallError::RespondToModel(format!(
                "failed to reload config for turn cwd {turn_cwd}: {err}",
            ))
        })?;
        let config_toml = deserialize_config_toml_with_base(
            config_layer_stack.effective_config(),
            codex_home.as_path(),
        )
        .map_err(|err| {
            FunctionCallError::RespondToModel(format!(
                "failed to deserialize config for turn cwd {turn_cwd}: {err}",
            ))
        })?;
        let config_overrides = ConfigOverrides {
            cwd: Some(turn.cwd.to_path_buf()),
            config_profile: live_config.active_profile.clone(),
            ..Default::default()
        };
        let reloaded_config = Config::load_config_with_layer_stack(
            LOCAL_FS.as_ref(),
            config_toml,
            config_overrides,
            codex_home,
            config_layer_stack,
        )
        .await
        .map_err(|err| {
            FunctionCallError::RespondToModel(format!(
                "failed to rebuild config for turn cwd {turn_cwd}: {err}",
            ))
        })?;
        let mut reloaded_for_comparison = reloaded_config.clone();
        reloaded_for_comparison.config_layer_stack = live_config.config_layer_stack.clone();
        reloaded_for_comparison.startup_warnings = live_config.startup_warnings.clone();
        reloaded_for_comparison.active_profile = live_config.active_profile.clone();
        reloaded_for_comparison.active_project = live_config.active_project.clone();
        reloaded_for_comparison.cwd = live_config.cwd.clone();
        reloaded_for_comparison.model = live_config.model.clone();
        reloaded_for_comparison.model_provider_id = live_config.model_provider_id.clone();
        reloaded_for_comparison.model_provider = live_config.model_provider.clone();
        reloaded_for_comparison.model_providers = live_config.model_providers.clone();
        reloaded_for_comparison.model_reasoning_effort = live_config.model_reasoning_effort;
        reloaded_for_comparison.model_reasoning_summary = live_config.model_reasoning_summary;
        reloaded_for_comparison.developer_instructions = live_config.developer_instructions.clone();
        reloaded_for_comparison.compact_prompt = live_config.compact_prompt.clone();
        reloaded_for_comparison.permissions.approval_policy =
            live_config.permissions.approval_policy.clone();
        reloaded_for_comparison.permissions.sandbox_policy =
            live_config.permissions.sandbox_policy.clone();
        reloaded_for_comparison
            .permissions
            .file_system_sandbox_policy =
            live_config.permissions.file_system_sandbox_policy.clone();
        reloaded_for_comparison.permissions.network_sandbox_policy =
            live_config.permissions.network_sandbox_policy;
        reloaded_for_comparison.permissions.shell_environment_policy =
            live_config.permissions.shell_environment_policy.clone();
        reloaded_for_comparison.codex_linux_sandbox_exe =
            live_config.codex_linux_sandbox_exe.clone();

        let has_enabled_project_layer = reloaded_config
            .config_layer_stack
            .get_layers(
                ConfigLayerStackOrdering::LowestPrecedenceFirst,
                /*include_disabled*/ false,
            )
            .into_iter()
            .any(|layer| matches!(layer.name, ConfigLayerSource::Project { .. }));

        if has_enabled_project_layer && reloaded_for_comparison != *live_config {
            config = reloaded_config;
        }
    }
    config.model = Some(turn.model_info.slug.clone());
    config.model_provider = turn.provider.info().clone();
    config.model_providers.insert(
        config.model_provider_id.clone(),
        config.model_provider.clone(),
    );
    config.model_reasoning_effort = turn
        .reasoning_effort
        .or(turn.model_info.default_reasoning_level);
    config.model_reasoning_summary = Some(turn.reasoning_summary);
    config.developer_instructions = turn.developer_instructions.clone();
    config.compact_prompt = turn.compact_prompt.clone();
    apply_spawn_agent_runtime_overrides(&mut config, turn)?;

    Ok(config)
}

pub(crate) fn reject_full_fork_spawn_overrides(
    agent_type: Option<&str>,
    model: Option<&str>,
    reasoning_effort: Option<ReasoningEffort>,
) -> Result<(), FunctionCallError> {
    if agent_type.is_some() || model.is_some() || reasoning_effort.is_some() {
        return Err(FunctionCallError::RespondToModel(
            "Full-history forked agents inherit the parent agent type, model, and reasoning effort; omit agent_type, model, and reasoning_effort, or spawn without a full-history fork.".to_string(),
        ));
    }
    Ok(())
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

pub(crate) async fn apply_requested_spawn_agent_model_overrides(
    session: &Session,
    turn: &TurnContext,
    config: &mut Config,
    requested_provider: Option<&str>,
    requested_model: Option<&str>,
    requested_reasoning_effort: Option<ReasoningEffort>,
) -> Result<(), FunctionCallError> {
    let mut provider_changed_without_explicit_model = false;
    if requested_provider.is_none()
        && requested_model.is_none()
        && requested_reasoning_effort.is_none()
    {
        return Ok(());
    }

    if let Some(requested_provider) = requested_provider {
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
        if requested_model.is_none() {
            provider_changed_without_explicit_model = true;
            config.model = resolve_spawn_agent_provider_default_model(turn, config).await;
            if let Some(model) = config.model.as_deref() {
                let model_info = session
                    .services
                    .models_manager
                    .get_model_info(model, &config.to_models_manager_config())
                    .await;
                if requested_reasoning_effort.is_none() {
                    config.model_reasoning_effort = model_info.default_reasoning_level;
                }
            } else if requested_reasoning_effort.is_none() {
                config.model_reasoning_effort = None;
            }
        }
    }

    if let Some(requested_model) = requested_model {
        let available_models = session
            .services
            .models_manager
            .list_models(RefreshStrategy::Offline)
            .await;
        let selected_model_name = find_spawn_agent_model_name(&available_models, requested_model)?;
        let selected_model_info = session
            .services
            .models_manager
            .get_model_info(&selected_model_name, &config.to_models_manager_config())
            .await;

        config.model = Some(selected_model_name.clone());
        if let Some(reasoning_effort) = requested_reasoning_effort {
            validate_spawn_agent_reasoning_effort(
                &selected_model_name,
                &selected_model_info.supported_reasoning_levels,
                reasoning_effort,
            )?;
            config.model_reasoning_effort = Some(reasoning_effort);
        } else {
            config.model_reasoning_effort = selected_model_info.default_reasoning_level;
        }

        return Ok(());
    }

    if let Some(reasoning_effort) = requested_reasoning_effort {
        let resolved_model = if provider_changed_without_explicit_model {
            config.model.clone().ok_or_else(|| {
                FunctionCallError::RespondToModel(
                    "Cannot set reasoning_effort when provider override did not resolve a default model; pass `model` explicitly.".to_string(),
                )
            })?
        } else {
            config
                .model
                .clone()
                .unwrap_or_else(|| turn.model_info.slug.clone())
        };
        let resolved_model_info = session
            .services
            .models_manager
            .get_model_info(&resolved_model, &config.to_models_manager_config())
            .await;
        validate_spawn_agent_reasoning_effort(
            &resolved_model,
            &resolved_model_info.supported_reasoning_levels,
            reasoning_effort,
        )?;
        config.model_reasoning_effort = Some(reasoning_effort);
    }

    Ok(())
}

async fn resolve_spawn_agent_provider_default_model(
    turn: &TurnContext,
    config: &Config,
) -> Option<String> {
    if let Some(model) = config.model_provider.model.clone() {
        return Some(model);
    }
    config
        .model
        .clone()
        .or_else(|| Some(turn.model_info.slug.clone()))
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
