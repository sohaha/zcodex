# ZTeam 恢复应拆成 loaded-list 自动恢复、thread-list 手动 attach，并把 federation 保持为本地 adapter 接缝

## 背景

- `a4` 需要在不改 federation 公共协议、daemon 或身份语义的前提下，为 `ZTeam` 补齐恢复语义、显式再附着入口和 federation adapter seam。
- `tui` 已有两条可直接复用的事实链路：
  - `thread/start|resume|fork -> replace_chat_widget_with_app_server_thread -> thread/loaded/list`，适合恢复当前进程里仍然活着的 descendant subagent；
  - `thread/list -> thread/read`，适合在本地缓存之外回看最近线程元数据并恢复最近状态。

## 观察

- “自动恢复”和“显式再附着”虽然都叫恢复，但事实源不同：
  - 自动恢复只需要处理当前 app-server 里已经加载的线程，所以 `thread/loaded/list` 更准，也能避免把陈旧 subagent 混进当前 workbench；
  - 手动 `/zteam attach` 需要在当前缓存缺失时尽量找回最近 worker，因此要退回 `thread/list` 扫描最近 descendant，再用 `thread/read` 拉细节。
- `thread/read(include_turns = true)` 不是对所有 subagent 都稳定可用：
  新 spawn 但还没有首条用户消息的线程、或 ephemeral 线程，会触发现有已知错误，需要沿用 TUI 里已经存在的 include-turns fallback 逻辑。
- 对关闭或未加载线程，恢复逻辑不应伪装成“仍然 live”：
  用显式的 `Pending / Live(thread_id) / ReattachRequired(thread_id)` 连接状态，比旧的 `thread_id + closed` 更容易让 workbench、slash error message 和 attach 提示收口到一致语义。
- federation 在这个阶段真正需要的不是 transport 改造，而是**让 ZTeam 代码结构里存在一个可展示、可传递、但不污染公共协议的 seam**。
  也就是：保留 `WorkerSource` / `FederationAdapter` 和 workbench 摘要展示，先让 `--federation-*` 启动参数可被 ZTeam 感知，而不是把 peer 直接并入 `/root/...` 身份树。

## 结论

- 今后遇到“本地工作台要恢复最近协作状态，但底层 transport 不应扩协议”的需求时，优先采用两层恢复合同：
  1. `thread/loaded/list` 做自动恢复，只接当前进程里仍然 live 的 descendant；
  2. `thread/list + thread/read` 做显式 attach，允许恢复最近状态并明确标注 `ReattachRequired`。
- 连接状态应做成显式枚举，让 UI、命令错误文案和恢复逻辑共享同一套真相源。
- federation 若仍处于“本地特性底座”阶段，应先作为 adapter seam 进入 TUI 层可见状态，而不是提前扩到公共 RPC、身份模型或跨实例树语义。

## 后续默认做法

- 做类似恢复功能时，先问：
  - 当前需要的是“恢复 live 连接”还是“恢复最近状态”？
  - 有没有现成的 loaded-list / thread-list / thread-read 事实源可组合，而不是新增协议？
  - UI 是否需要明确区分 `未注册`、`已附着`、`待再附着` 三种态？
- 如果答案分别是“二者都要”“有”“需要”，就优先复用现有线程 API 做双层恢复，而不是把需求自然滑回到底层 transport 改造。
