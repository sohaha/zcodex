# 技术方案评审报告

## 文档信息
- **功能名称**：federation-team-mode
- **创建日期**：2026-04-23
- **状态**：已评审

## 摘要

> 下游 Agent 请优先阅读本节，需要细节时再查阅完整文档。

- **评审结论**：✅ 有条件通过。方向正确，可以直接按重构路线推进。
- **主要风险**：高层协作真相源漂移、恢复语义定义不清、`federation_bridge` 继续变胖。
- **必须解决**：先定义 session/message/task 高层 contract；先确定恢复真相源；禁止把 federated peer 映射成 `/root/...`。
- **建议优化**：保留 Mission 扩展位但不进入 MVP；让 CLI `federation` 命令退回诊断工具角色。
- **技术债务**：现有 `TextTask/TextResult` 和调试型命令面仍会在短期内存在，需要在实现中逐步降级为内部细节。

---

- PRD：`.agents/federation-team-mode/prd.md`
- 架构：`.agents/federation-team-mode/architecture.md`
- UI 规范：`.agents/federation-team-mode/ui-spec.md`

## 1. 评审结论

| 维度 | 评分 | 说明 |
|------|------|------|
| 架构合理性 | ⭐⭐⭐⭐⭐ | 保持 federation runtime 独立身份域，同时把协作工作流上移到 app-server，是当前最稳的方案。 |
| 技术选型 | ⭐⭐⭐⭐ | 复用现有 Rust workspace 与 app-server seam 成本合理，但需要新增高层 schema 与 reducer。 |
| 可扩展性 | ⭐⭐⭐⭐⭐ | Team Mode 与 Mission Mode 分层后，后续加入 validator、里程碑和更多角色不会反复撬根语义。 |
| 可维护性 | ⭐⭐⭐⭐ | 若及时拆出 `collaboration_*` 模块，可维护性较好；若继续堆到 `federation_bridge.rs`，会迅速恶化。 |
| 安全性 | ⭐⭐⭐⭐ | 同机优先、无兼容包袱有利于收口；但仍要注意内部 envelope 不应泄露到用户可见历史。 |

**总体评价**：✅ 有条件通过

## 2. 技术风险评估

| 风险 | 等级 | 影响范围 | 缓解措施 |
|------|------|----------|----------|
| 协作 session 与 daemon transport 各维护一份真相源 | 高 | 恢复、通知、UI 状态 | 明确 app-server 投影为唯一对外真相源，daemon 仅做 transport/runtime |
| 把 federated peer 伪装成 `/root` subagent | 高 | root tree、history、resume、analytics | 在协议与任务中显式禁止，维持 `InstanceId` 与 `AgentPath` 分离 |
| `federation_bridge.rs` 继续承担业务编排 | 高 | 维护成本、测试成本 | 引入 `collaboration_session` / `collaboration_state` 模块，bridge 只做接线 |
| 任务板扩张为 PM 系统 | 中 | 范围与工期 | MVP 仅保留 owner/status/blocked/summary |
| 恢复语义含糊导致 session 重复或角色漂移 | 中 | 用户体验、一致性 | 先定义状态机与恢复合同，再写代码 |

## 3. 技术可行性分析

### 3.1 核心功能可行性

| 功能 | 可行性 | 复杂度 | 说明 |
|------|--------|--------|------|
| 协作 session 与角色化 worker | ✅ 可行 | M | 复用现有 `thread/start` 与 federation runtime，可通过 app-server 承接 |
| task/message/result 三类消息 | ✅ 可行 | M | 当前仅缺高层 contract，不缺底层 transport |
| 主线程结果回流 | ✅ 可行 | M | app-server 已有事件流基础，关键在 schema 与 reducer |
| 共享任务板 | ✅ 可行 | M | 可先做轻量 task state，而不是完整 project management |
| 恢复与重连 | ⚠️ 有挑战 | L | 需要先统一 session 真相源与状态机 |
| Mission validator / 里程碑编排 | ⚠️ 有挑战 | XL | 适合后续版本，不应进入当前 MVP |

### 3.2 技术难点

| 难点 | 解决方案 | 预估工时 |
|------|----------|----------|
| 协作状态真相源 | app-server 新增 `collaboration_state`，仅把 transport 作为输入 | 2-3 天 |
| 恢复语义 | 先定义 `sessionId`、participant roster、message window、task board 恢复合同 | 2 天 |
| UI 状态投影 | 把 worker 状态、任务板、消息流统一投到一个工作台模型 | 3-4 天 |
| 历史污染防止 | 对任何协作 envelope 在可见历史层做明确过滤 | 1-2 天 |

## 4. 架构改进建议

### 4.1 必须修改（阻塞项）

- [ ] 先定义高层 `session / message / task` 协议，不能继续让 `TextTask/TextResult` 直接承担产品语义。
- [ ] 明确恢复与重连合同，特别是 `resume` 后 session roster 与最近状态的恢复方式。
- [ ] 明确禁止把 federated peer 接入 `/root` tree 和 `SessionSource::SubAgent`。

### 4.2 建议优化（非阻塞）

- [ ] 将 `codex federation ...` 重新定位为诊断/管理入口，不再作为主产品路径。
- [ ] 在 UI 中把 Team Mode 与后续 Mission Mode 做模式切换预留，但当前只开放 Team Mode。
- [ ] 尽早给 `task`、`message`、`result` 设计统一视觉语义，减少前后端分歧。

## 5. 实施建议

### 5.1 开发顺序建议

```mermaid
graph LR
    A[高层协议定义] --> B[app-server 协作状态]
    B --> C[恢复与在线状态]
    C --> D[TUI/客户端工作台]
    D --> E[中途通讯与任务板]
    E --> F[文档与回归收口]
```

### 5.2 里程碑建议

| 里程碑 | 内容 | 建议工时 | 风险等级 |
|--------|------|----------|----------|
| M1 | 定义 session/message/task schema，明确状态机与恢复合同 | 3-4 天 | 中 |
| M2 | 落地 app-server 协作 API 与高层真相源 | 4-6 天 | 高 |
| M3 | 落地工作台 UI、任务板、消息回流与恢复入口 | 4-6 天 | 高 |

### 5.3 技术债务预警

| 潜在债务 | 产生原因 | 建议处理时机 |
|----------|----------|--------------|
| 旧 envelope 与新高层 contract 双栈并存 | MVP 需要借 transport 过渡 | M2 完成后开始收口 |
| CLI `federation` 与产品协作入口重复 | 历史上先有诊断命令面 | M3 前统一定位 |
| `federation_bridge.rs` 继续增重 | 为赶进度继续堆逻辑 | M2 期间必须拆分 |

## 6. 代码规范建议

### 6.1 目录结构规范

```text
app-server-protocol/src/protocol/v2.rs
app-server/src/collaboration_session.rs
app-server/src/collaboration_state.rs
app-server/src/collaboration_events.rs
tui/src/collaboration/workbench.rs
tui/src/collaboration/task_board.rs
tui/src/collaboration/message_stream.rs
```

### 6.2 命名规范

- **文件命名**：按职责命名，不使用泛化的 `manager`、`utils` 收容所有逻辑。
- **组件命名**：围绕协作语义命名，如 `WorkerStatusCard`、`TaskBoardPane`、`ResultPanel`。
- **函数命名**：动词 + 协作对象，如 `restore_collaboration_session`、`project_message_to_thread`。
- **变量命名**：优先显式表达语义，如 `participant_status`、`session_roster`、`result_message`。

### 6.3 代码风格

- 保持 runtime transport 与产品状态投影分层，不混在同一模块。
- 所有可见 UI 状态必须由结构化模型驱动，避免直接解析底层 transport payload。

## 7. 评审结论

- **是否通过**：✅ 有条件通过
- **阻塞问题数**：3 个
- **建议优化数**：3 个
- **下一步行动**：先按本评审输出的关键阻塞项修正文档与任务拆解，再进入执行阶段
