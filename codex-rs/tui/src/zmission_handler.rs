//! ZMission TUI 事件处理。
//!
//! 处理 `/zmission` 斜杠命令，与 `codex_mission::MissionPlanner` 交互，
//! 展示阶段信息到 TUI 聊天区域，并在阶段完成后弹出确认面板。

use crate::app_event::AppEvent;
use crate::app_server_session::AppServerSession;
use crate::bottom_pane::SelectionAction;
use crate::bottom_pane::SelectionItem;
use crate::bottom_pane::SelectionViewParams;
use crate::bottom_pane::popup_consts::standard_popup_hint_line;
use crate::key_hint;
use crate::zmission_command::Command;
use codex_mission::MissionPlanner;
use codex_mission::MissionStateStore;
use codex_mission::MissionStatusReport;
use color_eyre::eyre::Result;

impl crate::app::App {
    pub(crate) async fn handle_zmission_command(
        &mut self,
        _app_server: &mut AppServerSession,
        command: Command,
    ) -> Result<()> {
        let workspace = self.config.cwd.to_path_buf();
        match command {
            Command::Start { goal } => {
                self.handle_zmission_start(workspace, goal);
            }
            Command::Status => {
                self.handle_zmission_status(workspace);
            }
            Command::Continue { note } => {
                self.handle_zmission_continue(workspace, note);
            }
        }
        Ok(())
    }

    fn handle_zmission_start(&mut self, workspace: std::path::PathBuf, goal: Option<String>) {
        let Some(goal) = goal else {
            self.chat_widget
                .add_error_message("请提供 Mission 目标：/zmission start <goal>".to_string());
            return;
        };

        let planner = MissionPlanner::for_workspace(&workspace);
        match planner.start(&goal) {
            Ok(step) => {
                let msg = format_planning_step("🚀 Mission 已启动", &step);
                self.chat_widget.add_info_message(msg, None);

                if step.definition.is_some() {
                    self.show_phase_confirm_selection(&step.state);
                }
            }
            Err(err) => {
                self.chat_widget
                    .add_error_message(format!("Mission 启动失败：{err}"));
            }
        }
    }

    fn handle_zmission_status(&mut self, workspace: std::path::PathBuf) {
        let store = MissionStateStore::for_workspace(&workspace);
        match store.status_report() {
            Ok(MissionStatusReport::Empty { state_path }) => {
                self.chat_widget.add_info_message(
                    format!("Mission 状态：未启动\n状态文件：{}", state_path.display()),
                    None,
                );
            }
            Ok(MissionStatusReport::Active { state_path, state }) => {
                let mut lines = Vec::new();
                lines.push(format!("Mission 状态：{}", state.status.label()));
                lines.push(format!("目标：{}", state.goal));
                if let Some(phase) = state.phase {
                    lines.push(format!("阶段：{}", phase.label()));
                }
                lines.push(format!("已完成 {} 个阶段", state.completed_phases.len()));
                lines.push(format!("状态文件：{}", state_path.display()));
                self.chat_widget.add_info_message(lines.join("\n"), None);
            }
            Err(err) => {
                self.chat_widget
                    .add_error_message(format!("读取 Mission 状态失败：{err}"));
            }
        }
    }

    fn handle_zmission_continue(&mut self, workspace: std::path::PathBuf, note: Option<String>) {
        let planner = MissionPlanner::for_workspace(&workspace);
        match planner.continue_planning(note) {
            Ok(step) => {
                let msg = format_planning_step("✅ 阶段已确认", &step);
                self.chat_widget.add_info_message(msg, None);

                if step.definition.is_some() {
                    self.show_phase_confirm_selection(&step.state);
                }
            }
            Err(err) => {
                self.chat_widget
                    .add_error_message(format!("Mission 推进失败：{err}"));
            }
        }
    }

    /// 在阶段完成后弹出确认选择面板。
    fn show_phase_confirm_selection(&mut self, state: &codex_mission::MissionState) {
        let phase_label = state.phase.map(|p| p.label()).unwrap_or("unknown");
        let goal = state.goal.clone();

        let continue_actions: Vec<SelectionAction> = {
            vec![Box::new(move |tx| {
                tx.send(AppEvent::ZmissionCommand(Command::Continue { note: None }));
            })]
        };

        let items = vec![
            SelectionItem {
                name: "继续下一阶段".to_string(),
                display_shortcut: Some(key_hint::plain(crossterm::event::KeyCode::Enter)),
                actions: continue_actions,
                dismiss_on_select: true,
                ..Default::default()
            },
            SelectionItem {
                name: "暂停".to_string(),
                display_shortcut: Some(key_hint::plain(crossterm::event::KeyCode::Esc)),
                is_default: false,
                dismiss_on_select: true,
                ..Default::default()
            },
        ];

        self.chat_widget.show_selection_view(SelectionViewParams {
            title: Some(format!("Mission 阶段完成：{phase_label}")),
            subtitle: Some(format!("目标：{goal}")),
            footer_hint: Some(standard_popup_hint_line()),
            items,
            ..Default::default()
        });
    }
}

fn format_planning_step(prefix: &str, step: &codex_mission::MissionPlanningStep) -> String {
    let mut lines = Vec::new();
    lines.push(prefix.to_string());
    lines.push(format!("状态：{}", step.state.status.label()));
    lines.push(format!("目标：{}", step.state.goal));
    if let Some(def) = step.definition {
        lines.push(format!("当前阶段：{} ({})", def.title, def.phase.label()));
        lines.push(format!("提示：{}", def.prompt));
        lines.push(format!("出口条件：{}", def.exit_condition));
    } else {
        lines.push("规划阶段已完成，Mission 进入执行状态。".to_string());
    }
    lines.join("\n")
}
