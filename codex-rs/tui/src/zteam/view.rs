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
        lines.push(section_header("概况"));
        push_wrapped(
            &mut lines,
            inner_width,
            format!("整体状态：{}", overview_status(&snapshot)),
            "",
        );
        if let Some(adapter) = &snapshot.federation_adapter {
            push_wrapped(
                &mut lines,
                inner_width,
                format!("外部 adapter：{}", adapter.summary()),
                "",
            );
        }
        if let Some(blocked) = blocking_note(&snapshot) {
            push_wrapped(&mut lines, inner_width, format!("阻塞提示：{blocked}"), "");
        }
        lines.push(Line::from(""));

        lines.push(section_header("Worker 面板"));
        for worker in WorkerSlot::ALL {
            lines.extend(worker_panel_lines(
                snapshot.worker(worker),
                worker,
                inner_width,
            ));
        }
        lines.push(Line::from(""));

        lines.push(section_header("任务板"));
        for worker in WorkerSlot::ALL {
            push_wrapped(
                &mut lines,
                inner_width,
                format!(
                    "{worker}：{}",
                    snapshot
                        .worker(worker)
                        .last_dispatched_task
                        .as_deref()
                        .map(super::preview)
                        .unwrap_or_else(|| "等待任务".to_string())
                ),
                "  ",
            );
        }
        lines.push(Line::from(""));

        lines.push(section_header("消息流"));
        if snapshot.activity.is_empty() {
            push_wrapped(
                &mut lines,
                inner_width,
                "• 暂无分派、relay 或 worker 生命周期事件。".to_string(),
                "  ",
            );
        } else {
            for entry in &snapshot.activity {
                push_wrapped(
                    &mut lines,
                    inner_width,
                    format!("• {}", entry.summary),
                    "  ",
                );
            }
        }
        lines.push(Line::from(""));

        lines.push(section_header("结果回流"));
        if snapshot.recent_results.is_empty() {
            push_wrapped(
                &mut lines,
                inner_width,
                "• 暂无阶段结果。".to_string(),
                "  ",
            );
        } else {
            for entry in &snapshot.recent_results {
                push_wrapped(
                    &mut lines,
                    inner_width,
                    format!("• {}：{}", entry.worker, entry.summary),
                    "  ",
                );
            }
        }

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

        Paragraph::new(Line::from(format!("{MODE_NAME} 工作台").bold())).render(title_area, buf);
        if body_area.height > 0 {
            Paragraph::new(Text::from(self.body_lines(body_area.width)))
                .wrap(Wrap { trim: false })
                .render(body_area, buf);
        }
        if hint_area.height > 0 {
            Paragraph::new(Line::from(vec![
                "Esc 关闭".dim(),
                " · ".dim(),
                "/zteam start".cyan(),
                " 创建 worker".dim(),
                " · ".dim(),
                "/zteam attach".cyan(),
                " 再附着".dim(),
                " · ".dim(),
                "/zteam <worker> <任务>".cyan(),
            ]))
            .render(hint_area, buf);
        }
    }
}

fn section_header(title: &str) -> Line<'static> {
    Line::from(title.to_string().bold())
}

fn worker_panel_lines(
    worker: &super::WorkerState,
    slot: WorkerSlot,
    inner_width: usize,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let registration = match &worker.connection {
        super::WorkerConnection::Pending => {
            format!("等待注册 · {}", slot.canonical_task_name())
        }
        super::WorkerConnection::Live(thread_id) => {
            format!("已附着 · {} · {thread_id}", slot.canonical_task_name())
        }
        super::WorkerConnection::ReattachRequired(thread_id) => {
            format!("待再附着 · {} · {thread_id}", slot.canonical_task_name())
        }
    };
    push_wrapped(
        &mut lines,
        inner_width,
        format!("{slot}：{registration}"),
        "  ",
    );
    push_wrapped(
        &mut lines,
        inner_width,
        format!("  来源：{}", worker.source.label()),
        "    ",
    );
    push_wrapped(
        &mut lines,
        inner_width,
        format!(
            "  最近任务：{}",
            worker
                .last_dispatched_task
                .as_deref()
                .map(super::preview)
                .unwrap_or_else(|| "无".to_string())
        ),
        "    ",
    );
    push_wrapped(
        &mut lines,
        inner_width,
        format!(
            "  最近结果：{}",
            worker
                .last_result
                .as_deref()
                .map(super::preview)
                .unwrap_or_else(|| "无".to_string())
        ),
        "    ",
    );
    lines
}

fn overview_status(snapshot: &Snapshot) -> String {
    if !snapshot.start_requested {
        return format!(
            "尚未启动。先运行 `/zteam start` 创建 {} worker。",
            worker_task_list(&WorkerSlot::ALL)
        );
    }

    let reattach = reattach_workers(snapshot);
    if !reattach.is_empty() {
        return format!(
            "{} 需要再附着。运行 `/zteam attach` 尝试恢复最近的 worker 连接。",
            worker_list(&reattach)
        );
    }

    let missing = missing_workers(snapshot);
    if !missing.is_empty() {
        return format!("已请求创建 worker，等待 {} 注册。", worker_list(&missing));
    }

    format!(
        "{} worker 已就绪，可继续分派任务或转发消息。",
        worker_task_list(&WorkerSlot::ALL)
    )
}

fn blocking_note(snapshot: &Snapshot) -> Option<String> {
    if !snapshot.start_requested {
        return Some("当前还没有可分派目标；工作台只会显示空态。".to_string());
    }

    let missing = missing_workers(snapshot);
    if !missing.is_empty() {
        return Some(format!(
            "主线程尚未收到 {} 的 spawn 回执；root -> worker 与 worker -> worker 路由暂不可用。",
            worker_list(&missing)
        ));
    }

    let reattach = reattach_workers(snapshot);
    if !reattach.is_empty() {
        return Some(format!(
            "{} 的最近线程当前未附着；先运行 `/zteam attach` 尝试重新附着，必要时再用 `/zteam start` 重建 worker。",
            worker_list(&reattach)
        ));
    }

    None
}

fn missing_workers(snapshot: &Snapshot) -> Vec<WorkerSlot> {
    WorkerSlot::ALL
        .into_iter()
        .filter(|worker| {
            matches!(
                snapshot.worker(*worker).connection,
                super::WorkerConnection::Pending
            )
        })
        .collect()
}

fn reattach_workers(snapshot: &Snapshot) -> Vec<WorkerSlot> {
    WorkerSlot::ALL
        .into_iter()
        .filter(|worker| {
            matches!(
                snapshot.worker(*worker).connection,
                super::WorkerConnection::ReattachRequired(_)
            )
        })
        .collect()
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
        .map(|worker| worker.task_name())
        .collect::<Vec<_>>()
        .join("/")
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
