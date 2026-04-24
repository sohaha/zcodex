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
        // Legacy threads may lack canonical agent_path metadata. Only fall back to
        // exact display/task-name matches so ordinary descendants are not
        // accidentally absorbed into a ZTeam slot.
        }) if agent_role == slot.role_name()
            && slot_matches_legacy_agent_nickname(slot, agent_nickname) =>
        {
            true
        }
        _ => false,
    }
}

fn slot_matches_legacy_agent_nickname(slot: WorkerSlot, agent_nickname: &str) -> bool {
    let nickname = agent_nickname.trim();
    nickname == slot.display_name() || nickname.eq_ignore_ascii_case(slot.task_name())
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

#[cfg(test)]
mod tests {
    use super::WorkerSlot;
    use super::local_thread_matches_slot;
    use codex_app_server_protocol::SessionSource;
    use codex_app_server_protocol::Thread;
    use codex_app_server_protocol::ThreadStatus;
    use codex_protocol::ThreadId;
    use codex_protocol::protocol::SubAgentSource;
    use codex_utils_absolute_path::test_support::PathBufExt;
    use codex_utils_absolute_path::test_support::test_path_buf;
    use pretty_assertions::assert_eq;

    fn thread_with_spawn_metadata(
        slot: WorkerSlot,
        agent_path: Option<&str>,
        agent_nickname: &str,
        agent_role: &str,
    ) -> Thread {
        Thread {
            id: ThreadId::new().to_string(),
            forked_from_id: None,
            preview: String::new(),
            ephemeral: false,
            model_provider: "openai".to_string(),
            created_at: 0,
            updated_at: 0,
            status: ThreadStatus::Idle,
            path: None,
            cwd: test_path_buf("/tmp").abs(),
            cli_version: "0.0.0".to_string(),
            source: SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
                parent_thread_id: ThreadId::new(),
                depth: 1,
                parent_model: None,
                agent_path: agent_path.map(|value| value.parse().expect("valid agent path")),
                agent_nickname: Some(agent_nickname.to_string()),
                agent_role: Some(agent_role.to_string()),
            }),
            agent_nickname: Some(agent_nickname.to_string()),
            agent_role: Some(agent_role.to_string()),
            git_info: None,
            name: Some(slot.display_name().to_string()),
            turns: Vec::new(),
        }
    }

    #[test]
    fn local_thread_matches_slot_accepts_canonical_agent_path() {
        let thread = thread_with_spawn_metadata(
            WorkerSlot::Frontend,
            Some("/root/frontend"),
            "其他名字",
            "other-role",
        );

        assert!(local_thread_matches_slot(WorkerSlot::Frontend, &thread));
    }

    #[test]
    fn local_thread_matches_slot_only_uses_exact_legacy_nickname_matches() {
        let exact_legacy = thread_with_spawn_metadata(
            WorkerSlot::Backend,
            None,
            "backend",
            WorkerSlot::Backend.role_name(),
        );
        let loose_legacy = thread_with_spawn_metadata(
            WorkerSlot::Backend,
            None,
            "server",
            WorkerSlot::Backend.role_name(),
        );

        assert!(local_thread_matches_slot(
            WorkerSlot::Backend,
            &exact_legacy
        ));
        assert_eq!(
            local_thread_matches_slot(WorkerSlot::Backend, &loose_legacy),
            false
        );
    }
}
