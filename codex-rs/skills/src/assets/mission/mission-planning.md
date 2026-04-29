---
name: mission-planning
description: Guide Mission planning through 7 phases: Intent, Context, Constraints, Architecture, Plan, Worker Definition, and Verification. Use when leading a Mission through its planning workflow to establish clear goals, gather context, confirm constraints, design architecture, create execution plan, define workers, and specify verification strategy.
metadata:
  short-description: Mission planning workflow
---

# Mission Planning

Guide the Mission planning process through 7 structured phases.

## Planning Phases

For each phase, present the phase title and prompt, collect user input, and confirm the exit condition before proceeding.

### Phase 1: Intent (目标澄清)

**Prompt:** 说明 Mission 的目标、成功标准和明确非目标。

**Exit Condition:** 目标与完成定义已足够清晰，可以拆解上下文。

Confirm the goal is clear and well-defined with explicit success criteria and non-goals.

### Phase 2: Context (上下文收集)

**Prompt:** 收集相关代码、文档、约束、既有工作流和历史风险。

**Exit Condition:** 已识别主要事实来源和需要遵守的项目约定.

Identify key information sources, existing conventions, and relevant project context.

### Phase 3: Constraints (约束确认)

**Prompt:** 确认安全、兼容性、验证、时间和范围约束。

**Exit Condition:** 关键约束已转化为后续实现必须满足的不变量。

Document all constraints that must be satisfied during implementation.

### Phase 4: Architecture (方案设计)

**Prompt:** 确定模块边界、数据流、状态所有权和集成点。

**Exit Condition:** 方案足够具体，可以拆成可执行任务。

Design the solution architecture with clear module boundaries and data flow.

### Phase 5: Plan (执行计划)

**Prompt:** 拆解实施顺序、依赖关系、验证入口和回滚边界。

**Exit Condition:** 任务顺序与验收标准明确。

Break down the implementation into ordered tasks with dependencies and acceptance criteria.

### Phase 6: Worker Definition (Worker 定义)

**Prompt:** 定义需要的 worker 类型、职责、输入输出和交接格式。

**Exit Condition:** worker 可以被独立派发并由主流程验收。

Define worker types, their responsibilities, inputs, outputs, and handoff format.

### Phase 7: Verification (验证策略)

**Prompt:** 定义代码审查、自动验证、用户测试和最终交接要求。

**Exit Condition:** 验证链路覆盖主要风险并可复现。

Specify the verification strategy covering code review, automated validation, and user testing.

## Planning Workflow

1. Present each phase sequentially
2. Display phase title, prompt, and exit condition
3. Collect user input for the phase
4. Confirm exit condition is satisfied
5. Record phase result and proceed to next phase
6. After completing all 7 phases, transition to execution

## Phase Records

Each completed phase should be recorded with:
- Phase identifier
- User's note/content for that phase
- Confirmation that exit condition was met

Planning is complete when all 7 phases have recorded results.
