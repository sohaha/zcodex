//! ZMission Phase Agent 子代理系统。
//!
//! 将 Mission 的 7 个规划阶段分配给独立的子代理处理：
//! - 每个阶段有专门的 PhaseAgent 负责
//! - 主代理（Orchestrator）只负责分配和编排
//! - 用户可以在 TUI 中切换不同子代理界面
//! - 阶段完成时弹出确认面板，用户可选择继续或补充内容

mod agent_manager;
mod confirmation_view;
mod phase_agent;
mod phase_view;

pub(crate) use agent_manager::PhaseAgentManager;
pub(crate) use confirmation_view::PhaseConfirmationView;
pub(crate) use confirmation_view::UserAction;
pub(crate) use phase_agent::PhaseAgent;
pub(crate) use phase_agent::PhaseAgentId;
pub(crate) use phase_agent::PhaseAgentRole;
pub(crate) use phase_agent::PhaseAgentState;
pub(crate) use phase_view::PhaseAgentView;

use codex_mission::MissionPhase;

/// 将 MissionPhase 映射到对应的 PhaseAgentRole。
pub(crate) fn phase_to_agent_role(phase: MissionPhase) -> PhaseAgentRole {
    match phase {
        MissionPhase::Intent => PhaseAgentRole::IntentClarifier,
        MissionPhase::Context => PhaseAgentRole::ContextGatherer,
        MissionPhase::Constraints => PhaseAgentRole::ConstraintAnalyzer,
        MissionPhase::Architecture => PhaseAgentRole::Architect,
        MissionPhase::Plan => PhaseAgentRole::Planner,
        MissionPhase::WorkerDefinition => PhaseAgentRole::WorkerDesigner,
        MissionPhase::Verification => PhaseAgentRole::VerificationDesigner,
    }
}

/// 获取阶段的显示名称。
pub(crate) fn phase_display_name(phase: MissionPhase) -> &'static str {
    match phase {
        MissionPhase::Intent => "目标澄清",
        MissionPhase::Context => "上下文收集",
        MissionPhase::Constraints => "约束确认",
        MissionPhase::Architecture => "方案设计",
        MissionPhase::Plan => "执行计划",
        MissionPhase::WorkerDefinition => "Worker 定义",
        MissionPhase::Verification => "验证策略",
    }
}

/// 获取阶段的描述。
pub(crate) fn phase_description(phase: MissionPhase) -> &'static str {
    match phase {
        MissionPhase::Intent => "明确 Mission 的目标、成功标准和非目标",
        MissionPhase::Context => "收集相关代码、文档、约束和历史风险",
        MissionPhase::Constraints => "确认安全、兼容性、验证、时间和范围约束",
        MissionPhase::Architecture => "确定模块边界、数据流、状态所有权和集成点",
        MissionPhase::Plan => "拆解实施顺序、依赖关系、验证入口和回滚边界",
        MissionPhase::WorkerDefinition => "定义需要的 worker 类型、职责、输入输出和交接格式",
        MissionPhase::Verification => "定义代码审查、自动验证、用户测试和最终交接要求",
    }
}
