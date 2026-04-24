use super::MODE_NAME;
use super::Snapshot;
use super::State;
use super::WorkerSlot;
use crate::bottom_pane::BottomPaneView;
use crate::bottom_pane::CancellationEvent;
use crate::bottom_pane::ViewCompletion;
use crate::history_cell::card_inner_width;
use crate::history_cell::with_border_with_inner_width;
use crate::render::renderable::Renderable;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use ratatui::buffer::Buffer;
use ratatui::layout::Constraint;
use ratatui::layout::Layout;
use ratatui::layout::Rect;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::text::Text;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Widget;
use ratatui::widgets::Wrap;
use textwrap::Options;
use textwrap::wrap;

pub(crate) const WORKBENCH_VIEW_ID: &str = "zteam-workbench";
const MAX_CARD_INNER_WIDTH: usize = 96;

pub(crate) struct WorkbenchView {
    state: State,
    completion: Option<ViewCompletion>,
}

impl WorkbenchView {
    pub(crate) fn new(state: State) -> Self {
        Self {
            state,
            completion: None,
        }
    }

    fn body_lines(&self, width: u16) -> Vec<Line<'static>> {
        let inner_width = card_inner_width(width, MAX_CARD_INNER_WIDTH).unwrap_or(1);
        let snapshot = self.state.snapshot();
        let mut lines = Vec::new();
        lines.push(section_header("Mission"));
        lines.extend(mission_lines(&snapshot, inner_width));
        lines.push(Line::from(""));

        lines.push(section_header("Acceptance"));
        lines.extend(acceptance_lines(&snapshot, inner_width));
        lines.push(Line::from(""));

        lines.push(section_header("Worker Assignments"));
        lines.extend(worker_assignment_lines(&snapshot, inner_width));
        lines.push(Line::from(""));

        lines.push(section_header("Validation"));
        lines.extend(validation_lines(&snapshot, inner_width));
        lines.push(Line::from(""));

        lines.push(section_header("Activity"));
        lines.extend(activity_lines(&snapshot, inner_width));

        with_border_with_inner_width(lines, inner_width)
    }
}

impl BottomPaneView for WorkbenchView {
    fn handle_key_event(&mut self, key_event: KeyEvent) {
        if matches!(key_event.code, KeyCode::Esc | KeyCode::Enter) {
            self.completion = Some(ViewCompletion::Cancelled);
        }
    }

    fn is_complete(&self) -> bool {
        self.completion.is_some()
    }

    fn completion(&self) -> Option<ViewCompletion> {
        self.completion
    }

    fn on_ctrl_c(&mut self) -> CancellationEvent {
        self.completion = Some(ViewCompletion::Cancelled);
        CancellationEvent::Handled
    }

    fn view_id(&self) -> Option<&'static str> {
        Some(WORKBENCH_VIEW_ID)
    }
}

impl Renderable for WorkbenchView {
    fn desired_height(&self, width: u16) -> u16 {
        if width == 0 {
            return 0;
        }
        let hint_height = 1u16;
        let body_height = Paragraph::new(Text::from(self.body_lines(width)))
            .wrap(Wrap { trim: false })
            .line_count(width)
            .try_into()
            .unwrap_or(u16::MAX);
        1u16.saturating_add(body_height).saturating_add(hint_height)
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let hint_height = if area.height > 1 { 1 } else { 0 };
        let title_height = 1;
        let body_height = area
            .height
            .saturating_sub(title_height)
            .saturating_sub(hint_height);
        let [title_area, body_area, hint_area] = Layout::vertical([
            Constraint::Length(title_height),
            Constraint::Length(body_height),
            Constraint::Length(hint_height),
        ])
        .areas(area);

        Paragraph::new(Line::from(format!("{MODE_NAME} Mission Board").bold()))
            .render(title_area, buf);
        if body_area.height > 0 {
            Paragraph::new(Text::from(self.body_lines(body_area.width)))
                .wrap(Wrap { trim: false })
                .render(body_area, buf);
        }
        if hint_area.height > 0 {
            Paragraph::new(Line::from(vec![
                "Esc 关闭".dim(),
                " · ".dim(),
                "/zteam start <goal>".cyan(),
                " 启动".dim(),
                " · ".dim(),
                "/zteam attach".cyan(),
                " 恢复".dim(),
                " · ".dim(),
                "/zteam relay".cyan(),
                " override".dim(),
            ]))
            .render(hint_area, buf);
        }
    }
}

fn section_header(title: &str) -> Line<'static> {
    Line::from(title.to_string().bold())
}

fn mission_lines(snapshot: &Snapshot, inner_width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    match &snapshot.mission {
        Some(mission) => {
            push_wrapped(
                &mut lines,
                inner_width,
                format!("目标：{}", mission.goal),
                "  ",
            );
            push_wrapped(
                &mut lines,
                inner_width,
                format!(
                    "模式：{} · 阶段：{} · Cycle：{}",
                    mission_mode_label(&mission.mode),
                    mission_phase_label(mission.phase),
                    mission.cycle
                ),
                "  ",
            );
            push_wrapped(
                &mut lines,
                inner_width,
                format!("整体状态：{}", overview_status(snapshot)),
                "  ",
            );
            if let Some(adapter) = &snapshot.federation_adapter {
                push_wrapped(
                    &mut lines,
                    inner_width,
                    format!("外部 adapter：{}", adapter.summary()),
                    "  ",
                );
            }
        }
        None if snapshot.start_requested => {
            push_wrapped(
                &mut lines,
                inner_width,
                "当前处于兼容启动模式，尚未生成 mission brief。重新运行 `/zteam start <目标>` 可切换到目标驱动协作。".to_string(),
                "  ",
            );
            push_wrapped(
                &mut lines,
                inner_width,
                format!("整体状态：{}", overview_status(snapshot)),
                "  ",
            );
        }
        None => {
            push_wrapped(
                &mut lines,
                inner_width,
                "尚未启动 mission。先运行 `/zteam start <目标>` 创建目标驱动协作。".to_string(),
                "  ",
            );
            if let Some(blocked) = blocking_note(snapshot) {
                push_wrapped(&mut lines, inner_width, format!("提示：{blocked}"), "  ");
            }
        }
    }
    lines
}

fn acceptance_lines(snapshot: &Snapshot, inner_width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let Some(mission) = &snapshot.mission else {
        push_wrapped(&mut lines, inner_width, "• 暂无验收项。".to_string(), "  ");
        return lines;
    };
    for check in &mission.acceptance_checks {
        push_wrapped(
            &mut lines,
            inner_width,
            format!(
                "• [{}] {}",
                acceptance_status_label(check.status),
                check.summary
            ),
            "  ",
        );
    }
    lines
}

fn worker_assignment_lines(snapshot: &Snapshot, inner_width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for worker in WorkerSlot::ALL {
        let worker_state = snapshot.worker(worker);
        let registration = worker_registration_text(worker_state, worker);
        push_wrapped(
            &mut lines,
            inner_width,
            format!("{worker}：{registration}"),
            "  ",
        );
        push_wrapped(
            &mut lines,
            inner_width,
            format!("  来源：{}", worker_state.source.label()),
            "    ",
        );
        push_wrapped(
            &mut lines,
            inner_width,
            format!(
                "  当前职责：{}",
                mission_role(snapshot.mission.as_ref(), worker).unwrap_or("等待 mission 规划")
            ),
            "    ",
        );
        push_wrapped(
            &mut lines,
            inner_width,
            format!(
                "  当前分派：{}",
                mission_assignment(snapshot.mission.as_ref(), worker)
                    .or(worker_state.last_dispatched_task.as_deref())
                    .map(super::preview)
                    .unwrap_or_else(|| "等待任务".to_string())
            ),
            "    ",
        );
        push_wrapped(
            &mut lines,
            inner_width,
            format!(
                "  最近结果：{}",
                worker_state
                    .last_result
                    .as_deref()
                    .map(super::preview)
                    .unwrap_or_else(|| "无".to_string())
            ),
            "    ",
        );
    }
    lines
}

fn validation_lines(snapshot: &Snapshot, inner_width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    match &snapshot.mission {
        Some(mission) => {
            push_wrapped(
                &mut lines,
                inner_width,
                format!(
                    "验证概览：{}",
                    mission
                        .validation_summary
                        .as_deref()
                        .unwrap_or("尚未形成验证结论。")
                ),
                "  ",
            );
            if let Some(blocker) = &mission.blocker {
                push_wrapped(&mut lines, inner_width, format!("阻塞：{blocker}"), "  ");
            } else if let Some(blocked) = blocking_note(snapshot) {
                push_wrapped(&mut lines, inner_width, format!("阻塞：{blocked}"), "  ");
            }
            push_wrapped(
                &mut lines,
                inner_width,
                format!(
                    "下一步：{}",
                    mission
                        .next_action
                        .as_deref()
                        .unwrap_or("等待主线程决定下一轮动作。")
                ),
                "  ",
            );
        }
        None => {
            push_wrapped(
                &mut lines,
                inner_width,
                "验证概览：当前还没有 mission，暂无验证结论。".to_string(),
                "  ",
            );
            if let Some(blocked) = blocking_note(snapshot) {
                push_wrapped(&mut lines, inner_width, format!("阻塞：{blocked}"), "  ");
            }
        }
    }

    if snapshot.recent_results.is_empty() {
        push_wrapped(
            &mut lines,
            inner_width,
            "阶段结果：暂无回流结果。".to_string(),
            "  ",
        );
    } else {
        for entry in &snapshot.recent_results {
            push_wrapped(
                &mut lines,
                inner_width,
                format!("阶段结果：{}：{}", entry.worker, entry.summary),
                "  ",
            );
        }
    }
    lines
}

fn activity_lines(snapshot: &Snapshot, inner_width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if snapshot.activity.is_empty() {
        push_wrapped(
            &mut lines,
            inner_width,
            "• 暂无分派、relay 或 worker 生命周期事件。".to_string(),
            "  ",
        );
        return lines;
    }
    for entry in &snapshot.activity {
        push_wrapped(
            &mut lines,
            inner_width,
            format!("• {}", entry.summary),
            "  ",
        );
    }
    lines
}

fn worker_registration_text(worker: &super::WorkerState, slot: WorkerSlot) -> String {
    match &worker.connection {
        super::WorkerConnection::Pending => {
            format!("等待注册 · {}", slot.canonical_task_name())
        }
        super::WorkerConnection::Live(thread_id) => {
            format!("已附着 · {} · {thread_id}", slot.canonical_task_name())
        }
        super::WorkerConnection::ReattachRequired(thread_id) => {
            format!("待再附着 · {} · {thread_id}", slot.canonical_task_name())
        }
    }
}

fn mission_role(mission: Option<&super::Mission>, worker: WorkerSlot) -> Option<&str> {
    match (mission, worker) {
        (Some(mission), WorkerSlot::Frontend) => mission.frontend_role.as_deref(),
        (Some(mission), WorkerSlot::Backend) => mission.backend_role.as_deref(),
        (None, _) => None,
    }
}

fn mission_assignment(mission: Option<&super::Mission>, worker: WorkerSlot) -> Option<&str> {
    match (mission, worker) {
        (Some(mission), WorkerSlot::Frontend) => mission.frontend_assignment.as_deref(),
        (Some(mission), WorkerSlot::Backend) => mission.backend_assignment.as_deref(),
        (None, _) => None,
    }
}

fn mission_mode_label(mode: &super::MissionMode) -> &'static str {
    match mode {
        super::MissionMode::Solo(WorkerSlot::Frontend) => "solo-frontend",
        super::MissionMode::Solo(WorkerSlot::Backend) => "solo-backend",
        super::MissionMode::Parallel => "parallel",
        super::MissionMode::SerialHandoff => "serial-handoff",
        super::MissionMode::Blocked => "blocked",
    }
}

fn mission_phase_label(phase: super::MissionPhase) -> &'static str {
    match phase {
        super::MissionPhase::Idle => "idle",
        super::MissionPhase::Bootstrapping => "bootstrapping",
        super::MissionPhase::Planning => "planning",
        super::MissionPhase::Executing => "executing",
        super::MissionPhase::Validating => "validating",
        super::MissionPhase::Blocked => "blocked",
        super::MissionPhase::Completed => "completed",
    }
}

fn acceptance_status_label(status: super::AcceptanceStatus) -> &'static str {
    match status {
        super::AcceptanceStatus::Pending => "待验证",
        super::AcceptanceStatus::Met => "已满足",
        super::AcceptanceStatus::Failed => "受阻",
    }
}

fn overview_status(snapshot: &Snapshot) -> String {
    if !snapshot.start_requested {
        return format!(
            "尚未启动。先运行 `/zteam start` 创建 {} worker。",
            super::worker_task_list(&WorkerSlot::ALL)
        );
    }

    let reattach = snapshot.reattach_workers();
    if !reattach.is_empty() {
        return format!(
            "{} 需要再附着。运行 `/zteam attach` 尝试恢复最近的 worker 连接。",
            super::worker_list(&reattach)
        );
    }

    let pending = snapshot.pending_workers();
    if !pending.is_empty() {
        let live = snapshot.live_workers();
        if live.is_empty() {
            return format!(
                "已提交创建请求，等待 {} 注册。",
                super::worker_list(&pending)
            );
        }
        return format!(
            "已收到 {}，仍等待 {} 注册。",
            super::worker_list(&live),
            super::worker_list(&pending)
        );
    }

    format!(
        "{} worker 已就绪，可继续分派任务或转发消息。",
        super::worker_task_list(&WorkerSlot::ALL)
    )
}

fn blocking_note(snapshot: &Snapshot) -> Option<String> {
    let restart_command = if snapshot.mission.is_some() {
        "`/zteam start <goal>`"
    } else {
        "`/zteam start`"
    };
    if !snapshot.start_requested {
        return Some("当前还没有可分派目标；工作台只会显示空态。".to_string());
    }

    let pending = snapshot.pending_workers();
    if !pending.is_empty() {
        let live = snapshot.live_workers();
        if live.is_empty() {
            return Some(format!(
                "主线程尚未回流任何 worker 注册事件；若长时间无变化，说明主线程可能没有真正创建 worker。先检查主线程是否执行了 `spawn_agent`，必要时重新运行 {restart_command}。"
            ));
        }
        return Some(format!(
            "当前仅 {} 已注册，仍缺 {}；root -> worker 与 worker -> worker 路由暂不可用。若长时间无变化，说明主线程可能只创建了一部分 worker。",
            super::worker_list(&live),
            super::worker_list(&pending)
        ));
    }

    let reattach = snapshot.reattach_workers();
    if !reattach.is_empty() {
        return Some(format!(
            "{} 的最近线程当前未附着；先运行 `/zteam attach` 尝试重新附着，必要时再用 {restart_command} 重建 worker。",
            super::worker_list(&reattach),
        ));
    }

    None
}

fn push_wrapped(lines: &mut Vec<Line<'static>>, width: usize, text: String, indent: &str) {
    let options = if indent.is_empty() {
        Options::new(width)
    } else {
        Options::new(width).subsequent_indent(indent)
    };
    for line in wrap(&text, options) {
        lines.push(Line::from(line.into_owned()));
    }
}
