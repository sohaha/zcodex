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
            self.chat_widget.pending_mission_goal = true;
            self.chat_widget
                .add_info_message("请输入 Mission 目标：".to_string(), None);
            return;
        };

        let planner = MissionPlanner::for_workspace(&workspace);
        match planner.start(&goal) {
            Ok(step) => {
                let msg = format_planning_step("🚀 Mission 已启动", &step);
                self.chat_widget.add_info_message(msg, None);

                if let Some(definition) = step.definition {
                    let prompt = format_phase_analysis_prompt(&step.state.goal, &definition);
                    self.chat_widget.mission_phase_running = true;
                    self.chat_widget.submit_user_message_as_plain_user_turn(
                        crate::chatwidget::UserMessage::from(prompt.as_str()),
                    );
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

                if let Some(definition) = step.definition {
                    let prompt = format_phase_analysis_prompt(&step.state.goal, &definition);
                    self.chat_widget.mission_phase_running = true;
                    self.chat_widget.submit_user_message_as_plain_user_turn(
                        crate::chatwidget::UserMessage::from(prompt.as_str()),
                    );
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

                if let Some(definition) = step.definition {
                    // 注入阶段分析 prompt，让 LLM 开始分析当前阶段
                    let prompt = format_phase_analysis_prompt(&step.state.goal, &definition);
                    chat_widget.mission_phase_running = true;
                    chat_widget.submit_user_message_as_plain_user_turn(
                        crate::chatwidget::UserMessage::from(prompt.as_str()),
                    );
                }
            }
            Err(err) => {
                chat_widget.add_error_message(format!("Mission 启动失败：{err}"));
            }
        }
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

/// 构建阶段分析 prompt，让 LLM 对当前阶段进行分析。
fn format_phase_analysis_prompt(
    goal: &str,
    definition: &codex_mission::MissionPhaseDefinition,
) -> String {
    format!(
        "你正在执行一个 Mission 规划流程。\n\n\
         Mission 目标：{goal}\n\n\
         当前阶段：{title} ({phase})\n\
         阶段提示：{prompt_text}\n\
         出口条件：{exit_condition}\n\n\
         请根据上述信息完成当前阶段的分析。完成后，将分析结果写入计划文件。",
        goal = goal,
        title = definition.title,
        phase = definition.phase.label(),
        prompt_text = definition.prompt,
        exit_condition = definition.exit_condition,
    )
}
