//! ZMission TUI 事件处理（Phase Agent 子代理架构）。
//!
//! 主代理（Orchestrator）只负责分配和编排，每个阶段由独立的 Phase Agent 子代理处理：
//! - 用户输入需求后，主代理创建 PhaseAgentManager 管理 7 个阶段子代理
//! - 每个阶段激活对应的 Phase Agent，用户切换到子代理界面进行交互
//! - 阶段完成后弹出确认面板，用户可选择继续或补充内容
//! - 补充内容会累积到当前子代理的上下文，不推进阶段

use crate::app_event::AppEvent;
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
        app_server: &mut AppServerSession,
        command: Command,
    ) -> Result<()> {
        let workspace = self.config.cwd.to_path_buf();
        match command {
            Command::Start { goal } => {
                self.handle_zmission_start(app_server, workspace, goal)
                    .await;
            }
            Command::Status => {
                self.handle_zmission_status(workspace);
            }
            Command::Continue { note } => {
                self.handle_zmission_continue(app_server, workspace, note)
                    .await;
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

    async fn handle_zmission_start(
        &mut self,
        app_server: &mut AppServerSession,
        workspace: std::path::PathBuf,
        goal: Option<String>,
    ) {
        let Some(goal) = goal else {
            self.chat_widget.pending_mission_goal = true;
            self.chat_widget
                .add_info_message("请输入 Mission 目标：".to_string(), None);
            return;
        };

        // 创建 Phase Agent 管理器（如果尚未创建）
        if self.phase_agent_manager.is_none() {
            self.phase_agent_manager =
                Some(Arc::new(RwLock::new(PhaseAgentManager::new(workspace))));
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
                    "🚀 Mission 已启动（延迟 fork 模式）\n\
                    目标：{}\n\
                    当前阶段：{} ({})\n\
                    等待主线程空闲后将自动创建 Phase Agent 子线程...",
                    goal,
                    phase_name,
                    phase.label()
                );
                self.chat_widget.add_info_message(msg, None);

                // 设置待 fork 阶段，等待主线程空闲后 fork
                manager_guard.set_pending_spawn_phase(phase);
                drop(manager_guard);

                // 立即尝试 fork（如果主线程空闲）
                self.try_spawn_pending_phase_agent(app_server).await;
            }
            Err(err) => {
                self.chat_widget
                    .add_error_message(format!("Mission 启动失败：{err}"));
            }
        }
    }

    /// 尝试创建待 fork 的 Phase Agent 子线程。
    /// 如果主线程空闲则立即 fork，否则等待下次调用。
    pub(crate) async fn try_spawn_pending_phase_agent(
        &mut self,
        app_server: &mut AppServerSession,
    ) {
        // 检查是否有待 fork 的阶段
        let pending_phase = {
            let Some(manager) = &self.phase_agent_manager else {
                return;
            };
            let Ok(mut manager_guard) = manager.write() else {
                return;
            };
            manager_guard.take_pending_spawn_phase()
        };

        let Some(phase) = pending_phase else {
            return;
        };

        // 检查主线程是否空闲
        if self.chat_widget.is_agent_turn_running() {
            self.chat_widget.add_info_message(
                "⏳ 主线程忙碌中，等待空闲后将自动创建 Phase Agent 子线程...".to_string(),
                None,
            );
            // 重新设置待 fork 阶段，下次再试
            if let Some(manager) = &self.phase_agent_manager {
                if let Ok(mut manager_guard) = manager.write() {
                    manager_guard.set_pending_spawn_phase(phase);
                }
            }
            return;
        }

        // 主线程空闲，可以 fork
        self.chat_widget.add_info_message(
            format!(
                "✅ 主线程空闲，正在为阶段 {} 创建 Phase Agent 子线程...",
                crate::zmission::phase_display_name(phase)
            ),
            None,
        );

        if let Err(err) = self.spawn_phase_agent_thread(app_server, phase).await {
            self.chat_widget.add_error_message(format!(
                "创建 Phase Agent 线程失败：{err}\n\
                请稍后重试或检查 AppServer 连接。"
            ));
        }
    }

    /// 为指定阶段创建子线程（真正的 Phase Agent）。
    async fn spawn_phase_agent_thread(
        &mut self,
        app_server: &mut AppServerSession,
        phase: codex_mission::MissionPhase,
    ) -> Result<()> {
        use codex_protocol::models::ContentItem;

        self.chat_widget.add_info_message(
            format!("[DEBUG] spawn_phase_agent_thread 开始执行，阶段：{phase}"),
            None,
        );

        let manager = self
            .phase_agent_manager
            .as_ref()
            .ok_or_else(|| color_eyre::eyre::eyre!("PhaseAgentManager 未初始化"))?;

        // 获取 role 和 prompt
        let (role, prompt, has_thread) = {
            let manager_guard = manager
                .read()
                .map_err(|_| color_eyre::eyre::eyre!("无法获取管理器锁"))?;

            let Some(agent) = manager_guard.get_agent(phase) else {
                return Err(color_eyre::eyre::eyre!("找不到阶段 {phase} 的 PhaseAgent"));
            };

            let has_thread = agent.thread_id.is_some();
            let prompt = manager_guard.build_current_prompt().unwrap_or_default();

            (agent.role, prompt, has_thread)
        };

        // 如果已有线程，重新激活
        if has_thread {
            return self
                .reinject_to_phase_agent_thread(app_server, phase, &prompt)
                .await;
        }

        // 获取当前线程 ID 作为父线程
        let parent_thread_id = match self.chat_widget.thread_id() {
            Some(id) => id,
            None => {
                self.chat_widget.add_error_message(
                    "❌ 无法创建 Phase Agent：没有主线程\n\
                    请先发送任意消息建立主会话，然后再运行 /zmission start"
                        .to_string(),
                );
                return Err(color_eyre::eyre::eyre!(
                    "没有主线程，无法 fork 子线程。请先确保有一个活跃的主对话线程。"
                ));
            }
        };

        self.chat_widget.add_info_message(
            format!("[DEBUG] 父线程 ID: {parent_thread_id}, 准备 fork..."),
            None,
        );

        // 配置子线程（Phase Agent）
        let mut fork_config = self.config.clone();
        fork_config.ephemeral = true;
        fork_config.developer_instructions = Some(format!(
            "你是 {role} Phase Agent，专门负责处理 Mission 的 {phase} 阶段。\n\n\
            你的职责：\n\
            1. 专注于当前阶段的分析任务\n\
            2. 将分析结果写入对应的产物文件\n\
            3. 不要处理其他阶段的任务\n\n\
            {role_prompt}",
            role = role.display_name(),
            phase = crate::zmission::phase_display_name(phase),
            role_prompt = role.role_prompt()
        ));

        // 调试：输出配置信息
        self.chat_widget.add_info_message(
            format!(
                "[DEBUG] fork_config: model={model:?}, cwd={cwd:?}",
                model = fork_config.model,
                cwd = fork_config.cwd
            ),
            None,
        );

        // Fork 子线程
        self.chat_widget
            .add_info_message("[DEBUG] 正在调用 fork_thread...".to_string(), None);

        let forked = match app_server.fork_thread(fork_config, parent_thread_id).await {
            Ok(forked) => forked,
            Err(e) => {
                let err_msg = format!(
                    "Fork 子线程失败：{e}\n\
                    详细错误：{e:?}\n\
                    父线程：{parent_thread_id}\n\
                    请确认主线程是否已在服务器端完全初始化。"
                );
                self.chat_widget.add_error_message(err_msg.clone());
                return Err(color_eyre::eyre::eyre!(err_msg));
            }
        };

        let child_thread_id = forked.session.thread_id;

        self.chat_widget.add_info_message(
            format!("[DEBUG] fork_thread 成功，子线程 ID: {child_thread_id}"),
            None,
        );

        // 存储 thread_id 到 PhaseAgent
        {
            let mut manager_guard = manager
                .write()
                .map_err(|_| color_eyre::eyre::eyre!("无法获取管理器锁"))?;
            if let Some(agent) = manager_guard.get_agent_mut(phase) {
                agent.thread_id = Some(child_thread_id);
            }
        }

        // 创建线程事件通道
        self.ensure_thread_channel(child_thread_id);

        self.chat_widget
            .add_info_message("[DEBUG] 正在注入提示到子线程...".to_string(), None);

        // 注入阶段提示到子线程
        let prompt_item = codex_protocol::models::ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText { text: prompt }],
            end_turn: None,
            phase: None,
        };

        app_server
            .thread_inject_items(child_thread_id, vec![prompt_item])
            .await
            .map_err(|e| color_eyre::eyre::eyre!("注入提示到 Phase Agent 线程失败：{e}"))?;

        // 通知用户
        self.chat_widget.add_info_message(
            format!(
                "✅ {phase} Phase Agent 已启动（线程：{thread_id}）\n\
                提示已注入子线程，正在执行阶段分析...",
                phase = crate::zmission::phase_display_name(phase),
                thread_id = child_thread_id
            ),
            None,
        );

        Ok(())
    }

    /// 向已存在的 Phase Agent 线程重新注入提示。
    async fn reinject_to_phase_agent_thread(
        &mut self,
        app_server: &mut AppServerSession,
        phase: codex_mission::MissionPhase,
        prompt: &str,
    ) -> Result<()> {
        use codex_protocol::models::ContentItem;

        let manager = self
            .phase_agent_manager
            .as_ref()
            .ok_or_else(|| color_eyre::eyre::eyre!("PhaseAgentManager 未初始化"))?;

        let thread_id = {
            let manager_guard = manager
                .read()
                .map_err(|_| color_eyre::eyre::eyre!("无法获取管理器锁"))?;

            let Some(agent) = manager_guard.get_agent(phase) else {
                return Err(color_eyre::eyre::eyre!("找不到阶段 {phase} 的 PhaseAgent"));
            };

            agent
                .thread_id
                .ok_or_else(|| color_eyre::eyre::eyre!("PhaseAgent {phase} 没有关联的线程"))?
        };

        // 注入新的提示到现有线程
        let prompt_item = codex_protocol::models::ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: prompt.to_string(),
            }],
            end_turn: None,
            phase: None,
        };

        app_server
            .thread_inject_items(thread_id, vec![prompt_item])
            .await
            .map_err(|e| color_eyre::eyre::eyre!("注入提示失败：{e}"))?;

        self.chat_widget.add_info_message(
            format!(
                "🔄 重新激活 {phase} Phase Agent（线程：{thread_id}）",
                phase = crate::zmission::phase_display_name(phase),
                thread_id = thread_id
            ),
            None,
        );

        Ok(())
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

    async fn handle_zmission_continue(
        &mut self,
        app_server: &mut AppServerSession,
        _workspace: std::path::PathBuf,
        note: Option<String>,
    ) {
        let Some(manager) = &self.phase_agent_manager else {
            self.chat_widget.add_error_message(
                "没有活跃的 Mission。请先运行 `/zmission start <目标>`".to_string(),
            );
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
                let next_phase = next_agent_id.phase;

                let msg = format!(
                    "✅ 阶段已确认，进入下一阶段\n\
                    当前阶段：{} ({})",
                    phase_name,
                    next_phase.label()
                );
                self.chat_widget.add_info_message(msg, None);

                // 设置待 fork 阶段，等待主线程空闲后 fork
                manager_guard.set_pending_spawn_phase(next_phase);
                drop(manager_guard);

                // 立即尝试 fork（如果主线程空闲）
                self.try_spawn_pending_phase_agent(app_server).await;

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

        // 创建确认视图，传递 app_event_tx 以自动触发命令
        let view = crate::zmission::PhaseConfirmationView::new(
            phase_name,
            phase_desc,
            artifact_preview,
            self.app_event_tx.clone(),
        );

        self.chat_widget
            .open_phase_confirmation_view(Box::new(view));
    }

    /// 处理阶段确认结果（由确认视图回调）。
    /// 注意：实际的 Continue 操作由 confirmation_view 通过 AppEvent 发送，
    /// 这里仅用于接收用户动作并做相应处理。
    pub(crate) fn handle_phase_confirmation_result(&mut self, action: UserAction) {
        match action {
            UserAction::Continue => {
                // Continue 操作已由 confirmation_view 发送 AppEvent 处理
                // 这里可以添加额外的 UI 反馈
                self.chat_widget
                    .add_info_message("正在推进到下一阶段...".to_string(), None);
            }
            UserAction::Supplement(content) => {
                // 补充内容已记录，需要通过子线程重新注入
                self.chat_widget.add_info_message(
                    format!(
                        "📝 补充内容已记录：{}...",
                        if content.len() > 30 {
                            &content[..30]
                        } else {
                            &content
                        }
                    ),
                    None,
                );
                // 实际的补充内容注入通过 AppEvent 由 confirmation_view 发送 Continue 命令触发
            }
        }
    }

    /// 打开 Phase Agent 视图界面。
    pub(crate) fn open_phase_agent_view(&mut self) {
        let Some(manager) = &self.phase_agent_manager else {
            self.chat_widget.add_info_message(
                "没有活跃的 Mission。请先运行 `/zmission start <目标>`".to_string(),
                None,
            );
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
        _workspace: std::path::PathBuf,
        goal: String,
    ) {
        // 发送 ZmissionCommand 事件，让 App 使用新的 Phase Agent 架构处理
        chat_widget
            .send_zmission_command(crate::zmission_command::Command::Start { goal: Some(goal) });
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
