use super::WorkerSlot;
use codex_app_server_protocol::FederationThreadStartParams;
use codex_app_server_protocol::SessionSource;
use codex_app_server_protocol::Thread;
use codex_protocol::protocol::SubAgentSource;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FederationWorkerConfig {
    pub(crate) name: String,
    pub(crate) role: String,
    pub(crate) scope: Option<String>,
    pub(crate) state_root: Option<String>,
    pub(crate) lease_ttl_secs: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FederationAdapter {
    frontend: FederationWorkerConfig,
    backend: FederationWorkerConfig,
}

impl FederationAdapter {
    pub(crate) fn from_thread_start_params(params: FederationThreadStartParams) -> Self {
        Self {
            frontend: federation_worker_config(&params, WorkerSlot::Frontend),
            backend: federation_worker_config(&params, WorkerSlot::Backend),
        }
    }

    pub(crate) fn summary(&self) -> String {
        format!(
            "frontend => {}，backend => {}",
            self.frontend.name, self.backend.name
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) enum WorkerSource {
    #[default]
    LocalThreadSpawn,
    #[allow(dead_code)]
    FederationBridge(FederationWorkerConfig),
}

impl WorkerSource {
    pub(crate) fn label(&self) -> String {
        match self {
            Self::LocalThreadSpawn => "本地 subagent".to_string(),
            Self::FederationBridge(config) => format!("federation adapter ({})", config.name),
        }
    }
}

pub(crate) fn local_thread_matches_slot(slot: WorkerSlot, thread: &Thread) -> bool {
    match &thread.source {
        SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
            agent_path: Some(agent_path),
            ..
        }) if agent_path.to_string() == slot.canonical_task_name() => true,
        SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
            agent_nickname: Some(agent_nickname),
            agent_role: Some(agent_role),
            ..
        }) if agent_role == slot.role_name()
            && slot_matches_agent_nickname(slot, agent_nickname) =>
        {
            true
        }
        _ => false,
    }
}

fn slot_matches_agent_nickname(slot: WorkerSlot, agent_nickname: &str) -> bool {
    let nickname = agent_nickname.trim();
    nickname == slot.display_name()
        || nickname.eq_ignore_ascii_case(slot.task_name())
        || matches!(
            slot,
            WorkerSlot::Frontend
                if nickname == "前端"
                    || nickname.eq_ignore_ascii_case("android")
                    || nickname.eq_ignore_ascii_case("android frontend")
        )
        || matches!(
            slot,
            WorkerSlot::Backend if nickname == "后端" || nickname.eq_ignore_ascii_case("server")
        )
}

fn federation_worker_config(
    params: &FederationThreadStartParams,
    slot: WorkerSlot,
) -> FederationWorkerConfig {
    FederationWorkerConfig {
        name: format!("{}-{}", params.name, slot.task_name()),
        role: slot.role_name().to_string(),
        scope: params.scope.clone(),
        state_root: params.state_root.clone(),
        lease_ttl_secs: params.lease_ttl_secs,
    }
}
