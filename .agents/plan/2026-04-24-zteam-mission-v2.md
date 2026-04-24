# ZTeam Mission V2

> 不适用项写 `无`，不要留空。
> 只写已确认事实；若信息不清楚或需要假设，先回到对话澄清，不要把未确认内容写进计划。

## 背景
- 当前状态：
  - 现有 `ZTeam` 已经在 `tui` 中落地为固定双 worker 的本地协作入口，公开命令面是 `/zteam start|status|attach|<frontend|backend>|relay`，并以 `frontend/backend` 作为固定 slot 与用户心智。[docs/slash_commands.md](/workspace/docs/slash_commands.md)
  - 当前实现的核心能力集中在命令解析、worker 注册/恢复、手动分派、worker 间 relay、最近结果回流和 workbench 展示，而不是面向目标的自动编排。[codex-rs/tui/src/zteam.rs](/workspace/codex-rs/tui/src/zteam.rs#L84) [codex-rs/tui/src/zteam.rs](/workspace/codex-rs/tui/src/zteam.rs#L341) [codex-rs/tui/src/zteam/view.rs](/workspace/codex-rs/tui/src/zteam/view.rs#L46)
  - `/zteam start` 当前只是向主线程提交一条 prompt，要求后续调用 `spawn_agent` 创建两个长期 worker，并不会同时生成目标、阶段、验收或下一步编排。[codex-rs/tui/src/zteam.rs](/workspace/codex-rs/tui/src/zteam.rs#L730) [codex-rs/tui/src/app.rs](/workspace/codex-rs/tui/src/app.rs#L2057)
  - 现有实现已经具备可复用的本地 runtime 底座：固定 worker 身份锚点、`ThreadStarted/ItemCompleted/ThreadClosed` 通知回流、`InterAgentCommunication` 路由、`attach` 恢复和底部工作台持续渲染 surface。[codex-rs/tui/src/zteam.rs](/workspace/codex-rs/tui/src/zteam.rs#L406) [codex-rs/tui/src/zteam/recovery.rs](/workspace/codex-rs/tui/src/zteam/recovery.rs#L51) [.agents/llmdoc/memory/reflections/2026-04-23-zteam-worker-routing-should-bind-fixed-task-names-to-thread-notifications.md](/workspace/.agents/llmdoc/memory/reflections/2026-04-23-zteam-worker-routing-should-bind-fixed-task-names-to-thread-notifications.md)
  - 已有稳定反思已指出：当前 `ZTeam` 更像本地双 worker 控制台，真正缺的是工作流产品化，而不是继续扩 federation/runtime 原语。[.agents/llmdoc/memory/reflections/2026-04-23-federation-practicality-needs-workflow-productization.md](/workspace/.agents/llmdoc/memory/reflections/2026-04-23-federation-practicality-needs-workflow-productization.md)
- 触发原因：
  - 用户明确指出“固定双 worker 底座在实际使用中意义不大”，并要求基于现状对 `ZTeam` 做 `Mission V2` 深度设计，目标是从“手动控制台”升级到“面向任务目标的协作工作流”。
  - 用户随后显式要求使用 `Cadence`，因此当前需要先把 `Mission V2` 设计沉淀为新的规划文件，再进入 issue 生成。
- 预期影响：
  - 后续实现边界将从“继续强化 frontend/backend 双 worker 控制台”切换到“mission-first 的本地协作模式”。
  - 旧 `ZTeam` 运行时能力会被保留并重用，但不再作为主要用户契约；用户主路径将从手动 dispatch/relay 转向输入目标、观察阶段、按需干预。

## 目标
- 目标结果：
  - 定义并推进一个新的 `ZTeam Mission V2`：用户以目标启动 `ZTeam`，系统在现有本地双线程底座上自动决定单 worker、双 worker、串行交接或并行执行，并在同一个 workbench 中持续呈现目标、阶段、验收、阻塞、结果回流和下一步建议。
- 完成定义（DoD）：
  1. 计划明确 `Mission V2` 的用户契约、主命令面、状态机、workbench 信息架构和恢复语义。
  2. 计划明确现有 `frontend/backend` 只保留为内部 slot/runtime 实现细节，不再作为主产品心智。
  3. 计划明确 V2 不新增公共协议、不扩 federation 身份域、不引入 N worker 动态 roster，而是在现有 `tui` 局部收口。
  4. 计划给出可以直接进入 issue 生成的阶段拆分、验证入口、风险和回滚边界。
  5. 计划对兼容路径做出清晰规定：旧 `/zteam frontend|backend|relay` 保留但降级为高级手动干预能力。
- 非目标：
  - 不在本阶段直接修改代码。
  - 不做通用 Team/Task 平台，不引入独立任务数据库或跨会话 mission 持久化。
  - 不做动态 `N` worker roster。
  - 不新增 verifier 第三 worker 或新的 app-server 公共 RPC。
  - 不重做 federation protocol、daemon、client，也不把外部 peer 重新并入 `/root/...` 身份空间。

## 范围
- 范围内：
  - `codex-rs/tui` 中 `ZTeam` 的命令面、状态模型、workbench 信息架构和恢复逻辑重构。
  - 以现有本地双 worker runtime 为底座，新增 mission 层：目标、模式、阶段、assignment、acceptance checks、validation summary、blocker、next action。
  - 对现有 `docs/slash_commands.md`、`docs/config.md` 和相关帮助文案进行产品语义更新。
  - 定义向后兼容策略，使旧手动命令继续可用但不再是推荐主路径。
- 范围外：
  - 新增第三个长期 worker。
  - 把 `ZTeam` 扩展成完整任务管理系统、PR 管家或自动提交流水线。
  - 修改 `codex-core` 的多智能体公共工具语义。
  - 引入新的服务、数据库、远端依赖或账号配置。

## 影响
- 受影响模块：
  - `codex-rs/tui/src/zteam.rs`
  - `codex-rs/tui/src/zteam/view.rs`
  - `codex-rs/tui/src/zteam/recovery.rs`
  - `codex-rs/tui/src/app.rs`
  - `codex-rs/tui/src/slash_command.rs`
  - `codex-rs/tui/src/bottom_pane/chat_composer.rs`
  - `codex-rs/tui/src/chatwidget/tests/slash_commands.rs`
  - `codex-rs/tui` 对应 snapshot
  - `docs/slash_commands.md`
  - `docs/config.md`
- 受影响接口/命令：
  - `/zteam`
  - `/zteam start <goal>` 作为新的推荐主路径
  - `/zteam start` 保留兼容，但降级为“只启动协作者”的高级入口
  - `/zteam status`
  - `/zteam attach`
  - `/zteam frontend ...`
  - `/zteam backend ...`
  - `/zteam relay ...`
- 受影响数据/模式：
  - `ZTeam` 共享状态将从“连接状态 + 最近任务/结果”扩展为 mission state。
  - 不新增独立持久化数据模式；恢复仍优先基于现有线程历史和本地状态。
  - `tui.zteam_enabled` 配置项继续保留并作为行为真相源。
- 受影响用户界面/行为：
  - 用户不再先理解“两个固定前后端 worker”，而是先输入目标，再观察系统拆分和验证。
  - workbench 重点从“slot 状态/relay 记录”转向“Mission / Acceptance / Assignments / Validation / Activity”。

## 约束与依赖
- 约束（时间/兼容性/安全/性能/发布窗口等）：
  - 当前仓库需要持续同步 upstream，新增能力必须继续收敛在 `tui` 局部，不把产品语义下压到 `core`/`protocol` 公共层。
  - 现有 `ZTeam` 运行时和入口已存在，V2 不能用“彻底推翻再造”的方式扩大风险；应优先渐进重构。
  - 运行中命令门禁已存在，V2 仍需保持“主线程执行中只允许查看状态，不允许插入新的编排动作”的约束，除非后续 issue 明确调整。
  - 用户可见 TUI 变化必须补 snapshot 覆盖。
  - 当前阶段是 `Cadence Planning`，只生成计划文档，不改代码。
- 外部依赖（系统/人员/数据/权限等）：
  - 无外部人员依赖。
  - 本轮 `available capabilities` 适配检查结论：
    - 已使用 `llmdoc` 提取稳定文档与反思事实。
    - 已使用 `think` 完成前置设计分歧收敛。
    - 已使用 `using-cadence` / `cadence-planning` 进入当前流程。
    - 已使用 `ztldr` 和本地只读命令提取 `zteam` 结构证据。
    - 其余当前会话可用 skill/MCP/能力对本轮计划起草没有更直接收益，记为 `none-applicable`。

## 实施策略
- 总体方案：
  - 把 `ZTeam` 定义成 `Mission-first, dual-runtime-second` 的本地协作模式。
  - 用户层以目标驱动：`/zteam start <goal>` 成为唯一推荐入口。
  - 编排层由 root 维护 mission brief，并决定本轮模式：`solo`、`parallel`、`serial_handoff`、`blocked`。
  - 运行时层继续复用现有两个 worker 线程、恢复、通知和 `InterAgentCommunication` 路由，不新增公共协议。
  - 旧命令保留为高级 override，不再承担默认工作流。
- 关键决策：
  - 保留双线程底座，但隐藏固定前后端角色：
    - `frontend/backend` 继续作为内部 slot 和恢复锚点；
    - UI 主视图改为 `Worker A / Worker B + 当前职责`，避免产品语义继续锁死在 web full-stack 场景。
  - 引入 mission 状态机：
    - `Idle`
    - `Bootstrapping`
    - `Planning`
    - `Executing`
    - `Validating`
    - `Blocked`
    - `Completed`
  - 引入 mission 对象字段：
    - `goal`
    - `mode`
    - `phase`
    - `acceptance_checks`
    - `worker_a_role`
    - `worker_b_role`
    - `worker_a_assignment`
    - `worker_b_assignment`
    - `worker_a_last_result`
    - `worker_b_last_result`
    - `validation_summary`
    - `blocker`
    - `next_action`
    - `cycle`
  - planner 允许显式降级到单 worker mission，避免为了双 worker 而双 worker。
  - validation 由 root 汇总，不引入第三个 verifier worker。
  - `dispatch/relay` 降级为 override，保留兼容但不再是推荐路径。
- 明确不采用的方案（如有）：
  - 不把 V2 做成动态 `N` worker 系统。
  - 不沿着 `claude-code-rev` 直接复制完整 Team/Task 文件系统。
  - 不新增 `Mission` 独立持久化文件或数据库。
  - 不要求 worker 数量成为用户一等概念。
  - 不继续把 V2 的主要价值定义为“更方便地手动 relay”。

## 阶段拆分
### 阶段一：命令面与产品语义切换
- 目标：
  - 将 `ZTeam` 主路径切换为 `mission-first`，明确新旧命令的角色。
- 交付物：
  - 新的 `/zteam` 用法与帮助文案。
  - `/zteam start <goal>` 主路径接线设计。
  - 旧命令兼容与降级策略。
- 完成条件：
  - 用户层面不再把 `ZTeam` 理解为“固定前后端双 worker 控制台”。
- 依赖：
  - 当前已确认的命令面和帮助文档事实。

### 阶段二：Mission 状态模型与 planner
- 目标：
  - 在现有共享状态之上增加 mission 对象、phase 和 mode 语义。
- 交付物：
  - mission 状态模型。
  - planner 决策规则。
  - assignment 与 acceptance checks 结构。
- 完成条件：
  - root 可以根据目标决定 `solo` / `parallel` / `serial_handoff` / `blocked`，且状态转移清晰。
- 依赖：
  - 阶段一已冻结的命令与产品边界。

### 阶段三：Workbench V2 与验证回环
- 目标：
  - 把 workbench 从“slot 状态板”升级为“Mission board”。
- 交付物：
  - `Mission`
  - `Acceptance`
  - `Worker Assignments`
  - `Validation`
  - `Activity`
    这 5 个面板的信息架构与渲染策略。
- 完成条件：
  - 用户能在一个工作台中看见目标、阶段、职责、结果、阻塞和下一步建议，而不是只看连接状态。
- 依赖：
  - 阶段二的 mission 状态模型。

### 阶段四：恢复、降级与兼容命令收口
- 目标：
  - 在保留现有 recovery 底座的同时，补足 mission 恢复、单 worker 降级和人工 override 行为。
- 交付物：
  - `attach` 的 V2 语义。
  - 恢复不完整时的 `Blocked` / `Degraded` 规则。
  - 旧命令和新状态模型的兼容说明。
- 完成条件：
  - 重开 TUI、只恢复一个 worker 或缺失本轮 assignment 时，系统仍能给出明确状态而不误报“已就绪”。
- 依赖：
  - 阶段三的 workbench 和阶段二的 mission 状态机。

## 测试与验证
- 核心验证：
  - 本阶段只验证计划质量：回读文件、自审边界、核对引用、确认无占位。
- 必过检查：
  - 计划中没有 `TODO`、`TBD`、未闭合占位。
  - 计划明确区分“内部双线程底座”和“用户可见 mission 契约”。
  - 计划没有偷偷扩大到动态 `N` worker、公共协议重构或完整 Team/Task 平台。
- 回归验证：
  - 无；当前阶段不改代码。
- 手动检查：
  1. 回读并确认命令面已从“手动控制台”转向“目标驱动”。
  2. 回读并确认状态机、mission 字段和兼容策略都已写死，没有关键歧义。
  3. 回读并确认影响范围仍主要落在 `codex-rs/tui` 与文档。
  4. 回读并确认旧 `frontend/backend` 已降为内部实现细节，而不是产品主语义。
- 未执行的验证（如有）：
  - 未运行任何 Rust 测试；当前阶段为规划文档起草。

## 风险与缓解
- 关键风险：
  - 计划虽然改成 `mission-first`，实现时却仍沿用旧的 slot/status 视角，导致 UI 文案变了但用户价值没变。
  - 为了支持 planner/validation，代码过度下沉到 `core` 或额外造协议，扩大 upstream 冲突面。
  - 兼容旧命令时出现“双语义共存”：新旧路径都像主路径，用户继续困惑。
  - 恢复逻辑误把“最近状态恢复”展示成“live attach 成功”，导致误判可继续分派。
- 触发信号：
  - issue 分解仍然把 `relay`、`dispatch`、`worker 注册状态` 当成核心成果，而不是把 mission planner/workbench 作为主结果。
  - 设计开始引入新的公共 schema、daemon 生命周期或额外 worker。
  - 文档和帮助文本仍优先展示 `/zteam frontend ...`、`/zteam relay ...` 的例子。
  - 恢复文案继续把部分恢复说成“已就绪”。
- 缓解措施：
  - 在 issue 生成时按“命令面 -> mission 状态 -> workbench -> 恢复兼容”顺序拆分，避免回到旧实现中心。
  - 保持所有新增数据都先落在 `tui/src/zteam*` 局部模块。
  - 明确要求主文档、slash help、snapshot 一起更新，防止产品语义漂移。
  - 在恢复 issue 中单独要求“只能在真实 live attach 成功时标记 Live”。
- 回滚/恢复方案（如需要）：
  - 由于 V2 仍基于现有 `ZTeam` 局部模块演进，不引入新持久化和公共协议；若方向错误，可回滚到当前固定双 worker 工作台实现，不需要迁移数据。

## 参考
- [docs/slash_commands.md](/workspace/docs/slash_commands.md)
- [docs/config.md:63](/workspace/docs/config.md#L63)
- [codex-rs/tui/src/slash_command.rs:85](/workspace/codex-rs/tui/src/slash_command.rs#L85)
- [codex-rs/tui/src/zteam.rs:37](/workspace/codex-rs/tui/src/zteam.rs#L37)
- [codex-rs/tui/src/zteam.rs:84](/workspace/codex-rs/tui/src/zteam.rs#L84)
- [codex-rs/tui/src/zteam.rs:341](/workspace/codex-rs/tui/src/zteam.rs#L341)
- [codex-rs/tui/src/zteam.rs:406](/workspace/codex-rs/tui/src/zteam.rs#L406)
- [codex-rs/tui/src/zteam.rs:648](/workspace/codex-rs/tui/src/zteam.rs#L648)
- [codex-rs/tui/src/zteam.rs:730](/workspace/codex-rs/tui/src/zteam.rs#L730)
- [codex-rs/tui/src/zteam/view.rs:46](/workspace/codex-rs/tui/src/zteam/view.rs#L46)
- [codex-rs/tui/src/zteam/recovery.rs:51](/workspace/codex-rs/tui/src/zteam/recovery.rs#L51)
- [codex-rs/tui/src/app.rs:2057](/workspace/codex-rs/tui/src/app.rs#L2057)
- [.agents/plan/2026-04-23-zteam-tui-collaboration-mode.md](/workspace/.agents/plan/2026-04-23-zteam-tui-collaboration-mode.md)
- [.agents/llmdoc/memory/reflections/2026-04-23-federation-practicality-needs-workflow-productization.md](/workspace/.agents/llmdoc/memory/reflections/2026-04-23-federation-practicality-needs-workflow-productization.md)
- [.agents/llmdoc/memory/reflections/2026-04-23-zteam-worker-routing-should-bind-fixed-task-names-to-thread-notifications.md](/workspace/.agents/llmdoc/memory/reflections/2026-04-23-zteam-worker-routing-should-bind-fixed-task-names-to-thread-notifications.md)
- [.agents/llmdoc/memory/reflections/2026-04-23-zteam-workbench-should-use-refreshable-bottom-pane-surface.md](/workspace/.agents/llmdoc/memory/reflections/2026-04-23-zteam-workbench-should-use-refreshable-bottom-pane-surface.md)
- [.agents/llmdoc/memory/reflections/2026-04-24-zteam-docs-should-split-config-entry-from-command-workflow-and-real-cases.md](/workspace/.agents/llmdoc/memory/reflections/2026-04-24-zteam-docs-should-split-config-entry-from-command-workflow-and-real-cases.md)
