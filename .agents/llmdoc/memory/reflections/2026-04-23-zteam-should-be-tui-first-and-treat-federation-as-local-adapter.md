# ZTeam 应先做 TUI 模式，并把 federation 当成本地特性 adapter

## 背景

- 在评估 federation 的下一步时，仓库里已经同时存在两套可用基础：
  - `tui -> app-server thread/start -> federation_bridge` 这条多实例桥接底座；
  - `spawn_agent`、`send_message`、`followup_task` 这条本地多智能体持续协作链路。
- 用户新增了两个决定性约束：
  - 项目要定期同步 upstream `openai/codex`，因此尽量不要改 upstream 既有功能面；
  - federation 明确是当前仓库的本地特性，不是 upstream 公共能力。

## 观察

- 如果继续把目标表述成“federation v2”，实现很容易自然滑向协议、daemon、公共 RPC 和身份语义重构；这会直接扩大与 upstream 的冲突面。
- 但用户真正要的是“前端 worker + 后端 worker + 中途沟通 + 主线程看板”，这首先是 `TUI` 的协作工作流问题，而不是 transport 问题。
- 现有本地多智能体链路已经具备持续通信和目标唤醒语义，足够支撑 `ZTeam` 的本地 MVP。
- 现有 federation runtime 更适合作为后续外部 worker 的 adapter seam，而不是当前产品模式的名字和中心心智。

## 结论

- 当 federation 被明确视为本地特性、且仓库又要长期追 upstream 时，优先级应改为：
  1. 在 `codex-rs/tui` 做一个本地命名空间明确的 `z*` 模式；
  2. 先用现有 local multi-agent 完成工作流闭环；
  3. 再把 federation 作为 adapter 挂到该模式上。
- 这类模式命名应避免继续强调 transport。`ZTeam` 比 `ZFederation`、`ZMission` 更稳，因为它直接指向用户价值，同时降低对底层架构的过度承诺。
- 本地特性可以自由演进，但不应借“本地特性”之名去污染 upstream 通用语义；局部模块、新命名空间和配置开关是更稳的收敛方式。

## 后续默认做法

- 以后遇到“本地特性 + 需要长期同步 upstream + 用户真正要的是工作流”的组合时，先问：
  - 这是不是应该先做 `TUI/CLI` 的局部模式，而不是先改协议层？
  - 有没有现成的 upstream 能力能完成 MVP，只需要在上方编排？
  - 底层本地特性是否更适合作为 adapter，而不是直接成为产品表面？
- 如果答案是“是”，优先采用 `z*` 本地模式 + adapter 架构，而不是先启动底层重构。
