# ADR: Federation Team Collaboration Should Stay Outside Root Tree Semantics

## Decision

跨实例团队协作不应复用现有 `/root` subagent tree。推荐在 app-server 上增设独立的 federated collaboration session/state，把 federation 保持为独立身份域与 transport/control plane。

## Context

- `InterAgentCommunication` 使用 `AgentPath` 标识 author/recipient，本质是 root session 内的 mailbox。
- `SessionSource::SubAgent(ThreadSpawn { parent_thread_id, depth, agent_path, ... })` 把父子谱系编码进线程来源。
- `AgentRegistry` 把 `/root` 当唯一树根，并用它组织 live agent subtree。
- federation 目前只提供 `InstanceId`、`InstanceCard`、`TextTask`、`TextResult`、ack、lease 与 mailbox。
- app-server bridge 仅把 federation `TextTask` 转为本地 `Op::UserTurn`，不会创建 federation 专用 `SessionSource`。

## Consequences

### Positive

- 保住 root tree 语义纯度，避免 analytics、history、resume、agent limit 等基础逻辑被远端 peer 生命周期污染。
- 让跨实例协作有独立演进空间，可以定义结构化 shared context / task state / artifact sync。
- 继续复用 app-server 作为外部客户端稳定入口。

### Negative

- 无法直接复用现有 multi-agent tree UI 作为 federated team UI。
- 需要新增 app-server protocol、状态 reducer 与恢复语义。

## Rejected Alternative

把 federated peer 直接映射为 `/root/...` 远端子代理。

拒绝原因：

- 语义域不一致：`AgentPath` 是树内路径，不是跨实例身份。
- 远端 lease/reconnect 与本地 thread lifecycle 不一致。
- 会扩大 `InterAgentCommunication` 内部 envelope 泄露到 thread history 的风险面。

## Next Step Shape

1. 在 app-server v2 设计 thread-scoped federated collaboration API。
2. 为 federation transport 增加结构化 envelope，而不是继续扩大纯文本 payload。
3. 明确 resume/reconnect/lease-expiry 对 collaboration session 的恢复与清理合同。
