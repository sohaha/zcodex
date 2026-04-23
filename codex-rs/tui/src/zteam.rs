use codex_app_server_protocol::ServerNotification;
use codex_app_server_protocol::SessionSource;
use codex_app_server_protocol::Thread;
use codex_app_server_protocol::ThreadItem;
use codex_protocol::AgentPath;
use codex_protocol::ThreadId;
use codex_protocol::models::MessagePhase;
use codex_protocol::protocol::InterAgentCommunication;
use codex_protocol::protocol::SubAgentSource;
use std::fmt;

pub(crate) const MODE_NAME: &str = "ZTeam";
pub(crate) const COMMAND_NAME: &str = "/zteam";
const FRONTEND_TASK_NAME: &str = "frontend";
const BACKEND_TASK_NAME: &str = "backend";
const FRONTEND_ROLE: &str = "frontend-engineer";
const BACKEND_ROLE: &str = "backend-engineer";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
            Self::Frontend => "前端",
            Self::Backend => "后端",
        }
    }

    fn canonical_task_name(self) -> String {
        format!("/root/{}", self.task_name())
    }

    fn matches_thread(self, thread: &Thread) -> bool {
        match &thread.source {
            SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
                agent_path: Some(agent_path),
                ..
            }) if agent_path.to_string() == self.canonical_task_name() => true,
            SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
                agent_role: Some(agent_role),
                ..
            }) if agent_role == self.role_name() => true,
            _ => false,
        }
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
            "start" => Ok(Self::Start),
            "status" => Ok(Self::Status),
            "relay" => {
                let Some(from_raw) = parts.next() else {
                    return Err(usage().to_string());
                };
                let Some(to_raw) = parts.next() else {
                    return Err(usage().to_string());
                };
                let Some(message) = trimmed
                    .splitn(4, char::is_whitespace)
                    .nth(3)
                    .map(str::trim)
                    .filter(|message| !message.is_empty())
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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct State {
    start_requested: bool,
    frontend: WorkerState,
    backend: WorkerState,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct WorkerState {
    thread_id: Option<ThreadId>,
    last_dispatched_task: Option<String>,
    last_result: Option<String>,
    closed: bool,
}

impl State {
    pub(crate) fn mark_start_requested(&mut self) {
        self.start_requested = true;
    }

    pub(crate) fn worker_thread_id(&self, worker: WorkerSlot) -> Option<ThreadId> {
        self.worker(worker).thread_id
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

    pub(crate) fn record_dispatch(&mut self, worker: WorkerSlot, message: &str) {
        let worker = self.worker_mut(worker);
        worker.closed = false;
        worker.last_dispatched_task = Some(message.to_string());
    }

    pub(crate) fn observe_notification(
        &mut self,
        thread_id: ThreadId,
        notification: &ServerNotification,
    ) {
        match notification {
            ServerNotification::ThreadStarted(notification) => {
                self.observe_thread_started(thread_id, &notification.thread);
            }
            ServerNotification::ItemCompleted(notification) => {
                self.observe_completed_item(thread_id, &notification.item);
            }
            ServerNotification::ThreadClosed(notification) => {
                if notification.thread_id == thread_id.to_string() {
                    self.observe_thread_closed(thread_id);
                }
            }
            _ => {}
        }
    }

    pub(crate) fn status_message(&self) -> String {
        let mut lines = Vec::new();
        lines.push(format!("{MODE_NAME} 状态："));
        if self.start_requested {
            lines.push("已请求创建 frontend/backend worker。".to_string());
        } else {
            lines.push("尚未请求创建 worker；先运行 `/zteam start`。".to_string());
        }
        lines.push(worker_status_line(WorkerSlot::Frontend, &self.frontend));
        lines.push(worker_status_line(WorkerSlot::Backend, &self.backend));
        lines.join("\n")
    }

    pub(crate) fn missing_worker_message(&self, worker: WorkerSlot) -> String {
        format!(
            "{} worker 尚未注册。先运行 `/zteam start`，并等待主线程创建 `{}`。",
            worker.display_name(),
            worker.canonical_task_name()
        )
    }

    pub(crate) fn missing_relay_message(&self, from: WorkerSlot, to: WorkerSlot) -> String {
        format!(
            "无法在 {from} 和 {to} 之间中转消息。先运行 `/zteam start`，并确认两个 worker 都已注册。"
        )
    }

    fn observe_thread_started(&mut self, thread_id: ThreadId, thread: &Thread) {
        if let Some(worker) = WorkerSlot::ALL
            .into_iter()
            .find(|worker| worker.matches_thread(thread))
        {
            let worker_state = self.worker_mut(worker);
            worker_state.thread_id = Some(thread_id);
            worker_state.closed = false;
        }
    }

    fn observe_completed_item(&mut self, thread_id: ThreadId, item: &ThreadItem) {
        let Some(worker) = self.worker_slot_for_thread(thread_id) else {
            return;
        };
        let ThreadItem::AgentMessage { text, phase, .. } = item else {
            return;
        };
        if text.trim().is_empty() || *phase == Some(MessagePhase::Commentary) {
            return;
        }
        let worker_state = self.worker_mut(worker);
        worker_state.last_result = Some(text.clone());
        worker_state.closed = false;
    }

    fn observe_thread_closed(&mut self, thread_id: ThreadId) {
        let Some(worker) = self.worker_slot_for_thread(thread_id) else {
            return;
        };
        let worker_state = self.worker_mut(worker);
        worker_state.thread_id = None;
        worker_state.closed = true;
    }

    fn worker_slot_for_thread(&self, thread_id: ThreadId) -> Option<WorkerSlot> {
        if self.frontend.thread_id == Some(thread_id) {
            return Some(WorkerSlot::Frontend);
        }
        if self.backend.thread_id == Some(thread_id) {
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

impl WorkerSlot {
    const ALL: [Self; 2] = [Self::Frontend, Self::Backend];
}

pub(crate) fn entry_message() -> String {
    format!(
        "{MODE_NAME} 入口已启用。先用 `/zteam start` 创建 frontend/backend worker，再用 `/zteam frontend <任务>` 或 `/zteam backend <任务>` 分派任务。"
    )
}

pub(crate) fn entry_hint() -> &'static str {
    "可用 `/zteam status` 查看当前状态，或用 `/zteam relay frontend backend <消息>` 在 worker 间转发中途消息。"
}

pub(crate) fn disabled_message() -> String {
    format!("{MODE_NAME} 已在当前 TUI 配置中关闭，{COMMAND_NAME} 不再可用。")
}

pub(crate) fn disabled_hint() -> &'static str {
    "在 `config.toml` 中设置 `[tui].zteam_enabled = true` 后可再次启用。"
}

pub(crate) fn usage() -> &'static str {
    "用法：/zteam start | /zteam status | /zteam frontend <任务> | /zteam backend <任务> | /zteam relay <frontend|backend> <frontend|backend> <消息>"
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

fn worker_agent_path(worker: WorkerSlot) -> AgentPath {
    match AgentPath::root().resolve(worker.task_name()) {
        Ok(path) => path,
        Err(err) => unreachable!("zteam worker task name should always resolve: {err}"),
    }
}

fn worker_status_line(worker: WorkerSlot, state: &WorkerState) -> String {
    let state_text = match (state.thread_id, state.closed) {
        (Some(thread_id), _) => format!("已注册 ({thread_id})"),
        (None, true) => "已关闭".to_string(),
        (None, false) => "未注册".to_string(),
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
        "- {}：{}；最近任务：{}；最近结果：{}",
        worker.display_name(),
        state_text,
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
    use codex_app_server_protocol::ThreadClosedNotification;
    use codex_app_server_protocol::ThreadStartedNotification;
    use codex_app_server_protocol::ThreadStatus;

    use codex_protocol::models::MessagePhase;
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
    fn command_parser_supports_start_dispatch_relay_and_status() {
        assert_eq!(Command::parse("start"), Ok(Command::Start));
        assert_eq!(Command::parse("status"), Ok(Command::Status));
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
    }

    #[test]
    fn command_parser_rejects_invalid_forms() {
        assert_eq!(Command::parse(""), Err(usage().to_string()));
        assert_eq!(Command::parse("frontend"), Err(usage().to_string()));
        assert_eq!(Command::parse("relay frontend"), Err(usage().to_string()));
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
    fn closing_worker_clears_dispatch_target() {
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
        assert!(state.status_message().contains("已关闭"));
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
}
