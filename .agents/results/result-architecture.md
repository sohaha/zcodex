CHARTER_CHECK:
- Clarification level: LOW
- Task domain: architecture
- Must NOT do: 不扩展到 zfeder/federation 改名；不修改代码；不把评审退化成 UI 文案点评或 PM 排期
- Success criteria: 明确当前 ZTeam 架构问题；给出 findings-first 结论并附具体文件/行号；比较至少两种后续演进方案；说明风险与验证边界
- Assumptions: 审查范围限定在 `/workspace/codex-rs` 当前本地改动与现有实现；以 TUI 本地双 worker 模式为目标心智；不要求本次落地实现

status: completed

# ZTeam 架构审查

## 问题定义

当前 `ZTeam` 在 `codex-rs/tui` 中已经形成一个可运行的本地双 worker 工作台：命令入口、状态机、恢复、attach、relay 和工作台渲染都围绕 app-server thread 生命周期与 `InterAgentCommunication` 收敛。

真正的架构问题不在“能不能工作”，而在两个边界仍未收口：

1. **worker 身份只绑定 slot，不绑定一次具体运行代际**，所以 restart / attach / 恢复仍可能把旧线程重新认作当前 worker。
2. **所谓 federation adapter 还不是可执行的 endpoint seam，只是展示态元数据**，因此当前实现本质上仍是 local-only，后续一旦真的接 transport，会被迫穿透多个本地假设。

## Findings

### 1. 缺少 worker 运行代际边界，重复 `/zteam start` 后旧线程仍可能被重新认作当前 worker

严重性：中

证据：

- `mark_start_requested()` 会把两个 slot 直接重置回 `Pending`，但没有生成新的 run/session epoch，也没有记录本次 start 对应的 spawn 期望集。[`codex-rs/tui/src/zteam.rs:263`](/workspace/codex-rs/tui/src/zteam.rs:263)
- `observe_thread_started()` 只要看到 thread metadata 匹配 slot，就会把该线程绑定成当前 worker；没有校验它是否属于“这一次 start”。[`codex-rs/tui/src/zteam.rs:505`](/workspace/codex-rs/tui/src/zteam.rs:505)
- attach/recovery 侧的 `latest_local_threads_for_primary()` 会在**整个 primary descendant 树**里为每个 slot 选最近线程，而不是“当前一次启动的线程”。[`codex-rs/tui/src/zteam/recovery.rs:51`](/workspace/codex-rs/tui/src/zteam/recovery.rs:51)

影响：

- 当前双 worker MVP 在“只启动一次”的 happy path 上是自洽的。
- 但一旦用户重复运行 `/zteam start`、部分 worker 重建、或旧线程延迟回流 `ThreadStarted`，系统没有硬边界区分“旧 worker”与“新 worker”。
- 这会让 restart/replace 语义长期不稳定，后续扩到更多 worker 或外部 transport 时问题会被放大。

### 2. federation adapter 目前只是展示态，不是实际可路由的架构接缝

严重性：中

证据：

- `configure_federation_adapter()` 只是在状态里存一个 `FederationAdapter` 摘要供工作台展示。[`codex-rs/tui/src/zteam.rs:328`](/workspace/codex-rs/tui/src/zteam.rs:328)
- 真正的 dispatch / relay 仍然只接受本地 `ThreadId`，并固定发送 `InterAgentCommunication` 到本地线程。[`codex-rs/tui/src/zteam.rs:341`](/workspace/codex-rs/tui/src/zteam.rs:341)
- worker 来源在 live 注册、结果回流、恢复时都仍然被硬编码成 `WorkerSource::LocalThreadSpawn`。[`codex-rs/tui/src/zteam.rs:517`](/workspace/codex-rs/tui/src/zteam.rs:517) [`codex-rs/tui/src/zteam.rs:572`](/workspace/codex-rs/tui/src/zteam.rs:572) [`codex-rs/tui/src/zteam/recovery.rs:92`](/workspace/codex-rs/tui/src/zteam/recovery.rs:92)
- `WorkerSource::FederationBridge` 已定义但当前未进入任何真实恢复或分派链路。[`codex-rs/tui/src/zteam/worker_source.rs:38`](/workspace/codex-rs/tui/src/zteam/worker_source.rs:38)

影响：

- 如果把现在的实现准确描述为“**TUI-first、本地 thread-based 的双 worker 模式**”，它是成立的。
- 但如果把它描述成“已经有 transport-agnostic/federation-ready seam”，那并不成立。
- 保留这种“表面有 adapter，底层全是 local-only”的状态太久，会在未来接 federation 时制造大量穿透式重构。

### 3. ZTeam live attach 复用了 agent picker 的会话物化 helper，恢复边界泄漏到 UI 选择语义

严重性：中

证据：

- `restore_zteam_worker_candidate()` 在需要 live attach 时，直接复用 `attach_live_thread_for_selection()`。[`codex-rs/tui/src/app.rs:3957`](/workspace/codex-rs/tui/src/app.rs:3957)
- 这个 helper 的设计目标其实是“当 picker 知道某线程但本地还没 event channel 时，把它物化到本地 replay/live 状态”。它的文档和 fallback 都是围绕 selection/replay cache 组织的。[`codex-rs/tui/src/app.rs:3455`](/workspace/codex-rs/tui/src/app.rs:3455)

影响：

- 现在这样复用可以省代码，而且行为大体正确。
- 但 ZTeam attach 语义实际上被绑定到了“picker 如何 materialize thread session”的内部实现。
- 后续如果 agent picker 的 replay/live 策略调整，ZTeam attach 也会被连带影响，边界不够稳。

### 4. `/zteam attach` 仍是 account-wide 扫描 + per-thread read，规模稍大就会变成成本热点

严重性：中偏低

证据：

- `restore_zteam_workers_from_thread_list()` 会对所有 `SubAgentThreadSpawn` 做分页扫描。[`codex-rs/tui/src/app.rs:3987`](/workspace/codex-rs/tui/src/app.rs:3987)
- 每个候选线程再做一次 `thread_read(include_turns=true/false)`。[`codex-rs/tui/src/app.rs:4023`](/workspace/codex-rs/tui/src/app.rs:4023)
- 最后才在客户端用 `descendant_threads()` 和 slot 匹配做过滤。[`codex-rs/tui/src/zteam/recovery.rs:101`](/workspace/codex-rs/tui/src/zteam/recovery.rs:101)

影响：

- 对显式 `/zteam attach` 来说，这个成本今天还能接受。
- 但如果会话寿命变长、subagent 总量增加、或未来 worker roster 扩容，这条路径会先在延迟和 app-server 读放大上出问题。

### 5. 生命周期文案和阻塞判定分散在多处 helper，当前一致，但未来容易漂移

严重性：低

证据：

- `status_summary()/startup_summary()` 在状态层做一套优先级判断。[`codex-rs/tui/src/zteam.rs:648`](/workspace/codex-rs/tui/src/zteam.rs:648)
- 工作台视图又在 `overview_status()` 和 `blocking_note()` 重复编码类似规则。[`codex-rs/tui/src/zteam/view.rs:284`](/workspace/codex-rs/tui/src/zteam/view.rs:284)
- 命令失败提示还有 `missing_worker_message()/missing_relay_message()/pending_worker_guidance()` 自己的一套导向。[`codex-rs/tui/src/zteam.rs:444`](/workspace/codex-rs/tui/src/zteam.rs:444)

影响：

- 这不是当前 bug。
- 但后续只要再加一种连接态、单 worker 重建、或 adapter 特殊态，多个入口很容易出现语义漂移。

## 当前架构判断

### 1. TUI-first 双 worker 架构是否自洽

结论：**作为 local-only 的 TUI 工作台，是自洽的；作为 transport-agnostic 架构，还不自洽。**

正向证据：

- 命令面统一从 `AppEvent::ZteamCommand` 进入 app 层处理。[`codex-rs/tui/src/app_event.rs:104`](/workspace/codex-rs/tui/src/app_event.rs:104) [`codex-rs/tui/src/app.rs:5063`](/workspace/codex-rs/tui/src/app.rs:5063)
- `start/status/attach/dispatch/relay` 都收敛在同一个 handler，控制面集中。[`codex-rs/tui/src/app.rs:2049`](/workspace/codex-rs/tui/src/app.rs:2049)
- 状态机最核心的 worker lifecycle 已收敛成 `Pending / Live / ReattachRequired`。[`codex-rs/tui/src/zteam/recovery.rs:18`](/workspace/codex-rs/tui/src/zteam/recovery.rs:18)
- 运行态事实统一来自 thread notifications，工作台只是消费共享 snapshot。[`codex-rs/tui/src/app.rs:2868`](/workspace/codex-rs/tui/src/app.rs:2868) [`codex-rs/tui/src/chatwidget.rs:9685`](/workspace/codex-rs/tui/src/chatwidget.rs:9685)

保留意见：

- 自洽的前提是把产品定义收窄成“本地 thread worker 工作台”。
- 一旦目标升级为“可平滑接入 federation / 外部 worker”，当前边界不够。

### 2. 状态机 / 恢复 / attach / relay 语义是否清晰

结论：**比之前清晰很多，但还缺两层硬边界：运行代际边界与 attach seam 边界。**

清晰之处：

- Loaded 自动恢复和 thread-list attach 已经分开。[`codex-rs/tui/src/app.rs:3881`](/workspace/codex-rs/tui/src/app.rs:3881) [`codex-rs/tui/src/app.rs:3987`](/workspace/codex-rs/tui/src/app.rs:3987)
- `recover_local_worker()` 只在“允许 live 且线程非 `NotLoaded`”时给 `Live`，否则标 `ReattachRequired`。[`codex-rs/tui/src/zteam/recovery.rs:78`](/workspace/codex-rs/tui/src/zteam/recovery.rs:78)
- relay 只在双方都是 live thread 时成功，否则给出 attach/start 指引。[`codex-rs/tui/src/zteam.rs:357`](/workspace/codex-rs/tui/src/zteam.rs:357) [`codex-rs/tui/src/zteam.rs:463`](/workspace/codex-rs/tui/src/zteam.rs:463)

不够清晰之处：

- “重新 start 后谁算当前 worker”没有 run boundary。
- “ZTeam attach”与“picker attach”还是同一条 helper 链。

## 方案比较

### 方案 A：继续保持 TUI-first，本地状态拥有真相源，但补齐 run boundary + endpoint seam

建议：**推荐**

做法：

- 给每次 `/zteam start` 生成 `zteam_run_id` 或 generation。
- worker 绑定从 `slot -> thread_id` 升级为 `slot -> WorkerBinding { run_id, transport, thread_id/peer_id }`。
- 把当前 `attach_live_thread_for_selection()` 抽成更中性的 thread materializer，再由 picker 和 ZTeam 复用，而不是让 ZTeam 依附 picker 语义。
- 把 `FederationAdapter` 从“展示摘要”推进到“可构造 `WorkerEndpoint`”的真正 seam，但仍不扩 app-server 公共协议。

成本比较：

- 实现成本：低到中
- 运维成本：低
- 团队复杂度：低到中
- 后续变更成本：明显下降

适配度：

- 最符合“ZTeam 是本地特性、要尽量贴住 upstream”的约束。

### 方案 B：把 worker 编排和恢复真相源下沉到 app-server/core，TUI 只做视图

建议：**不推荐当前就做**

做法：

- 让 app-server 或更下层统一持有 worker roster、run generation、恢复索引和 attach 状态。
- TUI 只订阅状态并发送命令。

成本比较：

- 实现成本：高
- 运维成本：中
- 团队复杂度：高
- 后续变更成本：理论上更低，但前提是要接受更大同步面

适配度：

- 只有在未来明确要把 ZTeam 暴露给多运行面共享时才值得做。
- 以当前“本地特性 + 尽量不污染 upstream”的约束看，过重。

## 推荐结论

推荐走**方案 A**。

理由：

1. 当前实现已经证明 TUI-first local orchestration 是成立的，不需要把问题下沉到公共协议层。
2. 真正要补的不是“再造一层服务”，而是把 **worker 代际** 和 **worker endpoint** 这两个边界补硬。
3. 这样既能保住当前最小闭环，也能让以后接 federation 时不必穿透整个本地状态模型。

## 最值得补强的 5 个点

1. 为 `/zteam start` 引入 `run_id/generation`，避免旧 worker 线程回流时重新抢占当前 slot。
2. 把 `WorkerSource` 升级成真正的 `WorkerEndpoint/WorkerBinding`，让 local thread 与 future federation 共享一个可执行抽象，而不是只有展示名。
3. 抽出独立的 `thread materializer` 或 `worker attach service`，切断 ZTeam attach 对 agent picker helper 的隐式依赖。
4. 给 attach/recovery 增加更窄的候选索引，减少 account-wide `thread/list + thread/read` 扫描。
5. 把状态摘要、阻塞原因、命令错误提示统一收敛成单一派生模型，避免多处 helper 漂移。

## 风险

- 如果继续保持现状，最先出现的问题不是功能失效，而是**重建/恢复语义逐步变脏**，随后才会演化成用户层面的“为什么 attach 到旧 worker”“为什么工作台显示 adapter 但不能用”。
- 一旦未来真的接 federation，而当前又没有先做 endpoint seam，局部 UI 状态模型会被迫变成 transport-aware 的杂糅层。

## 验证

- 静态验证：
  - 已审查 `zteam.rs`、`zteam/view.rs`、`zteam/recovery.rs`、`zteam/worker_source.rs`、`app.rs`、`chatwidget.rs`、相关 slash command tests 的当前实现与本地 diff。
- 动态验证：
  - 尝试执行 `cargo nextest run -p codex-tui zteam`。
  - 当前在本地环境失败，构建卡在若干无关基础 crate 的编译阶段，输出只暴露 `rustc` 退出码，未得到可定位到 ZTeam 代码的诊断。

## 产物

- `.agents/results/result-architecture.md`
- `.agents/results/architecture/zteam-2026-04-24-review.md`

