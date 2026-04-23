CHARTER_CHECK:
- Clarification level: LOW
- Task domain: architecture
- Must NOT do: 不改源码；不扩展 federation/public RPC；不把 unrelated 脏工作区混入 a3 结论
- Success criteria: 明确判断 a3 是否满足 5 条验收项；给出文件级证据；记录验证命令和范围边界
- Assumptions: 以当前工作区实现与 issue a3 合同为准；未提交的无关改动不纳入 a3 范围；定向 `codex-tui` 测试可作为主要验证证据

status: pass

architecture_problem: >
  a3 的架构问题不是新增协议，而是要确认 ZTeam 协作工作台是否以 TUI 本地视图形式闭环，
  让主线程直接看到 worker 状态、任务、消息和结果，同时避免把 federation 或 app-server
  公共 RPC 变成实现前提。

recommendation_summary: >
  审查结论为通过。当前实现把 ZTeam 工作台收敛在 `codex-rs/tui`：独立 `zteam/view.rs`
  负责渲染，`zteam.rs` 负责状态聚合与通知映射，`chatwidget.rs`/`app.rs` 只做入口和薄接线。
  该结构满足 issue a3 的本地工作台目标，没有把功能外溢到 federation/public RPC。

tradeoffs:
  - 选择单个 bottom-pane 工作台而非新增整页路由，接线成本低、复用现有 popup/view 生命周期，但在极矮终端上可视空间受限。
  - 选择复用现有 `ServerNotification`/`InterAgentCommunication` 和线程事件回灌，而非扩公共协议，范围受控，但工作台状态依赖现有线程通知语义。

risks:
  - “一屏看到”目前由工作台的完整渲染高度与 snapshot 证明；在更小终端高度下可能出现裁切，这是体验风险，不是本次合同缺口。
  - 工作区存在大量无关脏改动；本审查只针对 a3 相关 TUI 文件和定向测试，不对其他并行任务背书。

validation_steps:
  - 阅读 issue：`.agents/issues/2026-04-23-zteam-tui-collaboration-mode.toml`
  - 审查实现：`codex-rs/tui/src/zteam.rs`、`codex-rs/tui/src/zteam/view.rs`、`codex-rs/tui/src/chatwidget.rs`、`codex-rs/tui/src/app.rs`、`codex-rs/tui/src/bottom_pane/mod.rs`
  - 审查测试与快照：`codex-rs/tui/src/chatwidget/tests/slash_commands.rs`、`codex-rs/tui/src/chatwidget/snapshots/codex_tui__chatwidget__tests__zteam_workbench_empty_view.snap`、`codex-rs/tui/src/chatwidget/snapshots/codex_tui__chatwidget__tests__zteam_workbench_active_view.snap`、`codex-rs/tui/src/chatwidget/snapshots/codex_tui__chatwidget__tests__zteam_entry_disabled_notice.snap`
  - 运行验证：`codex ztok shell env -u RUSTC_WRAPPER cargo test -p codex-tui zteam`

evidence:
  - 可进入工作台：`chatwidget.rs:9711`-`9730` 提供 `/zteam` 入口与 `WorkbenchView` 打开逻辑；`app.rs:2047`-`2073` 让 `/zteam start|status` 直接打开工作台。
  - 一屏呈现四块信息：`zteam/view.rs:42`-`141` 同时渲染“概况 / Worker 面板 / 任务板 / 消息流 / 结果回流”；active snapshot 展示 frontend/backend 状态、任务、消息、结果共屏。
  - 空态/阻塞态/禁用态反馈：`zteam/view.rs:224`-`274` 给出未启动、等待注册、worker 关闭三类阻塞提示；`chatwidget.rs:9711`-`9718` 与 `zteam.rs:470`-`479` 给出禁用态消息；empty snapshot 与 disabled snapshot 覆盖空态和禁用态。
  - 范围收敛：a3 相关变更集中在 `codex-rs/tui/src/**` 和对应 snapshot/tests；未发现 app-server protocol v2、federation bridge 或公共 RPC 改动是此实现前提。
  - snapshot 覆盖：`slash_commands.rs:341`-`356` 断言空工作台快照，`slash_commands.rs:418`-`488` 断言活跃工作台快照，`slash_commands.rs:359`-`375` 断言禁用态提示快照。

artifacts_created:
  - `.agents/results/architecture/2026-04-23-zteam-a3-spec-review.md`
