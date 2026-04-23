# federation 的实用性应区分“架构成立”与“工作流已产品化”

## 背景

- 在评估 `codex federation minimal intrusion` 的实用性时，计划与实现都已经不再停留在纯设计层：仓库里已有独立 `federation-protocol` / `federation-daemon` / `federation-client`、CLI 子命令、`app-server thread/start` bridge，以及端到端测试。
- 这很容易让人把“底层链路已经跑通”误判成“功能已经适合作为主工作流推广”。

## 观察

- 从架构边界看，这个方案是成立的：它把跨实例身份、信封、ack 和本地 state 布局放进独立 crate，没有复用 `/root/...`、`AgentPath`、`InterAgentCommunication` 或 `SessionSource` 语义。
- 从工程接线看，bridge 也选对了 seam：挂在 `app-server thread/start`，把入站 `TextTask` 转成普通本地 `Op::UserTurn`，完成后再回投 `TextResult`。
- 但从真实使用体验看，它仍偏底层构件：
  - 请求方结果默认回到 daemon inbox，而不是自然回到当前会话或 TUI/exec 视图。
  - `thread/start` 接 bridge 时要求 daemon 先可用；CLI 的 `federation` 子命令会自动拉起 daemon，但常规 TUI/exec 路径本身不负责这层引导。
  - bridge 只对 fresh `thread/start` 生效，不会在 `thread/resume` 自动恢复。
  - 当前 payload 只有 `TextTask` / `TextResult`，更像本机多实例 IPC 原语，不是完整协作系统。

## 结论

- 以后判断 federation 类能力时，先分两层：
  - “架构是否成立”：看它是否保持独立身份空间、独立协议和最小 core 侵入。
  - “是否已具备高频实用性”：看结果是否回到用户主视图、worker 生命周期是否自然、失败是否有面向用户的工作流承接。
- 当前这套 federation 应定位为：
  - 值得保留的实验性基础设施能力；
  - 适合高级用户、脚本化编排、IDE/app-server 集成和跨 repo 常驻 worker；
  - 还不适合作为普通 Codex 用户的主协作工作流。

## 后续默认做法

- 若后续继续推进 federation，优先补“工作流产品化”而不是继续堆底层能力：
  - 请求方结果回流到当前线程/界面；
  - 常驻 worker 与恢复语义；
  - 更高层的 peer 选择、投递与结果聚合入口。
- 在这些工作完成前，不要把“已有 daemon/client/bridge/E2E”当成 federation 已经适合主路径推广的证据。
