# Federation 最小入侵设计

## 背景
- 当前状态：
  仓库现有多子代理能力服务于单个 root session tree，核心语义依赖同一进程内的 `AgentControl`、`AgentRegistry`、`AgentPath`、mailbox 与 thread/session 生命周期；尚无多个独立 Codex 实例（不同目录）之间的原生通信与命名机制。
- 触发原因：
  需要支持多个独立 Codex 实例在不同目录下运行，并在启动时声明名称与职责进行互相通信；同时明确要求后续要持续与上游对齐，因此设计必须尽量不改上游核心逻辑，优先旁路式扩展。
- 预期影响：
  形成一份可进入 `cadence-issue-generation` 的设计计划，明确最小入侵的 federation 架构、模块边界、MVP 范围、协议与落地顺序，为后续 issue 拆分与实现提供依据。

## 目标
- 目标结果：
  产出一份围绕“最小入侵 federation”的可执行设计计划，明确独立运行面、实例身份模型、消息与 ack 语义、CLI/daemon/client 边界、以及与现有 `multi_agent` 体系的隔离策略。
- 完成定义（DoD）：
  1. 计划明确说明哪些现有核心语义不能复用或改写。
  2. 计划明确 MVP 仅覆盖单机、多目录、多个独立 Codex 实例的注册、发现、文本任务投递与结果回传。
  3. 计划明确 crate/模块落点与最小接入点，且默认避免深改 `codex-rs/core`。
  4. 计划提供可进入 issue 拆分的阶段拆分、验证思路、风险与回滚边界。
- 非目标：
  不直接实现代码；不在本阶段生成 issue 文件；不设计跨机器 federation、自动调度、广播、共享上下文同步、复杂 UI、权限继承编排或统一远端/本地子代理语义。

## 范围
- 范围内：
  - 单机、本地多个独立 Codex 实例的 federation 设计。
  - 独立 `instance_id`/`display_name`/`role`/`task_scope`/`cwd` 身份模型。
  - 独立 daemon/client/protocol/CLI 边界。
  - 共享目录或本地 socket 驱动的注册表、心跳、邮箱、ack、结果回传设计。
  - 与现有 `spawn_agent`/`AgentPath`/`InterAgentCommunication` 的隔离策略。
- 范围外：
  - 直接修改 `spawn_agent`、`AgentControl`、`AgentRegistry`、`SessionSource`、`SubAgentSource` 的核心语义。
  - 跨机器网络传输与鉴权体系。
  - 现有 TUI、app-server、mcp-server 的完整 UI/协议整合。
  - 自动路由、并行聚合、群聊广播、共享历史、共享权限与调度优化。

## 影响
- 受影响模块：
  - `codex-rs/cli`：新增 federation 子命令与可选启动参数桥接。
  - 新增 federation 相关 crate：协议、daemon、client。
  - 可能新增独立 state 目录与本地 socket 运行面。
- 受影响接口/命令：
  - 新增 `codex federation ...` 子命令面。
  - 主 `codex` 可选新增 `--federation-enable`、`--federation-name`、`--federation-role`、`--federation-scope` 等参数。
- 受影响数据/模式：
  - 新增独立 federation 注册表、inbox/ack/state 持久化格式。
  - 不改现有 thread rollout、session history、subagent tree 持久化模型。
- 受影响用户界面/行为：
  - 用户可在启动时给实例命名、定义职责与 scope。
  - 用户可通过新命令查看在线 peers、发送任务、查看收件结果。
  - 默认不改变现有单实例 CLI/TUI 的多子代理行为。

## 约束与依赖
- 约束（时间/兼容性/安全/性能/发布窗口等）：
  - 首要约束是便于持续与上游同步，避免深改上游核心运行面与已有多代理协议。
  - 设计必须最小入侵、可旁路启用、可独立禁用或移除。
  - MVP 仅要求单机可用，不以跨机器扩展为第一优先级。
  - 只写已确认事实，不预先承诺未验证的运行细节。
- 外部依赖（系统/人员/数据/权限等）：
  - 无额外外部人员依赖。
  - 需要本地可用的文件系统目录与本地 IPC 能力（如 Unix socket）作为候选基础设施。

## 实施策略
- 总体方案：
  采用“旁路 federation 层”而不是修改现有 `multi_agent` 内核：新增独立的 protocol/client/daemon 组合，由 `codex-rs/cli` 提供命令面与可选启动桥接；主程序只通过薄接入层向 federation 注册实例、发送心跳、接收文本任务并转成本地普通输入，不复用现有 root tree 内 mailbox 或 `AgentPath` 语义。
- 关键决策：
  - 身份主键使用独立 `instance_id`，不复用 `AgentPath` 或 `ThreadId`。
  - federation 先做“单机、本地 daemon + 本地 state”，再考虑后续升级传输层。
  - 独立设计 `Envelope`、`AckState`、`InstanceCard` 等协议类型，不污染现有 `protocol` 中的单实例协作语义。
  - MVP 先支持文本任务与文本结果，不引入复杂 payload、自动调度或共享上下文。
  - daemon 负责注册、发现、邮箱、ack、心跳与清理；主 Codex 仍负责本地任务执行。
- 明确不采用的方案（如有）：
  - 不直接把跨实例通信塞进 `spawn_agent`、`send_message`、`followup_task`。
  - 不直接扩展 `SessionSource::SubAgent` 或 `SubAgentSource::ThreadSpawn` 表达 federation 来源。
  - 不把远端实例伪装成 `/root/...` 路径树中的 agent。
  - 不在 MVP 阶段直接依赖 app-server/MCP 充当 federation 的发现层与状态机。

## 阶段拆分

### 阶段一：设计定稿与边界冻结
- 目标：
  冻结 federation 的边界、身份模型、消息模型、daemon/client/CLI 模块职责，并明确“不侵入 core 语义”的约束。
- 交付物：
  - 规划文档定稿。
  - 后续 issue 拆分所需的模块、命令、协议与验证边界。
- 完成条件：
  - 设计文档可直接拆为 issue。
  - 没有未闭合的关键边界、占位符或互相冲突的约束。
- 依赖：
  - 当前对仓库运行面与上游同步约束的已确认事实。

### 阶段二：MVP issue 拆分
- 目标：
  将设计拆成最小可执行 issue，覆盖 protocol、daemon、client、CLI 接线与主程序桥接。
- 交付物：
  - `.agents/issues/*.toml` issue 文件。
  - 每个 issue 的范围、DoD、验证入口与依赖关系。
- 完成条件：
  - issue 之间依赖明确，可按顺序推进。
  - 每个 issue 都能在不深改 `core` 的前提下闭环实施。
- 依赖：
  - 阶段一定稿的设计计划。

### 阶段三：MVP 实现与本地验证
- 目标：
  完成单机 federation MVP：实例注册、peer 列表、文本任务投递、接收、回传与基础 ack。
- 交付物：
  - 新增 federation crate 与 CLI 接线。
  - 最小可运行的 daemon/client。
  - 受影响命令与文档更新。
- 完成条件：
  - 两个不同目录下的独立 Codex 实例可在本机互相注册、发现、发送文本任务并回传结果。
  - 不改现有单实例 `multi_agent` 语义。
- 依赖：
  - 已确认的 issue 计划。

### 阶段四：演进评估
- 目标：
  基于 MVP 结果评估是否进入 V1：更稳定的 ack、健康状态、daemon 重启恢复与传输升级。
- 交付物：
  - 演进建议与后续 backlog。
- 完成条件：
  - 明确 V1/V2 是否继续，以及继续时的边界。
- 依赖：
  - MVP 实现与验证结果。

## 测试与验证
- 核心验证：
  - 规划阶段验证文档本身是否足以支撑 issue 拆分与执行，不涉及代码运行。
- 必过检查：
  - 计划文件无占位符、无 `TODO/TBD`、无未确认假设。
  - 设计边界与当前已确认约束一致：单机、多目录、最小入侵、便于上游同步。
- 回归验证：
  - 无；本阶段不改代码。
- 手动检查：
  - 回读计划文档，逐节核对背景、目标、范围、约束、实施策略、阶段拆分、验证、风险是否自洽。
  - 按 `plan-reviewer` 阻断标准审查：是否仍有会导致 issue 生成或执行阶段卡住的歧义、冲突或缺失。
- 未执行的验证（如有）：
  - 无自动化验证；本阶段仅做文档结构与语义自检。

## 风险与缓解
- 关键风险：
  - 设计仍隐式耦合到现有 `core` 多代理模型，导致后续实现时侵入面扩大。
  - MVP 范围膨胀，把跨机器、自动调度、复杂 UI 等后续议题提前引入。
  - daemon/client/protocol 切分不清，导致 issue 拆分后职责交叉。
  - 为追求复用而错误复用 `AgentPath`、`ThreadId`、`InterAgentCommunication` 等现有语义。
- 触发信号：
  - 计划或后续 issue 中出现“修改 `spawn_agent` 以支持 federation”“新增 `SessionSource::Federation`”“把远端实例映射成 `/root/...`”等表述。
  - 设计文档开始承诺跨机器、广播、共享上下文、权限传播等超出 MVP 的能力。
- 缓解措施：
  - 在 issue 生成前再次显式检查“最小入侵”和“不可复用的现有概念”。
  - 将 daemon、client、protocol、CLI 桥接分别作为独立 issue。
  - 在每个后续 issue 的 `done_when` 与 `notes` 中重复声明不修改现有 `multi_agent` 语义。
- 回滚/恢复方案（如需要）：
  - 若后续实现发现设计必须深改 `core` 才能成立，则停止当前 execution，回到 `cadence-planning` 或 `cadence-issue-generation` 重新收缩方案；不在执行阶段临时扩大侵入面。

## 参考
- [.agents/llmdoc/architecture/runtime-surfaces.md](/workspace/.agents/llmdoc/architecture/runtime-surfaces.md)
- [.agents/llmdoc/architecture/rust-workspace-map.md](/workspace/.agents/llmdoc/architecture/rust-workspace-map.md)
- [codex-rs/core/src/agent/control.rs:128](/workspace/codex-rs/core/src/agent/control.rs#L128)
- [codex-rs/core/src/agent/registry.rs:16](/workspace/codex-rs/core/src/agent/registry.rs#L16)
- [codex-rs/protocol/src/agent_path.rs:17](/workspace/codex-rs/protocol/src/agent_path.rs#L17)
- [codex-rs/protocol/src/protocol.rs:718](/workspace/codex-rs/protocol/src/protocol.rs#L718)
