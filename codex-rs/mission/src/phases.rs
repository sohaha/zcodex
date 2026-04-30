use crate::MissionPhase;

/// 描述一个 Mission 规划阶段的出口条件与提示。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MissionPhaseDefinition {
    pub phase: MissionPhase,
    pub title: &'static str,
    pub prompt: &'static str,
    pub exit_condition: &'static str,
    /// 阶段产物文件名（相对于 `.agents/mission/plans/` 目录）。
    /// 推进到下一阶段时，校验此文件必须存在且非空。
    pub artifact_filename: &'static str,
}

pub const PHASE_DEFINITIONS: [MissionPhaseDefinition; 7] = [
    MissionPhaseDefinition {
        phase: MissionPhase::Intent,
        title: "目标澄清",
        prompt: "说明 Mission 的目标、成功标准和明确非目标。",
        exit_condition: "目标与完成定义已足够清晰，可以拆解上下文。",
        artifact_filename: "intent.md",
    },
    MissionPhaseDefinition {
        phase: MissionPhase::Context,
        title: "上下文收集",
        prompt: "收集相关代码、文档、约束、既有工作流和历史风险。",
        exit_condition: "已识别主要事实来源和需要遵守的项目约定。",
        artifact_filename: "context.md",
    },
    MissionPhaseDefinition {
        phase: MissionPhase::Constraints,
        title: "约束确认",
        prompt: "确认安全、兼容性、验证、时间和范围约束。",
        exit_condition: "关键约束已转化为后续实现必须满足的不变量。",
        artifact_filename: "constraints.md",
    },
    MissionPhaseDefinition {
        phase: MissionPhase::Architecture,
        title: "方案设计",
        prompt: "确定模块边界、数据流、状态所有权和集成点。",
        exit_condition: "方案足够具体，可以拆成可执行任务。",
        artifact_filename: "architecture.md",
    },
    MissionPhaseDefinition {
        phase: MissionPhase::Plan,
        title: "执行计划",
        prompt: "拆解实施顺序、依赖关系、验证入口和回滚边界。",
        exit_condition: "任务顺序与验收标准明确。",
        artifact_filename: "plan.md",
    },
    MissionPhaseDefinition {
        phase: MissionPhase::WorkerDefinition,
        title: "Worker 定义",
        prompt: "定义需要的 worker 类型、职责、输入输出和交接格式。",
        exit_condition: "worker 可以被独立派发并由主流程验收。",
        artifact_filename: "worker_definition.md",
    },
    MissionPhaseDefinition {
        phase: MissionPhase::Verification,
        title: "验证策略",
        prompt: "定义代码审查、自动验证、用户测试和最终交接要求。",
        exit_condition: "验证链路覆盖主要风险并可复现。",
        artifact_filename: "verification.md",
    },
];

pub fn phase_definition(phase: MissionPhase) -> &'static MissionPhaseDefinition {
    PHASE_DEFINITIONS
        .iter()
        .find(|definition| definition.phase == phase)
        .expect("所有 MissionPhase 都必须有定义")
}
