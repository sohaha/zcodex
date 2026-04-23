# ZTeam TUI 协作模式

> 不适用项写 `无`，不要留空。
> 只写已确认事实；若信息不清楚或需要假设，先回到对话澄清，不要把未确认内容写进计划。

## 背景
- 当前状态：
  - 现有 federation 已经接入 `tui -> app-server thread/start -> federation_bridge`，TUI 侧可通过 `--federation-enable` 等参数透传 `FederationThreadStartParams`，但 bridge 目前仍只处理 `TextTask/TextResult` 这一层原语。
  - 现有本地多智能体链路已经具备 `spawn_agent`、`send_message`、`followup_task`，并通过 `InterAgentCommunication` 支持同一 root tree 内的持续通信与中途打断。
  - 当前仓库需要定期同步 upstream `openai/codex`，因此新能力应尽量做成局部、可收敛的本地分叉，而不是扩大对 `core`、`protocol`、`app-server` 公共语义的改写面。
  - 用户已明确：federation 属于当前仓库的本地特性能力，因此后续设计可以按本地特性演进，但仍应把改动收敛在本地特性边界内，降低和 upstream 主干的冲突面。
- 触发原因：
  - 用户希望真正可用的“一个 Codex 写前端、一个 Codex 写后端、过程中可随时沟通”的协作模式，而不是停留在 federation daemon 的调试命令和一次性 task/result 投递。
  - 用户明确要求本轮按 `Cadence` 推进，并把重点放在 `tui` 项目，同时尽量不改动 upstream 已有功能面。
- 预期影响：
  - 形成一份可进入 `cadence-issue-generation` 的设计计划，把功能重心从“继续重写 federation runtime”收敛为“TUI 主导的协作模式”，并为后续 issue 拆分提供明确边界。

## 目标
- 目标结果：
  - 定义一个以 `TUI` 为主入口、命名为 `ZTeam` 的协作模式：主线程在一个工作台内协调 `frontend` 与 `backend` 两个 worker，支持任务分派、进度可见、中途通信和结果回流。
- 完成定义（DoD）：
  1. 计划明确 `ZTeam` 的产品定位、命名、MVP 范围与非目标。
  2. 计划明确 `ZTeam` 优先复用现有 `multi_agent_v2` 与现有 federation bridge，而不是先改底层身份语义。
  3. 计划明确 `ZTeam` 具备配置开关，且默认开启。
  4. 计划明确实现以 `codex-rs/tui` 为主，尽量把新增逻辑放在新的 `zteam` 模块中，避免膨胀 `chatwidget.rs`、`bottom_pane/mod.rs`、`app-server` 公共协议面。
  5. 计划给出后续 issue 可直接使用的阶段拆分、验证入口、风险与回退边界。
- 非目标：
  - 不在本阶段直接写代码。
  - 不把 federated peer 塞回 `/root/...`、`AgentPath` 或 `SessionSource::SubAgent`。
  - 不在 MVP 阶段先做跨机器 federation、复杂 Mission 编排、自动合并、复杂权限或富项目管理系统。
  - 不为了 `ZTeam` 先重写现有 federation protocol、daemon 命令面或 app-server v2 公共 schema。

## 范围
- 范围内：
  - `codex-rs/tui` 中新增 `ZTeam` 模式入口、工作台、worker 视图和协作状态模型。
  - 以现有 `spawn_agent` / `send_message` / `followup_task` 为主路径，完成本地双 worker 协作 MVP。
  - 为未来接入现有 federation bridge 预留 adapter seam，使 `ZTeam` 后续可以挂载外部 worker，但这不是本轮 MVP 的阻塞项。
  - 形成与 `ZTeam` 配套的产品、架构、UI 和实施文档边界。
- 范围外：
  - 重做 `codex-federation-protocol`、`codex-federation-daemon`、`codex-federation-client` 的基础契约。
  - 扩展 `InterAgentCommunication` 去表示跨实例身份。
  - 对 upstream 已有多智能体工具名、现有 CLI 子命令行为做破坏式修改。
  - 把本轮目标扩展成“完整团队平台”或“Mission Orchestrator”。

## 影响
- 受影响模块：
  - `codex-rs/tui`：
    - 预期新增 `src/zteam/` 或等价模块，承载 worker roster、task board、message stream、result panel。
    - 预期小范围修改 `src/lib.rs`、`src/app.rs`、`src/chatwidget.rs` 或等价入口，完成模式接线。
    - 预期复用 `src/app_server_session.rs` 现有会话启动能力，不优先新增远端协议。
  - `codex-rs/core`：
    - 以复用现有 `spawn_agent`、`send_message`、`followup_task` 为主；不计划改写现有多智能体核心语义。
  - `codex-rs/app-server`：
    - 默认不作为 MVP 主改动区；只有当 `TUI` 接线必须补 seam 时，才允许做最小接入修改。
- 受影响接口/命令：
  - 预期新增 `TUI` 内的 `ZTeam` 入口，优先考虑 `/zteam` 及其子命令或等价模式切换入口。
  - 预期新增配置开关，建议落在 `tui` 配置域，例如 `[tui].zteam_enabled = true`；默认开启，关闭时隐藏或禁用 `ZTeam` 入口。
  - 保持现有 `spawn_agent` / `send_message` / `followup_task` 工具行为不变。
  - 保持现有 federation CLI 与 `--federation-*` 启动参数不变，作为后续 adapter 的底座而非本轮主交互面。
- 受影响数据/模式：
  - 新增 `ZTeam` 工作台自己的本地状态模型：成员、任务、消息、结果、阻塞、恢复标记。
  - 预期新增 `TUI` 配置项以控制 `ZTeam` 的启用/关闭，默认值为开启。
  - 不修改现有 root tree rollout/history 的基础格式。
- 受影响用户界面/行为：
  - 用户将通过 `ZTeam` 工作台协调前端/后端 worker，而不是手工在多个窗口中抄送任务。
  - 对普通 `TUI` 单线程使用路径不产生默认行为变化。

## 约束与依赖
- 约束（时间/兼容性/安全/性能/发布窗口等）：
  - 首要约束是便于持续同步 upstream，因此新增功能应优先以 `TUI` 局部模式、局部模块、新命名空间实现。
  - federation 已明确为本地特性；可以在本地特性边界内自由演进，但仍应避免把本地语义扩散成 upstream 公共基础设施语义。
  - 由于功能尚未发布，可不考虑对当前 federation 草稿的向后兼容；但“不兼容重构”不等于“扩大侵入面”，仍需避免改写 upstream 基础语义。
  - `ZTeam` 的 MVP 必须先证明“协作工作流可用”，而不是先证明“federation transport 更复杂”。
  - `ZTeam` 必须具备显式配置开关，且默认开启；关闭后应能完全隐藏或停用相关入口，避免半启用状态。
  - 若触及 `TUI` 用户可见界面，后续执行阶段必须补 snapshot 覆盖。
- 外部依赖（系统/人员/数据/权限等）：
  - 无外部人员依赖。
  - 当前可用能力筛查结论：
    - 已使用 `llmdoc` 召回仓库稳定文档和相关反思。
    - 已使用 `ztldr` 读取 `tui`、`app-server`、`core` 中 federation 与 multi-agent 的结构性证据。
    - 其余当前会话技能和能力在本轮 `Planning` 阶段没有比上述能力更直接的收益，记为 `none-applicable`。

## 实施策略
- 总体方案：
  - 将本次能力定义为 `ZTeam`，它是一个 **TUI 主导的协作模式**，不是 federation runtime 的直接重命名。
  - `ZTeam` 先用现有本地多智能体能力完成“主线程 + frontend worker + backend worker”的协作闭环，把“中途可通信、主线程可观察、结果能回流”先做成真正可用的工作流。
  - 现有 federation runtime 作为本地特性底座保留，后续只通过 adapter 方式接入 `ZTeam`，不在本轮先把产品语义压进 federation protocol。
  - `ZTeam` 对用户的可见性由配置开关控制：默认开启以确保功能可达，关闭时退回现有普通 `TUI` 路径。
- 关键决策：
  - 模式名采用 `ZTeam`：
    - `Z` 前缀明确它是本地分叉能力，便于与 upstream 区分和后续同步审查。
    - `Team` 直接对应“前端/后端 worker 协作”，比 `Mission`、`Mesh`、`Federation` 更少过度承诺底层架构。
  - 配置开关建议命名为 `tui.zteam_enabled`，语义为“是否暴露 ZTeam 模式入口”，默认值为 `true`。
  - MVP 的“通信能力”优先建立在现有 `spawn_agent` / `send_message` / `followup_task` 之上，因为这条链路已经具备持续通信和目标唤醒语义。
  - 现有 federation 只保留为将来的外部 worker adapter；短期不以“改 envelope / 改 daemon / 改 public RPC”作为主线。
  - `ZTeam` 的 UI、状态聚合和恢复入口尽量下沉到新的 `tui` 模块，不让中心文件继续变胖。
- 明确不采用的方案（如有）：
  - 不把当前目标继续定义成“federation v2 transport 升级”。
  - 不在本轮先引入 `session_id + task/message/result` 的大范围 app-server 新公共接口，再让 `TUI` 去消费。
  - 不把远端 peer 伪装成 `/root/frontend`、`/root/backend` 这类树内 agent。
  - 不命名为 `ZMission`：该名字更适合后续高级编排层，当前会扩大产品承诺。
  - 不命名为 `ZFederation`：该名字会继续把用户注意力拉回 transport，而不是协作体验。

## 阶段拆分
### 阶段一：`ZTeam` 边界冻结与信息架构定稿
- 目标：
  - 冻结 `ZTeam` 的命名、MVP、用户路径、工作台信息架构，以及与 federation runtime / local multi-agent 的边界。
- 交付物：
  - 本计划文件。
  - 后续 issue 和专题文档的统一边界。
- 完成条件：
  - 不再存在“到底先改 federation 还是先做 TUI 模式”的关键歧义。
- 依赖：
  - 当前仓库已确认的 federation 与 multi-agent 事实。

### 阶段二：本地双 worker MVP
- 目标：
  - 以现有本地多智能体能力为底座，完成 `frontend` / `backend` 双 worker 的创建、协作和结果回流。
- 交付物：
  - `ZTeam` worker roster。
  - 任务分派入口。
  - 中途消息与结果回流视图。
- 完成条件：
  - 用户能在单个 `TUI` 工作台内稳定驱动两个 worker 并看见过程。
- 依赖：
  - 阶段一定义的状态模型和 UI 布局。

### 阶段三：恢复、阻塞与工作台收口
- 目标：
  - 补齐 `ZTeam` 的阻塞态、未读态、最近结果、恢复入口和配置开关，使其从“能演示”升级为“能持续用”。
- 交付物：
  - 工作台的任务板、阻塞提示、恢复入口和空态/异常态。
  - 默认开启的 `ZTeam` 配置项，以及关闭时的隐藏/禁用行为。
  - 定向 snapshot 与行为测试。
- 完成条件：
  - 重开 `TUI` 或 worker 轮转时，用户仍能理解当前协作状态，不会退化为无头对话。
  - 关闭配置后，`ZTeam` 入口和相关提示不会继续泄露到普通路径。
- 依赖：
  - 阶段二的工作流闭环。

### 阶段四：federation adapter 预留与最小接线
- 目标：
  - 为 `ZTeam` 后续接入现有 federation bridge 预留 adapter seam，但不把本轮实现绑死在 federation protocol 重构上。
- 交付物：
  - worker source 抽象。
  - 对现有 federation 启动参数与 bridge 的接入说明。
- 完成条件：
  - 后续如需把外部实例挂到 `ZTeam`，不会推翻本轮 `TUI` 工作台设计。
- 依赖：
  - 阶段二、三已稳定的本地协作模型。

## 测试与验证
- 核心验证：
  - 本阶段仅做规划文档验证；以回读计划、自审边界和核对引用证据为主。
- 必过检查：
  - 计划文件无 `TODO`、`TBD`、未闭合的“待决定模式名”或“先做 federation 还是先做 TUI”冲突。
  - 计划中的主路径保持 `TUI` 优先、局部模块优先、upstream 低侵入。
- 回归验证：
  - 无；本阶段不改代码。
- 手动检查：
  - 回读计划，逐项核对：
    1. `ZTeam` 是否已固定命名且解释充分。
    2. MVP 是否明确优先复用本地多智能体能力。
    3. federation 是否被正确降级为未来 adapter，而非当前主战场。
    4. 配置开关是否明确为默认开启、可关闭。
    5. 影响范围是否主要落在 `codex-rs/tui`。
- 未执行的验证（如有）：
  - 未运行任何 Rust 测试；当前阶段是 `Cadence Planning`，只生成计划文档。

## 风险与缓解
- 关键风险：
  - 再次把目标滑回“继续重构 federation runtime”，导致实现范围脱离 `TUI` 主路径。
  - 在 `TUI` 接线时把太多逻辑塞进中心文件，增加后续与 upstream 同步冲突。
  - 将本地 worker 与未来 federated worker 的身份域混为一谈，重新踩回 `/root` vs `InstanceId` 的语义污染。
  - 模式名如果过于偏底层，会把产品心智重新拉回 transport。
  - 配置开关如果只做“半禁用”，会产生入口隐藏但后台行为仍启用的错配。
- 触发信号：
  - issue 草案开始以 `app-server-protocol`、`federation-protocol`、`daemon command` 为首批主任务，而 `tui` 工作台排在后面。
  - 设计中出现“新增 `SessionSource::Federation`”“把 peer 变成 `/root/...` 节点”之类表述。
  - `ZTeam` 的实现需要明显改动既有工具行为，而不是在其上方编排。
- 缓解措施：
  - 在 issue 生成阶段把任务顺序强制收口为“工作台状态模型 -> 本地 worker MVP -> 恢复/阻塞 -> federation adapter 预留”。
  - 新增逻辑优先落独立 `zteam` 模块，入口文件只做薄接线。
  - 在每个后续 issue 的 `notes` 中重复声明“不改 upstream 多智能体基础语义”和“federation 仅作 adapter 预留”。
  - 把配置开关定义成单一真相源：决定入口显隐、命令可用性与模式初始化是否允许进入。
- 回滚/恢复方案（如需要）：
  - 若执行阶段发现 `ZTeam` 无法在不改底层协议的前提下成立，应暂停执行并回到 `cadence-planning` 重新收缩范围，而不是在实现中临时扩大对 `core` / `app-server` 的侵入面。

## 参考
- [codex-rs/tui/src/cli.rs:153](/workspace/codex-rs/tui/src/cli.rs#L153)
- [codex-rs/tui/src/lib.rs:689](/workspace/codex-rs/tui/src/lib.rs#L689)
- [codex-rs/tui/src/app_server_session.rs:191](/workspace/codex-rs/tui/src/app_server_session.rs#L191)
- [codex-rs/app-server/src/codex_message_processor.rs:2683](/workspace/codex-rs/app-server/src/codex_message_processor.rs#L2683)
- [codex-rs/app-server/src/federation_bridge.rs:36](/workspace/codex-rs/app-server/src/federation_bridge.rs#L36)
- [codex-rs/core/src/agent/control.rs:153](/workspace/codex-rs/core/src/agent/control.rs#L153)
- [codex-rs/core/src/tools/handlers/multi_agents_v2/message_tool.rs:1](/workspace/codex-rs/core/src/tools/handlers/multi_agents_v2/message_tool.rs#L1)
- [.agents/llmdoc/memory/reflections/2026-04-23-federation-practicality-needs-workflow-productization.md](/workspace/.agents/llmdoc/memory/reflections/2026-04-23-federation-practicality-needs-workflow-productization.md)
- [.agents/llmdoc/memory/reflections/2026-04-22-federation-protocol-should-stay-separate-from-root-tree-semantics.md](/workspace/.agents/llmdoc/memory/reflections/2026-04-22-federation-protocol-should-stay-separate-from-root-tree-semantics.md)
- [.agents/llmdoc/memory/reflections/2026-04-22-federation-bridge-should-attach-at-thread-start-and-propagate-through-cli-entrypoints.md](/workspace/.agents/llmdoc/memory/reflections/2026-04-22-federation-bridge-should-attach-at-thread-start-and-propagate-through-cli-entrypoints.md)
- [.agents/federation-team-mode/prd.md](/workspace/.agents/federation-team-mode/prd.md)
- [.agents/federation-team-mode/architecture.md](/workspace/.agents/federation-team-mode/architecture.md)
- [.agents/federation-team-mode/ui-spec.md](/workspace/.agents/federation-team-mode/ui-spec.md)
