use codex_app_server_protocol::FederationThreadStartParams;
use codex_app_server_protocol::ServerNotification;
use codex_app_server_protocol::Thread;
use codex_app_server_protocol::ThreadItem;
use codex_protocol::AgentPath;
use codex_protocol::ThreadId;
use codex_protocol::models::MessagePhase;
use codex_protocol::protocol::InterAgentCommunication;
use std::collections::VecDeque;
use std::fmt;
use std::sync::Arc;
use std::sync::RwLock;

mod autopilot;
mod recovery;
mod view;
mod worker_source;

pub(crate) use autopilot::AutoAction;
pub(crate) use autopilot::AutopilotState;
pub(crate) use autopilot::WaitingOn;
pub(crate) use recovery::RecoveredWorker;
pub(crate) use recovery::WorkerConnection;
pub(crate) use recovery::latest_local_threads_for_primary;
pub(crate) use recovery::recover_local_worker;
pub(crate) use view::WORKBENCH_VIEW_ID;
pub(crate) use view::WorkbenchView;
pub(crate) use worker_source::FederationAdapter;
pub(crate) use worker_source::WorkerSource;

pub(crate) const MODE_NAME: &str = "ZTeam";
pub(crate) const COMMAND_NAME: &str = "/zteam";
const FRONTEND_TASK_NAME: &str = "frontend";
const BACKEND_TASK_NAME: &str = "backend";
const FRONTEND_ROLE: &str = "frontend-engineer";
const BACKEND_ROLE: &str = "backend-engineer";
const MAX_ACTIVITY_ITEMS: usize = 6;
const MAX_RESULT_ITEMS: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum WorkerSlot {
    Frontend,
    Backend,
}

impl WorkerSlot {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "frontend" => Some(Self::Frontend),
            "backend" => Some(Self::Backend),
            _ => None,
        }
    }

    pub(crate) fn task_name(self) -> &'static str {
        match self {
            Self::Frontend => FRONTEND_TASK_NAME,
            Self::Backend => BACKEND_TASK_NAME,
        }
    }

    pub(crate) fn role_name(self) -> &'static str {
        match self {
            Self::Frontend => FRONTEND_ROLE,
            Self::Backend => BACKEND_ROLE,
        }
    }

    pub(crate) fn display_name(self) -> &'static str {
        match self {
            Self::Frontend => "协作者 A",
            Self::Backend => "协作者 B",
        }
    }

    fn canonical_task_name(self) -> String {
        format!("/root/{}", self.task_name())
    }
}

impl fmt::Display for WorkerSlot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.display_name())
    }
}

/// 每个 worker slot 的可选配置覆盖
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SlotConfig {
    pub role_name: Option<String>,
    pub display_name: Option<String>,
    pub domain_keywords: Vec<String>,
}

/// ZTeam 协作配置，从 config.toml 的 tui.zteam_frontend / tui.zteam_backend 构建
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct TeamConfig {
    pub frontend: SlotConfig,
    pub backend: SlotConfig,
}

/// 获取 slot 的 agent_type 角色名，优先使用 config 覆盖
fn slot_role_name(slot: WorkerSlot, config: &TeamConfig) -> &str {
    let override_val = match slot {
        WorkerSlot::Frontend => config.frontend.role_name.as_deref(),
        WorkerSlot::Backend => config.backend.role_name.as_deref(),
    };
    override_val.unwrap_or_else(|| slot.role_name())
}

/// 获取 slot 的显示名，优先使用 config 覆盖
fn slot_display_name(slot: WorkerSlot, config: &TeamConfig) -> String {
    let override_val = match slot {
        WorkerSlot::Frontend => config.frontend.display_name.as_deref(),
        WorkerSlot::Backend => config.backend.display_name.as_deref(),
    };
    override_val.unwrap_or_else(|| slot.display_name()).to_string()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Command {
    Start {
        goal: Option<String>,
    },
    Status,
    Attach,
    Dispatch {
        worker: WorkerSlot,
        message: String,
    },
    Relay {
        from: WorkerSlot,
        to: WorkerSlot,
        message: String,
    },
}

pub(crate) fn entry_available_during_task(args: Option<&str>) -> bool {
    let Some(head) = args.and_then(|args| args.split_whitespace().next()) else {
        return true;
    };
    head.eq_ignore_ascii_case("status")
}

impl Command {
    pub(crate) fn parse(args: &str) -> Result<Self, String> {
        let trimmed = args.trim();
        if trimmed.is_empty() {
            return Err(usage().to_string());
        }

        let mut parts = trimmed.split_whitespace();
        let Some(head) = parts.next() else {
            return Err(usage().to_string());
        };
        match head.to_ascii_lowercase().as_str() {
            "start" => {
                let goal = trimmed
                    .split_once(char::is_whitespace)
                    .map(|(_, goal)| sanitize_mission_goal(goal.trim()))
                    .transpose()?
                    .flatten();
                Ok(Self::Start { goal })
            }
            "status" => {
                if parts.next().is_some() {
                    return Err(usage().to_string());
                }
                Ok(Self::Status)
            }
            "attach" => {
                if parts.next().is_some() {
                    return Err(usage().to_string());
                }
                Ok(Self::Attach)
            }
            "relay" => {
                let Some((_, rest)) = trimmed.split_once(char::is_whitespace) else {
                    return Err(usage().to_string());
                };
                let rest = rest.trim_start();
                let Some((from_raw, rest)) = rest.split_once(char::is_whitespace) else {
                    return Err(usage().to_string());
                };
                let rest = rest.trim_start();
                let Some((to_raw, message)) = rest.split_once(char::is_whitespace) else {
                    return Err(usage().to_string());
                };
                let Some(message) = Some(message.trim()).filter(|message| !message.is_empty())
                else {
                    return Err(usage().to_string());
                };
                let Some(from) = WorkerSlot::parse(from_raw) else {
                    return Err(usage().to_string());
                };
                let Some(to) = WorkerSlot::parse(to_raw) else {
                    return Err(usage().to_string());
                };
                Ok(Self::Relay {
                    from,
                    to,
                    message: message.to_string(),
                })
            }
            other => {
                let Some(worker) = WorkerSlot::parse(other) else {
                    return Err(usage().to_string());
                };
                let Some(message) = trimmed
                    .split_once(char::is_whitespace)
                    .map(|(_, message)| message.trim())
                    .filter(|message| !message.is_empty())
                else {
                    return Err(usage().to_string());
                };
                Ok(Self::Dispatch {
                    worker,
                    message: message.to_string(),
                })
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct State {
    inner: Arc<RwLock<SharedState>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct SharedState {
    start_requested: bool,
    frontend: WorkerState,
    backend: WorkerState,
    mission: Option<Mission>,
    autopilot: AutopilotState,
    activity: VecDeque<ActivityEntry>,
    recent_results: VecDeque<ResultEntry>,
    federation_adapter: Option<FederationAdapter>,
    team_config: TeamConfig,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct WorkerState {
    connection: WorkerConnection,
    source: WorkerSource,
    last_dispatched_task: Option<String>,
    last_result: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ActivityEntry {
    summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResultEntry {
    worker: WorkerSlot,
    summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Mission {
    goal: String,
    mode: MissionMode,
    phase: MissionPhase,
    acceptance_checks: Vec<AcceptanceCheck>,
    frontend_role: Option<String>,
    backend_role: Option<String>,
    frontend_assignment: Option<String>,
    backend_assignment: Option<String>,
    validation_summary: Option<String>,
    blocker: Option<String>,
    next_action: Option<String>,
    cycle: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum MissionMode {
    Solo(WorkerSlot),
    Parallel,
    SerialHandoff,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MissionPhase {
    Idle,
    Bootstrapping,
    Planning,
    Executing,
    Validating,
    Blocked,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AcceptanceCheck {
    summary: String,
    status: AcceptanceStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AcceptanceStatus {
    Pending,
    Met,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Snapshot {
    start_requested: bool,
    frontend: WorkerState,
    backend: WorkerState,
    mission: Option<Mission>,
    autopilot: AutopilotState,
    activity: Vec<ActivityEntry>,
    recent_results: Vec<ResultEntry>,
    federation_adapter: Option<FederationAdapter>,
}

impl Snapshot {
    fn worker(&self, worker: WorkerSlot) -> &WorkerState {
        match worker {
            WorkerSlot::Frontend => &self.frontend,
            WorkerSlot::Backend => &self.backend,
        }
    }

    fn live_workers(&self) -> Vec<WorkerSlot> {
        WorkerSlot::ALL
            .into_iter()
            .filter(|worker| matches!(self.worker(*worker).connection, WorkerConnection::Live(_)))
            .collect()
    }

    fn pending_workers(&self) -> Vec<WorkerSlot> {
        WorkerSlot::ALL
            .into_iter()
            .filter(|worker| matches!(self.worker(*worker).connection, WorkerConnection::Pending))
            .collect()
    }

    fn reattach_workers(&self) -> Vec<WorkerSlot> {
        WorkerSlot::ALL
            .into_iter()
            .filter(|worker| {
                matches!(
                    self.worker(*worker).connection,
                    WorkerConnection::ReattachRequired(_)
                )
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AutopilotWorkItem {
    AttachFirstRepair(Vec<WorkerSlot>),
    RootPrompt { action: AutoAction, prompt: String },
}

impl State {
    pub(crate) fn new(team_config: TeamConfig) -> Self {
        Self {
            inner: Arc::new(RwLock::new(SharedState {
                team_config,
                ..SharedState::default()
            })),
        }
    }

    pub(crate) fn team_config(&self) -> TeamConfig {
        self.read_state().team_config.clone()
    }

    pub(crate) fn mark_start_requested_for_goal(&mut self, goal: Option<&str>) -> bool {
        let sanitized_goal = goal.and_then(|goal| sanitize_mission_goal(goal).ok().flatten());
        let mut state = self.write_state();
        let changed = !state.start_requested
            || WorkerSlot::ALL.into_iter().any(|worker| {
                let worker_state = state.worker(worker);
                !matches!(worker_state.connection, WorkerConnection::Pending)
                    || worker_state.last_dispatched_task.is_some()
                    || worker_state.last_result.is_some()
            })
            || state.mission.as_ref().map(|mission| mission.goal.as_str())
                != sanitized_goal.as_deref();
        if !changed {
            return false;
        }
        state.start_requested = true;
        for worker in WorkerSlot::ALL {
            *state.worker_mut(worker) = WorkerState::default();
        }
        state.mission = sanitized_goal.as_deref().map(|g| plan_mission(g, &state.team_config));
        reset_autopilot_for_new_mission(&mut state);
        push_activity(
            &mut state.activity,
            "主线程已提交创建默认协作者的启动指令。等待 spawn 事件注册；若长时间无变化，请检查主线程是否真正调用了 `spawn_agent`。".to_string(),
        );
        if let Some((goal, mode_label, next_action)) = state.mission.as_ref().map(|mission| {
            (
                preview(&mission.goal),
                mission.mode.label(),
                mission
                    .next_action
                    .as_deref()
                    .unwrap_or("等待 worker 进入协作上下文")
                    .to_string(),
            )
        }) {
            push_activity(
                &mut state.activity,
                format!("当前 mission：{goal}。模式：{mode_label}；下一步：{next_action}。"),
            );
        }
        true
    }

    pub(crate) fn worker_thread_id(&self, worker: WorkerSlot) -> Option<ThreadId> {
        self.read_state().worker(worker).connection.live_thread_id()
    }

    pub(crate) fn restore_worker(&mut self, recovered: RecoveredWorker) -> bool {
        let mut state = self.write_state();
        state.start_requested = true;
        let next_state = WorkerState {
            connection: recovered.connection.clone(),
            source: recovered.source,
            last_dispatched_task: recovered.last_dispatched_task,
            last_result: recovered.last_result,
        };
        if *state.worker(recovered.slot) == next_state {
            return false;
        }
        *state.worker_mut(recovered.slot) = next_state;
        let synthesized_recovery_mission = if state.mission.is_none() {
            let frontend = state.frontend.clone();
            let backend = state.backend.clone();
            state.mission = Some(plan_recovery_mission(&frontend, &backend, &state.team_config));
            reset_autopilot_for_recovery(&mut state);
            true
        } else {
            false
        };
        let frontend = state.frontend.clone();
        let backend = state.backend.clone();
        if let Some(mission) = state.mission.as_mut() {
            if synthesized_recovery_mission {
                apply_recovery_state(mission, &frontend, &backend);
            } else {
                sync_mission_phase(mission, &frontend, &backend);
            }
        }
        let restore_summary = match &recovered.connection {
            WorkerConnection::Pending => {
                format!(
                    "已恢复 {recovered_slot} 的最近协作记录，等待注册。",
                    recovered_slot = recovered.slot
                )
            }
            WorkerConnection::Live(thread_id) => format!(
                "已恢复 {worker}，并重新附着到 {thread_id}。",
                worker = recovered.slot
            ),
            WorkerConnection::ReattachRequired(thread_id) => format!(
                "已恢复 {worker} 的最近状态；线程 {thread_id} 当前未附着，可运行 `/zteam attach` 再附着。",
                worker = recovered.slot
            ),
        };
        push_activity(&mut state.activity, restore_summary);
        true
    }

    pub(crate) fn configure_federation_adapter(
        &mut self,
        params: Option<FederationThreadStartParams>,
    ) -> bool {
        let mut state = self.write_state();
        let next_adapter = params.map(FederationAdapter::from_thread_start_params);
        if state.federation_adapter == next_adapter {
            return false;
        }
        state.federation_adapter = next_adapter;
        true
    }

    pub(crate) fn build_root_dispatch(
        &self,
        worker: WorkerSlot,
        message: String,
    ) -> Option<(ThreadId, InterAgentCommunication)> {
        let thread_id = self.worker_thread_id(worker)?;
        let communication = InterAgentCommunication::new(
            AgentPath::root(),
            worker_agent_path(worker),
            Vec::new(),
            message,
            /*trigger_turn*/ true,
        );
        Some((thread_id, communication))
    }

    pub(crate) fn build_worker_relay(
        &self,
        from: WorkerSlot,
        to: WorkerSlot,
        message: String,
    ) -> Option<(ThreadId, InterAgentCommunication)> {
        let from_thread_id = self.worker_thread_id(from)?;
        let to_thread_id = self.worker_thread_id(to)?;
        if from_thread_id == to_thread_id {
            return None;
        }
        let communication = InterAgentCommunication::new(
            worker_agent_path(from),
            worker_agent_path(to),
            Vec::new(),
            message,
            /*trigger_turn*/ true,
        );
        Some((to_thread_id, communication))
    }

    pub(crate) fn take_autopilot_work_item(&mut self) -> Option<AutopilotWorkItem> {
        let mut state = self.write_state();
        if state.autopilot.pending_auto_action.is_some() {
            return None;
        }
        let Some(mission) = state.mission.clone() else {
            state.autopilot.waiting_on = WaitingOn::Idle;
            state.autopilot.queued_auto_action = None;
            return None;
        };
        let frontend = state.frontend.clone();
        let backend = state.backend.clone();
        let required_workers = mission.required_workers(&frontend, &backend);
        let repair_targets =
            current_repair_targets(&mission, &frontend, &backend, &state.autopilot);
        if !repair_targets.is_empty() {
            if !state.autopilot.attach_attempted {
                state.autopilot.attach_attempted = true;
                state.autopilot.waiting_on = WaitingOn::Repair(repair_targets.clone());
                state.autopilot.last_auto_action_result = Some(format!(
                    "自动 repair 已进入 attach-first 恢复：{}。",
                    worker_list(&repair_targets)
                ));
                return Some(AutopilotWorkItem::AttachFirstRepair(repair_targets));
            }
            if repair_targets.iter().all(|worker| {
                state.autopilot.repair_attempts.count(*worker) < autopilot::MAX_REPAIR_ATTEMPTS
            }) {
                let prompt = autopilot::repair_workers_prompt(
                    &mission,
                    state.autopilot.current_cycle,
                    &repair_targets,
                    &state.autopilot.repair_attempts,
                );
                mark_root_action_submitted(&mut state, AutoAction::RepairWorkers, &repair_targets);
                return Some(AutopilotWorkItem::RootPrompt {
                    action: AutoAction::RepairWorkers,
                    prompt,
                });
            }
            let repair_summary = state.autopilot.repair_attempts.summary();
            mark_autopilot_blocked(
                &mut state,
                format!(
                    "自动 repair 已达到上限：{repair_summary}。请人工决定是否继续 `/zteam attach` 或重新 `/zteam start <goal>`。"
                ),
            );
            return None;
        }
        if state.autopilot.manual_override_active
            && !required_workers_have_results(&mission, &frontend, &backend)
        {
            state.autopilot.waiting_on = WaitingOn::Results(required_workers);
            return None;
        }

        let next_action =
            state.autopilot.queued_auto_action.take().or_else(|| {
                infer_next_auto_action(&mission, &frontend, &backend, &state.autopilot)
            });
        let Some(action) = next_action else {
            sync_autopilot_waiting_on_from_state(&mut state, &frontend, &backend);
            return None;
        };
        let prompt = match action {
            AutoAction::PlanCycle => autopilot::plan_cycle_prompt(
                &mission,
                state.autopilot.current_cycle,
                &required_workers,
                state.autopilot.manual_override_active,
            ),
            AutoAction::DispatchCycle => autopilot::dispatch_cycle_prompt(
                &mission,
                state.autopilot.current_cycle,
                &required_workers,
                state.autopilot.manual_override_active,
            ),
            AutoAction::SummarizeResults => {
                let results = required_workers
                    .iter()
                    .filter_map(|worker| {
                        worker_state_for(*worker, &frontend, &backend)
                            .last_result
                            .as_deref()
                            .map(|result| autopilot::result_preview(*worker, result))
                    })
                    .collect::<Vec<_>>();
                autopilot::summarize_results_prompt(
                    &mission,
                    state.autopilot.current_cycle,
                    &required_workers,
                    &results,
                    state.autopilot.manual_override_active,
                )
            }
            AutoAction::CompleteMission => {
                autopilot::complete_mission_prompt(&mission, state.autopilot.current_cycle)
            }
            AutoAction::BootstrapWorkers | AutoAction::RepairWorkers => return None,
        };
        mark_root_action_submitted(&mut state, action, &required_workers);
        Some(AutopilotWorkItem::RootPrompt { action, prompt })
    }

    pub(crate) fn finish_attach_first_repair(&mut self) -> bool {
        let mut state = self.write_state();
        let Some(mission) = state.mission.clone() else {
            return false;
        };
        let frontend = state.frontend.clone();
        let backend = state.backend.clone();
        let repair_targets =
            current_repair_targets(&mission, &frontend, &backend, &state.autopilot);
        if repair_targets.is_empty() {
            state.autopilot.attach_attempted = false;
            state.autopilot.waiting_on = WaitingOn::Idle;
            state.autopilot.queued_auto_action = Some(if state.autopilot.cycle_planned {
                AutoAction::DispatchCycle
            } else {
                AutoAction::PlanCycle
            });
            state.autopilot.last_auto_action_result = Some(
                "attach-first repair 已恢复需要参与的 worker，准备继续当前 cycle。".to_string(),
            );
            if let Some(mission_state) = state.mission.as_mut() {
                mission_state.phase = MissionPhase::Planning;
                mission_state.blocker = None;
                mission_state.validation_summary = Some(
                    "attach-first repair 已恢复需要参与的 worker，可继续自动协作。".to_string(),
                );
                mission_state.next_action = Some("准备重新规划并派发当前 cycle。".to_string());
            }
            return true;
        }
        state.autopilot.waiting_on = WaitingOn::Repair(repair_targets.clone());
        state.autopilot.last_auto_action_result = Some(format!(
            "attach-first repair 未完全恢复，仍缺 {}。",
            worker_list(&repair_targets)
        ));
        true
    }

    pub(crate) fn record_dispatch(&mut self, worker: WorkerSlot, message: &str) -> bool {
        let mut state = self.write_state();
        let worker_state = state.worker_mut(worker);
        if let Some(thread_id) = worker_state.connection.known_thread_id() {
            worker_state.connection = WorkerConnection::Live(thread_id);
        }
        worker_state.last_dispatched_task = Some(message.to_string());
        if state.mission.is_none() {
            state.mission = Some(plan_manual_override_mission(worker, message, &state.team_config));
        }
        let frontend = state.frontend.clone();
        let backend = state.backend.clone();
        if let Some(mission) = state.mission.as_mut() {
            mission.phase = MissionPhase::Executing;
            mission.blocker = None;
            mission.validation_summary = Some(
                "当前由旧命令触发手动 override；本轮结果回流后需重新判断是否继续沿 mission 主路径推进。"
                    .to_string(),
            );
            mission.assignment_mut(worker).replace(preview(message));
            mission.next_action =
                Some("等待本轮手动分派结果回流，再决定是否回到 mission 主流程。".to_string());
            sync_acceptance_checks(mission, &frontend, &backend);
        }
        state.autopilot.manual_override_active = true;
        state.autopilot.pending_auto_action = None;
        state.autopilot.queued_auto_action = None;
        state.autopilot.cycle_planned = false;
        state.autopilot.cycle_dispatched = true;
        state.autopilot.waiting_on = WaitingOn::Results(vec![worker]);
        state.autopilot.last_auto_action_result = Some(format!(
            "手动 override 已接管当前 cycle：{}。",
            preview(message)
        ));
        push_activity(
            &mut state.activity,
            format!("主线程 -> {worker}：{}", preview(message)),
        );
        true
    }

    pub(crate) fn record_relay(&mut self, from: WorkerSlot, to: WorkerSlot, message: &str) -> bool {
        let mut state = self.write_state();
        let target_state = state.worker_mut(to);
        if let Some(thread_id) = target_state.connection.known_thread_id() {
            target_state.connection = WorkerConnection::Live(thread_id);
        }
        target_state.last_dispatched_task = Some(message.to_string());
        if state.mission.is_none() {
            state.mission = Some(plan_manual_relay_mission(from, to, message, &state.team_config));
        }
        let frontend = state.frontend.clone();
        let backend = state.backend.clone();
        if let Some(mission) = state.mission.as_mut() {
            mission.phase = MissionPhase::Executing;
            mission.blocker = None;
            mission.validation_summary = Some(
                "当前由旧命令触发手动 override；请在 relay 结果回流后确认是否回到 mission 主路径。"
                    .to_string(),
            );
            mission.assignment_mut(to).replace(preview(message));
            mission.next_action = Some(
                "等待 relay 目标 worker 回流结果，再确认是否恢复 mission 主流程。".to_string(),
            );
            sync_acceptance_checks(mission, &frontend, &backend);
        }
        state.autopilot.manual_override_active = true;
        state.autopilot.pending_auto_action = None;
        state.autopilot.queued_auto_action = None;
        state.autopilot.cycle_planned = false;
        state.autopilot.cycle_dispatched = true;
        state.autopilot.waiting_on = WaitingOn::Results(vec![to]);
        state.autopilot.last_auto_action_result = Some(format!(
            "手动 relay override 已接管当前 cycle：{}。",
            preview(message)
        ));
        push_activity(
            &mut state.activity,
            format!("{from} -> {to}：{}", preview(message)),
        );
        true
    }

    pub(crate) fn observe_notification(
        &mut self,
        thread_id: ThreadId,
        is_primary_thread: bool,
        notification: &ServerNotification,
    ) -> bool {
        match notification {
            ServerNotification::ThreadStarted(notification) => {
                self.observe_thread_started(thread_id, &notification.thread)
            }
            ServerNotification::TurnCompleted(_) => {
                self.observe_turn_completed(thread_id, is_primary_thread)
            }
            ServerNotification::ItemCompleted(notification) => {
                self.observe_completed_item(thread_id, is_primary_thread, &notification.item)
            }
            ServerNotification::ThreadClosed(notification) => {
                if notification.thread_id == thread_id.to_string() {
                    self.observe_thread_closed(thread_id)
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    #[cfg(test)]
    pub(crate) fn status_message(&self) -> String {
        let snapshot = self.snapshot();
        let mut lines = Vec::new();
        lines.push(format!("{MODE_NAME} 状态："));
        lines.push(status_summary(&snapshot));
        if let Some(adapter) = &snapshot.federation_adapter {
            lines.push(format!("外部 adapter：{}", adapter.summary()));
        }
        for worker in WorkerSlot::ALL {
            lines.push(worker_status_line(worker, snapshot.worker(worker)));
        }
        lines.join("\n")
    }

    pub(crate) fn missing_worker_message(&self, worker: WorkerSlot) -> String {
        let snapshot = self.snapshot();
        match &snapshot.worker(worker).connection {
            WorkerConnection::Pending => format!(
                "{} 尚未注册。{}",
                worker.display_name(),
                pending_worker_guidance(&snapshot, worker)
            ),
            WorkerConnection::Live(_) => format!(
                "{} 当前已附着；如果仍然分派失败，请重新运行 `/zteam status` 检查状态。",
                worker.display_name()
            ),
            WorkerConnection::ReattachRequired(thread_id) => format!(
                "{} 最近的线程 `{thread_id}` 当前未附着。先运行 `/zteam attach` 尝试再附着，或用 {} 重建协作。",
                worker.display_name(),
                recommended_restart_command(&snapshot)
            ),
        }
    }

    pub(crate) fn missing_relay_message(&self, from: WorkerSlot, to: WorkerSlot) -> String {
        let snapshot = self.snapshot();
        let from_connection = &snapshot.worker(from).connection;
        let to_connection = &snapshot.worker(to).connection;
        let needs_attach = matches!(from_connection, WorkerConnection::ReattachRequired(_))
            || matches!(to_connection, WorkerConnection::ReattachRequired(_));
        if needs_attach {
            return format!(
                "无法在 {from} 和 {to} 之间中转消息。先运行 `/zteam attach` 重新附着最近的协作者，或用 {} 重新创建。",
                recommended_restart_command(&snapshot)
            );
        }
        let pending = snapshot.pending_workers();
        if !pending.is_empty() {
            let registered = snapshot.live_workers();
            if registered.is_empty() {
                return format!(
                    "无法在 {from} 和 {to} 之间中转消息。当前还没有任何协作者完成注册；先等待 {} 的创建结果，必要时重新运行 {}。",
                    recommended_restart_command(&snapshot),
                    recommended_restart_command(&snapshot)
                );
            }
            return format!(
                "无法在 {from} 和 {to} 之间中转消息。当前仅 {} 已注册，仍缺 {}；先运行 `/zteam status` 检查进度，必要时重新运行 {}。",
                worker_list(&registered),
                worker_list(&pending),
                recommended_restart_command(&snapshot)
            );
        }
        format!(
            "无法在 {from} 和 {to} 之间中转消息。先运行 {}，并确认默认协作者都已注册。",
            recommended_restart_command(&snapshot)
        )
    }

    fn snapshot(&self) -> Snapshot {
        let state = self.read_state();
        Snapshot {
            start_requested: state.start_requested,
            frontend: state.frontend.clone(),
            backend: state.backend.clone(),
            mission: state.mission.clone(),
            autopilot: state.autopilot.clone(),
            activity: state.activity.iter().cloned().collect(),
            recent_results: state.recent_results.iter().cloned().collect(),
            federation_adapter: state.federation_adapter.clone(),
        }
    }

    fn observe_thread_started(&mut self, thread_id: ThreadId, thread: &Thread) -> bool {
        let Some(worker) = WorkerSlot::ALL
            .into_iter()
            .find(|worker| worker_source::local_thread_matches_slot(*worker, thread))
        else {
            return false;
        };
        let mut state = self.write_state();
        {
            let worker_state = state.worker_mut(worker);
            if worker_state.connection == WorkerConnection::Live(thread_id) {
                return false;
            }
            worker_state.connection = WorkerConnection::Live(thread_id);
            worker_state.source = WorkerSource::LocalThreadSpawn;
        }
        let pending = WorkerSlot::ALL
            .into_iter()
            .filter(|slot| matches!(state.worker(*slot).connection, WorkerConnection::Pending))
            .collect::<Vec<_>>();
        let live = WorkerSlot::ALL
            .into_iter()
            .filter(|slot| matches!(state.worker(*slot).connection, WorkerConnection::Live(_)))
            .collect::<Vec<_>>();
        push_activity(
            &mut state.activity,
            format!("{worker} 已注册到 `{}`。", worker.canonical_task_name()),
        );
        let frontend = state.frontend.clone();
        let backend = state.backend.clone();
        if let Some(mission) = state.mission.as_mut() {
            sync_mission_phase(mission, &frontend, &backend);
        }
        sync_autopilot_waiting_on_from_state(&mut state, &frontend, &backend);
        if pending.is_empty() {
            push_activity(
                &mut state.activity,
                format!(
                    "{} 已全部注册，可开始分派任务或中转消息。",
                    worker_task_list(&WorkerSlot::ALL)
                ),
            );
        } else if !live.is_empty() {
            push_activity(
                &mut state.activity,
                format!(
                    "当前已收到 {}，仍等待 {} 注册。",
                    worker_list(&live),
                    worker_list(&pending)
                ),
            );
        }
        true
    }

    fn observe_completed_item(
        &mut self,
        thread_id: ThreadId,
        is_primary_thread: bool,
        item: &ThreadItem,
    ) -> bool {
        let ThreadItem::AgentMessage { text, phase, .. } = item else {
            return false;
        };
        if text.trim().is_empty() || *phase == Some(MessagePhase::Commentary) {
            return false;
        }
        if is_primary_thread {
            let state = self.read_state();
            if state.autopilot.pending_auto_action.is_some() {
                drop(state);
                let mut state = self.write_state();
                if let Some(result) = autopilot::parse_result_marker(text) {
                    state.autopilot.parsed_result = Some(result);
                    return true;
                }
                return false;
            }
        }
        let worker = {
            let state = self.read_state();
            let Some(worker) = state.worker_slot_for_thread(thread_id) else {
                return false;
            };
            worker
        };
        let mut state = self.write_state();
        {
            let worker_state = state.worker_mut(worker);
            worker_state.last_result = Some(text.clone());
            worker_state.source = WorkerSource::LocalThreadSpawn;
        }
        let frontend = state.frontend.clone();
        let backend = state.backend.clone();
        if let Some(mission) = state.mission.as_mut() {
            if required_workers_have_results(mission, &frontend, &backend) {
                mission.phase = MissionPhase::Validating;
                mission.validation_summary = Some(
                    "已收到当前需要参与的协作者阶段结果，等待主线程归纳验证结论。".to_string(),
                );
                mission.next_action = Some("检查阶段结果并决定下一轮分派或收口".to_string());
                sync_acceptance_checks(mission, &frontend, &backend);
                state.autopilot.waiting_on =
                    WaitingOn::Results(mission.required_workers(&frontend, &backend));
            } else {
                sync_mission_phase(mission, &frontend, &backend);
                if mission.mode == MissionMode::SerialHandoff
                    && worker == WorkerSlot::Backend
                    && backend
                        .last_result
                        .as_deref()
                        .is_some_and(|result| !result.trim().is_empty())
                    && frontend
                        .last_result
                        .as_deref()
                        .is_none_or(|result| result.trim().is_empty())
                {
                    mission.next_action =
                        Some("后端阶段结果已就绪，可分派前端接手实现与联调".to_string());
                }
            }
        }
        sync_autopilot_waiting_on_from_state(&mut state, &frontend, &backend);
        push_result(&mut state.recent_results, worker, preview(text));
        true
    }

    fn observe_turn_completed(&mut self, thread_id: ThreadId, is_primary_thread: bool) -> bool {
        if !is_primary_thread {
            return false;
        }
        let mut state = self.write_state();
        if state.worker_slot_for_thread(thread_id).is_some() {
            return false;
        }
        finalize_root_auto_action(&mut state)
    }

    fn observe_thread_closed(&mut self, thread_id: ThreadId) -> bool {
        let worker = {
            let state = self.read_state();
            let Some(worker) = state.worker_slot_for_thread(thread_id) else {
                return false;
            };
            worker
        };
        let mut state = self.write_state();
        let worker_state = state.worker_mut(worker);
        worker_state.connection = WorkerConnection::ReattachRequired(thread_id);
        if let Some(mission) = state.mission.as_mut() {
            mission.phase = MissionPhase::Blocked;
            mission.blocker = Some(format!(
                "{worker} 最近线程已关闭，需 `/zteam attach` 或重新启动协作。"
            ));
            mission.next_action = Some("先恢复 worker 连接，再继续当前 mission。".to_string());
        }
        state.autopilot.attach_attempted = false;
        state.autopilot.waiting_on = WaitingOn::Repair(vec![worker]);
        state.autopilot.queued_auto_action = None;
        state.autopilot.last_auto_action_result = Some(format!(
            "检测到 {worker} 断开，准备进入 attach-first repair。"
        ));
        let restart_command = if state.mission.is_some() {
            "`/zteam start <goal>`"
        } else {
            "`/zteam start`"
        };
        push_activity(
            &mut state.activity,
            format!("{worker} 已关闭，可运行 `/zteam attach` 再附着或 {restart_command} 重建。"),
        );
        true
    }

    fn read_state(&self) -> std::sync::RwLockReadGuard<'_, SharedState> {
        #[expect(clippy::expect_used)]
        self.inner.read().expect("zteam state poisoned")
    }

    fn write_state(&self) -> std::sync::RwLockWriteGuard<'_, SharedState> {
        #[expect(clippy::expect_used)]
        self.inner.write().expect("zteam state poisoned")
    }
}

impl SharedState {
    fn worker_slot_for_thread(&self, thread_id: ThreadId) -> Option<WorkerSlot> {
        if self.frontend.connection.known_thread_id() == Some(thread_id) {
            return Some(WorkerSlot::Frontend);
        }
        if self.backend.connection.known_thread_id() == Some(thread_id) {
            return Some(WorkerSlot::Backend);
        }
        None
    }

    fn worker(&self, worker: WorkerSlot) -> &WorkerState {
        match worker {
            WorkerSlot::Frontend => &self.frontend,
            WorkerSlot::Backend => &self.backend,
        }
    }

    fn worker_mut(&mut self, worker: WorkerSlot) -> &mut WorkerState {
        match worker {
            WorkerSlot::Frontend => &mut self.frontend,
            WorkerSlot::Backend => &mut self.backend,
        }
    }
}

fn sanitize_mission_goal(goal: &str) -> Result<Option<String>, String> {
    let sanitized = InterAgentCommunication::sanitize_visible_text(goal);
    let sanitized = sanitized.trim();
    if sanitized.is_empty() {
        if goal.trim().is_empty() {
            return Ok(None);
        }
        return Err("`/zteam start <目标>` 中包含的内容在净化内部协作消息后为空；请直接输入面向任务的目标。".to_string());
    }
    Ok(Some(sanitized.to_string()))
}

fn push_activity(activity: &mut VecDeque<ActivityEntry>, summary: String) {
    activity.push_front(ActivityEntry { summary });
    if activity.len() > MAX_ACTIVITY_ITEMS {
        activity.pop_back();
    }
}

fn push_result(results: &mut VecDeque<ResultEntry>, worker: WorkerSlot, summary: String) {
    results.push_front(ResultEntry { worker, summary });
    if results.len() > MAX_RESULT_ITEMS {
        results.pop_back();
    }
}

#[cfg(test)]
fn status_summary(snapshot: &Snapshot) -> String {
    format!("状态摘要：{}", startup_summary(snapshot))
}

#[cfg(test)]
fn startup_summary(snapshot: &Snapshot) -> String {
    if !snapshot.start_requested {
        return "尚未启动；先运行 `/zteam start <目标>`。".to_string();
    }

    let reattach = snapshot.reattach_workers();
    if !reattach.is_empty() {
        return format!(
            "{} 需要再附着。运行 `/zteam attach` 尝试恢复最近的协作连接。",
            worker_list(&reattach)
        );
    }

    let pending = snapshot.pending_workers();
    if !pending.is_empty() {
        let live = snapshot.live_workers();
        if live.is_empty() {
            return format!(
                "已提交创建请求，等待 {} 注册；若长时间无变化，请检查主线程是否真正创建了 worker。",
                worker_list(&pending)
            );
        }
        return format!(
            "当前已收到 {}，仍等待 {} 注册；若长时间无变化，请检查主线程是否只创建了一部分 worker。",
            worker_list(&live),
            worker_list(&pending)
        );
    }

    format!(
        "{} 已就绪，可继续分派任务或转发消息。",
        worker_task_list(&WorkerSlot::ALL)
    )
}

fn pending_worker_guidance(snapshot: &Snapshot, worker: WorkerSlot) -> String {
    let restart_command = recommended_restart_command(snapshot);
    let live = snapshot.live_workers();
    if live.is_empty() {
        return format!(
            "当前还没有任何协作者完成注册。先等待主线程创建 `{}`；若长时间无变化，请检查主线程是否真正调用了 `spawn_agent`，必要时重新运行 {restart_command}。",
            worker.canonical_task_name(),
        );
    }

    let other_live = live
        .into_iter()
        .filter(|registered| *registered != worker)
        .collect::<Vec<_>>();
    if other_live.is_empty() {
        return format!(
            "当前还在等待主线程创建 `{}`；可先运行 `/zteam status` 检查进度，若长时间无变化则重新运行 {restart_command}。",
            worker.canonical_task_name(),
        );
    }

    format!(
        "当前仅 {} 已注册，仍在等待 `{}`；若长时间无变化，说明主线程可能只创建了一部分协作者。先运行 `/zteam status` 检查，再决定是否重新运行 {restart_command}。",
        worker_list(&other_live),
        worker.canonical_task_name()
    )
}

fn recommended_restart_command(snapshot: &Snapshot) -> &'static str {
    if snapshot.mission.is_some() {
        "`/zteam start <goal>`"
    } else {
        "`/zteam start`"
    }
}

impl WorkerSlot {
    const ALL: [Self; 2] = [Self::Frontend, Self::Backend];
}

pub(crate) fn disabled_message() -> String {
    format!("{MODE_NAME} 已在当前 TUI 配置中关闭，{COMMAND_NAME} 不再可用。")
}

pub(crate) fn disabled_hint() -> &'static str {
    "在 `config.toml` 中设置 `[tui].zteam_enabled = true` 后可再次启用。"
}

pub(crate) fn usage() -> &'static str {
    "用法：/zteam start <目标> | /zteam start | /zteam status | /zteam attach | /zteam <frontend|backend> <任务> | /zteam relay <frontend|backend> <frontend|backend> <消息>"
}

pub(crate) fn start_prompt(goal: Option<&str>, config: &TeamConfig) -> String {
    let frontend_role = slot_role_name(WorkerSlot::Frontend, config);
    let backend_role = slot_role_name(WorkerSlot::Backend, config);
    match goal {
        Some(goal) => format!(
            concat!(
                "进入 ZTeam Mission 模式。当前目标：`{goal}`。\n",
                "立即使用 `spawn_agent` 创建两个长期 worker：\n",
                "1. `task_name = \"frontend\"`，`agent_type = \"{frontend_role}\"`\n",
                "2. `task_name = \"backend\"`，`agent_type = \"{backend_role}\"`\n",
                "对两个 worker 都说明：它们是长期协作者，主线程负责围绕当前目标拆分任务；需要彼此同步时优先使用 `send_message` 或 `followup_task`；完成阶段结果后继续待命，不要自行关闭。\n",
                "创建完成后，只用一条简短中文消息汇报两个 worker 的 canonical task name，并补一句你准备如何围绕当前目标组织第一轮协作。除非我下一条消息明确要求实现，否则不要开始业务修改。"
            ),
            goal = goal,
            frontend_role = frontend_role,
            backend_role = backend_role,
        ),
        None => format!(
            "进入 ZTeam 本地协作模式（兼容入口）。立即使用 `spawn_agent` 创建两个长期 worker：\n\\n             1. `task_name = \"frontend\"`，`agent_type = \"{frontend_role}\"`\n\\n             2. `task_name = \"backend\"`，`agent_type = \"{backend_role}\"`\n\\n             对两个 worker 都说明：它们是长期协作者，主线程负责拆分任务；需要彼此同步时优先使用 `send_message` 或 `followup_task`；完成阶段结果后继续待命，不要自行关闭。\n\\n             创建完成后，只用一条简短中文消息汇报两个 worker 的 canonical task name。除非我下一条消息明确分派任务，否则不要开始实现业务工作。"
        ),
    }
}

fn worker_list(workers: &[WorkerSlot]) -> String {
    workers
        .iter()
        .map(std::string::ToString::to_string)
        .collect::<Vec<_>>()
        .join("、")
}

fn worker_task_list(workers: &[WorkerSlot]) -> String {
    workers
        .iter()
        .map(std::string::ToString::to_string)
        .collect::<Vec<_>>()
        .join("、")
}

fn worker_agent_path(worker: WorkerSlot) -> AgentPath {
    match AgentPath::root().resolve(worker.task_name()) {
        Ok(path) => path,
        Err(err) => unreachable!("zteam worker task name should always resolve: {err}"),
    }
}

impl Mission {
    fn assignment_mut(&mut self, worker: WorkerSlot) -> &mut Option<String> {
        match worker {
            WorkerSlot::Frontend => &mut self.frontend_assignment,
            WorkerSlot::Backend => &mut self.backend_assignment,
        }
    }

    fn required_workers(&self, frontend: &WorkerState, backend: &WorkerState) -> Vec<WorkerSlot> {
        match self.mode {
            MissionMode::Solo(worker) => vec![worker],
            MissionMode::Parallel => WorkerSlot::ALL.to_vec(),
            MissionMode::SerialHandoff => {
                if backend
                    .last_result
                    .as_deref()
                    .is_some_and(|result| !result.trim().is_empty())
                    && frontend
                        .last_result
                        .as_deref()
                        .is_none_or(|result| result.trim().is_empty())
                {
                    vec![WorkerSlot::Frontend]
                } else {
                    vec![WorkerSlot::Backend]
                }
            }
            MissionMode::Blocked => Vec::new(),
        }
    }
}

impl MissionMode {
    fn label(&self) -> &'static str {
        match self {
            Self::Solo(WorkerSlot::Frontend) => "solo-frontend",
            Self::Solo(WorkerSlot::Backend) => "solo-backend",
            Self::Parallel => "parallel",
            Self::SerialHandoff => "serial-handoff",
            Self::Blocked => "blocked",
        }
    }
}

fn plan_mission(goal: &str, config: &TeamConfig) -> Mission {
    let goal = goal.trim().to_string();
    let mode = infer_mission_mode(&goal, config);
    let (frontend_role, backend_role, frontend_assignment, backend_assignment, next_action) =
        mission_assignments(&goal, &mode, config);
    let blocker = match mode {
        MissionMode::Blocked => Some("目标过于模糊，暂时无法规划可执行的协作路径。".to_string()),
        _ => None,
    };
    let phase = match mode {
        MissionMode::Blocked => MissionPhase::Blocked,
        _ => MissionPhase::Bootstrapping,
    };
    Mission {
        goal,
        mode,
        phase,
        acceptance_checks: vec![
            AcceptanceCheck {
                summary: "目标已拆成可执行的协作分工".to_string(),
                status: AcceptanceStatus::Pending,
            },
            AcceptanceCheck {
                summary: "需要参与的协作者已回流阶段结果".to_string(),
                status: AcceptanceStatus::Pending,
            },
            AcceptanceCheck {
                summary: "主线程已形成验证结论或下一轮动作".to_string(),
                status: AcceptanceStatus::Pending,
            },
        ],
        frontend_role,
        backend_role,
        frontend_assignment,
        backend_assignment,
        validation_summary: None,
        blocker,
        next_action: Some(next_action),
        cycle: 1,
    }
}

fn infer_mission_mode(goal: &str, config: &TeamConfig) -> MissionMode {
    let goal_lower = goal.to_ascii_lowercase();
    if goal.chars().count() <= 6
        || contains_any(
            goal,
            &[
                "看看",
                "分析一下",
                "讨论一下",
                "聊聊",
                "帮我想想",
                "怎么弄",
                "优化什么",
            ],
        )
    {
        return MissionMode::Blocked;
    }

    // 合并内置关键词和 config 自定义关键词
    let default_frontend = [
        "前端", "页面", "布局", "交互", "移动端", "组件", "样式", "导航", "表单", "ui",
        "frontend", "css", "react", "vue", "html",
    ];
    let default_backend = [
        "后端", "接口", "服务", "数据库", "登录", "token", "schema", "错误码", "api", "sql",
        "backend", "server", "migration", "database",
    ];

    let frontend_keywords: Vec<&str> = default_frontend
        .iter()
        .copied()
        .chain(config.frontend.domain_keywords.iter().map(String::as_str))
        .collect();
    let backend_keywords: Vec<&str> = default_backend
        .iter()
        .copied()
        .chain(config.backend.domain_keywords.iter().map(String::as_str))
        .collect();

    let frontend_like = contains_any(goal, &frontend_keywords);
    let backend_like = contains_any(goal, &backend_keywords);

    if contains_any(goal, &["先", "再", "联调", "对齐", "契约", "字段"]) {
        return MissionMode::SerialHandoff;
    }
    match (frontend_like, backend_like) {
        (true, true) => MissionMode::Parallel,
        (true, false) => MissionMode::Solo(WorkerSlot::Frontend),
        (false, true) => MissionMode::Solo(WorkerSlot::Backend),
        (false, false)
            if goal_lower.contains("同时") || goal.contains("并") || goal.contains("以及") =>
        {
            MissionMode::Parallel
        }
        _ => MissionMode::Parallel,
    }
}

fn mission_assignments(
    goal: &str,
    mode: &MissionMode,
    config: &TeamConfig,
) -> (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    String,
) {
    let fe_name = slot_display_name(WorkerSlot::Frontend, config);
    let be_name = slot_display_name(WorkerSlot::Backend, config);
    match mode {
        MissionMode::Solo(WorkerSlot::Frontend) => (
            Some("主导 UI/交互侧推进".to_string()),
            Some("待命并准备接收后续协助".to_string()),
            Some(format!("围绕当前目标推进前端侧工作：{goal}")),
            None,
            format!("等待{fe_name}进入协作上下文，再决定是否需要服务侧协助"),
        ),
        MissionMode::Solo(WorkerSlot::Backend) => (
            Some("待命并准备接收后续协助".to_string()),
            Some("主导服务/数据侧推进".to_string()),
            None,
            Some(format!("围绕当前目标推进后端侧工作：{goal}")),
            format!("等待{be_name}进入协作上下文，再决定是否需要 UI 侧协助"),
        ),
        MissionMode::Parallel => (
            Some("负责 UI/交互侧推进".to_string()),
            Some("负责接口/数据侧推进".to_string()),
            Some(format!("拆解并推进前端侧工作：{goal}")),
            Some(format!("拆解并推进后端侧工作：{goal}")),
            "等待两个协作者都进入协作上下文，再开始首轮并行分派".to_string(),
        ),
        MissionMode::SerialHandoff => (
            Some("在契约稳定后承接 UI/交互落地".to_string()),
            Some("先稳定接口/字段/约束，再交接给前端".to_string()),
            Some(format!("等待后端先产出可消费的协作结果：{goal}")),
            Some(format!("先整理服务侧契约与约束：{goal}")),
            "先让后端明确契约，再决定前端接手时机".to_string(),
        ),
        MissionMode::Blocked => (
            None,
            None,
            None,
            None,
            "当前 mission 缺少可执行路径，等待主线程重新组织".to_string(),
        ),
    }
}

fn contains_any(goal: &str, keywords: &[&str]) -> bool {
    keywords.iter().any(|keyword| goal.contains(keyword))
}

fn plan_manual_override_mission(worker: WorkerSlot, message: &str, config: &TeamConfig) -> Mission {
    let mode = MissionMode::Solo(worker);
    let goal = format!("手动分派：{}", preview(message));
    let (frontend_role, backend_role, frontend_assignment, backend_assignment, _) =
        mission_assignments(message, &mode, config);
    let mut mission = Mission {
        goal,
        mode,
        phase: MissionPhase::Executing,
        acceptance_checks: vec![
            AcceptanceCheck {
                summary: "目标已拆成可执行的协作分工".to_string(),
                status: AcceptanceStatus::Pending,
            },
            AcceptanceCheck {
                summary: "需要参与的协作者已回流阶段结果".to_string(),
                status: AcceptanceStatus::Pending,
            },
            AcceptanceCheck {
                summary: "主线程已形成验证结论或下一轮动作".to_string(),
                status: AcceptanceStatus::Pending,
            },
        ],
        frontend_role,
        backend_role,
        frontend_assignment,
        backend_assignment,
        validation_summary: Some(
            "当前由旧命令触发手动 override；本轮结果回流后需重新判断是否继续沿 mission 主路径推进。"
                .to_string(),
        ),
        blocker: None,
        next_action: Some("等待本轮手动分派结果回流，再决定是否回到 mission 主流程。".to_string()),
        cycle: 1,
    };
    mission.assignment_mut(worker).replace(preview(message));
    mission
}

fn plan_manual_relay_mission(from: WorkerSlot, to: WorkerSlot, message: &str, config: &TeamConfig) -> Mission {
    let goal = format!("手动协作同步：{}", preview(message));
    let (frontend_role, backend_role, frontend_assignment, backend_assignment, _) =
        mission_assignments(&goal, &MissionMode::Parallel, config);
    let mut mission = Mission {
        goal,
        mode: MissionMode::Parallel,
        phase: MissionPhase::Executing,
        acceptance_checks: vec![
            AcceptanceCheck {
                summary: "目标已拆成可执行的协作分工".to_string(),
                status: AcceptanceStatus::Pending,
            },
            AcceptanceCheck {
                summary: "需要参与的协作者已回流阶段结果".to_string(),
                status: AcceptanceStatus::Pending,
            },
            AcceptanceCheck {
                summary: "主线程已形成验证结论或下一轮动作".to_string(),
                status: AcceptanceStatus::Pending,
            },
        ],
        frontend_role,
        backend_role,
        frontend_assignment,
        backend_assignment,
        validation_summary: Some(
            "当前由旧命令触发手动 override；请在 relay 结果回流后确认是否回到 mission 主路径。"
                .to_string(),
        ),
        blocker: None,
        next_action: Some(
            "等待 relay 目标协作者回流结果，再确认是否恢复 mission 主流程。".to_string(),
        ),
        cycle: 1,
    };
    mission.assignment_mut(to).replace(preview(message));
    mission.assignment_mut(from).get_or_insert_with(|| {
        format!(
            "向 {} 转发协作事实：{}",
            to.display_name(),
            preview(message)
        )
    });
    mission
}

fn plan_recovery_mission(frontend: &WorkerState, backend: &WorkerState, config: &TeamConfig) -> Mission {
    let mode = match (
        worker_has_recovery_context(frontend),
        worker_has_recovery_context(backend),
    ) {
        (true, true) => MissionMode::Parallel,
        (true, false) => MissionMode::Solo(WorkerSlot::Frontend),
        (false, true) => MissionMode::Solo(WorkerSlot::Backend),
        (false, false) => MissionMode::Blocked,
    };
    let goal = recovery_goal(frontend, backend);
    let (frontend_role, backend_role, frontend_assignment, backend_assignment, _) =
        mission_assignments(&goal, &mode, config);
    Mission {
        goal,
        mode,
        phase: MissionPhase::Bootstrapping,
        acceptance_checks: vec![
            AcceptanceCheck {
                summary: "目标已拆成可执行的协作分工".to_string(),
                status: AcceptanceStatus::Pending,
            },
            AcceptanceCheck {
                summary: "需要参与的协作者已回流阶段结果".to_string(),
                status: AcceptanceStatus::Pending,
            },
            AcceptanceCheck {
                summary: "主线程已形成验证结论或下一轮动作".to_string(),
                status: AcceptanceStatus::Pending,
            },
        ],
        frontend_role,
        backend_role,
        frontend_assignment: frontend
            .last_dispatched_task
            .as_deref()
            .map(preview)
            .or(frontend_assignment),
        backend_assignment: backend
            .last_dispatched_task
            .as_deref()
            .map(preview)
            .or(backend_assignment),
        validation_summary: None,
        blocker: None,
        next_action: None,
        cycle: 1,
    }
}

fn apply_recovery_state(mission: &mut Mission, frontend: &WorkerState, backend: &WorkerState) {
    if let Some(task) = frontend.last_dispatched_task.as_deref() {
        mission.frontend_assignment = Some(preview(task));
    }
    if let Some(task) = backend.last_dispatched_task.as_deref() {
        mission.backend_assignment = Some(preview(task));
    }

    let required = mission.required_workers(frontend, backend);
    if required.is_empty() {
        mission.phase = MissionPhase::Blocked;
        mission.blocker = Some(
            "当前只恢复到零散历史记录，尚不足以重建可执行 mission。重新运行 `/zteam start <goal>`。"
                .to_string(),
        );
        mission.validation_summary = Some("恢复链路未能还原可执行的 mission brief。".to_string());
        mission.next_action = Some("用新的 `<goal>` 重新启动 mission。".to_string());
        sync_acceptance_checks(mission, frontend, backend);
        return;
    }

    let missing_live = required
        .iter()
        .copied()
        .filter(|worker| {
            !matches!(
                worker_state_for(*worker, frontend, backend).connection,
                WorkerConnection::Live(_)
            )
        })
        .collect::<Vec<_>>();
    if !missing_live.is_empty() {
        mission.phase = MissionPhase::Blocked;
        mission.blocker = Some(format!(
            "当前只恢复到历史上下文；{} 尚未 live 附着，需 `/zteam attach` 或重新运行 `/zteam start <goal>` 明确继续策略。",
            worker_list(&missing_live)
        ));
        mission.validation_summary = Some(
            "Mission Board 当前展示的是恢复态上下文，不保证与上次 brief 完全一致。".to_string(),
        );
        mission.next_action =
            Some("先恢复需要参与的 worker，或用新的 `<goal>` 重启 mission。".to_string());
        sync_acceptance_checks(mission, frontend, backend);
        return;
    }

    mission.blocker = None;
    if required_workers_have_results(mission, frontend, backend) {
        mission.phase = MissionPhase::Validating;
        mission.validation_summary =
            Some("已恢复最近一次协作结果；请先确认这些结果仍对应当前目标。".to_string());
        mission.next_action =
            Some("确认恢复结果是否仍有效，再决定继续分派还是重启 mission。".to_string());
    } else {
        mission.phase = MissionPhase::Planning;
        mission.validation_summary =
            Some("已恢复最近一次协作上下文；当前 brief 来自历史任务摘要。".to_string());
        mission.next_action = Some("先确认恢复的任务边界，再继续分派或重启 mission。".to_string());
    }
    sync_acceptance_checks(mission, frontend, backend);
}

fn worker_has_recovery_context(worker: &WorkerState) -> bool {
    worker.connection.known_thread_id().is_some()
        || worker.last_dispatched_task.is_some()
        || worker.last_result.is_some()
}

fn recovery_goal(frontend: &WorkerState, backend: &WorkerState) -> String {
    let frontend_task = frontend.last_dispatched_task.as_deref().map(preview);
    let backend_task = backend.last_dispatched_task.as_deref().map(preview);
    match (frontend_task, backend_task) {
        (Some(frontend_task), Some(backend_task)) => {
            format!("恢复最近一次 ZTeam 协作：{frontend_task} / {backend_task}")
        }
        (Some(frontend_task), None) => format!("恢复最近一次 ZTeam 协作：{frontend_task}"),
        (None, Some(backend_task)) => format!("恢复最近一次 ZTeam 协作：{backend_task}"),
        (None, None) => "恢复最近一次 ZTeam 协作上下文".to_string(),
    }
}

fn sync_mission_phase(mission: &mut Mission, frontend: &WorkerState, backend: &WorkerState) {
    if matches!(
        mission.phase,
        MissionPhase::Blocked | MissionPhase::Validating | MissionPhase::Completed
    ) {
        sync_acceptance_checks(mission, frontend, backend);
        return;
    }
    let required = mission.required_workers(frontend, backend);
    if required.is_empty() {
        mission.phase = MissionPhase::Blocked;
        mission.blocker = Some("当前 mission 没有可执行 worker。".to_string());
        mission.next_action = Some("重新整理目标或重建协作上下文".to_string());
        sync_acceptance_checks(mission, frontend, backend);
        return;
    }
    let all_live = required.iter().all(|worker| {
        matches!(
            worker_state_for(*worker, frontend, backend).connection,
            WorkerConnection::Live(_)
        )
    });
    if all_live {
        mission.phase = MissionPhase::Planning;
        mission.next_action = Some("按当前 mission 分工开始首轮任务分派".to_string());
    } else {
        mission.phase = MissionPhase::Bootstrapping;
        mission.next_action = Some("等待需要参与的协作者进入协作上下文".to_string());
    }
    sync_acceptance_checks(mission, frontend, backend);
}

fn required_workers_have_results(
    mission: &Mission,
    frontend: &WorkerState,
    backend: &WorkerState,
) -> bool {
    mission
        .required_workers(frontend, backend)
        .iter()
        .all(|worker| {
            worker_state_for(*worker, frontend, backend)
                .last_result
                .as_deref()
                .is_some_and(|result| !result.trim().is_empty())
        })
}

fn sync_acceptance_checks(mission: &mut Mission, frontend: &WorkerState, backend: &WorkerState) {
    let has_assignments =
        mission.frontend_assignment.is_some() || mission.backend_assignment.is_some();
    let has_results = required_workers_have_results(mission, frontend, backend);
    let has_validation = mission.validation_summary.is_some() || mission.blocker.is_some();
    for (index, check) in mission.acceptance_checks.iter_mut().enumerate() {
        check.status = match index {
            0 if has_assignments => AcceptanceStatus::Met,
            0 => AcceptanceStatus::Failed,
            1 if has_results => AcceptanceStatus::Met,
            1 if mission.blocker.is_some() => AcceptanceStatus::Failed,
            1 => AcceptanceStatus::Pending,
            2 if has_validation => AcceptanceStatus::Met,
            _ => AcceptanceStatus::Pending,
        };
    }
}

fn worker_state_for<'a>(
    worker: WorkerSlot,
    frontend: &'a WorkerState,
    backend: &'a WorkerState,
) -> &'a WorkerState {
    match worker {
        WorkerSlot::Frontend => frontend,
        WorkerSlot::Backend => backend,
    }
}

fn mission_assignment_value(mission: &Mission, worker: WorkerSlot) -> Option<&str> {
    match worker {
        WorkerSlot::Frontend => mission.frontend_assignment.as_deref(),
        WorkerSlot::Backend => mission.backend_assignment.as_deref(),
    }
}

fn reset_autopilot_for_new_mission(state: &mut SharedState) {
    state.autopilot = AutopilotState {
        current_cycle: state.mission.as_ref().map_or(1, |mission| mission.cycle),
        pending_auto_action: Some(AutoAction::BootstrapWorkers),
        waiting_on: WaitingOn::RootTurn,
        last_auto_action_result: Some(
            "已触发自动 bootstrap，等待主线程创建默认 worker。".to_string(),
        ),
        ..AutopilotState::default()
    };
}

fn reset_autopilot_for_recovery(state: &mut SharedState) {
    state.autopilot = AutopilotState {
        current_cycle: state.mission.as_ref().map_or(1, |mission| mission.cycle),
        last_auto_action_result: Some(
            "已恢复 mission 上下文，准备按当前状态决定是否自动续跑。".to_string(),
        ),
        ..AutopilotState::default()
    };
}

fn infer_next_auto_action(
    mission: &Mission,
    frontend: &WorkerState,
    backend: &WorkerState,
    autopilot: &AutopilotState,
) -> Option<AutoAction> {
    if mission.phase == MissionPhase::Completed {
        return None;
    }
    let required_workers = mission.required_workers(frontend, backend);
    if required_workers.is_empty() {
        return None;
    }
    if autopilot.cycle_dispatched && required_workers_have_results(mission, frontend, backend) {
        return Some(AutoAction::SummarizeResults);
    }
    let all_live = required_workers.iter().all(|worker| {
        matches!(
            worker_state_for(*worker, frontend, backend).connection,
            WorkerConnection::Live(_)
        )
    });
    if !all_live {
        return None;
    }
    if !autopilot.cycle_planned {
        return Some(AutoAction::PlanCycle);
    }
    if !autopilot.cycle_dispatched {
        return Some(AutoAction::DispatchCycle);
    }
    None
}

fn sync_autopilot_waiting_on_from_state(
    state: &mut SharedState,
    frontend: &WorkerState,
    backend: &WorkerState,
) {
    let Some(mission) = state.mission.as_ref() else {
        state.autopilot.waiting_on = WaitingOn::Idle;
        return;
    };
    if state.autopilot.pending_auto_action.is_some() {
        state.autopilot.waiting_on = WaitingOn::RootTurn;
        return;
    }
    let repair_targets = current_repair_targets(mission, frontend, backend, &state.autopilot);
    if !repair_targets.is_empty() {
        state.autopilot.waiting_on = WaitingOn::Repair(repair_targets);
        return;
    }
    let required_workers = mission.required_workers(frontend, backend);
    if required_workers.is_empty() {
        state.autopilot.waiting_on = WaitingOn::Idle;
        return;
    }
    if state.autopilot.cycle_dispatched {
        if required_workers_have_results(mission, frontend, backend) {
            state.autopilot.waiting_on = WaitingOn::Idle;
        } else {
            state.autopilot.waiting_on = WaitingOn::Results(required_workers);
        }
        return;
    }
    let missing_live = required_workers
        .into_iter()
        .filter(|worker| {
            !matches!(
                worker_state_for(*worker, frontend, backend).connection,
                WorkerConnection::Live(_)
            )
        })
        .collect::<Vec<_>>();
    if missing_live.is_empty() {
        state.autopilot.waiting_on = WaitingOn::Idle;
    } else {
        state.autopilot.waiting_on = WaitingOn::Workers(missing_live);
    }
}

fn current_repair_targets(
    mission: &Mission,
    frontend: &WorkerState,
    backend: &WorkerState,
    autopilot: &AutopilotState,
) -> Vec<WorkerSlot> {
    let requested = match &autopilot.waiting_on {
        WaitingOn::Repair(workers) if !workers.is_empty() => workers.clone(),
        _ => mission.required_workers(frontend, backend),
    };
    requested
        .into_iter()
        .filter(|worker| {
            !matches!(
                worker_state_for(*worker, frontend, backend).connection,
                WorkerConnection::Live(_)
            )
        })
        .collect()
}

fn mark_root_action_submitted(
    state: &mut SharedState,
    action: AutoAction,
    target_workers: &[WorkerSlot],
) {
    let current_cycle = state.autopilot.current_cycle;
    let assignment_updates = if action == AutoAction::DispatchCycle {
        state
            .mission
            .as_ref()
            .map(|mission| {
                target_workers
                    .iter()
                    .filter_map(|worker| {
                        mission_assignment_value(mission, *worker)
                            .map(|assignment| (*worker, assignment.to_string()))
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    state.autopilot.pending_auto_action = Some(action);
    state.autopilot.parsed_result = None;
    state.autopilot.queued_auto_action = None;
    state.autopilot.waiting_on = WaitingOn::RootTurn;
    state.autopilot.last_auto_action_result = Some(format!("已触发自动动作：{}。", action.label()));
    for (worker, assignment) in assignment_updates {
        let worker_state = state.worker_mut(worker);
        worker_state.last_result = None;
        worker_state.last_dispatched_task = Some(assignment);
    }
    if let Some(mission) = state.mission.as_mut() {
        match action {
            AutoAction::PlanCycle => {
                mission.phase = MissionPhase::Planning;
                mission.blocker = None;
                mission.validation_summary =
                    Some(format!("autopilot 正在规划第 {current_cycle} 轮协作。"));
                mission.next_action = Some("等待主线程完成本轮自动规划。".to_string());
            }
            AutoAction::DispatchCycle => {
                mission.phase = MissionPhase::Executing;
                mission.blocker = None;
                mission.validation_summary =
                    Some(format!("autopilot 正在发起第 {current_cycle} 轮自动派工。"));
                mission.next_action = Some("等待 worker 回流本轮结果。".to_string());
            }
            AutoAction::SummarizeResults => {
                mission.phase = MissionPhase::Validating;
                mission.validation_summary =
                    Some(format!("autopilot 正在归纳第 {current_cycle} 轮结果。"));
                mission.next_action = Some("等待主线程给出继续、repair 或完成判定。".to_string());
            }
            AutoAction::RepairWorkers => {
                for worker in target_workers {
                    state.autopilot.repair_attempts.increment(*worker);
                }
                mission.phase = MissionPhase::Bootstrapping;
                mission.blocker = None;
                mission.validation_summary =
                    Some("autopilot 正在自动重建缺失 worker。".to_string());
                mission.next_action = Some("等待缺失 worker 重新注册。".to_string());
            }
            AutoAction::CompleteMission => {
                mission.phase = MissionPhase::Completed;
                mission.blocker = None;
                mission.next_action = Some("等待主线程输出最终收口摘要。".to_string());
            }
            AutoAction::BootstrapWorkers => {}
        }
    }
}

fn mark_autopilot_blocked(state: &mut SharedState, summary: String) {
    state.autopilot.pending_auto_action = None;
    state.autopilot.queued_auto_action = None;
    state.autopilot.waiting_on = WaitingOn::Idle;
    state.autopilot.last_auto_action_result = Some(summary.clone());
    if let Some(mission) = state.mission.as_mut() {
        mission.phase = MissionPhase::Blocked;
        mission.blocker = Some(summary.clone());
        mission.validation_summary = Some(summary);
        mission.next_action = Some("等待人工决定后续动作。".to_string());
    }
}

fn finalize_root_auto_action(state: &mut SharedState) -> bool {
    let Some(action) = state.autopilot.pending_auto_action.take() else {
        return false;
    };
    let frontend = state.frontend.clone();
    let backend = state.backend.clone();
    let Some(_) = state.mission.clone() else {
        state.autopilot.waiting_on = WaitingOn::Idle;
        return true;
    };
    let parsed = state.autopilot.parsed_result.take();
    match action {
        AutoAction::BootstrapWorkers => {
            state.autopilot.last_auto_action_result = Some(
                parsed
                    .as_ref()
                    .map(|result| result.summary.clone())
                    .unwrap_or_else(|| "主线程已完成默认 worker bootstrap。".to_string()),
            );
            sync_autopilot_waiting_on_from_state(state, &frontend, &backend);
        }
        AutoAction::PlanCycle => {
            state.autopilot.cycle_planned = true;
            state.autopilot.cycle_dispatched = false;
            state.autopilot.queued_auto_action = Some(AutoAction::DispatchCycle);
            state.autopilot.last_auto_action_result = Some(
                parsed
                    .as_ref()
                    .map(|result| result.summary.clone())
                    .unwrap_or_else(|| {
                        format!("第 {} 轮自动规划已完成。", state.autopilot.current_cycle)
                    }),
            );
            if let Some(mission_state) = state.mission.as_mut() {
                mission_state.phase = MissionPhase::Planning;
                mission_state.validation_summary = state.autopilot.last_auto_action_result.clone();
                mission_state.next_action = Some("准备自动派发当前 cycle。".to_string());
            }
            state.autopilot.waiting_on = WaitingOn::Idle;
        }
        AutoAction::DispatchCycle => {
            state.autopilot.cycle_dispatched = true;
            state.autopilot.last_auto_action_result = Some(
                parsed
                    .as_ref()
                    .map(|result| result.summary.clone())
                    .unwrap_or_else(|| {
                        format!("第 {} 轮自动派工已发出。", state.autopilot.current_cycle)
                    }),
            );
            if let Some(mission_state) = state.mission.as_mut() {
                mission_state.phase = MissionPhase::Executing;
                mission_state.validation_summary = state.autopilot.last_auto_action_result.clone();
                mission_state.next_action =
                    Some("等待需要参与的 worker 回流阶段结果。".to_string());
            }
            sync_autopilot_waiting_on_from_state(state, &frontend, &backend);
        }
        AutoAction::SummarizeResults => {
            let Some(result) = parsed else {
                mark_autopilot_blocked(
                    state,
                    "自动归纳结果缺少可解析的 autopilot 标记，已进入 blocked。".to_string(),
                );
                return true;
            };
            apply_summary_result(state, result);
        }
        AutoAction::RepairWorkers => {
            state.autopilot.attach_attempted = false;
            state.autopilot.last_auto_action_result = Some(
                parsed
                    .as_ref()
                    .map(|result| result.summary.clone())
                    .unwrap_or_else(|| {
                        "自动 repair 指令已发出，等待缺失 worker 重新注册。".to_string()
                    }),
            );
            sync_autopilot_waiting_on_from_state(state, &frontend, &backend);
            if let Some(mission_state) = state.mission.as_mut() {
                mission_state.phase = MissionPhase::Bootstrapping;
                mission_state.validation_summary = state.autopilot.last_auto_action_result.clone();
                mission_state.next_action = Some("等待缺失 worker 重新注册。".to_string());
            }
        }
        AutoAction::CompleteMission => {
            state.autopilot.waiting_on = WaitingOn::Idle;
            state.autopilot.manual_override_active = false;
            state.autopilot.cycle_planned = false;
            state.autopilot.cycle_dispatched = false;
            state.autopilot.last_auto_action_result = Some(
                parsed
                    .as_ref()
                    .map(|result| result.summary.clone())
                    .unwrap_or_else(|| "mission 已自动收口。".to_string()),
            );
            if let Some(mission_state) = state.mission.as_mut() {
                mission_state.phase = MissionPhase::Completed;
                mission_state.blocker = None;
                mission_state.validation_summary = state.autopilot.last_auto_action_result.clone();
                mission_state.next_action = Some("mission 已完成。".to_string());
                sync_acceptance_checks(mission_state, &frontend, &backend);
            }
        }
    }
    true
}

fn apply_summary_result(state: &mut SharedState, result: autopilot::ParsedAutopilotResult) {
    state.autopilot.last_auto_action_result = Some(result.summary.clone());
    match result.status.as_str() {
        "continue" => {
            state.autopilot.manual_override_active = false;
            state.autopilot.current_cycle = result.cycle.max(state.autopilot.current_cycle + 1);
            state.autopilot.cycle_planned = false;
            state.autopilot.cycle_dispatched = false;
            state.autopilot.attach_attempted = false;
            state.autopilot.waiting_on = WaitingOn::Idle;
            state.autopilot.queued_auto_action = Some(AutoAction::PlanCycle);
            if let Some(mission) = state.mission.as_mut() {
                mission.phase = MissionPhase::Planning;
                mission.blocker = None;
                mission.cycle = state.autopilot.current_cycle;
                mission.validation_summary = Some(result.summary);
                mission.next_action = Some(format!(
                    "准备自动规划第 {} 轮协作。",
                    state.autopilot.current_cycle
                ));
            }
        }
        "complete" => {
            state.autopilot.manual_override_active = false;
            state.autopilot.waiting_on = WaitingOn::Idle;
            state.autopilot.queued_auto_action = Some(AutoAction::CompleteMission);
            if let Some(mission) = state.mission.as_mut() {
                mission.phase = MissionPhase::Validating;
                mission.blocker = None;
                mission.validation_summary = Some(result.summary);
                mission.next_action = Some("准备自动收口当前 mission。".to_string());
            }
        }
        "repair" => {
            state.autopilot.manual_override_active = false;
            state.autopilot.attach_attempted = false;
            let frontend = state.frontend.clone();
            let backend = state.backend.clone();
            let targets = if result.waiting_on.is_empty() {
                state
                    .mission
                    .as_ref()
                    .map(|mission| mission.required_workers(&frontend, &backend))
                    .filter(|workers| !workers.is_empty())
                    .unwrap_or_else(|| WorkerSlot::ALL.to_vec())
            } else {
                result.waiting_on
            };
            state.autopilot.waiting_on = WaitingOn::Repair(targets.clone());
            if let Some(mission) = state.mission.as_mut() {
                mission.phase = MissionPhase::Blocked;
                mission.blocker = Some(format!("主线程要求先 repair：{}", worker_list(&targets)));
                mission.validation_summary = Some(result.summary);
                mission.next_action = Some(format!(
                    "先恢复 {}，再继续当前 mission。",
                    worker_list(&targets)
                ));
            }
        }
        "blocked" => {
            mark_autopilot_blocked(state, result.summary);
        }
        _ => {
            mark_autopilot_blocked(
                state,
                format!("自动归纳结果返回了未知状态 `{}`。", result.status),
            );
        }
    }
}

#[cfg(test)]
fn worker_status_line(worker: WorkerSlot, state: &WorkerState) -> String {
    let state_text = match &state.connection {
        WorkerConnection::Pending => "未注册".to_string(),
        WorkerConnection::Live(thread_id) => format!("已附着 ({thread_id})"),
        WorkerConnection::ReattachRequired(thread_id) => {
            format!("待再附着 ({thread_id})")
        }
    };
    let task_text = state
        .last_dispatched_task
        .as_deref()
        .map(preview)
        .unwrap_or_else(|| "无".to_string());
    let result_text = state
        .last_result
        .as_deref()
        .map(preview)
        .unwrap_or_else(|| "无".to_string());
    format!(
        "- {}：{}；来源：{}；最近任务：{}；最近结果：{}",
        worker.display_name(),
        state_text,
        state.source.label(),
        task_text,
        result_text
    )
}

fn preview(text: &str) -> String {
    const LIMIT: usize = 60;
    let trimmed = text.trim();
    if trimmed.chars().count() <= LIMIT {
        return trimmed.to_string();
    }
    let truncated: String = trimmed.chars().take(LIMIT).collect();
    format!("{truncated}...")
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_app_server_protocol::ItemCompletedNotification;
    use codex_app_server_protocol::ItemStartedNotification;
    use codex_app_server_protocol::ServerNotification;
    use codex_app_server_protocol::SessionSource;
    use codex_app_server_protocol::ThreadClosedNotification;
    use codex_app_server_protocol::ThreadStartedNotification;
    use codex_app_server_protocol::ThreadStatus;

    use codex_protocol::models::MessagePhase;
    use codex_protocol::protocol::SubAgentSource;
    use codex_utils_absolute_path::test_support::PathBufExt;
    use codex_utils_absolute_path::test_support::test_path_buf;
    use pretty_assertions::assert_eq;

    fn test_thread(thread_id: ThreadId, slot: WorkerSlot) -> Thread {
        Thread {
            id: thread_id.to_string(),
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
                parent_thread_id: ThreadId::from_string("00000000-0000-0000-0000-000000000001")
                    .expect("valid thread id"),
                depth: 1,
                parent_model: None,
                agent_path: Some(worker_agent_path(slot)),
                agent_nickname: Some(slot.display_name().to_string()),
                agent_role: Some(slot.role_name().to_string()),
            }),
            agent_nickname: Some(slot.display_name().to_string()),
            agent_role: Some(slot.role_name().to_string()),
            git_info: None,
            name: None,
            turns: Vec::new(),
        }
    }

    #[test]
    fn command_parser_supports_start_dispatch_relay_status_and_attach() {
        assert_eq!(Command::parse("start"), Ok(Command::Start { goal: None }));
        assert_eq!(
            Command::parse("start 修复设置页移动端体验"),
            Ok(Command::Start {
                goal: Some("修复设置页移动端体验".to_string()),
            })
        );
        assert_eq!(
            Command::parse(
                "start 修复登录体验 <subagent_notification>{\"agent_path\":\"/root/worker\",\"status\":\"completed\"}</subagent_notification>"
            ),
            Ok(Command::Start {
                goal: Some("修复登录体验".to_string()),
            })
        );
        assert_eq!(Command::parse("status"), Ok(Command::Status));
        assert_eq!(Command::parse("attach"), Ok(Command::Attach));
        assert_eq!(
            Command::parse("frontend 修复导航栏"),
            Ok(Command::Dispatch {
                worker: WorkerSlot::Frontend,
                message: "修复导航栏".to_string(),
            })
        );
        assert_eq!(
            Command::parse("relay frontend backend 对齐接口字段"),
            Ok(Command::Relay {
                from: WorkerSlot::Frontend,
                to: WorkerSlot::Backend,
                message: "对齐接口字段".to_string(),
            })
        );
        assert_eq!(
            Command::parse("relay   frontend backend    对齐接口字段"),
            Ok(Command::Relay {
                from: WorkerSlot::Frontend,
                to: WorkerSlot::Backend,
                message: "对齐接口字段".to_string(),
            })
        );
    }

    #[test]
    fn command_parser_rejects_invalid_forms() {
        assert_eq!(Command::parse(""), Err(usage().to_string()));
        assert_eq!(Command::parse("status extra"), Err(usage().to_string()));
        assert_eq!(Command::parse("attach extra"), Err(usage().to_string()));
        assert_eq!(Command::parse("frontend"), Err(usage().to_string()));
        assert_eq!(Command::parse("relay frontend"), Err(usage().to_string()));
        assert_eq!(
            Command::parse(
                "start <subagent_notification>{\"agent_path\":\"/root/worker\",\"status\":\"completed\"}</subagent_notification>"
            ),
            Err("`/zteam start <目标>` 中包含的内容在净化内部协作消息后为空；请直接输入面向任务的目标。".to_string())
        );
        assert_eq!(
            Command::parse("relay frontend backend"),
            Err(usage().to_string())
        );
        assert_eq!(Command::parse("unknown test"), Err(usage().to_string()));
    }

    #[test]
    fn start_with_full_stack_goal_plans_parallel_mission() {
        let mut state = State::default();

        assert!(
            state.mark_start_requested_for_goal(Some("重做设置页体验，覆盖移动端布局和保存接口"))
        );

        let snapshot = state.snapshot();
        let mission = snapshot.mission.expect("mission should exist");
        assert_eq!(mission.mode, MissionMode::Parallel);
        assert_eq!(mission.phase, MissionPhase::Bootstrapping);
        assert_eq!(mission.frontend_role.as_deref(), Some("负责 UI/交互侧推进"));
        assert_eq!(mission.backend_role.as_deref(), Some("负责接口/数据侧推进"));
        assert_eq!(mission.acceptance_checks.len(), 3);
    }

    #[test]
    fn start_requested_goal_is_sanitized_before_mission_creation() {
        let mut state = State::default();

        assert!(state.mark_start_requested_for_goal(Some(
            "修复设置页保存链路 <subagent_notification>{\"agent_path\":\"/root/worker\",\"status\":\"completed\"}</subagent_notification>"
        )));

        let snapshot = state.snapshot();
        let mission = snapshot.mission.expect("mission should exist");
        assert_eq!(mission.goal, "修复设置页保存链路");
    }

    #[test]
    fn start_with_backend_goal_prefers_backend_solo() {
        let mut state = State::default();

        assert!(state.mark_start_requested_for_goal(Some("排查登录接口在 token 过期时返回 500")));

        let snapshot = state.snapshot();
        let mission = snapshot.mission.expect("mission should exist");
        assert_eq!(mission.mode, MissionMode::Solo(WorkerSlot::Backend));
        assert_eq!(
            mission.backend_assignment.as_deref(),
            Some("围绕当前目标推进后端侧工作：排查登录接口在 token 过期时返回 500")
        );
        assert_eq!(mission.frontend_assignment, None);
    }

    #[test]
    fn start_with_handoff_goal_prefers_serial_handoff() {
        let mut state = State::default();

        assert!(
            state.mark_start_requested_for_goal(Some(
                "先稳定资料编辑接口字段，再交给前端完成表单联调"
            ))
        );

        let snapshot = state.snapshot();
        let mission = snapshot.mission.expect("mission should exist");
        assert_eq!(mission.mode, MissionMode::SerialHandoff);
        assert_eq!(
            mission.backend_assignment.as_deref(),
            Some("先整理服务侧契约与约束：先稳定资料编辑接口字段，再交给前端完成表单联调")
        );
    }

    #[test]
    fn vague_goal_starts_blocked_mission() {
        let mut state = State::default();

        assert!(state.mark_start_requested_for_goal(Some("先看看")));

        let snapshot = state.snapshot();
        let mission = snapshot.mission.expect("mission should exist");
        assert_eq!(mission.mode, MissionMode::Blocked);
        assert_eq!(mission.phase, MissionPhase::Blocked);
        assert_eq!(
            mission.blocker.as_deref(),
            Some("目标过于模糊，暂时无法规划可执行的协作路径。")
        );
    }

    #[test]
    fn worker_registration_moves_required_mission_to_planning() {
        let backend_id =
            ThreadId::from_string("00000000-0000-0000-0000-000000000020").expect("valid thread");
        let mut state = State::default();
        state.mark_start_requested_for_goal(Some("排查登录接口在 token 过期时返回 500"));

        let notification = ServerNotification::ThreadStarted(ThreadStartedNotification {
            thread: test_thread(backend_id, WorkerSlot::Backend),
        });
        assert!(state.observe_notification(
            backend_id,
            /*is_primary_thread*/ false,
            &notification,
        ));

        let snapshot = state.snapshot();
        let mission = snapshot.mission.expect("mission should exist");
        assert_eq!(mission.phase, MissionPhase::Planning);
        assert_eq!(
            mission.next_action.as_deref(),
            Some("按当前 mission 分工开始首轮任务分派")
        );
    }

    #[test]
    fn serial_handoff_backend_result_switches_next_required_worker() {
        let backend_id =
            ThreadId::from_string("00000000-0000-0000-0000-000000000020").expect("valid thread");
        let mut state = State::default();
        state.mark_start_requested_for_goal(Some("先稳定资料编辑接口字段，再交给前端完成表单联调"));

        let backend_started = ServerNotification::ThreadStarted(ThreadStartedNotification {
            thread: test_thread(backend_id, WorkerSlot::Backend),
        });
        assert!(state.observe_notification(
            backend_id,
            /*is_primary_thread*/ false,
            &backend_started,
        ));
        state.record_dispatch(WorkerSlot::Backend, "先产出接口契约");

        let backend_done = ServerNotification::ItemCompleted(ItemCompletedNotification {
            item: ThreadItem::AgentMessage {
                id: "msg-handoff-backend".to_string(),
                text: "后端阶段结果已回流".to_string(),
                phase: Some(MessagePhase::FinalAnswer),
                memory_citation: None,
            },
            thread_id: backend_id.to_string(),
            turn_id: "turn-handoff-backend".to_string(),
        });
        assert!(state.observe_notification(
            backend_id,
            /*is_primary_thread*/ false,
            &backend_done,
        ));

        let snapshot = state.snapshot();
        let mission = snapshot.mission.expect("mission should exist");
        assert_eq!(mission.phase, MissionPhase::Bootstrapping);
        assert_eq!(
            mission.next_action.as_deref(),
            Some("后端阶段结果已就绪，可分派前端接手实现与联调")
        );
    }

    #[test]
    fn completed_results_move_mission_to_validating() {
        let frontend_id =
            ThreadId::from_string("00000000-0000-0000-0000-000000000010").expect("valid thread");
        let backend_id =
            ThreadId::from_string("00000000-0000-0000-0000-000000000020").expect("valid thread");
        let mut state = State::default();
        state.mark_start_requested_for_goal(Some("重做设置页体验，覆盖移动端布局和保存接口"));

        let frontend_started = ServerNotification::ThreadStarted(ThreadStartedNotification {
            thread: test_thread(frontend_id, WorkerSlot::Frontend),
        });
        let backend_started = ServerNotification::ThreadStarted(ThreadStartedNotification {
            thread: test_thread(backend_id, WorkerSlot::Backend),
        });
        assert!(state.observe_notification(
            frontend_id,
            /*is_primary_thread*/ false,
            &frontend_started,
        ));
        assert!(state.observe_notification(
            backend_id,
            /*is_primary_thread*/ false,
            &backend_started,
        ));
        state.record_dispatch(WorkerSlot::Frontend, "推进前端任务");
        state.record_dispatch(WorkerSlot::Backend, "推进后端任务");

        let frontend_done = ServerNotification::ItemCompleted(ItemCompletedNotification {
            item: ThreadItem::AgentMessage {
                id: "msg-frontend-done".to_string(),
                text: "前端阶段结果已回流".to_string(),
                phase: Some(MessagePhase::FinalAnswer),
                memory_citation: None,
            },
            thread_id: frontend_id.to_string(),
            turn_id: "turn-frontend-done".to_string(),
        });
        let backend_done = ServerNotification::ItemCompleted(ItemCompletedNotification {
            item: ThreadItem::AgentMessage {
                id: "msg-backend-done".to_string(),
                text: "后端阶段结果已回流".to_string(),
                phase: Some(MessagePhase::FinalAnswer),
                memory_citation: None,
            },
            thread_id: backend_id.to_string(),
            turn_id: "turn-backend-done".to_string(),
        });
        assert!(state.observe_notification(
            frontend_id,
            /*is_primary_thread*/ false,
            &frontend_done,
        ));
        assert!(state.observe_notification(
            backend_id,
            /*is_primary_thread*/ false,
            &backend_done,
        ));

        let snapshot = state.snapshot();
        let mission = snapshot.mission.expect("mission should exist");
        assert_eq!(mission.phase, MissionPhase::Validating);
        assert_eq!(
            mission.validation_summary.as_deref(),
            Some("已收到当前需要参与的协作者阶段结果，等待主线程归纳验证结论。")
        );
    }

    #[test]
    fn state_registers_workers_and_records_results() {
        let frontend_id =
            ThreadId::from_string("00000000-0000-0000-0000-000000000010").expect("valid thread");
        let backend_id =
            ThreadId::from_string("00000000-0000-0000-0000-000000000020").expect("valid thread");
        let mut state = State::default();

        state.observe_notification(
            frontend_id,
            /*is_primary_thread*/ false,
            &ServerNotification::ThreadStarted(ThreadStartedNotification {
                thread: test_thread(frontend_id, WorkerSlot::Frontend),
            }),
        );
        state.observe_notification(
            backend_id,
            /*is_primary_thread*/ false,
            &ServerNotification::ThreadStarted(ThreadStartedNotification {
                thread: test_thread(backend_id, WorkerSlot::Backend),
            }),
        );
        state.record_dispatch(WorkerSlot::Frontend, "完成协作工作台布局");
        state.record_dispatch(WorkerSlot::Backend, "整理 API 契约");

        state.observe_notification(
            frontend_id,
            /*is_primary_thread*/ false,
            &ServerNotification::ItemCompleted(ItemCompletedNotification {
                item: ThreadItem::AgentMessage {
                    id: "msg-1".to_string(),
                    text: "前端阶段结果：工作台布局已完成。".to_string(),
                    phase: Some(MessagePhase::FinalAnswer),
                    memory_citation: None,
                },
                thread_id: frontend_id.to_string(),
                turn_id: "turn-1".to_string(),
            }),
        );
        state.observe_notification(
            backend_id,
            /*is_primary_thread*/ false,
            &ServerNotification::ItemCompleted(ItemCompletedNotification {
                item: ThreadItem::AgentMessage {
                    id: "msg-2".to_string(),
                    text: "后端阶段结果：接口契约已整理。".to_string(),
                    phase: Some(MessagePhase::FinalAnswer),
                    memory_citation: None,
                },
                thread_id: backend_id.to_string(),
                turn_id: "turn-2".to_string(),
            }),
        );

        assert_eq!(
            state.worker_thread_id(WorkerSlot::Frontend),
            Some(frontend_id)
        );
        assert_eq!(
            state.worker_thread_id(WorkerSlot::Backend),
            Some(backend_id)
        );
        let status = state.status_message();
        assert!(status.contains("工作台布局已完成"));
        assert!(status.contains("接口契约已整理"));
    }

    #[test]
    fn relay_requires_both_workers_registered() {
        let frontend_id =
            ThreadId::from_string("00000000-0000-0000-0000-000000000010").expect("valid thread");
        let mut state = State::default();
        state.observe_notification(
            frontend_id,
            /*is_primary_thread*/ false,
            &ServerNotification::ThreadStarted(ThreadStartedNotification {
                thread: test_thread(frontend_id, WorkerSlot::Frontend),
            }),
        );

        assert_eq!(
            state.build_worker_relay(
                WorkerSlot::Frontend,
                WorkerSlot::Backend,
                "同步接口".to_string()
            ),
            None
        );
    }

    #[test]
    fn closing_worker_marks_thread_for_reattach() {
        let frontend_id =
            ThreadId::from_string("00000000-0000-0000-0000-000000000010").expect("valid thread");
        let mut state = State::default();
        state.observe_notification(
            frontend_id,
            /*is_primary_thread*/ false,
            &ServerNotification::ThreadStarted(ThreadStartedNotification {
                thread: test_thread(frontend_id, WorkerSlot::Frontend),
            }),
        );

        state.observe_notification(
            frontend_id,
            /*is_primary_thread*/ false,
            &ServerNotification::ThreadClosed(ThreadClosedNotification {
                thread_id: frontend_id.to_string(),
            }),
        );

        assert_eq!(state.worker_thread_id(WorkerSlot::Frontend), None);
        assert!(state.status_message().contains("待再附着"));
        assert!(
            state
                .missing_worker_message(WorkerSlot::Frontend)
                .contains("/zteam attach")
        );
    }

    #[test]
    fn commentary_messages_do_not_replace_worker_result() {
        let frontend_id =
            ThreadId::from_string("00000000-0000-0000-0000-000000000010").expect("valid thread");
        let mut state = State::default();
        state.observe_notification(
            frontend_id,
            /*is_primary_thread*/ false,
            &ServerNotification::ThreadStarted(ThreadStartedNotification {
                thread: test_thread(frontend_id, WorkerSlot::Frontend),
            }),
        );
        state.observe_notification(
            frontend_id,
            /*is_primary_thread*/ false,
            &ServerNotification::ItemCompleted(ItemCompletedNotification {
                item: ThreadItem::AgentMessage {
                    id: "msg-1".to_string(),
                    text: "处理中".to_string(),
                    phase: Some(MessagePhase::Commentary),
                    memory_citation: None,
                },
                thread_id: frontend_id.to_string(),
                turn_id: "turn-1".to_string(),
            }),
        );

        assert!(state.status_message().contains("最近结果：无"));
    }

    #[test]
    fn root_dispatch_uses_root_and_worker_paths() {
        let backend_id =
            ThreadId::from_string("00000000-0000-0000-0000-000000000020").expect("valid thread");
        let mut state = State::default();
        state.observe_notification(
            backend_id,
            /*is_primary_thread*/ false,
            &ServerNotification::ThreadStarted(ThreadStartedNotification {
                thread: test_thread(backend_id, WorkerSlot::Backend),
            }),
        );

        let (thread_id, communication) = state
            .build_root_dispatch(WorkerSlot::Backend, "整理接口".to_string())
            .expect("backend dispatch should be available");

        assert_eq!(thread_id, backend_id);
        assert_eq!(communication.author.to_string(), "/root");
        assert_eq!(communication.recipient.to_string(), "/root/backend");
        assert_eq!(communication.content, "整理接口");
        assert!(communication.trigger_turn);
    }

    #[test]
    fn ignore_non_agent_message_item_completion() {
        let backend_id =
            ThreadId::from_string("00000000-0000-0000-0000-000000000020").expect("valid thread");
        let mut state = State::default();
        state.observe_notification(
            backend_id,
            /*is_primary_thread*/ false,
            &ServerNotification::ThreadStarted(ThreadStartedNotification {
                thread: test_thread(backend_id, WorkerSlot::Backend),
            }),
        );
        state.observe_notification(
            backend_id,
            /*is_primary_thread*/ false,
            &ServerNotification::ItemStarted(ItemStartedNotification {
                item: ThreadItem::UserMessage {
                    id: "user-1".to_string(),
                    content: Vec::new(),
                },
                thread_id: backend_id.to_string(),
                turn_id: "turn-1".to_string(),
            }),
        );

        assert!(state.status_message().contains("最近结果：无"));
    }

    #[test]
    fn restore_worker_records_reattach_required_state() {
        let frontend_id =
            ThreadId::from_string("00000000-0000-0000-0000-000000000010").expect("valid thread");
        let mut state = State::default();

        let changed = state.restore_worker(RecoveredWorker {
            slot: WorkerSlot::Frontend,
            connection: WorkerConnection::ReattachRequired(frontend_id),
            source: WorkerSource::LocalThreadSpawn,
            last_dispatched_task: Some("修复导航栏布局".to_string()),
            last_result: Some("已补上移动端断点。".to_string()),
        });

        assert!(changed);
        let status = state.status_message();
        assert!(status.contains("待再附着"));
        assert!(status.contains("修复导航栏布局"));
        assert!(status.contains("已补上移动端断点"));
        assert!(
            state
                .missing_worker_message(WorkerSlot::Frontend)
                .contains("/zteam attach")
        );
    }

    #[test]
    fn restore_worker_without_existing_mission_creates_recovery_mission() {
        let frontend_id =
            ThreadId::from_string("00000000-0000-0000-0000-000000000010").expect("valid thread");
        let mut state = State::default();

        assert!(state.restore_worker(RecoveredWorker {
            slot: WorkerSlot::Frontend,
            connection: WorkerConnection::ReattachRequired(frontend_id),
            source: WorkerSource::LocalThreadSpawn,
            last_dispatched_task: Some("修复导航栏布局".to_string()),
            last_result: Some("已补上移动端断点。".to_string()),
        }));

        let snapshot = state.snapshot();
        let mission = snapshot.mission.expect("mission should exist");
        assert_eq!(mission.mode, MissionMode::Solo(WorkerSlot::Frontend));
        assert_eq!(mission.phase, MissionPhase::Blocked);
        assert_eq!(
            mission.frontend_assignment.as_deref(),
            Some("修复导航栏布局")
        );
        assert!(
            mission
                .blocker
                .as_deref()
                .is_some_and(|blocker| blocker.contains("/zteam attach"))
        );
        assert!(
            mission
                .validation_summary
                .as_deref()
                .is_some_and(|summary| {
                    summary.contains("恢复态上下文") || summary.contains("历史")
                })
        );
    }

    #[test]
    fn manual_dispatch_without_goal_creates_override_mission() {
        let backend_id =
            ThreadId::from_string("00000000-0000-0000-0000-000000000020").expect("valid thread");
        let mut state = State::default();
        state.observe_notification(
            backend_id,
            /*is_primary_thread*/ false,
            &ServerNotification::ThreadStarted(ThreadStartedNotification {
                thread: test_thread(backend_id, WorkerSlot::Backend),
            }),
        );

        assert!(state.record_dispatch(
            WorkerSlot::Backend,
            "排查登录接口为什么在 token 过期时返回 500"
        ));

        let snapshot = state.snapshot();
        let mission = snapshot.mission.expect("mission should exist");
        assert_eq!(mission.mode, MissionMode::Solo(WorkerSlot::Backend));
        assert_eq!(mission.phase, MissionPhase::Executing);
        assert_eq!(
            mission.backend_assignment.as_deref(),
            Some("排查登录接口为什么在 token 过期时返回 500")
        );
        assert!(
            mission
                .validation_summary
                .as_deref()
                .is_some_and(|summary| summary.contains("手动 override"))
        );
        assert!(
            mission
                .next_action
                .as_deref()
                .is_some_and(|next_action| next_action.contains("mission 主流程"))
        );
    }

    #[test]
    fn autopilot_bootstrap_finishes_with_plan_then_dispatch() {
        let backend_id =
            ThreadId::from_string("00000000-0000-0000-0000-000000000020").expect("valid thread");
        let mut state = State::default();

        assert!(state.mark_start_requested_for_goal(Some("排查登录接口在 token 过期时返回 500")));
        {
            let mut guard = state.write_state();
            assert!(finalize_root_auto_action(&mut guard));
        }
        state.observe_notification(
            backend_id,
            /*is_primary_thread*/ false,
            &ServerNotification::ThreadStarted(ThreadStartedNotification {
                thread: test_thread(backend_id, WorkerSlot::Backend),
            }),
        );

        let Some(AutopilotWorkItem::RootPrompt { action, .. }) = state.take_autopilot_work_item()
        else {
            panic!("expected plan_cycle autopilot prompt");
        };
        assert_eq!(action, AutoAction::PlanCycle);
        {
            let mut guard = state.write_state();
            guard.autopilot.parsed_result = Some(autopilot::ParsedAutopilotResult {
                action: AutoAction::PlanCycle,
                status: "planned".to_string(),
                cycle: 1,
                waiting_on: vec![WorkerSlot::Backend],
                summary: "后端 solo 方案已规划".to_string(),
            });
            assert!(finalize_root_auto_action(&mut guard));
        }

        let Some(AutopilotWorkItem::RootPrompt { action, .. }) = state.take_autopilot_work_item()
        else {
            panic!("expected dispatch_cycle autopilot prompt");
        };
        assert_eq!(action, AutoAction::DispatchCycle);
    }

    #[test]
    fn autopilot_summarize_continue_advances_cycle() {
        let frontend_id =
            ThreadId::from_string("00000000-0000-0000-0000-000000000010").expect("valid thread");
        let backend_id =
            ThreadId::from_string("00000000-0000-0000-0000-000000000020").expect("valid thread");
        let mut state = State::default();

        assert!(
            state.mark_start_requested_for_goal(Some("重做设置页体验，覆盖移动端布局和保存接口"))
        );
        {
            let mut guard = state.write_state();
            assert!(finalize_root_auto_action(&mut guard));
        }
        state.observe_notification(
            frontend_id,
            /*is_primary_thread*/ false,
            &ServerNotification::ThreadStarted(ThreadStartedNotification {
                thread: test_thread(frontend_id, WorkerSlot::Frontend),
            }),
        );
        state.observe_notification(
            backend_id,
            /*is_primary_thread*/ false,
            &ServerNotification::ThreadStarted(ThreadStartedNotification {
                thread: test_thread(backend_id, WorkerSlot::Backend),
            }),
        );

        let Some(AutopilotWorkItem::RootPrompt { action, .. }) = state.take_autopilot_work_item()
        else {
            panic!("expected plan_cycle prompt");
        };
        assert_eq!(action, AutoAction::PlanCycle);
        {
            let mut guard = state.write_state();
            guard.autopilot.parsed_result = Some(autopilot::ParsedAutopilotResult {
                action: AutoAction::PlanCycle,
                status: "planned".to_string(),
                cycle: 1,
                waiting_on: vec![WorkerSlot::Frontend, WorkerSlot::Backend],
                summary: "第 1 轮并行计划已确认".to_string(),
            });
            assert!(finalize_root_auto_action(&mut guard));
        }

        let Some(AutopilotWorkItem::RootPrompt { action, .. }) = state.take_autopilot_work_item()
        else {
            panic!("expected dispatch_cycle prompt");
        };
        assert_eq!(action, AutoAction::DispatchCycle);
        {
            let mut guard = state.write_state();
            guard.autopilot.parsed_result = Some(autopilot::ParsedAutopilotResult {
                action: AutoAction::DispatchCycle,
                status: "scheduled".to_string(),
                cycle: 1,
                waiting_on: vec![WorkerSlot::Frontend, WorkerSlot::Backend],
                summary: "第 1 轮并行派工已发出".to_string(),
            });
            assert!(finalize_root_auto_action(&mut guard));
        }

        state.observe_notification(
            frontend_id,
            /*is_primary_thread*/ false,
            &ServerNotification::ItemCompleted(ItemCompletedNotification {
                item: ThreadItem::AgentMessage {
                    id: "msg-front".to_string(),
                    text: "前端阶段结果已回流".to_string(),
                    phase: Some(MessagePhase::FinalAnswer),
                    memory_citation: None,
                },
                thread_id: frontend_id.to_string(),
                turn_id: "turn-front".to_string(),
            }),
        );
        state.observe_notification(
            backend_id,
            /*is_primary_thread*/ false,
            &ServerNotification::ItemCompleted(ItemCompletedNotification {
                item: ThreadItem::AgentMessage {
                    id: "msg-back".to_string(),
                    text: "后端阶段结果已回流".to_string(),
                    phase: Some(MessagePhase::FinalAnswer),
                    memory_citation: None,
                },
                thread_id: backend_id.to_string(),
                turn_id: "turn-back".to_string(),
            }),
        );

        let Some(AutopilotWorkItem::RootPrompt { action, .. }) = state.take_autopilot_work_item()
        else {
            panic!("expected summarize_results prompt");
        };
        assert_eq!(action, AutoAction::SummarizeResults);
        {
            let mut guard = state.write_state();
            guard.autopilot.parsed_result = Some(autopilot::ParsedAutopilotResult {
                action: AutoAction::SummarizeResults,
                status: "continue".to_string(),
                cycle: 2,
                waiting_on: vec![WorkerSlot::Frontend, WorkerSlot::Backend],
                summary: "需要进入第 2 轮补齐联调和收口".to_string(),
            });
            assert!(finalize_root_auto_action(&mut guard));
        }

        let snapshot = state.snapshot();
        assert_eq!(snapshot.autopilot.current_cycle, 2);
        assert_eq!(
            snapshot.mission.as_ref().map(|mission| mission.cycle),
            Some(2)
        );
        let Some(AutopilotWorkItem::RootPrompt { action, .. }) = state.take_autopilot_work_item()
        else {
            panic!("expected next cycle plan prompt");
        };
        assert_eq!(action, AutoAction::PlanCycle);
    }

    #[test]
    fn autopilot_repair_prefers_attach_first() {
        let backend_id =
            ThreadId::from_string("00000000-0000-0000-0000-000000000020").expect("valid thread");
        let mut state = State::default();

        assert!(state.mark_start_requested_for_goal(Some("排查登录接口在 token 过期时返回 500")));
        {
            let mut guard = state.write_state();
            assert!(finalize_root_auto_action(&mut guard));
        }
        state.observe_notification(
            backend_id,
            /*is_primary_thread*/ false,
            &ServerNotification::ThreadStarted(ThreadStartedNotification {
                thread: test_thread(backend_id, WorkerSlot::Backend),
            }),
        );
        state.observe_notification(
            backend_id,
            /*is_primary_thread*/ false,
            &ServerNotification::ThreadClosed(ThreadClosedNotification {
                thread_id: backend_id.to_string(),
            }),
        );

        let Some(AutopilotWorkItem::AttachFirstRepair(workers)) = state.take_autopilot_work_item()
        else {
            panic!("expected attach-first repair work item");
        };
        assert_eq!(workers, vec![WorkerSlot::Backend]);
    }

    #[test]
    fn attach_first_repair_replans_when_cycle_was_not_planned_yet() {
        let backend_id =
            ThreadId::from_string("00000000-0000-0000-0000-000000000020").expect("valid thread");
        let mut state = State::default();

        assert!(state.mark_start_requested_for_goal(Some("排查登录接口在 token 过期时返回 500")));
        {
            let mut guard = state.write_state();
            assert!(finalize_root_auto_action(&mut guard));
        }
        state.observe_notification(
            backend_id,
            /*is_primary_thread*/ false,
            &ServerNotification::ThreadStarted(ThreadStartedNotification {
                thread: test_thread(backend_id, WorkerSlot::Backend),
            }),
        );
        state.observe_notification(
            backend_id,
            /*is_primary_thread*/ false,
            &ServerNotification::ThreadClosed(ThreadClosedNotification {
                thread_id: backend_id.to_string(),
            }),
        );

        let Some(AutopilotWorkItem::AttachFirstRepair(_)) = state.take_autopilot_work_item() else {
            panic!("expected attach-first repair work item");
        };
        state.observe_notification(
            backend_id,
            /*is_primary_thread*/ false,
            &ServerNotification::ThreadStarted(ThreadStartedNotification {
                thread: test_thread(backend_id, WorkerSlot::Backend),
            }),
        );
        assert!(state.finish_attach_first_repair());

        let Some(AutopilotWorkItem::RootPrompt { action, .. }) = state.take_autopilot_work_item()
        else {
            panic!("expected plan_cycle after attach-first repair");
        };
        assert_eq!(action, AutoAction::PlanCycle);
    }

    #[test]
    fn autopilot_ignores_non_primary_root_events() {
        let backend_id =
            ThreadId::from_string("00000000-0000-0000-0000-000000000020").expect("valid thread");
        let primary_thread_id =
            ThreadId::from_string("00000000-0000-0000-0000-000000000001").expect("valid thread");
        let unrelated_thread_id =
            ThreadId::from_string("00000000-0000-0000-0000-000000000099").expect("valid thread");
        let mut state = State::default();

        assert!(state.mark_start_requested_for_goal(Some("排查登录接口在 token 过期时返回 500")));
        {
            let mut guard = state.write_state();
            assert!(finalize_root_auto_action(&mut guard));
        }
        state.observe_notification(
            backend_id,
            /*is_primary_thread*/ false,
            &ServerNotification::ThreadStarted(ThreadStartedNotification {
                thread: test_thread(backend_id, WorkerSlot::Backend),
            }),
        );

        let Some(AutopilotWorkItem::RootPrompt { action, .. }) = state.take_autopilot_work_item()
        else {
            panic!("expected plan_cycle autopilot prompt");
        };
        assert_eq!(action, AutoAction::PlanCycle);

        let marker_message = format!(
            "{}|action=plan_cycle|status=planned|cycle=1|waiting_on=backend|summary=忽略这条非主线程结果",
            autopilot::AUTOPILOT_RESULT_MARKER
        );
        let marker_item = ThreadItem::AgentMessage {
            id: "msg-root".to_string(),
            text: marker_message,
            phase: Some(MessagePhase::FinalAnswer),
            memory_citation: None,
        };

        assert!(!state.observe_completed_item(
            unrelated_thread_id,
            /*is_primary_thread*/ false,
            &marker_item,
        ));
        assert!(!state.observe_turn_completed(unrelated_thread_id, /*is_primary_thread*/ false,));
        assert!(state.take_autopilot_work_item().is_none());
        assert_eq!(
            state.read_state().autopilot.pending_auto_action,
            Some(AutoAction::PlanCycle)
        );

        assert!(state.observe_completed_item(
            primary_thread_id,
            /*is_primary_thread*/ true,
            &marker_item,
        ));
        assert!(state.observe_turn_completed(primary_thread_id, /*is_primary_thread*/ true));

        let Some(AutopilotWorkItem::RootPrompt { action, .. }) = state.take_autopilot_work_item()
        else {
            panic!("expected dispatch_cycle after primary root completion");
        };
        assert_eq!(action, AutoAction::DispatchCycle);
    }

    #[test]
    fn configure_federation_adapter_surfaces_summary() {
        let mut state = State::default();

        let changed = state.configure_federation_adapter(Some(FederationThreadStartParams {
            instance_id: None,
            name: "zteam".to_string(),
            role: Some("worker".to_string()),
            scope: Some("workspace".to_string()),
            state_root: Some("/tmp/federation".to_string()),
            lease_ttl_secs: Some(30),
        }));

        assert!(changed);
        let status = state.status_message();
        assert!(status.contains("外部 adapter"));
        assert!(status.contains("zteam-frontend"));
        assert!(status.contains("zteam-backend"));
    }

    #[test]
    fn entry_is_available_during_task_only_for_status_views() {
        assert!(entry_available_during_task(None));
        assert!(entry_available_during_task(Some("")));
        assert!(entry_available_during_task(Some("status")));
        assert!(entry_available_during_task(Some("STATUS")));
        assert!(!entry_available_during_task(Some("start")));
        assert!(!entry_available_during_task(Some("frontend 修复布局")));
    }

    #[test]
    fn mark_start_requested_resets_existing_worker_bindings() {
        let frontend_id =
            ThreadId::from_string("00000000-0000-0000-0000-000000000010").expect("valid thread");
        let mut state = State::default();
        state.observe_notification(
            frontend_id,
            /*is_primary_thread*/ false,
            &ServerNotification::ThreadStarted(ThreadStartedNotification {
                thread: test_thread(frontend_id, WorkerSlot::Frontend),
            }),
        );
        state.record_dispatch(WorkerSlot::Frontend, "修复导航栏布局");
        state.observe_notification(
            frontend_id,
            /*is_primary_thread*/ false,
            &ServerNotification::ItemCompleted(ItemCompletedNotification {
                item: ThreadItem::AgentMessage {
                    id: "msg-1".to_string(),
                    text: "前端阶段结果：已完成。".to_string(),
                    phase: Some(MessagePhase::FinalAnswer),
                    memory_citation: None,
                },
                thread_id: frontend_id.to_string(),
                turn_id: "turn-1".to_string(),
            }),
        );

        assert!(state.mark_start_requested_for_goal(None));
        assert_eq!(state.worker_thread_id(WorkerSlot::Frontend), None);
        let status = state.status_message();
        assert!(status.contains("协作者 A：未注册"));
        assert!(status.contains("最近任务：无"));
        assert!(status.contains("最近结果：无"));
    }
}
