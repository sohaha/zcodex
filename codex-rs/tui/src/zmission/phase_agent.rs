//! Phase Agent 定义。
//!
//! 每个 Mission 阶段对应一个专门的子代理角色。

use codex_mission::MissionPhase;
use std::collections::VecDeque;

/// 阶段子代理唯一标识。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PhaseAgentId {
    pub phase: MissionPhase,
}

impl PhaseAgentId {
    pub(crate) fn new(phase: MissionPhase) -> Self {
        Self { phase }
    }

    pub(crate) fn label(&self) -> String {
        format!("phase-{}", self.phase.label())
    }
}

impl std::fmt::Display for PhaseAgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label())
    }
}

/// 阶段子代理的角色定义。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PhaseAgentRole {
    /// 目标澄清专家
    IntentClarifier,
    /// 上下文收集专家
    ContextGatherer,
    /// 约束分析专家
    ConstraintAnalyzer,
    /// 架构师
    Architect,
    /// 计划制定专家
    Planner,
    /// Worker 设计专家
    WorkerDesigner,
    /// 验证策略设计专家
    VerificationDesigner,
}

impl PhaseAgentRole {
    pub(crate) fn display_name(&self) -> &'static str {
        match self {
            Self::IntentClarifier => "目标澄清专家",
            Self::ContextGatherer => "上下文收集专家",
            Self::ConstraintAnalyzer => "约束分析专家",
            Self::Architect => "架构师",
            Self::Planner => "计划制定专家",
            Self::WorkerDesigner => "Worker 设计专家",
            Self::VerificationDesigner => "验证策略专家",
        }
    }

    pub(crate) fn role_prompt(&self) -> &'static str {
        match self {
            Self::IntentClarifier => {
                "你是目标澄清专家。你的职责是深入理解用户需求，明确 Mission 的目标、成功标准、非目标范围，\n\
                并确保目标的可执行性和可验证性。输出必须写入 intent.md 文件。"
            }
            Self::ContextGatherer => {
                "你是上下文收集专家。你的职责是全面收集与 Mission 相关的代码库、文档、历史变更、\n\
                项目约定和潜在风险信息。输出必须写入 context.md 文件。"
            }
            Self::ConstraintAnalyzer => {
                "你是约束分析专家。你的职责是识别并明确所有技术约束、安全要求、兼容性需求、\n\
                验证标准、时间限制和范围边界。输出必须写入 constraints.md 文件。"
            }
            Self::Architect => {
                "你是架构师。你的职责是设计系统架构，确定模块边界、数据流、状态所有权、\n\
                集成点和接口契约。输出必须写入 architecture.md 文件。"
            }
            Self::Planner => {
                "你是计划制定专家。你的职责是将架构设计拆解为可执行的任务序列，\n\
                明确依赖关系、验证入口、回滚边界和里程碑。输出必须写入 plan.md 文件。"
            }
            Self::WorkerDesigner => {
                "你是 Worker 设计专家。你的职责是定义需要的 worker 类型、各自职责、\n\
                输入输出格式、交接协议和协作模式。输出必须写入 worker_definition.md 文件。"
            }
            Self::VerificationDesigner => {
                "你是验证策略专家。你的职责是设计完整的验证链路，包括代码审查标准、\n\
                自动化测试、用户验收测试和最终交接要求。输出必须写入 verification.md 文件。"
            }
        }
    }

    pub(crate) fn artifact_filename(&self) -> &'static str {
        match self {
            Self::IntentClarifier => "intent.md",
            Self::ContextGatherer => "context.md",
            Self::ConstraintAnalyzer => "constraints.md",
            Self::Architect => "architecture.md",
            Self::Planner => "plan.md",
            Self::WorkerDesigner => "worker_definition.md",
            Self::VerificationDesigner => "verification.md",
        }
    }
}

/// 阶段子代理的状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PhaseAgentState {
    /// 等待启动
    Pending,
    /// 运行中，正在处理阶段任务
    Running {
        started_at: std::time::Instant,
        thread_id: Option<codex_protocol::ThreadId>,
    },
    /// 等待用户确认（产物已生成）
    AwaitingConfirmation {
        completed_at: std::time::Instant,
        artifact_preview: String,
    },
    /// 已完成
    Completed {
        completed_at: std::time::Instant,
        artifact_path: std::path::PathBuf,
    },
    /// 出错
    Failed {
        error: String,
        failed_at: std::time::Instant,
    },
}

impl Default for PhaseAgentState {
    fn default() -> Self {
        Self::Pending
    }
}

impl PhaseAgentState {
    pub(crate) fn is_active(&self) -> bool {
        matches!(
            self,
            Self::Running { .. } | Self::AwaitingConfirmation { .. }
        )
    }

    pub(crate) fn is_completed(&self) -> bool {
        matches!(self, Self::Completed { .. })
    }

    pub(crate) fn label(&self) -> &'static str {
        match self {
            Self::Pending => "等待中",
            Self::Running { .. } => "运行中",
            Self::AwaitingConfirmation { .. } => "待确认",
            Self::Completed { .. } => "已完成",
            Self::Failed { .. } => "失败",
        }
    }
}

/// 阶段子代理实例。
#[derive(Debug, Clone)]
pub(crate) struct PhaseAgent {
    pub id: PhaseAgentId,
    pub role: PhaseAgentRole,
    pub state: PhaseAgentState,
    /// 关联的子线程 ID（真正的子代理）
    pub thread_id: Option<codex_protocol::ThreadId>,
    /// 该阶段的历史消息（用于上下文隔离）
    pub messages: VecDeque<PhaseAgentMessage>,
    /// 用户补充的内容（用于继续沟通）
    pub user_supplements: Vec<String>,
}

impl PhaseAgent {
    pub(crate) fn new(phase: MissionPhase) -> Self {
        let id = PhaseAgentId::new(phase);
        let role = super::phase_to_agent_role(phase);
        Self {
            id,
            role,
            state: PhaseAgentState::Pending,
            thread_id: None,
            messages: VecDeque::new(),
            user_supplements: Vec::new(),
        }
    }

    pub(crate) fn display_name(&self) -> String {
        format!("{} [{}]", self.role.display_name(), self.id.phase.label())
    }

    pub(crate) fn build_phase_prompt(
        &self,
        goal: &str,
        phase_def: &codex_mission::MissionPhaseDefinition,
    ) -> String {
        let role_prompt = self.role.role_prompt();
        let artifact_file = self.role.artifact_filename();

        format!(
            "{role_prompt}\n\n\
            Mission 目标：{goal}\n\n\
            当前阶段：{phase_title} ({phase_label})\n\
            阶段提示：{prompt}\n\
            出口条件：{exit_condition}\n\n\
            **重要：** 请将本阶段的分析结果写入产物文件：`{artifact_file}`\n\
            产物文件必须存在且非空，否则无法推进到下一阶段。\n\n\
            请根据上述信息完成当前阶段的分析。",
            role_prompt = role_prompt,
            goal = goal,
            phase_title = phase_def.title,
            phase_label = phase_def.phase.label(),
            prompt = phase_def.prompt,
            exit_condition = phase_def.exit_condition,
            artifact_file = artifact_file,
        )
    }

    /// 添加用户补充内容。
    pub(crate) fn add_supplement(&mut self, content: String) {
        self.user_supplements.push(content);
    }

    /// 获取完整的上下文提示（包含所有补充）。
    pub(crate) fn build_contextual_prompt(&self, base_prompt: &str) -> String {
        if self.user_supplements.is_empty() {
            return base_prompt.to_string();
        }

        let supplements = self
            .user_supplements
            .iter()
            .enumerate()
            .map(|(i, s)| format!("{}. {}", i + 1, s))
            .collect::<Vec<_>>()
            .join("\n");

        format!(
            "{base_prompt}\n\n\
            **用户补充内容：**\n{supplements}\n\n\
            请基于以上补充继续完善当前阶段的分析。",
            base_prompt = base_prompt,
            supplements = supplements,
        )
    }
}

/// 阶段子代理消息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PhaseAgentMessage {
    pub timestamp: std::time::Instant,
    pub sender: PhaseAgentMessageSender,
    pub content: String,
}

/// 消息发送者。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PhaseAgentMessageSender {
    Orchestrator,
    Agent,
    User,
}
