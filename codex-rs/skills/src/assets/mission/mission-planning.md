---
name: mission-planning
description: >
  Mission 规划流程：7 阶段强制执行。触发后必须从 Phase 1 开始，逐阶段完成并记录产物，
  禁止跳过任何阶段。Phase 5 完成前禁止执行任何代码变更。
  当任务涉及多文件改动、UI 行为变更、状态机修改、跨模块重构、新功能开发时触发。
metadata:
  short-description: Mission 7 阶段强制规划
  enforcement: strict
  triggers:
    - 多文件改动（≥3 个文件）
    - UI/UX 行为变更
    - 状态机或生命周期修改
    - 跨模块重构
    - 新功能开发
    - 架构决策
---

# Mission Planning — 强制执行

## 入口守卫（Entry Guard）

**进入 Mission 模式时，以下规则立即生效：**

1. **必须从 Phase 1 开始** — 禁止跳到任何后续阶段
2. **必须逐阶段推进** — 完成当前阶段并记录产物后才能进入下一阶段
3. **Phase 5 完成前禁止执行代码** — 任何 `apply_patch`、`shell_command` 修改源码的操作被禁止
4. **用户显式覆盖除外** — 仅当用户明确说"跳过规划"或"直接执行"时，才可绕过

**违反入口守卫时的处理：**
- 如果 agent 尝试在 Phase 5 前执行代码变更，必须阻止并提示"请先完成 Mission 规划"
- 如果 agent 尝试跳过阶段，必须阻止并提示"请先完成当前阶段"

## 阶段守卫（Phase Gate）

**每个阶段必须满足以下条件才能推进到下一阶段：**

1. **产物文件已创建** — 阶段对应的 artifact 文件必须存在且非空
2. **出口条件已确认** — agent 必须明确声明出口条件已满足
3. **用户已确认** — 用户必须确认阶段结果

**阶段产物要求：**

| 阶段 | 产物文件 | 出口条件 |
|------|----------|----------|
| Phase 1: Intent | `.agents/mission/plans/intent.md` | 目标与完成定义已足够清晰 |
| Phase 2: Context | `.agents/mission/plans/context.md` | 已识别主要事实来源和项目约定 |
| Phase 3: Constraints | `.agents/mission/plans/constraints.md` | 关键约束已转化为不变量 |
| Phase 4: Architecture | `.agents/mission/plans/architecture.md` | 方案足够具体，可拆成可执行任务 |
| Phase 5: Plan | `.agents/mission/plans/plan.md` | 任务顺序与验收标准明确 |
| Phase 6: Worker Definition | `.agents/mission/plans/worker_definition.md` | worker 可被独立派发并验收 |
| Phase 7: Verification | `.agents/mission/plans/verification.md` | 验证链路覆盖主要风险 |

## 执行禁令（Execution Ban）

**Phase 5 完成前，以下操作被禁止：**

- 修改任何源码文件（`.rs`、`.py`、`.js`、`.ts` 等）
- 创建新的源码文件
- 执行 `cargo build`、`cargo test`、`npm run` 等构建/测试命令
- 调用 worker 执行实现任务

**允许的操作：**

- 读取源码文件以收集上下文
- 执行搜索命令（`rg`、`grep`、`find`）
- 创建和编辑 `.agents/mission/plans/` 下的规划文档
- 与用户讨论和确认

## 阶段流程

### Phase 1: Intent（目标澄清）

**Prompt:** 说明 Mission 的目标、成功标准和明确非目标。

**Exit Condition:** 目标与完成定义已足够清晰，可以拆解上下文。

**产物要求：**
- 明确的目标陈述
- 可衡量的成功标准
- 明确的非目标

### Phase 2: Context（上下文收集）

**Prompt:** 收集相关代码、文档、约束、既有工作流和历史风险。

**Exit Condition:** 已识别主要事实来源和需要遵守的项目约定。

**产物要求：**
- 相关文件列表
- 现有约定和规范
- 历史风险和注意事项

### Phase 3: Constraints（约束确认）

**Prompt:** 确认安全、兼容性、验证、时间和范围约束。

**Exit Condition:** 关键约束已转化为后续实现必须满足的不变量。

**产物要求：**
- 安全约束
- 兼容性约束
- 验证约束
- 范围约束

### Phase 4: Architecture（方案设计）

**Prompt:** 确定模块边界、数据流、状态所有权和集成点。

**Exit Condition:** 方案足够具体，可以拆成可执行任务。

**产物要求：**
- 模块边界
- 数据流
- 状态所有权
- 集成点

### Phase 5: Plan（执行计划）

**Prompt:** 拆解实施顺序、依赖关系、验证入口和回滚边界。

**Exit Condition:** 任务顺序与验收标准明确。

**产物要求：**
- 任务拆解
- 依赖关系
- 验收标准
- 回滚边界

**Phase 5 完成后，执行禁令解除，可以开始执行代码变更。**

### Phase 6: Worker Definition（Worker 定义）

**Prompt:** 定义需要的 worker 类型、职责、输入输出和交接格式。

**Exit Condition:** worker 可以被独立派发并由主流程验收。

**产物要求：**
- Worker 类型定义
- 职责边界
- 输入输出格式
- 交接格式

### Phase 7: Verification（验证策略）

**Prompt:** 定义代码审查、自动验证、用户测试和最终交接要求。

**Exit Condition:** 验证链路覆盖主要风险并可复现。

**产物要求：**
- 代码审查策略
- 自动验证策略
- 用户测试策略
- 最终交接要求

## 状态跟踪

**Mission 状态必须持久化：**

- 使用 `.agents/mission/state.json` 跟踪当前阶段
- 每次阶段推进时更新状态文件
- 状态文件格式：`{"phase": "intent", "completed_phases": []}`

## 异常处理

**如果用户要求跳过规划：**
- 必须明确确认："用户要求跳过 Mission 规划，直接执行"
- 记录跳过原因
- 执行禁令解除

**如果用户要求跳过某个阶段：**
- 必须明确确认："用户要求跳过 Phase X"
- 记录跳过原因
- 跳过的阶段产物标记为 `skipped`

**如果 agent 判断任务不需要 Mission：**
- 必须在开始前明确声明："此任务不需要 Mission 规划，直接执行"
- 仅适用于单文件改动、简单修复、配置变更等琐碎任务
