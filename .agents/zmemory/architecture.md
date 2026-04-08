# Architecture

## 目标形态
- `codex-zmemory` 继续作为嵌入式 Rust 内核，负责 SQLite schema、runtime path resolution、版本链、搜索索引与治理统计等单一事实来源。
- 为了最大化对齐上游并复用 `nocturne_memory` web，整体路线从“embedded-only”升级为“内核 + upstream-compatible service adapter”；兼容 adapter 放在内核外侧，只负责暴露上游 frontend 所需的 browse/review/maintenance contract。
- 对齐优先级固定为：先收敛 DB/schema 与 runtime contract，再补 review/snapshot 数据语义，之后才是薄 HTTP adapter 和真实 web 复用验证；不要把问题继续缩减成单一 tool exposure。
- `system://workspace`、`system://defaults`、`system://alias`、`system://paths` 以及 `stats` / `doctor` 继续作为本地增强保留，但必须以“附加诊断信号”存在，而不是与上游兼容合同相冲突的第二套语义。
- CLI、core、兼容 adapter 与未来上游 web 必须稳定指向同一 `dbPath`、`namespace` 与 boot/runtime profile，避免不同入口各连一份库。

## 分层接入
1. **内核层**：`tool_api` → `service` → `schema` / `history` / `stats`，继续负责本地 durable memory 的真实读写、检索、版本与诊断。
2. **本地治理视图**：`system://boot/defaults/workspace/index/paths/recent/glossary/alias` 继续提供 runtime、path、alias、review pressure 等本地观察信号。
3. **本地调用面**：`codex zmemory` 子命令、`codex-core` tool handler、现有 MCP 风格别名继续复用同一内核合同，不额外复制存储层。
4. **上游兼容适配层**：在内核外新增薄适配层，对接 `/api/browse`、`/api/review`、`/api/maintenance` 等上游 frontend contract；该层只做请求/响应投影与运行时接线，不重新发明第二套 memory 内核。
5. **上游 web**：把 upstream frontend 当作同库同 runtime profile 的消费端，而不是直接读数据库或维护独立状态。

## 兼容边界
- **必须对齐的合同**：`namespace-aware` schema、FTS/索引主键、runtime `dbPath`/`namespace`/boot 解析规则，以及上游 review group / diff / rollback / approve / maintenance 所需的数据语义。
- **允许保留的扩展**：`system://workspace` / `system://defaults` 的 runtime introspection，`system://paths` / `system://alias` 的治理视图，以及 `stats` / `doctor` 的 path-level、alias-level 诊断指标。
- **不采用的路线**：不把上游 backend/frontend 整包塞进 `codex-zmemory` crate，不增加第二份 memory 数据库，不让 adapter 绕开本地 runtime path resolution。
- **工具面约束**：模型可见工具面保持最小且稳定；web 复用的优先阻塞点是 schema/runtime/review contract，而不是继续优先增加更多 model-visible actions。

## 本地分叉保留
- `system://index` 继续保留本地既有索引语义；`system://paths` 继续承担“显式列出当前活跃 path 全量”的职责，避免与上游 browse 语义混为一谈。
- `system://alias`、`stats`、`doctor` 保留 alias coverage、review priority、suggested keywords、path-level diagnostics 等本地治理强化。
- `system://workspace` / `system://defaults` 继续暴露 runtime path resolution 与默认 path policy；即便引入上游兼容 adapter，这些视图仍是本地排障的首选入口。
- 任何兼容层都必须复用同一内核与同一数据库；本地分叉增强可以附加，但不能各自维护一套脱离内核的状态。

## 实施守则
- 阶段 0 先冻结“本地增强保留清单”，避免后续 schema / adapter 实施时误删 `system://workspace`、`system://defaults`、`system://alias`、`system://paths` 与 `stats` / `doctor` 信号。
- 阶段 1 优先收敛 schema 与 runtime contract，再进入 review/snapshot 语义；在这之前不要直接硬接上游 web。
- 阶段 2 的 adapter 应保持足够薄：只做上游 contract 接线，不把 browse/review/maintenance 复杂度重新塞回 `codex-zmemory` 内核。
- 阶段 3 验收必须以“同库、同 namespace、同 runtime profile”与真实 browse/review/maintenance 主链路可用为准，而不是只看页面能否打开。
