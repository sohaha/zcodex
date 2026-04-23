CHARTER_CHECK:
- Clarification level: MEDIUM
- Task domain: planning
- Must NOT do: 不做代码实现; 不把底层 daemon 原语直接当成 MVP 成品; 不把评审/测试单独拆成脱离功能的孤立任务
- Success criteria: 给出基于现状的可执行 PM 方案; 产出 API 合同文件; 产出含依赖与验收标准的任务计划 JSON
- Assumptions: 目标是把 federation 从实验性 IPC 底座推进到“前端 Codex + 后端 Codex 可持续协作”的 MVP; 默认优先 app-server/TUI 主路径; 默认面向同机多实例、同仓或相邻仓协作

status: completed
summary:
  - 基于现有 `federation-protocol`、`federation-daemon`、`federation-client`、`app-server` bridge 和现有多 agent 能力，整理了“前后端双 Codex 可随时通讯”的产品化方案。
  - 结论是：当前仓库已经有可运行的 federation 底座，但仍停留在 `TextTask/TextResult + inbox` 原语层；真正缺的是 peer 发现、持续会话、消息回流、可见状态和恢复语义。
  - MVP 应聚焦“双 worker 协作工作台”：能启动两个角色化 worker、互发消息、共享上下文引用、把结果回流到主线程，而不是继续扩更多底层命令。
files_changed:
  - .agents/results/result-pm.md
  - .agents/results/plan-pm.json
  - .agents/skills/_shared/api-contracts/federation-collab-mvp.md
acceptance_criteria_checklist:
  - [x] 基于仓库现状与关键实现路径分析
  - [x] 明确真实使用场景、关键工作流、MVP 范围与非目标
  - [x] 输出 API-first 合同草案
  - [x] 输出可分派的任务计划 JSON

## 现状判断

- 已存在的底座能力：
  - `codex-rs/federation-protocol/src/envelope.rs` 只有 `TextTask` / `TextResult` 两类 payload，说明目前更像任务投递原语，不是协作产品面。
  - `codex-rs/federation-daemon/src/store.rs` 已支持注册实例、列 peer、发信封、读 inbox、写 ack、清理过期状态，说明本地 IPC 与状态模型已经成立。
  - `codex-rs/app-server/src/federation_bridge.rs` 会在 `thread/start` 时注册实例、轮询 inbox、把 `TextTask` 转成普通本地 `Op::UserTurn`，并把完成结果回投成 `TextResult`。
  - `codex-rs/app-server/tests/suite/v2/federation.rs` 已覆盖 `thread/start -> TextTask -> TextResult` 闭环。
  - `codex-rs/core/src/tools/handlers/multi_agents_v2/message_tool.rs` 已有单会话内 agent 间发消息的成熟交互模型，可复用其 UX 语义。

- 现阶段不足：
  - 只能发“任务”和“结果”，不能发中途协作消息，也没有共享上下文引用。
  - 结果默认回 inbox，不自然回到当前主线程、TUI 或 IDE 主视图。
  - bridge 只在 `thread/start` 挂载，缺少 resume/reconnect 的用户级恢复语义。
  - 缺少“前端 worker / 后端 worker / 当前负责人 / 最近消息 / 是否在线”的高层状态模型。

## 用户场景

### 核心用户

- 独立开发者：希望让一个 Codex 负责前端，一个 Codex 负责后端，主线程像 Tech Lead 一样协调。
- 小团队开发者：把本地多个 Codex 窗口当作不同角色 worker，减少上下文切换。
- IDE / app-server 客户端用户：希望在图形界面里看到谁在做什么、谁需要回复、谁已经产出可集成结果。

### 高频场景

1. 启动协作
   - 主线程一键创建“前端 worker”和“后端 worker”，都带角色标签和工作目录。

2. 分工执行
   - 主线程给两边下达各自任务；worker 开始处理。

3. 中途通讯
   - 前端 worker 发现 API 字段不确定，直接给后端 worker 发消息。
   - 后端 worker 发现接口会影响页面交互，也能主动通知前端 worker。

4. 结果回流
   - 任一 worker 产生阶段成果，主线程能直接看到摘要、阻塞和后续需要。

5. 中断恢复
   - 某个 worker 关闭、闪退或主线程重开后，仍能看到会话关系、最近消息和未处理状态。

## 最关键工作流

1. 创建双 worker 协作会话
   - 用户从主线程选择“启动协作模式”。
   - 指定两个角色：`frontend`、`backend`。
   - 系统为两个 worker 建立可发现、可恢复的 federation session。

2. 主线程向 worker 分派任务
   - 主线程给前端和后端分别发送首个任务。
   - 任务消息带 `session_id`、角色、来源线程、可选上下文引用。

3. worker 间随时互发消息
   - 前端可向后端发“需求澄清”“字段确认”“接口变更通知”。
   - 消息不是结束任务，而是中途通讯。

4. 主线程接收回流
   - 主线程自动收到 worker 的结果摘要、阻塞提醒和关键决策。
   - 主线程能决定是否继续分派、介入或合并成果。

5. 会话恢复
   - 重新进入 TUI/app-server 后，可恢复该协作 session，并继续看未读消息与在线状态。

## MVP 定义

### MVP 必须有

- 角色化 peer 会话
  - 至少支持两个命名 worker：前端、后端。
- 持续消息类型
  - 不止 `task/result`，至少支持 `task`、`message`、`result`。
- 主线程回流
  - worker 结果和关键消息会自动回到主线程视图，不要求用户手工读 inbox。
- 在线状态
  - 能看到 peer 在线/离线、最近心跳、最近一条消息时间。
- 恢复语义
  - `thread/resume` 或等效入口可恢复 federation session 绑定，而不是只在 fresh start 有效。

### MVP 不做

- 跨机器远程 federation。
- 多于两个 worker 的复杂网状协作编排。
- 自动冲突合并、自动代码整合或任务自动再分解。
- 富媒体附件、代码 diff 结构化引用、语音/实时共享。
- 完整 project management 面板或复杂权限体系。

## 产品判断

- 这不是“再补一个 daemon 命令”的问题，而是把 federation 从基础设施升级成协作工作流。
- 最接近用户价值的切口不是更复杂的 envelope，而是：
  - session 级协作关系
  - 主线程可见的消息回流
  - worker 间持续沟通
  - 恢复与状态可见性

## 推荐方案

- 产品层采用“协作 session”概念，而不是让用户直接操作 instance id 和 inbox。
- 技术层继续复用现有 `federation-daemon` 和 `app-server thread/start` bridge。
- UX 层借鉴现有 `multi_agents_v2`：
  - 有明确 sender / receiver
  - 有 interaction begin/end 事件
  - 有主线程可见的协作历史

## 风险与边界

- 如果只扩 daemon API，不补 app-server/TUI 主路径，功能仍会停留在工程演示层。
- 如果直接做复杂的多 worker 网状编排，MVP 会被“消息路由、恢复、一致性”拖慢。
- 如果不定义 `session_id` 和消息类别，后续恢复和聚合会变得混乱。
