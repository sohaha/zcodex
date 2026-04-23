# Federation Team Collaboration Architecture Review

CHARTER_CHECK:
- Clarification level: MEDIUM
- Task domain: architecture
- Must NOT do: 不把 federated peer 硬塞进 `/root` agent tree；不修改现有 core/app-server 代码；不把 UI/产品交互细节误当成协议层设计
- Success criteria: 明确当前架构问题；比较至少两种可行演进方案；给出推荐方案并说明实现成本、运维成本、团队复杂度、未来变更成本与验证步骤
- Assumptions: 目标是让不同 Codex 实例中的前端/后端 agent 持续协作，而不仅是一次性 task dispatch；现阶段以本机/同工作区 federation daemon 为主；保留现有 app-server `thread/start` bridge seam

## Status

recommended

## Architecture Problem

当前 federation 已经是一个“跨实例投递 TextTask / 回收 TextResult”的旁路 IPC 底座，但它还不是“团队协作模式”：

- federation 协议只有 `TextTask` / `TextResult` 两种 payload，没有持续会话、上下文同步、阶段状态、共享工件或事件订阅语义。
- app-server bridge 只在 fresh `thread/start` 上注册 peer，然后在本地线程空闲时轮询 inbox，把 `TextTask` 转成普通 `Op::UserTurn`，完成后再把最后一条 assistant 文本包回 `TextResult`。
- multi-agent / root tree 则是另一套更强语义：它依赖 `/root` 为树根、`AgentPath` 表示父子命名空间、`SessionSource::SubAgent(ThreadSpawn { parent_thread_id, depth, agent_path, ... })` 表示谱系、`InterAgentCommunication` 表示树内 mailbox。

因此真正的架构问题不是“怎样把 federation 接进 multi-agent”，而是：

**怎样在不污染现有 root tree 语义的前提下，为跨实例 peer 增加持续协作与上下文同步能力。**

## Current Evidence

- federation 的 thread seam 明确挂在 app-server `thread/start`，不是 core subagent tree：
  - `FederationThreadStartParams` 是 `ThreadStartParams.federation` 的一个可选字段，见 `app-server-protocol/src/protocol/v2.rs`。
  - bridge 在 `thread/start` 成功建线程后启动，见 `app-server/src/codex_message_processor.rs`。
- bridge 当前只做 task/result 桥接：
  - 注册 peer、心跳、轮询 inbox，见 `app-server/src/federation_bridge.rs`。
  - 只挑 `EnvelopePayload::TextTask`，把它 submit 成普通本地 `Op::UserTurn`，完成后发 `TextResult`，见同文件。
- federation 协议刻意保持独立身份空间：
  - `EnvelopePayload` 只有 `TextTask` / `TextResult`，身份用 `InstanceId`，见 `federation-protocol/src/envelope.rs`。
- root tree 语义是另一套约束：
  - `InterAgentCommunication` 的 author/recipient 都是 `AgentPath`，见 `protocol/src/protocol.rs`。
  - `SessionSource::SubAgent(ThreadSpawn { parent_thread_id, depth, agent_path, ... })` 绑定父子谱系，见 `protocol/src/protocol.rs`。
  - `AgentRegistry` 会显式注册 `/root`，`list_agents()` 也把 `/root` 当树根暴露出来，见 `core/src/agent/registry.rs` 与 `core/src/agent/control.rs`。
  - multi-agent v2 明确禁止把 task 指派给 root，并按 `AgentPath` 解析 target，见 `core/src/tools/handlers/multi_agents_v2/message_tool.rs`。

## Options

### Option A: 直接把 federated peer 映射成 `/root/...` 远端子代理

做法：

- 给每个远端实例分配一个 `AgentPath`，例如 `/root/frontend`、`/root/backend`。
- 复用 `InterAgentCommunication`、`SessionSource::SubAgent`、`list_agents`、环境上下文展示等现有能力，把远端 peer 伪装成 root tree 中的 live child。

优点：

- UI/TUI/app-server 现有多代理视图可以较快复用。
- 本地调用方可以继续用 `send_message` / `followup_task` / 相对路径引用。

缺点：

- 语义错误。`AgentPath` 代表单个 root session 内的树状命名空间，不代表跨实例 peer 身份。
- 会把“发现到一个远端实例”误写成“它是当前 root 的子线程”，污染 `parent_thread_id`、`depth`、analytics、agent limits、环境上下文与恢复逻辑。
- `InterAgentCommunication` 当前是树内 mailbox 文本信封；历史上旁路 reducer 没过滤它时，会把内部 JSON 泄露到用户 thread history。把 federation 再塞进去会放大这类污染面。
- reconnect / resume / peer lease 过期时，很难定义 `/root/...` 上的生命周期：它不是 thread manager 直接拥有的 live thread。

成本评估：

- 实现成本：中
- 运维成本：高
- 团队复杂度：高
- 未来变更成本：很高

结论：

不推荐。它表面复用最多，实际上把两个不同身份域强行折叠，会持续污染 root tree 语义。

### Option B: 保持 federation 为独立身份域，在 app-server 上新增“协作会话层”

做法：

- 保持 `InstanceId` / `InstanceCard` / `Envelope` 与 `/root` tree 分离。
- 在 app-server v2 增加显式的 federated collaboration surface，而不是复用 subagent surface。
- 本地 thread 仍是普通线程；federated peer 作为“外部协作成员”挂到 thread 或新建的 collaboration session 上。
- federation transport 升级为结构化协作 envelope，例如：
  - `TaskAssign`
  - `TaskUpdate`
  - `ContextDelta`
  - `ArtifactShare`
  - `TaskResult`
  - `Presence/Typing/Heartbeat`
- app-server 负责把这些事件投影成 thread-scoped 可观察状态，而不是伪造 `SessionSource::SubAgent`。

优点：

- 和现有 seam 一致：继续从 `thread/start` 或 thread-scoped API 进入，不碰 root tree。
- 身份清晰：本地 subagent 还是本地 subagent，federated peer 是 federated peer。
- 便于前端/后端 agent 持续通信，因为可以把“共享上下文”做成显式 state，而不是反复压成 prompt 文本。
- 可以先保留当前 `TextTask/TextResult` 作为兼容最小子集，再逐步加 richer envelope。

缺点：

- 需要新增协议类型、状态 reducer、订阅/恢复逻辑。
- 现有多代理 UI 不能直接照搬，需要区分 local team 与 federated team。

成本评估：

- 实现成本：高
- 运维成本：中
- 团队复杂度：中
- 未来变更成本：中

结论：

推荐作为跨实例“团队协作模式”的主线方案。

### Option C: 只产品化现有 multi-agent，本地团队协作走 root tree，federation 继续只做一次性 IPC

做法：

- 如果团队成员都运行在同一个 Codex 进程/线程树里，就继续用现有 `spawn_agent` / `send_message` / `followup_task`。
- federation 不扩协议，只保留脚本化 `TextTask` / `TextResult`。

优点：

- 最省事，完全顺着当前 root tree 设计。
- 本地团队协作体验最快能做起来。

缺点：

- 不满足“不同实例间持续协作”目标。
- 会把真正的跨实例需求长期推迟到另一条系统里，最终还是要补 Option B。

成本评估：

- 实现成本：低
- 运维成本：低
- 团队复杂度：低
- 未来变更成本：中

结论：

适合作为短期本地产品化路线，不是 federation 升级成团队协作模式的最终答案。

## Recommendation Summary

推荐 **Option B**，并且分两层推进：

1. **身份与编排层**
   - 保留 `federation-protocol` 独立身份域。
   - 新增 `FederatedCollaborationSession` 或 thread-scoped federation state，成员键用 `InstanceId`，不要生成 `/root/...` 伪路径。
   - 明确区分：
     - local subagents：root tree 内、`AgentPath` 驱动
     - federated peers：外部协作成员、`InstanceId` 驱动

2. **数据与事件层**
   - 继续让 app-server 成为外部客户端的主入口。
   - 不再把长期上下文同步塞进 `TextTask` 文本里，而是定义结构化 envelope + app-server 通知：
     - shared brief / summary delta
     - task ownership / status
     - artifact references
     - result streaming 或 incremental updates
   - 如果需要高保真 turn/item 级同步，优先在 app-server 层做 thread-scoped订阅/投影，再决定 federation transport 是否承载全文或只承载索引/摘要。

## Why This Is The Lightest Sufficient Architecture

- 它复用了已经成立的 seam：`thread/start` bridge 和 app-server 事件流，而不是去撬 core 的 thread tree 基础语义。
- 它满足“持续通信、同步上下文、协作推进”的目标，因为共享状态被提升成一等对象，而不是继续堆低层 message passing。
- 它避免了最危险的耦合：不把 peer 注册、租约失活、daemon mailbox 这些外部生命周期假装成 root tree child thread 生命周期。

## Main Risks

- 协议膨胀风险：如果一次性把所有协作状态都塞进 federation envelope，协议会过重。
- 双通道一致性风险：若 federation transport 与 app-server thread state 各自存一份真相，容易漂移。
- 恢复语义风险：当前 bridge 只覆盖 fresh `thread/start`，若要支持长期协作，必须定义 resume/reconnect 如何恢复 peer roster 与 pending work。
- 可见性风险：任何新的内部 envelope 若被旁路 reducer 当普通 assistant message 渲染，会再次出现内部 JSON 泄露问题。

## Validation Steps

1. 先写一份 federated collaboration state diagram，明确：
   - peer 注册/失活
   - session attach/detach
   - task assign/update/result
   - reconnect/resume
2. 用 app-server v2 schema 先定义最小结构化事件集，而不是先改 core root tree。
3. 做一个端到端样例：
   - 前端 agent 建立 thread
   - 绑定两个 federated peers
   - 下发任务
   - 接收增量更新
   - 汇总结果回主 thread
4. 单独验证这些边界：
   - peer lease 过期不会污染 `/root` agent list
   - thread/read / history 不会出现内部 envelope JSON
   - resume 后 federated collaboration session 的 roster 与未完成任务可恢复

## Artifacts Created

- `.agents/results/result-architecture.md`
- `.agents/results/architecture/federation-team-collaboration.md`
