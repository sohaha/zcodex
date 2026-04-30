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
use crate::chatwidget::UserMessage;
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
            Command::Reset => {
                self.handle_zmission_reset(workspace);
            }
        }
        Ok(())
    }

    fn handle_zmission_start(&mut self, workspace: std::path::PathBuf, goal: Option<String>) {
        let Some(goal) = goal else {
            self.chat_widget.add_info_message(
                "请输入 Mission 目标：/zmission start <目标>".to_string(),
                None,
            );
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

                if let Some(_definition) = step.definition {
                    self.show_phase_confirm_selection(&step.state);
                } else {
                    // 规划完成，加载执行方案并注入执行 prompt
                    self.inject_execution_prompt(&planner, &step.state.goal);
                }
            }
            Err(err) => {
                self.chat_widget
                    .add_error_message(format!("Mission 推进失败：{err}"));
            }
        }
    }

    fn handle_zmission_reset(&mut self, workspace: std::path::PathBuf) {
        let store = MissionStateStore::for_workspace(&workspace);
        match store.reset() {
            Ok(()) => {
                self.chat_widget.pending_mission_goal = true;
                self.chat_widget.add_info_message(
                    "🔄 当前 Mission 已结束。请输入新的 Mission 目标：".to_string(),
                    None,
                );
            }
            Err(err) => {
                self.chat_widget
                    .add_error_message(format!("重置 Mission 失败：{err}"));
            }
        }
    }

    /// 从 ChatWidget 内部直接调用，启动新 Mission（用于 pending_mission_goal 拦截）。
    pub(crate) fn handle_zmission_start_from_widget(
        chat_widget: &mut crate::chatwidget::ChatWidget,
        workspace: std::path::PathBuf,
        goal: String,
    ) {
        let planner = MissionPlanner::for_workspace(&workspace);
        match planner.start(&goal) {
            Ok(step) => {
                let msg = format_planning_step("🚀 Mission 已启动", &step);
                chat_widget.add_info_message(msg, None);

                if step.definition.is_some() {
                    // Reuse the phase confirm logic by emitting a continue event
                    // which will show the selection popup
                    let phase_label = step.state.phase.map(|p| p.label()).unwrap_or("unknown");
                    let state = step.state;
                    let goal_clone = state.goal.clone();

                    let continue_actions: Vec<SelectionAction> = {
                        vec![Box::new(move |tx| {
                            tx.send(AppEvent::ZmissionCommand(Command::Continue { note: None }));
                        })]
                    };
                    let reset_actions: Vec<SelectionAction> = {
                        vec![Box::new(move |tx| {
                            tx.send(AppEvent::ZmissionCommand(Command::Reset));
                        })]
                    };
                    let items = vec![
                        SelectionItem {
                            name: "继续下一阶段".to_string(),
                            display_shortcut: Some(key_hint::plain(
                                crossterm::event::KeyCode::Enter,
                            )),
                            actions: continue_actions,
                            dismiss_on_select: true,
                            ..Default::default()
                        },
                        SelectionItem {
                            name: "结束并开始新 Mission".to_string(),
                            display_shortcut: Some(key_hint::plain(
                                crossterm::event::KeyCode::Char('r'),
                            )),
                            actions: reset_actions,
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
                    chat_widget.show_selection_view(SelectionViewParams {
                        title: Some(format!("Mission 阶段完成：{phase_label}")),
                        subtitle: Some(format!("目标：{goal_clone}")),
                        footer_hint: Some(standard_popup_hint_line()),
                        items,
                        ..Default::default()
                    });
                }
            }
            Err(err) => {
                chat_widget.add_error_message(format!("Mission 启动失败：{err}"));
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

        let reset_actions: Vec<SelectionAction> = {
            vec![Box::new(move |tx| {
                tx.send(AppEvent::ZmissionCommand(Command::Reset));
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
                name: "结束并开始新 Mission".to_string(),
                display_shortcut: Some(key_hint::plain(crossterm::event::KeyCode::Char('r'))),
                actions: reset_actions,
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

    /// 规划完成后加载执行方案并注入执行 prompt。
    fn inject_execution_prompt(&mut self, planner: &MissionPlanner, goal: &str) {
        let plan_content = match planner.load_execution_plan() {
            Ok(content) => content,
            Err(_) => {
                // 没有方案文件时仅用目标
                self.chat_widget.add_info_message(
                    "⚠ 未找到执行方案文件，将基于目标直接执行。".to_string(),
                    None,
                );
                format!("按以下 Mission 目标执行：\n{goal}")
            }
        };

        let prompt = format!(
            "开始执行 Mission：{goal}\n\n\
             ## 执行方案\n\n\
             {plan_content}\n\n\
             请严格按照上述方案中的步骤执行。"
        );
        self.chat_widget
            .submit_user_message_as_plain_user_turn(UserMessage::from(prompt.as_str()));
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
