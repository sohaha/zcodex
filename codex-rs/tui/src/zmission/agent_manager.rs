//! Phase Agent 管理器。
//!
//! 负责管理所有阶段子代理的生命周期、状态转换和编排。

use super::PhaseAgent;
use super::PhaseAgentId;
use super::PhaseAgentState;
use super::phase_agent::PhaseAgentMessage;
use super::phase_agent::PhaseAgentMessageSender;
use codex_mission::MissionPhase;
use codex_mission::MissionPlanner;
use codex_mission::MissionState;
use std::path::PathBuf;

/// Phase Agent 管理器，负责编排 7 个阶段的子代理。
#[derive(Debug, Clone)]
pub(crate) struct PhaseAgentManager {
    /// 所有阶段子代理（按阶段顺序存储）
    agents: Vec<PhaseAgent>,
    /// 当前激活的子代理（用户正在交互的）
    active_agent: Option<MissionPhase>,
    /// Mission 目标
    goal: String,
    /// Mission 状态
    mission_state: Option<MissionState>,
    /// Mission Planner
    planner: Option<MissionPlanner>,
    /// 工作区路径
    workspace: PathBuf,
    /// 是否已启动
    started: bool,
    /// 等待主线程空闲后 fork 的阶段
    pending_spawn_phase: Option<MissionPhase>,
}

impl PhaseAgentManager {
    pub(crate) fn new(workspace: PathBuf) -> Self {
        // 为每个阶段创建子代理（按顺序）
        let agents: Vec<_> = MissionPhase::ALL
            .iter()
            .map(|&p| PhaseAgent::new(p))
            .collect();

        Self {
            agents,
            active_agent: None,
            goal: String::new(),
            mission_state: None,
            planner: None,
            workspace,
            started: false,
            pending_spawn_phase: None,
        }
    }

    /// 启动 Mission，激活第一个阶段的子代理。
    pub(crate) fn start_mission(&mut self, goal: String) -> anyhow::Result<PhaseAgentId> {
        let planner = MissionPlanner::for_workspace(&self.workspace);
        let step = planner.start(&goal)?;

        self.goal = goal;
        self.mission_state = Some(step.state);
        self.planner = Some(planner);
        self.started = true;

        // 激活第一个阶段的子代理
        if let Some(phase) = self.current_phase() {
            self.active_agent = Some(phase);
            let idx = Self::agent_index(phase);
            let agent = self.agents.get_mut(idx).unwrap();
            agent.state = PhaseAgentState::Running {
                started_at: std::time::Instant::now(),
                thread_id: None,
            };

            // 添加启动消息
            agent.messages.push_back(PhaseAgentMessage {
                timestamp: std::time::Instant::now(),
                sender: PhaseAgentMessageSender::Orchestrator,
                content: format!(
                    "Mission 已启动，进入 {} 阶段",
                    super::phase_display_name(phase)
                ),
            });

            return Ok(PhaseAgentId::new(phase));
        }

        anyhow::bail!("无法确定当前阶段")
    }

    /// 获取当前阶段。
    pub(crate) fn current_phase(&self) -> Option<MissionPhase> {
        self.mission_state.as_ref()?.phase
    }

    /// 获取当前激活的子代理 ID。
    pub(crate) fn active_agent_id(&self) -> Option<PhaseAgentId> {
        self.active_agent.map(PhaseAgentId::new)
    }

    /// 切换到指定阶段的子代理。
    pub(crate) fn switch_to_agent(&mut self, phase: MissionPhase) -> Option<&PhaseAgent> {
        let idx = Self::agent_index(phase);
        if idx >= self.agents.len() {
            return None;
        }
        self.active_agent = Some(phase);
        self.agents.get(idx)
    }

    /// 获取当前激活的子代理。
    pub(crate) fn active_agent(&self) -> Option<&PhaseAgent> {
        self.active_agent
            .and_then(|p| self.agents.get(Self::agent_index(p)))
    }

    /// 获取当前激活的子代理（可变）。
    pub(crate) fn active_agent_mut(&mut self) -> Option<&mut PhaseAgent> {
        self.active_agent.and_then(|p| {
            let idx = Self::agent_index(p);
            self.agents.get_mut(idx)
        })
    }

    /// 获取指定阶段的子代理。
    pub(crate) fn get_agent(&self, phase: MissionPhase) -> Option<&PhaseAgent> {
        self.agents.get(Self::agent_index(phase))
    }

    /// 获取指定阶段的子代理（可变）。
    pub(crate) fn get_agent_mut(&mut self, phase: MissionPhase) -> Option<&mut PhaseAgent> {
        let idx = Self::agent_index(phase);
        self.agents.get_mut(idx)
    }

    /// 获取所有子代理。
    pub(crate) fn all_agents(&self) -> &[PhaseAgent] {
        &self.agents
    }

    /// 获取指定阶段的子代理索引。
    fn agent_index(phase: MissionPhase) -> usize {
        phase.index()
    }

    /// 当前阶段完成，等待用户确认。
    pub(crate) fn mark_phase_awaiting_confirmation(
        &mut self,
        artifact_preview: String,
    ) -> Option<&PhaseAgent> {
        let phase = self.active_agent?;
        let idx = Self::agent_index(phase);
        let agent = self.agents.get_mut(idx)?;

        agent.state = PhaseAgentState::AwaitingConfirmation {
            completed_at: std::time::Instant::now(),
            artifact_preview,
        };

        agent.messages.push_back(PhaseAgentMessage {
            timestamp: std::time::Instant::now(),
            sender: PhaseAgentMessageSender::Agent,
            content: "阶段分析完成，等待用户确认".to_string(),
        });

        Some(agent)
    }

    /// 用户确认继续，推进到下一阶段。
    pub(crate) fn confirm_and_advance(
        &mut self,
        note: Option<String>,
    ) -> anyhow::Result<Option<PhaseAgentId>> {
        let planner = self
            .planner
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Mission 未启动"))?;

        // 确保当前阶段产物存在
        if let Some(phase) = self.current_phase() {
            planner.ensure_phase_artifact(phase, note.as_deref())?;
        }

        // 推进 Mission
        let step = planner.continue_planning(note)?;
        self.mission_state = Some(step.state.clone());

        // 保存当前阶段为前一阶段
        let prev_phase = self.active_agent;

        // 标记当前阶段完成
        if let Some(phase) = prev_phase {
            let prev_idx = Self::agent_index(phase);
            if let Some(agent) = self.agents.get_mut(prev_idx) {
                let artifact_path = planner.phase_artifact_path(phase);
                agent.state = PhaseAgentState::Completed {
                    completed_at: std::time::Instant::now(),
                    artifact_path,
                };
            }
        }

        // 激活下一阶段
        if let Some(next_phase) = step.state.phase {
            self.active_agent = Some(next_phase);
            let next_idx = Self::agent_index(next_phase);
            let next_agent = self.agents.get_mut(next_idx).unwrap();
            next_agent.state = PhaseAgentState::Running {
                started_at: std::time::Instant::now(),
                thread_id: None,
            };

            let prev_phase_name = prev_phase.map(super::phase_display_name).unwrap_or("当前");

            next_agent.messages.push_back(PhaseAgentMessage {
                timestamp: std::time::Instant::now(),
                sender: PhaseAgentMessageSender::Orchestrator,
                content: format!(
                    "{} 阶段已完成，进入 {} 阶段",
                    prev_phase_name,
                    super::phase_display_name(next_phase)
                ),
            });

            Ok(Some(PhaseAgentId::new(next_phase)))
        } else {
            // 所有阶段完成
            self.active_agent = None;
            Ok(None)
        }
    }

    /// 用户选择补充内容，继续当前阶段。
    pub(crate) fn supplement_and_continue(&mut self, supplement: String) -> Option<&PhaseAgent> {
        let phase = self.active_agent?;
        let idx = Self::agent_index(phase);
        let agent = self.agents.get_mut(idx)?;

        agent.add_supplement(supplement.clone());
        agent.messages.push_back(PhaseAgentMessage {
            timestamp: std::time::Instant::now(),
            sender: PhaseAgentMessageSender::User,
            content: supplement,
        });

        // 状态保持为 Running，子代理继续处理
        agent.state = PhaseAgentState::Running {
            started_at: std::time::Instant::now(),
            thread_id: None,
        };

        Some(agent)
    }

    /// 构建当前阶段的完整提示（包含角色定义和用户补充）。
    pub(crate) fn build_current_prompt(&self) -> Option<String> {
        let phase = self.active_agent?;
        let idx = Self::agent_index(phase);
        let agent = self.agents.get(idx)?;
        let _planner = self.planner.as_ref()?;
        let phase_def = codex_mission::phase_definition(phase);

        let base_prompt = agent.build_phase_prompt(&self.goal, &phase_def);
        Some(agent.build_contextual_prompt(&base_prompt))
    }

    /// 添加消息到当前子代理。
    pub(crate) fn add_message_to_active(
        &mut self,
        sender: PhaseAgentMessageSender,
        content: String,
    ) {
        if let Some(phase) = self.active_agent {
            let idx = Self::agent_index(phase);
            if let Some(agent) = self.agents.get_mut(idx) {
                agent.messages.push_back(PhaseAgentMessage {
                    timestamp: std::time::Instant::now(),
                    sender,
                    content,
                });
            }
        }
    }

    /// 检查是否所有阶段都已完成。
    pub(crate) fn is_all_phases_complete(&self) -> bool {
        self.agents.iter().all(|agent| agent.state.is_completed())
    }

    /// 获取已完成阶段数量。
    pub(crate) fn completed_phases_count(&self) -> usize {
        self.agents
            .iter()
            .filter(|agent| agent.state.is_completed())
            .count()
    }

    /// 获取 Mission 状态摘要。
    pub(crate) fn mission_summary(&self) -> String {
        if !self.started {
            return "Mission 未启动".to_string();
        }

        let completed = self.completed_phases_count();
        let total = MissionPhase::ALL.len();

        if let Some(phase) = self.current_phase() {
            format!(
                "Mission 进行中：{}/{} 阶段完成，当前阶段：{}",
                completed,
                total,
                super::phase_display_name(phase)
            )
        } else if self.is_all_phases_complete() {
            format!("Mission 规划完成：{}/{} 阶段", completed, total)
        } else {
            format!("Mission 状态异常")
        }
    }

    /// 重置 Mission。
    pub(crate) fn reset(&mut self) -> anyhow::Result<()> {
        if let Some(planner) = &self.planner {
            planner.store().reset()?;
        }

        self.agents.clear();
        for phase in MissionPhase::ALL {
            self.agents.push(PhaseAgent::new(phase));
        }

        self.active_agent = None;
        self.goal.clear();
        self.mission_state = None;
        self.planner = None;
        self.started = false;

        Ok(())
    }

    /// 获取当前阶段产物文件路径。
    pub(crate) fn current_artifact_path(&self) -> Option<std::path::PathBuf> {
        let planner = self.planner.as_ref()?;
        let phase = self.active_agent?;
        Some(planner.phase_artifact_path(phase))
    }

    /// 检查当前阶段产物是否存在。
    pub(crate) fn current_artifact_exists(&self) -> bool {
        if let Some(path) = self.current_artifact_path() {
            path.exists() && {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    !content.trim().is_empty()
                } else {
                    false
                }
            }
        } else {
            false
        }
    }

    /// 设置等待 fork 的阶段（用于延迟 fork 机制）。
    pub(crate) fn set_pending_spawn_phase(&mut self, phase: MissionPhase) {
        self.pending_spawn_phase = Some(phase);
    }

    /// 获取并清除等待 fork 的阶段。
    pub(crate) fn take_pending_spawn_phase(&mut self) -> Option<MissionPhase> {
        self.pending_spawn_phase.take()
    }
}

/// 用户确认动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UserConfirmationAction {
    /// 确认完成，继续下一阶段
    Continue,
    /// 需要补充内容
    Supplement,
}
