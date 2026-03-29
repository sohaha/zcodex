//! Implements the collaboration tool surface for spawning and managing sub-agents.
//!
//! This handler translates model tool calls into `AgentControl` operations and keeps spawned
//! agents aligned with the live turn that created them. Sub-agents start from the turn's effective
//! config, inherit runtime-only state such as provider, approval policy, sandbox, and cwd, and
//! then optionally layer role-specific config on top.

use crate::agent::AgentStatus;
use crate::agent::exceeds_thread_spawn_depth_limit;
use crate::codex::Session;
use crate::codex::TurnContext;
use crate::config::Config;
use crate::function_tool::FunctionCallError;
use crate::models_manager::manager::RefreshStrategy;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolOutput;
use crate::tools::context::ToolPayload;
pub(crate) use crate::tools::handlers::multi_agents_common::*;
use crate::tools::handlers::parse_arguments;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::ToolKind;
use async_trait::async_trait;
use codex_protocol::ThreadId;
use codex_protocol::models::ResponseInputItem;
use codex_protocol::openai_models::ReasoningEffort;
use codex_protocol::openai_models::ReasoningEffortPreset;
use codex_protocol::protocol::CollabAgentInteractionBeginEvent;
use codex_protocol::protocol::CollabAgentInteractionEndEvent;
use codex_protocol::protocol::CollabAgentRef;
use codex_protocol::protocol::CollabAgentSpawnBeginEvent;
use codex_protocol::protocol::CollabAgentSpawnEndEvent;
use codex_protocol::protocol::CollabCloseBeginEvent;
use codex_protocol::protocol::CollabCloseEndEvent;
use codex_protocol::protocol::CollabResumeBeginEvent;
use codex_protocol::protocol::CollabResumeEndEvent;
use codex_protocol::protocol::CollabWaitingBeginEvent;
use codex_protocol::protocol::CollabWaitingEndEvent;
use codex_protocol::user_input::UserInput;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value as JsonValue;
use tracing::warn;

pub(crate) fn parse_agent_id_target(target: &str) -> Result<ThreadId, FunctionCallError> {
    ThreadId::from_string(target).map_err(|err| {
        FunctionCallError::RespondToModel(format!("invalid agent id {target}: {err:?}"))
    })
}

pub(crate) fn parse_agent_id_targets(
    targets: Vec<String>,
) -> Result<Vec<ThreadId>, FunctionCallError> {
    if targets.is_empty() {
        return Err(FunctionCallError::RespondToModel(
            "agent ids must be non-empty".to_string(),
        ));
    }

    targets
        .into_iter()
        .map(|target| parse_agent_id_target(&target))
        .collect()
}

pub(crate) use close_agent::Handler as CloseAgentHandler;
pub(crate) use resume_agent::Handler as ResumeAgentHandler;
pub(crate) use send_input::Handler as SendInputHandler;
pub(crate) use spawn::Handler as SpawnAgentHandler;
pub(crate) use wait::Handler as WaitAgentHandler;

pub mod close_agent;
mod resume_agent;
mod send_input;
mod spawn;
pub(crate) mod wait;
async fn apply_requested_spawn_agent_model_overrides(
    session: &Session,
    turn: &TurnContext,
    config: &mut Config,
    requested_model: Option<&str>,
    requested_reasoning_effort: Option<ReasoningEffort>,
) -> Result<(), FunctionCallError> {
    if requested_model.is_none() && requested_reasoning_effort.is_none() {
        return Ok(());
    }

    if let Some(requested_model) = requested_model {
        let offline_models = session
            .services
            .models_manager
            .list_models(RefreshStrategy::Offline)
            .await;
        let selected_model_name = match find_spawn_agent_model_name(
            &offline_models,
            requested_model,
        ) {
            Ok(model) => model,
            Err(_) => {
                let refreshed_models = session
                    .services
                    .models_manager
                    .list_models(RefreshStrategy::Online)
                    .await;
                match find_spawn_agent_model_name(&refreshed_models, requested_model) {
                    Ok(model) => model,
                    Err(_) => {
                        warn!(
                            requested_model,
                            fallback_model = %turn.model_info.slug,
                            "spawn_agent requested model unavailable after refreshing model catalog; falling back to parent model"
                        );
                        return Ok(());
                    }
                }
            }
        };
        let selected_model_info = session
            .services
            .models_manager
            .get_model_info(&selected_model_name, config)
            .await;

        if let Some(reasoning_effort) = requested_reasoning_effort {
            if validate_spawn_agent_reasoning_effort(
                &selected_model_name,
                &selected_model_info.supported_reasoning_levels,
                reasoning_effort,
            )
            .is_err()
            {
                warn!(
                    requested_model = %selected_model_name,
                    requested_reasoning_effort = %reasoning_effort,
                    fallback_model = %turn.model_info.slug,
                    "spawn_agent requested reasoning effort is unsupported for the requested model; falling back to parent model"
                );
                return Ok(());
            }
            config.model = Some(selected_model_name);
            config.model_reasoning_effort = Some(reasoning_effort);
        } else {
            config.model = Some(selected_model_name);
            config.model_reasoning_effort = selected_model_info.default_reasoning_level;
        }

        return Ok(());
    }

    if let Some(reasoning_effort) = requested_reasoning_effort {
        validate_spawn_agent_reasoning_effort(
            &turn.model_info.slug,
            &turn.model_info.supported_reasoning_levels,
            reasoning_effort,
        )?;
        config.model_reasoning_effort = Some(reasoning_effort);
    }

    Ok(())
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

#[cfg(test)]
#[path = "multi_agents_tests.rs"]
mod tests;
