# ZTeam Mission Autopilot

> 不适用项写 `无`，不要留空。
> 只写已确认事实；若信息不清楚或需要假设，先回到对话澄清，不要把未确认内容写进计划。

## 背景
- 当前状态：
  - `ZTeam Mission V2` 已完成命令面、mission 状态模型、Mission Board 和恢复/override 收口，用户可通过 `/zteam start <goal>` 进入 mission-first 工作台，而不再只看到旧的双 worker 控制台。[.agents/plan/2026-04-24-zteam-mission-v2.md](/workspace/.agents/plan/2026-04-24-zteam-mission-v2.md)
  - 当前实现仍然把“自动创建两个固定 worker”限定在 `/zteam start` 提交的那一条 root user turn 中；后续阶段不会自动继续分派、自动验证、自动 repair，也不会因为后续任务需要而自动补建 worker。[codex-rs/tui/src/app.rs:2057](/workspace/codex-rs/tui/src/app.rs:2057) [codex-rs/tui/src/zteam.rs:914](/workspace/codex-rs/tui/src/zteam.rs:914)
  - worker 阶段结果回流后，系统只会更新 mission 状态和 next action，仍然需要用户手动使用 `/zteam frontend ...`、`/zteam backend ...` 或 relay 才能继续推进下一轮协作。[codex-rs/tui/src/zteam.rs:688](/workspace/codex-rs/tui/src/zteam.rs:688)
  - worker 关闭或缺失时，系统会显式标记 `ReattachRequired` / `Blocked`，但不会自动 attach、自动重建或自动恢复当前 mission cycle，只会提示 `/zteam attach` 或重新 `/zteam start <goal>`。[codex-rs/tui/src/zteam.rs:566](/workspace/codex-rs/tui/src/zteam.rs:566) [codex-rs/tui/src/zteam.rs:740](/workspace/codex-rs/tui/src/zteam.rs:740)
  - 工作台虽已切到 Mission Board，但其本质仍是“显示状态”，不是“持续自动编排协作流程”。当前痛点已经从“产品语义不对”转成“mission 不会自己往下走”。
- 触发原因：
  - 用户明确追问：`/zteam start` 虽然会请求主线程自动建两个固定 agent，但“后续工作需要不会自动建立吗”，并要求继续优化完善该功能。
  - 现有 `Mission V2` 已解决目标、恢复态和 override 统一语义，但没有解决自动闭环，导致日常使用仍需大量手动命令干预。
- 预期影响：
  - 后续实现边界将从“mission-first 的状态展示”推进到“mission-first 的自动编排闭环”。
  - `ZTeam` 的主要价值将从“可视化双 worker 状态”升级为“自动驱动一轮本地协作任务，必要时才允许人工 override”。

## 目标
- 目标结果：
  - 在保留现有固定双 worker 本地 runtime 底座的前提下，为 `ZTeam` 增加一个 `Mission Autopilot` 控制器，使 `/zteam start <goal>` 之后能够自动推进 bootstrapping、worker 注册、cycle 规划、任务分派、结果归纳、验证判断、失败 repair 和完成收口，而不是停在等待用户手动 dispatch。
- 完成定义（DoD）：
  1. 计划明确 `Mission Autopilot` 的状态机、触发点、自动动作类型和停止条件。
  2. 计划明确 autopilot 仍然通过现有 root agent 与 `spawn_agent` / `followup_task` / `send_message` 能力推进，而不是在 TUI 中硬编码业务拆分逻辑。
  3. 计划明确自动派工、自动验证、自动 repair 与手动 override 的优先级边界，避免双语义并存。
  4. 计划明确恢复语义：优先自动 attach，失败时有限重建，并保证只有真实 live attach 才标记 `Live`。
  5. 计划给出可直接进入 issue 生成的阶段拆分、验证入口、风险和回滚边界。
- 非目标：
  - 不把 `ZTeam` 扩展成动态 `N` worker 系统。
  - 不新增第三个 verifier worker。
  - 不新增 app-server 公共 RPC、独立数据库、独立任务队列或跨会话 mission 持久化。
  - 不让裸 `/zteam` 在无用户意图时偷偷自动创建 worker。
  - 不修改 federation protocol 或把外部 peer 引回 `/root/...` 本地协作身份模型。

## 范围
- 范围内：
  - `codex-rs/tui` 中 `ZTeam` 的 mission 自动编排状态机与事件驱动逻辑。
  - 基于现有 root agent 的自动 follow-up prompt 设计：自动规划 cycle、自动分派当前轮任务、自动验证阶段结果、自动 repair 缺失 worker。
  - `ZTeam` workbench 对 autopilot 状态的展示：当前 cycle、waiting on、auto action、repair 状态、manual override 标记。
  - 与 autopilot 一致的 slash help、文档和快照更新。
- 范围外：
  - 改成通用 team orchestration 平台。
  - 允许用户显式配置 worker 数量或角色矩阵。
  - 在 `core` / `protocol` 公共层新增 mission schema。
  - 为 autopilot 引入新的远端服务、后台 daemon 或文件级 scratchpad 协议。

## 影响
- 受影响模块：
  - `codex-rs/tui/src/app.rs`
  - `codex-rs/tui/src/zteam.rs`
  - `codex-rs/tui/src/zteam/view.rs`
  - `codex-rs/tui/src/zteam/recovery.rs`
  - 新增 `codex-rs/tui/src/zteam/*` 子模块 1 到 2 个，用于收纳 autopilot 状态机与 prompt 生成
  - `codex-rs/tui/src/chatwidget/tests/**`
  - `codex-rs/tui/src/**/snapshots/**`
  - `docs/slash_commands.md`
- 受影响接口/命令：
  - `/zteam start <goal>`
  - `/zteam start`
  - `/zteam status`
  - `/zteam attach`
  - `/zteam frontend ...`
  - `/zteam backend ...`
  - `/zteam relay ...`
- 受影响数据/模式：
  - `ZTeam` 共享状态将从“mission state + worker state”扩展为“mission state + autopilot state + worker state”。
  - 新增 autopilot 相关字段，例如：
    - `current_cycle`
    - `waiting_on`
    - `pending_auto_action`
    - `last_auto_action_result`
    - `repair_attempts`
    - `manual_override_active`
  - 不新增独立持久化 schema；状态仍然只存在于当前 TUI 会话与现有线程恢复语义之上。
- 受影响用户界面/行为：
  - 用户执行 `/zteam start <goal>` 后，预期将看到 mission 自动推进，而不是停在“等待你继续 dispatch”。
  - workbench 将新增自动动作、当前等待对象、repair 情况和 override 标记，帮助用户理解为什么系统正在等待、推进或阻塞。

## 约束与依赖
- 约束（时间/兼容性/安全/性能/发布窗口等）：
  - 当前仓库仍需长期同步 upstream，自动编排逻辑必须继续收敛在 `codex-rs/tui/src/zteam*` 局部，不把产品 contract 下压到 `core` / `protocol`。
  - `frontend/backend` 固定 canonical task name 与现有恢复锚点必须保留，除非后续有明确证据表明它们已无法支撑自动 repair。
  - `ZTeam` 当前已有运行中命令门禁；autopilot 不得破坏“主线程已有 turn 进行中时，不允许插入新的用户显式 zteam 操作”的硬边界。
  - 自动动作必须显式、可观察、可中断，不能通过静默 fallback 掩盖失败。
  - 用户可见 TUI 变化必须补 snapshot 覆盖。
  - 当前阶段是 `Cadence Planning`，只生成计划文档，不改代码。
- 外部依赖（系统/人员/数据/权限等）：
  - 无外部人员依赖。
  - 无新增账号、token 或第三方系统依赖。
  - 本轮 `available capabilities` 适配检查结论：
    - 已使用 `llmdoc` 召回稳定文档与既有反思。
    - 已使用 `think` 收敛设计方向。
    - 已使用 `using-cadence` / `cadence-planning` 进入当前流程。
    - 已使用 `ztldr` 与本地只读命令确认当前 `ZTeam` 结构事实。
    - 其余当前会话可用能力对本轮计划起草没有更直接收益，记为 `none-applicable`。

## 实施策略
- 总体方案：
  - 采用“固定双 worker 底座 + Mission Autopilot 控制器”的方案，而不是直接升级到动态 `N` worker。
  - autopilot 只负责“什么时候触发下一步”，不负责“在 TUI 里硬编码任务内容”。具体的 cycle 规划、任务拆分和阶段验证仍然交给 root agent 通过结构化 follow-up prompt 完成。
  - autopilot 将每次自动动作限制为单一职责：
    - `bootstrap_workers`
    - `plan_cycle`
    - `dispatch_cycle`
    - `summarize_results`
    - `repair_workers`
    - `complete_mission`
  - 所有自动动作都必须在 workbench 中可见，并在超时、失败或冲突时进入显式 `Blocked` / `Repairing` 状态，而不是沉默等待。
- 关键决策：
  - 保留固定双 worker，不做动态 roster：
    - 当前最大缺口是自动闭环，而不是 worker 数量。
    - 先验证 autopilot 能否在双 worker 模型中跑通日常任务，再决定是否值得扩到 `N` worker。
  - 自动推进仍通过 root agent：
    - `/zteam start <goal>` 之后的后续步骤由 autopilot 自动触发 root follow-up turn；
    - root agent 负责基于当前 mission 状态决定本轮 assignment、handoff 与 validation。
  - 自动 repair 采用“两段式”：
    - 先自动尝试 attach / 恢复；
    - 只有 attach 失败且达到 repair 条件时，才自动要求 root 按原 canonical task name 重建缺失 worker。
  - 手动命令保留但降级为 interrupting override：
    - 用户手工 `/zteam frontend ...`、`/zteam backend ...`、`/zteam relay ...` 时，autopilot 当前 cycle 必须被显式打断并标记 `manual_override_active`；
    - 后续是否恢复 autopilot，由 root 在下一轮验证或规划中决定。
  - 裸 `/zteam` 继续只打开工作台：
    - 避免把“查看状态”与“隐式启动协作”混在一个入口里，降低入口语义混乱。
- 明确不采用的方案（如有）：
  - 不把下一版做成“打开工作台即自动 spawn”。
  - 不复制 `reference-projects/claude-code-rev` 的完整 agent swarm / task filesystem 模型。
  - 不把 cycle 计划、worker prompt 和验证逻辑直接硬编码为 TUI 内部固定业务规则。
  - 不用静默重试或后台无限拉起掩盖 worker 生命周期失败。

## 阶段拆分
### 阶段一：Autopilot 状态机与自动动作契约
- 目标：
  - 在现有 mission state 之上增加 autopilot state、auto action 类型、等待条件和停止条件。
- 交付物：
  - 明确的 autopilot 状态字段。
  - `Bootstrapping`、`WaitingWorkers`、`PlanningCycle`、`ExecutingCycle`、`ValidatingCycle`、`Repairing`、`Completed` 等状态转移规则。
  - 自动动作与超时/失败语义表。
- 完成条件：
  - 不看代码细节，仅凭状态模型即可判断：系统当前在等谁、下一步是谁触发、失败会去哪。
- 依赖：
  - 已完成的 `Mission V2` mission state。

### 阶段二：Root follow-up prompt 与 cycle 自动推进
- 目标：
  - 为 root agent 设计单职责 follow-up prompt，使 autopilot 能自动规划 cycle、自动分派任务、自动收集并归纳阶段结果。
- 交付物：
  - 各类自动动作的 prompt contract。
  - root turn / follow-up turn 的触发规则。
  - `solo`、`parallel`、`serial_handoff` 三类模式下的 cycle 推进顺序。
- 完成条件：
  - `/zteam start <goal>` 后，无需用户手动 dispatch，也能推进至少一轮完整 cycle。
- 依赖：
  - 阶段一 autopilot 状态机。

### 阶段三：自动验证与 repair 闭环
- 目标：
  - 让 autopilot 在结果回流后自动进入验证，在 worker 缺失时自动进入 repair。
- 交付物：
  - 自动验证触发条件。
  - attach-first repair 策略。
  - 有限重建次数与 blocked 条件。
  - manual override 与 autopilot 的冲突处理规则。
- 完成条件：
  - worker 掉线、只恢复单侧或用户手动插入 override 时，系统仍能显式收口到下一步，而不是停在模糊状态。
- 依赖：
  - 阶段二 cycle 自动推进。

### 阶段四：Workbench、文档与测试收口
- 目标：
  - 把 autopilot 的自动推进语义、repair 状态和 override 状态完整暴露给用户，并用测试锁定行为。
- 交付物：
  - Mission Board 的 autopilot 扩展字段与提示。
  - `docs/slash_commands.md` 的自动推进/repair/override 说明。
  - 相关测试与 snapshot 更新。
- 完成条件：
  - 用户仅看工作台和文档，就能理解系统是否正在自动推进、为什么阻塞、是否需要自己介入。
- 依赖：
  - 阶段一到三的状态和行为 contract。

## 测试与验证
- 核心验证：
  - 本阶段只验证计划质量：回读文件、自审边界、核对引用、确认无占位。
- 必过检查：
  - 计划没有 `TODO`、`TBD`、未闭合占位。
  - 计划明确区分“自动推进时机由 TUI 控制”与“业务任务内容由 root agent 决定”。
  - 计划没有偷偷扩展为动态 `N` worker、公共协议重构或新的服务层。
- 回归验证：
  - 无；当前阶段不改代码。
- 手动检查：
  1. 回读并确认当前问题被准确界定为“没有自动闭环”，而不是再次泛化成 worker 数量问题。
  2. 回读并确认各阶段拆分围绕 autopilot 闭环，而不是回到旧的 slot/status 视角。
  3. 回读并确认自动 repair 仍遵守“只有真实 live attach 才标记 Live”的既有硬边界。
  4. 回读并确认手动 override 的优先级与恢复语义已写清，没有双主路径。
- 未执行的验证（如有）：
  - 未运行任何 Rust 测试；当前阶段为规划文档起草。

## 风险与缓解
- 关键风险：
  - autopilot 触发 root follow-up 的时机不稳，导致重复派工、重复验证或 cycle 乱序。
  - root agent 无法稳定理解 follow-up prompt，导致自动动作挂起或产出错误形态。
  - repair 逻辑误把历史恢复当成 live attach，重新引入 V2 前已经规避的误报问题。
  - manual override 与 autopilot 同时生效，导致工作台显示和实际推进路径分叉。
- 触发信号：
  - 同一个 worker 在同一 cycle 收到重复任务。
  - workbench 显示“自动推进中”，但活动流没有对应 auto action 记录。
  - attach 未真正成功时，UI 仍显示 `Live`。
  - 用户手动 dispatch 后，autopilot 仍继续旧 cycle，没有显式 override 标记。
- 缓解措施：
  - 每个 auto action 都绑定单一 phase、单一 waiting condition 与幂等保护。
  - prompt 设计采用单职责、结构化、不可二义的形式，避免一个 prompt 同时承担 spawn、规划、验证三件事。
  - repair issue 单独锁定“attach-first, live-only”语义测试。
  - 手动 override 一律中断当前 auto action，并在状态中显式记录。
- 回滚/恢复方案（如需要）：
  - 由于本轮仍然只在 `codex-rs/tui/src/zteam*` 局部增加 autopilot 状态机与 UI 逻辑，不引入新持久化和公共协议；若方向错误，可回滚到当前 `Mission V2` 的“mission-first 但手动推进”版本，不需要迁移数据。

## 参考
- [app.rs:2057](/workspace/codex-rs/tui/src/app.rs:2057)
- [zteam.rs:566](/workspace/codex-rs/tui/src/zteam.rs:566)
- [zteam.rs:688](/workspace/codex-rs/tui/src/zteam.rs:688)
- [zteam.rs:740](/workspace/codex-rs/tui/src/zteam.rs:740)
- [zteam.rs:914](/workspace/codex-rs/tui/src/zteam.rs:914)
- [docs/slash_commands.md:32](/workspace/docs/slash_commands.md:32)
- [.agents/plan/2026-04-24-zteam-mission-v2.md](/workspace/.agents/plan/2026-04-24-zteam-mission-v2.md)
- [.agents/issues/2026-04-24-zteam-mission-v2.toml](/workspace/.agents/issues/2026-04-24-zteam-mission-v2.toml)
- [.agents/llmdoc/memory/reflections/2026-04-24-zteam-mission-v2-should-keep-goal-recovery-and-override-in-one-mission-surface.md](/workspace/.agents/llmdoc/memory/reflections/2026-04-24-zteam-mission-v2-should-keep-goal-recovery-and-override-in-one-mission-surface.md)
- [.agents/llmdoc/memory/reflections/2026-04-23-federation-practicality-needs-workflow-productization.md](/workspace/.agents/llmdoc/memory/reflections/2026-04-23-federation-practicality-needs-workflow-productization.md)
