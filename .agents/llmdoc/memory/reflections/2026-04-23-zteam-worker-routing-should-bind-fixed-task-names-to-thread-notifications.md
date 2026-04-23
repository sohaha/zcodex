# ZTeam worker 路由应绑定固定 task name、线程通知和直接 InterAgentCommunication

## 背景

- `a2` 需要在不改写 `spawn_agent` / `send_message` / `followup_task` 外部语义、也不扩 app-server 公共 RPC 的前提下，落地 ZTeam 的本地双 worker 编排闭环。
- TUI 当前已经能：
  - 通过 slash command 触发本地命令；
  - 向指定线程提交 `Op::InterAgentCommunication`；
  - 为 inactive thread 缓存通知、回放历史，并在 `ThreadStarted` / `ItemCompleted` 等通知里看到 subagent 线程与其输出。

## 观察

- 真正缺的不是“再造一层 spawn 协议”，而是 **把已有 multi-agent runtime 的事实接成 TUI 可消费的状态模型**：
  - 创建期需要一个稳定的 worker 身份锚点；
  - 运行期需要可直接路由到 worker 线程的消息通道；
  - 收口期需要从 inactive thread 的通知里拿到 worker 结果，而不是等主线程再把结果翻译成新协议。
- `task_name = frontend/backend` 这类固定 canonical task name 足以在本地模式里承担 worker 身份锚点；
  再配合 `ThreadStarted.thread.source` 的 `agent_path/agent_role`，TUI 就能把线程稳定绑定到 frontend/backend slot。
- 一旦 thread id 已经注册，后续 root -> worker 或 worker -> worker 的消息不需要再绕模型 prompt；
  直接向目标线程提交 `Op::InterAgentCommunication`，既复用现有语义，也让 TUI 命令面变得可预测。
- worker 结果回流同样不需要新事件面：inactive worker 线程自己的 `ItemCompleted(ThreadItem::AgentMessage)` 就已经是可消费事实源。

## 结论

- 这类“TUI 工作台编排现有 multi-agent runtime”的需求，优先采用：
  1. 固定 canonical task name 作为 worker 身份锚点；
  2. `ThreadStarted` / `ThreadClosed` / `ItemCompleted` 作为状态回流事实源；
  3. 直接 `Op::InterAgentCommunication` 作为命令路由面。
- 只有当现有线程通知无法表达所需状态时，才考虑扩 app-server 或公共协议；不要因为要做工作台就先发明新 RPC。

## 后续默认做法

- 以后在 TUI 上层为现有 runtime 做产品化工作流时，先检查：
  - 是否已经有固定身份锚点可映射到状态 slot；
  - inactive thread 通知是否已经足够做结果回流；
  - 是否可以直接用现有 `Op` 提交路径完成交互，而不是新增“仅为 UI 服务”的中间协议。
- 如果三者都满足，优先收敛到 TUI 本地状态层和薄接线，而不是把实现重心下压到公共 transport。
