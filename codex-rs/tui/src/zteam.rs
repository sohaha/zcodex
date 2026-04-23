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

mod recovery;
mod view;
mod worker_source;

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
            Self::Frontend => "Android 前端",
            Self::Backend => "后端",
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Command {
    Start,
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
                if parts.next().is_some() {
                    return Err(usage().to_string());
                }
                Ok(Self::Start)
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
    activity: VecDeque<ActivityEntry>,
    recent_results: VecDeque<ResultEntry>,
    federation_adapter: Option<FederationAdapter>,
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
struct Snapshot {
    start_requested: bool,
    frontend: WorkerState,
    backend: WorkerState,
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
}

impl State {
    pub(crate) fn mark_start_requested(&mut self) -> bool {
        let mut state = self.write_state();
        let changed = !state.start_requested
            || WorkerSlot::ALL.into_iter().any(|worker| {
                let worker_state = state.worker(worker);
                !matches!(worker_state.connection, WorkerConnection::Pending)
                    || worker_state.last_dispatched_task.is_some()
                    || worker_state.last_result.is_some()
            });
        if !changed {
            return false;
        }
        state.start_requested = true;
        for worker in WorkerSlot::ALL {
            *state.worker_mut(worker) = WorkerState::default();
        }
        push_activity(
            &mut state.activity,
            format!(
                "主线程已请求创建 {} worker。等待 spawn 事件注册。",
                default_worker_task_list()
            ),
        );
        true
    }

    pub(crate) fn worker_thread_id(&self, worker: WorkerSlot) -> Option<ThreadId> {
        self.read_state().worker(worker).connection.live_thread_id()
    }

    pub(crate) fn restore_worker(&mut self, recovered: RecoveredWorker) -> bool {
        let mut state = self.write_state();
        state.start_requested = true;
        let worker_state = state.worker_mut(recovered.slot);
        let next_state = WorkerState {
            connection: recovered.connection.clone(),
            source: recovered.source,
            last_dispatched_task: recovered.last_dispatched_task,
            last_result: recovered.last_result,
        };
        if *worker_state == next_state {
            return false;
        }
        *worker_state = next_state;
        let restore_summary = match &worker_state.connection {
            WorkerConnection::Pending => {
                format!(
                    "已恢复 {recovered_slot} worker 的最近协作记录，等待注册。",
                    recovered_slot = recovered.slot
                )
            }
            WorkerConnection::Live(thread_id) => format!(
                "已恢复 {worker} worker，并重新附着到 {thread_id}。",
                worker = recovered.slot
            ),
            WorkerConnection::ReattachRequired(thread_id) => format!(
                "已恢复 {worker} worker 的最近状态；线程 {thread_id} 当前未附着，可运行 `/zteam attach` 再附着。",
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

    pub(crate) fn record_dispatch(&mut self, worker: WorkerSlot, message: &str) -> bool {
        let mut state = self.write_state();
        let worker_state = state.worker_mut(worker);
        if let Some(thread_id) = worker_state.connection.known_thread_id() {
            worker_state.connection = WorkerConnection::Live(thread_id);
        }
        worker_state.last_dispatched_task = Some(message.to_string());
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
        push_activity(
            &mut state.activity,
            format!("{from} -> {to}：{}", preview(message)),
        );
        true
    }

    pub(crate) fn observe_notification(
        &mut self,
        thread_id: ThreadId,
        notification: &ServerNotification,
    ) -> bool {
        match notification {
            ServerNotification::ThreadStarted(notification) => {
                self.observe_thread_started(thread_id, &notification.thread)
            }
            ServerNotification::ItemCompleted(notification) => {
                self.observe_completed_item(thread_id, &notification.item)
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
        let state = self.read_state();
        let mut lines = Vec::new();
        lines.push(format!("{MODE_NAME} 状态："));
        if state.start_requested {
            lines.push(format!(
                "已请求创建 {} worker。",
                default_worker_task_list()
            ));
        } else {
            lines.push("尚未请求创建 worker；先运行 `/zteam start`。".to_string());
        }
        if let Some(adapter) = &state.federation_adapter {
            lines.push(format!("外部 adapter：{}", adapter.summary()));
        }
        for worker in WorkerSlot::ALL {
            lines.push(worker_status_line(worker, state.worker(worker)));
        }
        lines.join("\n")
    }

    pub(crate) fn missing_worker_message(&self, worker: WorkerSlot) -> String {
        match &self.read_state().worker(worker).connection {
            WorkerConnection::Pending => format!(
                "{} worker 尚未注册。先运行 `/zteam start`，并等待主线程创建 `{}`。",
                worker.display_name(),
                worker.canonical_task_name()
            ),
            WorkerConnection::Live(_) => format!(
                "{} worker 当前已附着；如果仍然分派失败，请重新运行 `/zteam status` 检查状态。",
                worker.display_name()
            ),
            WorkerConnection::ReattachRequired(thread_id) => format!(
                "{} worker 最近的线程 `{thread_id}` 当前未附着。先运行 `/zteam attach` 尝试再附着，或用 `/zteam start` 重建 worker。",
                worker.display_name()
            ),
        }
    }

    pub(crate) fn missing_relay_message(&self, from: WorkerSlot, to: WorkerSlot) -> String {
        let state = self.read_state();
        let from_connection = &state.worker(from).connection;
        let to_connection = &state.worker(to).connection;
        let needs_attach = matches!(from_connection, WorkerConnection::ReattachRequired(_))
            || matches!(to_connection, WorkerConnection::ReattachRequired(_));
        if needs_attach {
            return format!(
                "无法在 {from} 和 {to} 之间中转消息。先运行 `/zteam attach` 重新附着最近的 worker，或用 `/zteam start` 重新创建。"
            );
        }
        format!(
            "无法在 {from} 和 {to} 之间中转消息。先运行 `/zteam start`，并确认两个 worker 都已注册。"
        )
    }

    fn snapshot(&self) -> Snapshot {
        let state = self.read_state();
        Snapshot {
            start_requested: state.start_requested,
            frontend: state.frontend.clone(),
            backend: state.backend.clone(),
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
        let worker_state = state.worker_mut(worker);
        if worker_state.connection == WorkerConnection::Live(thread_id) {
            return false;
        }
        worker_state.connection = WorkerConnection::Live(thread_id);
        worker_state.source = WorkerSource::LocalThreadSpawn;
        push_activity(
            &mut state.activity,
            format!(
                "{worker} worker 已注册到 `{}`。",
                worker.canonical_task_name()
            ),
        );
        true
    }

    fn observe_completed_item(&mut self, thread_id: ThreadId, item: &ThreadItem) -> bool {
        let worker = {
            let state = self.read_state();
            let Some(worker) = state.worker_slot_for_thread(thread_id) else {
                return false;
            };
            worker
        };
        let ThreadItem::AgentMessage { text, phase, .. } = item else {
            return false;
        };
        if text.trim().is_empty() || *phase == Some(MessagePhase::Commentary) {
            return false;
        }
        let mut state = self.write_state();
        let worker_state = state.worker_mut(worker);
        worker_state.last_result = Some(text.clone());
        worker_state.source = WorkerSource::LocalThreadSpawn;
        push_result(&mut state.recent_results, worker, preview(text));
        true
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
        push_activity(
            &mut state.activity,
            format!(
                "{worker} worker 已关闭，可运行 `/zteam attach` 再附着或 `/zteam start` 重建。"
            ),
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
    "用法：/zteam start | /zteam status | /zteam attach | /zteam <frontend|backend> <任务> | /zteam relay <frontend|backend> <frontend|backend> <消息>"
}

pub(crate) fn start_prompt() -> String {
    concat!(
        "进入 ZTeam 本地协作模式。立即使用 `spawn_agent` 创建两个长期 worker：\n",
        "1. `task_name = \"frontend\"`，`agent_type = \"frontend-engineer\"`\n",
        "2. `task_name = \"backend\"`，`agent_type = \"backend-engineer\"`\n",
        "对两个 worker 都说明：它们是长期协作者，主线程负责拆分任务；需要彼此同步时优先使用 `send_message` 或 `followup_task`；完成阶段结果后继续待命，不要自行关闭。\n",
        "创建完成后，只用一条简短中文消息汇报两个 worker 的 canonical task name。除非我下一条消息明确分派任务，否则不要开始实现业务工作。"
    )
    .to_string()
}

fn default_worker_task_list() -> String {
    WorkerSlot::ALL
        .into_iter()
        .map(WorkerSlot::task_name)
        .collect::<Vec<_>>()
        .join("/")
}

fn worker_agent_path(worker: WorkerSlot) -> AgentPath {
    match AgentPath::root().resolve(worker.task_name()) {
        Ok(path) => path,
        Err(err) => unreachable!("zteam worker task name should always resolve: {err}"),
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
        assert_eq!(Command::parse("start"), Ok(Command::Start));
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
        assert_eq!(Command::parse("start extra"), Err(usage().to_string()));
        assert_eq!(Command::parse("status extra"), Err(usage().to_string()));
        assert_eq!(Command::parse("attach extra"), Err(usage().to_string()));
        assert_eq!(Command::parse("frontend"), Err(usage().to_string()));
        assert_eq!(Command::parse("relay frontend"), Err(usage().to_string()));
        assert_eq!(
            Command::parse("relay frontend backend"),
            Err(usage().to_string())
        );
        assert_eq!(Command::parse("unknown test"), Err(usage().to_string()));
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
            &ServerNotification::ThreadStarted(ThreadStartedNotification {
                thread: test_thread(frontend_id, WorkerSlot::Frontend),
            }),
        );
        state.observe_notification(
            backend_id,
            &ServerNotification::ThreadStarted(ThreadStartedNotification {
                thread: test_thread(backend_id, WorkerSlot::Backend),
            }),
        );
        state.record_dispatch(WorkerSlot::Frontend, "完成协作工作台布局");
        state.record_dispatch(WorkerSlot::Backend, "整理 API 契约");

        state.observe_notification(
            frontend_id,
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
            &ServerNotification::ThreadStarted(ThreadStartedNotification {
                thread: test_thread(frontend_id, WorkerSlot::Frontend),
            }),
        );

        state.observe_notification(
            frontend_id,
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
            &ServerNotification::ThreadStarted(ThreadStartedNotification {
                thread: test_thread(frontend_id, WorkerSlot::Frontend),
            }),
        );
        state.observe_notification(
            frontend_id,
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
            &ServerNotification::ThreadStarted(ThreadStartedNotification {
                thread: test_thread(backend_id, WorkerSlot::Backend),
            }),
        );
        state.observe_notification(
            backend_id,
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
            &ServerNotification::ThreadStarted(ThreadStartedNotification {
                thread: test_thread(frontend_id, WorkerSlot::Frontend),
            }),
        );
        state.record_dispatch(WorkerSlot::Frontend, "修复导航栏布局");
        state.observe_notification(
            frontend_id,
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

        assert!(state.mark_start_requested());
        assert_eq!(state.worker_thread_id(WorkerSlot::Frontend), None);
        let status = state.status_message();
        assert!(status.contains("Android 前端：未注册"));
        assert!(status.contains("最近任务：无"));
        assert!(status.contains("最近结果：无"));
    }
}
