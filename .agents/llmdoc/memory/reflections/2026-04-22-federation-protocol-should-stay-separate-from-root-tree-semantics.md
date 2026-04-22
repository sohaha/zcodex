## 背景
- `codex federation minimal intrusion` 的 a1 需要先落一个独立协议 crate，只负责多实例注册、文本信封、ack 与本地 state 目录契约。
- 现有仓库已经有 `AgentPath`、`ThreadId`、`InterAgentCommunication`、`SessionSource::SubAgent` 这套单实例 root tree 语义，如果直接复用，看起来省事，后续会把 federation 强行绑回 `core`。

## 观察
- `codex-rs/protocol` 里的 `AgentPath` 明确要求 `/root/...` 绝对路径和受限的 agent name 语法，它表达的是单个 root session tree 内部地址，不适合拿来当跨实例身份。
- `InterAgentCommunication` 直接面向现有 mailbox/turn 唤醒路径，字段也是 `author/recipient/other_recipients` 的树内路由，不是独立实例之间的持久信封。
- federation 的本地 state 需要稳定文件命名和读写约束，后续 daemon/client 才能围绕同一目录契约继续实现；这类内容也不该塞回 `codex-core` 或现有 `protocol` 的运行时语义里。

## 结论
- 跨独立 Codex 实例的 federation 契约应保持为单独的 `codex-federation-protocol` crate，由它持有 `InstanceId`、`InstanceCard`、`Envelope`、`AckState`、租约/心跳与 state layout。
- 只要需求仍是“最小入侵 federation”，就不要把远端实例伪装成 `/root/...` 路径、不要新增 `SessionSource::Federation` 来挤进旧语义，也不要把 federation mailbox 回写到现有 `InterAgentCommunication`。

## 本次处理
- 新增 `codex-rs/federation-protocol/`，把实例 ID、文本任务/结果信封、ack 状态、租约/心跳和本地目录布局放进独立 crate。
- 用测试锁住两个边界：
  - `InstanceId` 只能是 UUID，不能解析 `/root/...` 这类 agent path 形状。
  - `InstanceCard.display_name` 不复用 agent path 的命名限制，避免把“展示名”和“树内地址”混成一个概念。

## 可复用经验
- 当新能力横跨多个独立 Codex 进程时，先问“它是不是 root tree 外的新身份空间”；如果是，先拆独立 crate，再考虑桥接。
- 需要持久化目录契约时，优先在协议层给出文件名和稳定/临时写入约束，别等到 daemon 实现时再临时拼路径。
- 若后续实现里出现把 federation peer 映射回 `AgentPath`、`ThreadId` 或 `SessionSource::SubAgent` 的冲动，应视为越界信号，先回到规划边界收缩方案。
