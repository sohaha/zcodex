# ZTeam / Federation Architecture Deep Review

CHARTER_CHECK:
- Clarification level: LOW
- Task domain: architecture
- Must NOT do: 不改源码；不把 UI 细节误判为协议问题；不建议继续膨胀 `codex-core`
- Success criteria: 明确当前 zteam/tui/federation 的架构问题；比较至少两种不同方案；给出推荐、风险、验证步骤；结果写入规定产物
- Assumptions: 当前任务是只读架构审查与结果落盘，不做实现与测试扩张

## Status

recommended

## Architecture Problem

当前 ZTeam 已经形成“本地 TUI 团队模式 + federation bridge 预留”的雏形，但控制面和边界还不够稳定。核心问题不是 UI 是否能展示 team，而是：

**如何把 ZTeam 从 prompt 驱动的体验功能，收敛成 TUI 内部可恢复、可识别、可扩展的确定性协作控制面，同时为 federation 预留真正可执行的 worker source seam。**

## Findings

### 1. 高: `start` 仍是 prompt 驱动，不是确定性控制面

- 证据：
  - `codex-rs/tui/src/app.rs:2057`
  - `codex-rs/tui/src/zteam.rs:604`
- 现状：
  - `/zteam start` 只是提交一段自然语言 prompt，依赖模型自己调用 `spawn_agent` 并生成固定 worker。
- 风险：
  - 上游模型行为、系统 prompt 或工具调用偏移会直接破坏 worker roster、一致性恢复和后续自动 attach。
- 架构含义：
  - 这不是“实现还没收尾”，而是控制面真相源仍在模型输出里，代码只能事后猜测结果。

### 2. 中: worker 身份识别仍依赖启发式 fallback

- 证据：
  - `codex-rs/tui/src/zteam/worker_source.rs:55`
  - `codex-rs/tui/src/zteam/worker_source.rs:74`
- 现状：
  - 优先匹配 canonical `agent_path`，失败后回退到 `agent_role + nickname` 推断。
- 风险：
  - 同一 primary 下的普通 descendant thread 可能被误识别成 ZTeam worker，污染恢复、attach 和 workbench 状态。
- 架构含义：
  - 领域身份没有单一真相源，恢复逻辑被迫做“猜测式归类”。

### 3. 中: federation adapter 目前只是展示态摘要，不是真正 adapter seam

- 证据：
  - `codex-rs/tui/src/zteam.rs:302`
  - `codex-rs/tui/src/zteam/worker_source.rs:38`
- 现状：
  - `SharedState` 里只缓存 `FederationAdapter` 摘要；`WorkerSource::FederationBridge(...)` 仍未真正接线。
- 风险：
  - 后续切 federation-backed worker 时，现有 seam 不能承接 start、dispatch、recovery、status 等行为。
- 架构含义：
  - 现在的 adapter 更像 UI state，而不是 transport/source abstraction。

### 4. 中: ZTeam 领域逻辑仍散落在 `app.rs`、`chatwidget.rs`、`zteam.rs`

- 证据：
  - `codex-rs/tui/src/app.rs:3957`
  - `codex-rs/tui/src/app.rs:4043`
  - `codex-rs/tui/src/zteam.rs:184`
  - `codex-rs/tui/src/chatwidget.rs:9685`
- 现状：
  - 恢复/attach 在 `app.rs`，状态容器在 `zteam.rs`，UI 包装在 `chatwidget.rs`。
- 风险：
  - 后续如果继续叠加 team lifecycle、resume 或 federation source，TUI orchestration 耦合会继续升高。
- 架构含义：
  - 当前 `zteam` 更像状态盒子而非完整 coordinator，边界还不够厚。

## Options

### Option A: 维持 prompt 驱动 + 薄状态模块

- 做法：
  - 继续用 `/zteam start` prompt 触发模型生成 worker。
  - 继续依赖现有恢复筛选与启发式识别。
  - federation 先只保留摘要态和后续接线位。
- 实现成本：低
- 运维成本：中到高
- 团队复杂度：中
- 未来变更成本：高
- 优点：
  - 改动最小，能延续当前体验。
  - 对现有 TUI 流程冲击最小。
- 缺点：
  - 控制面不确定性继续存在。
  - 身份漂移和恢复误吸附风险不会消失。
  - federation seam 仍是假接口，未来扩展时需要二次重构。

### Option B: 在 `tui/src/zteam/` 内提炼确定性 coordinator / control plane，并把 worker source 做成真正抽象

- 做法：
  - 把 worker 创建、注册、恢复、attach 收敛到显式 coordinator。
  - 由代码生成固定的 worker spawn 请求或专用启动入口，不再依赖自由文本 prompt 当真相源。
  - 把 canonical metadata 作为 worker 身份主真相源；昵称/角色启发式仅保留迁移兜底。
  - 把 federation source 提升成可执行的 `WorkerSource`/transport seam，统一 start、dispatch、recovery、status。
- 实现成本：中
- 运维成本：中
- 团队复杂度：中
- 未来变更成本：低到中
- 优点：
  - 恢复链路与启动链路共享同一套真相源。
  - 本地 ZTeam 与未来 federation-backed worker 可以复用同一抽象边界。
  - 不需要把新公共语义塞进 `codex-core`。
- 缺点：
  - 需要重整 `app.rs` / `zteam.rs` / `chatwidget.rs` 的职责分布。
  - 需要定义更稳定的 worker metadata 和 coordinator 生命周期。

## Recommendation Summary

推荐 **Option B**。

原因：

- 当前最大风险来自“不确定控制面”，不是 UI 缺一块按钮。
- 恢复链路已经基本选对方向，值得继续把 `loaded auto-recovery -> candidate filtering -> live attach` 这条链收敛到确定性 coordinator，而不是继续让启动端保持 prompt 驱动。
- 这条路能在 `tui` 内完成主重构，不需要把 ZTeam 语义下沉到 `codex-core`，符合仓库长期约束。

## Tradeoffs

- 相比维持现状，推荐方案会增加一次边界重整成本，但换来更低的长期演进摩擦。
- 相比直接把 federation 做进当前 UI state，推荐方案先补控制面，再补 transport，有利于避免“先接线、后返工”。
- 相比把更多逻辑继续塞进 `app.rs`，coordinator 方案会引入新的内部抽象，但它解决的是已经出现的职责扩散，而不是抽象炫技。

## Risks

- 如果 canonical worker metadata 设计不稳，可能出现一次迁移期兼容成本。
- 如果 coordinator 与现有 attach/recovery seam 拆分不当，可能引入 TUI 生命周期回归。
- 如果 federation source 抽象过早泛化，可能做成“接口很多但只实现一种 source”的空壳。

## Validation Steps

1. 先把 `start -> register -> recover -> attach` 的状态机画清楚，确认哪些状态属于 TUI、哪些属于 worker source。
2. 定义 worker 身份主真相源，验证能覆盖：
   - fresh start
   - loaded auto-recovery
   - thread-list attach
3. 用本地模式先证明 coordinator 抽象成立，再把 federation 接到同一 source seam，而不是反过来。
4. 验证 `latest_local_threads_for_primary(...)` 过滤和 live attach seam 在新边界下仍保持单一入口。

## Artifacts Created

- `.agents/results/result-architecture.md`
- `.agents/results/architecture/2026-04-24-zteam-architecture-deep-review.md`
