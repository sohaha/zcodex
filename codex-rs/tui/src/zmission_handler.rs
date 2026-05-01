//! ZMission TUI 事件处理（Phase Agent 子代理架构）。
//!
//! 主代理（Orchestrator）只负责分配和编排，每个阶段由独立的 Phase Agent 子代理处理：
//! - 用户输入需求后，主代理创建 PhaseAgentManager 管理 7 个阶段子代理
//! - 每个阶段激活对应的 Phase Agent，用户切换到子代理界面进行交互
//! - 阶段完成后弹出确认面板，用户可选择继续或补充内容
//! - 补充内容会累积到当前子代理的上下文，不推进阶段

use crate::app_server_session::AppServerSession;
use crate::chatwidget::UserMessage;
use crate::zmission::PhaseAgentManager;
use crate::zmission::PhaseAgentView;
use crate::zmission::UserAction;
use crate::zmission_command::Command;
use codex_mission::MissionPlanner;
use codex_mission::MissionStateStore;
use codex_mission::MissionStatusReport;
use color_eyre::eyre::Result;
use std::sync::Arc;
use std::sync::RwLock;

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
            Command::View => {
                self.open_phase_agent_view();
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

        // 创建 Phase Agent 管理器（如果尚未创建）
        if self.phase_agent_manager.is_none() {
            self.phase_agent_manager = Some(Arc::new(RwLock::new(PhaseAgentManager::new(workspace))));
        }

        let manager = self.phase_agent_manager.as_ref().unwrap();
        let mut manager_guard = match manager.write() {
            Ok(guard) => guard,
            Err(_) => {
                self.chat_widget
                    .add_error_message("无法初始化 Phase Agent 管理器".to_string());
                return;
            }
        };

        match manager_guard.start_mission(goal.clone()) {
            Ok(agent_id) => {
                let phase = agent_id.phase;
                let phase_name = crate::zmission::phase_display_name(phase);

                // 显示 Mission 启动信息
                let msg = format!(
                    "🚀 Mission 已启动（Phase Agent 模式）\n\
                    目标：{}\n\
                    当前阶段：{} ({})\n\
                    使用 /zmission view 切换到子代理界面",
                    goal,
                    phase_name,
                    phase.label()
                );
                self.chat_widget.add_info_message(msg, None);

                // 获取子代理的完整提示并提交
                if let Some(prompt) = manager_guard.build_current_prompt() {
                    self.chat_widget.mission_phase_running = true;
                    self.chat_widget.submit_user_message_as_plain_user_turn(
                        crate::chatwidget::UserMessage::from(prompt.as_str()),
                    );
                }

                // 显示阶段确认面板（如果产物已存在）
                if manager_guard.current_artifact_exists() {
                    drop(manager_guard);
                    self.show_phase_confirmation();
                }
            }
            Err(err) => {
                self.chat_widget
                    .add_error_message(format!("Mission 启动失败：{err}"));
            }
        }
    }

    fn handle_zmission_status(&mut self, workspace: std::path::PathBuf) {
        // 优先使用 Phase Agent 管理器的状态
        if let Some(manager) = &self.phase_agent_manager {
            if let Ok(manager_guard) = manager.read() {
                let summary = manager_guard.mission_summary();
                let completed = manager_guard.completed_phases_count();

                let mut lines = Vec::new();
                lines.push(summary);
                lines.push(format!("已完成阶段数：{}/7", completed));

                if let Some(phase) = manager_guard.active_agent_id() {
                    lines.push(format!("当前阶段：{}", phase.label()));

                    if let Some(agent) = manager_guard.get_agent(phase.phase) {
                        lines.push(format!("子代理状态：{}", agent.state.label()));
                        lines.push(format!("子代理角色：{}", agent.role.display_name()));
                    }
                }

                self.chat_widget.add_info_message(lines.join("\n"), None);
                return;
            }
        }

        // 回退到传统状态查询
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

    fn handle_zmission_continue(&mut self, _workspace: std::path::PathBuf, note: Option<String>) {
        let Some(manager) = &self.phase_agent_manager else {
            self.chat_widget
                .add_error_message("没有活跃的 Mission。请先运行 `/zmission start <目标>`".to_string());
            return;
        };

        let mut manager_guard = match manager.write() {
            Ok(guard) => guard,
            Err(_) => {
                self.chat_widget
                    .add_error_message("无法访问 Phase Agent 管理器".to_string());
                return;
            }
        };

        match manager_guard.confirm_and_advance(note) {
            Ok(Some(next_agent_id)) => {
                let phase_name = crate::zmission::phase_display_name(next_agent_id.phase);

                let msg = format!(
                    "✅ 阶段已确认，进入下一阶段\n\
                    当前阶段：{} ({})",
                    phase_name,
                    next_agent_id.phase.label()
                );
                self.chat_widget.add_info_message(msg, None);

                // 提交新阶段的提示
                if let Some(prompt) = manager_guard.build_current_prompt() {
                    self.chat_widget.mission_phase_running = true;
                    self.chat_widget.submit_user_message_as_plain_user_turn(
                        crate::chatwidget::UserMessage::from(prompt.as_str()),
                    );
                }

                drop(manager_guard);
                self.show_phase_confirmation();
            }
            Ok(None) => {
                // 所有阶段完成
                drop(manager_guard);

                let msg = "🎉 Mission 规划阶段全部完成！\n准备进入执行阶段...".to_string();
                self.chat_widget.add_info_message(msg, None);

                // 注入执行 prompt
                self.inject_execution_prompt_from_manager();
            }
            Err(err) => {
                self.chat_widget
                    .add_error_message(format!("Mission 推进失败：{err}"));
            }
        }
    }

    fn handle_zmission_reset(&mut self, _workspace: std::path::PathBuf) {
        if let Some(manager) = &self.phase_agent_manager {
            let mut manager_guard = match manager.write() {
                Ok(guard) => guard,
                Err(_) => {
                    self.chat_widget
                        .add_error_message("无法访问 Phase Agent 管理器".to_string());
                    return;
                }
            };

            if let Err(err) = manager_guard.reset() {
                self.chat_widget
                    .add_error_message(format!("重置 Mission 失败：{err}"));
                return;
            }
        }

        self.phase_agent_manager = None;
        self.chat_widget.pending_mission_goal = true;
        self.chat_widget.add_info_message(
            "🔄 当前 Mission 已结束。请输入新的 Mission 目标：".to_string(),
            None,
        );
    }

    /// 显示阶段确认面板。
    fn show_phase_confirmation(&mut self) {
        let Some(manager) = &self.phase_agent_manager else {
            return;
        };

        let manager_guard = match manager.read() {
            Ok(guard) => guard,
            Err(_) => return,
        };

        let Some(agent) = manager_guard.active_agent() else {
            return;
        };

        let phase_name = crate::zmission::phase_display_name(agent.id.phase);
        let phase_desc = crate::zmission::phase_description(agent.id.phase);

        // 读取产物预览
        let artifact_preview = if let Some(path) = manager_guard.current_artifact_path() {
            std::fs::read_to_string(&path)
                .map(|content| {
                    if content.len() > 500 {
                        format!("{}...", &content[..500])
                    } else {
                        content
                    }
                })
                .unwrap_or_else(|_| "产物文件已创建".to_string())
        } else {
            "产物文件已创建".to_string()
        };

        drop(manager_guard);

        // 创建确认视图
        let view = crate::zmission::PhaseConfirmationView::new(
            phase_name,
            phase_desc,
            artifact_preview,
        );

        self.chat_widget.open_phase_confirmation_view(Box::new(view));
    }

    /// 处理阶段确认结果。
    pub(crate) fn handle_phase_confirmation_result(&mut self, action: UserAction) {
        match action {
            UserAction::Continue => {
                // 用户确认继续，推进到下一阶段
                self.handle_zmission_continue(self.config.cwd.to_path_buf(), None);
            }
            UserAction::Supplement(content) => {
                // 用户选择补充内容
                let Some(manager) = &self.phase_agent_manager else {
                    return;
                };

                let mut manager_guard = match manager.write() {
                    Ok(guard) => guard,
                    Err(_) => return,
                };

                if let Some(agent) = manager_guard.supplement_and_continue(content.clone()) {
                    let phase_name = crate::zmission::phase_display_name(agent.id.phase);

                    self.chat_widget.add_info_message(
                        format!("📝 补充内容已添加到 {} 阶段\n继续当前阶段分析...", phase_name),
                        None,
                    );

                    // 提交包含补充内容的提示
                    if let Some(prompt) = manager_guard.build_current_prompt() {
                        self.chat_widget.mission_phase_running = true;
                        self.chat_widget.submit_user_message_as_plain_user_turn(
                            crate::chatwidget::UserMessage::from(prompt.as_str()),
                        );
                    }
                }
            }
        }
    }

    /// 打开 Phase Agent 视图界面。
    pub(crate) fn open_phase_agent_view(&mut self) {
        let Some(manager) = &self.phase_agent_manager else {
            self.chat_widget
                .add_info_message("没有活跃的 Mission。请先运行 `/zmission start <目标>`".to_string(), None);
            return;
        };

        let view = match PhaseAgentView::new(manager.clone()) {
            Some(v) => v,
            None => {
                self.chat_widget
                    .add_error_message("无法创建 Phase Agent 视图".to_string());
                return;
            }
        };

        self.chat_widget.open_phase_agent_view(Box::new(view));
    }

    /// 从 ChatWidget 内部直接调用，启动新 Mission（用于 pending_mission_goal 拦截）。
    pub(crate) fn handle_zmission_start_from_widget(
        chat_widget: &mut crate::chatwidget::ChatWidget,
        workspace: std::path::PathBuf,
        goal: String,
    ) {
        // 注意：这里无法访问 App 的 phase_agent_manager，需要在 App 中处理
        // 作为兼容方案，使用传统方式启动
        let planner = MissionPlanner::for_workspace(&workspace);
        match planner.start(&goal) {
            Ok(step) => {
                let msg = format_planning_step_legacy("🚀 Mission 已启动", &step);
                chat_widget.add_info_message(msg, None);

                if let Some(definition) = step.definition {
                    let prompt =
                        format_phase_analysis_prompt_legacy(&step.state.goal, &definition, &planner);
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

    /// 从 Phase Agent Manager 注入执行 prompt。
    fn inject_execution_prompt_from_manager(&mut self) {
        let Some(manager) = &self.phase_agent_manager else {
            return;
        };

        let manager_guard = match manager.read() {
            Ok(guard) => guard,
            Err(_) => return,
        };

        let goal = manager_guard.mission_summary();

        // 尝试读取 plan.md
        let plan_content = manager_guard
            .current_artifact_path()
            .and_then(|_| {
                // 这里应该读取 plan.md，但 current_artifact_path 返回当前阶段的产物
                // 实际实现需要调整
                None
            })
            .unwrap_or_else(|| "执行方案已生成".to_string());

        drop(manager_guard);

        let prompt = format!(
            "🎯 Mission 规划阶段全部完成！\n\n\
            目标：{goal}\n\n\
            ## 执行方案\n\n\
            {plan_content}\n\n\
            请严格按照上述方案中的步骤执行。"
        );

        self.chat_widget
            .submit_user_message_as_plain_user_turn(UserMessage::from(prompt.as_str()));
    }

    /// 注入执行 prompt（传统方式，用于兼容性）。
    fn inject_execution_prompt_legacy(&mut self, planner: &MissionPlanner, goal: &str) {
        let plan_content = match planner.load_execution_plan() {
            Ok(content) => content,
            Err(_) => {
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

/// 格式化规划步骤信息（兼容函数）。
fn format_planning_step_legacy(prefix: &str, step: &codex_mission::MissionPlanningStep) -> String {
    let mut lines = Vec::new();
    lines.push(prefix.to_string());
    lines.push(format!("状态：{}", step.state.status.label()));
    lines.push(format!("目标：{}", step.state.goal));
    if let Some(def) = step.definition {
        lines.push(format!("当前阶段：{} ({})", def.title, def.phase.label()));
        lines.push(format!("提示：{}", def.prompt));
        lines.push(format!("出口条件：{}", def.exit_condition));
        lines.push(format!("产物文件：{}", def.artifact_filename));
    } else {
        lines.push("规划阶段已完成，Mission 进入执行状态。".to_string());
    }
    lines.join("\n")
}

/// 构建阶段分析 prompt（兼容函数）。
fn format_phase_analysis_prompt_legacy(
    goal: &str,
    definition: &codex_mission::MissionPhaseDefinition,
    planner: &MissionPlanner,
) -> String {
    let artifact_path = planner.phase_artifact_path(definition.phase);
    format!(
        "你正在执行一个 Mission 规划流程。\n\n\
         Mission 目标：{goal}\n\n\
         当前阶段：{title} ({phase})\n\
         阶段提示：{prompt_text}\n\
         出口条件：{exit_condition}\n\n\
         **重要：** 请将本阶段的分析结果写入产物文件：`{artifact_path}`\n\
         产物文件必须存在且非空，否则无法推进到下一阶段。\n\n\
         请根据上述信息完成当前阶段的分析。",
        goal = goal,
        title = definition.title,
        phase = definition.phase.label(),
        prompt_text = definition.prompt,
        exit_condition = definition.exit_condition,
        artifact_path = artifact_path.display(),
    )
}
