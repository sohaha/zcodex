# Technical Review

## 已验证事实
- 当前本地 `codex-zmemory` 仍以嵌入式 Rust 内核工作；模型侧动作、CLI 与 core handler 共享同一套 `tool_api / service / schema`，这一点不变。
- 本轮用于 web 复用评估的上游参考是本地 checkout `/workspace/nocturne_memory`（HEAD `a574c2d92bcfe377441e35d27f883fe1cb39e1e1`）；其 frontend 通过 `/api/browse`、`/api/review`、`/api/maintenance` 等 HTTP contract 工作，而不是直接读取 SQLite。
- 因此，“数据库兼容”并不只等于几张主表长得像；真正需要先收敛的是 schema、runtime contract 与 review/snapshot 数据语义，然后才是薄 adapter 接线。
- 当前本地已具备更强的运行时诊断与治理视图：`system://workspace`、`system://defaults`、`system://alias`、`system://paths`、`stats`、`doctor`。这些能力值得保留，但不能继续被当作“因此完全不需要上游兼容层”的理由。
- `limit` 参数的分页合同仍保持现状：`boot/index/paths/recent/glossary/alias` 共享截断语义，`defaults/workspace` 继续作为导出视图但不承诺分页。

## 分叉决策
- **架构升级**：从“embedded-only，不扩任何 service surface”的旧口径，升级为“嵌入式内核 + upstream-compatible adapter”的新口径；adapter 是复用上游 web 的必要接线层，但不等于把 `codex-zmemory` 改造成独立 memory service。
- **兼容优先级**：先对齐 `namespace-aware` schema 与 runtime `dbPath`/`namespace`/boot contract，再对齐 review/snapshot/maintenance 数据合同；model-visible tool surface 不是这一轮的主阻塞点。
- **保留增强**：`system://workspace` / `system://defaults` 的 runtime introspection、`system://paths` / `system://alias` 的治理视图、以及 `stats` / `doctor` 的本地诊断指标全部保留，作为上游兼容合同之外的附加信号。
- **适配边界**：兼容层只负责 browse/review/maintenance 请求投影与同库同 runtime 接线；不接受“adapter 自己再维护一份缓存/数据库/状态机”的扩张。
- **不采用的方向**：不把上游 backend/frontend 整包导入 crate，不继续以“tool 已够用”掩盖 schema/runtime/review contract 缺口，也不允许 web/CLI/core 连接不同库制造“记忆混乱”假象。

## 欠缺与下一步
- 阶段 1 需要把本地 schema / FTS / runtime contract 与上游 `namespace-aware` 期望逐项对齐，并建立“跨入口指向同一 dbPath”的验证。
- 阶段 2 需要补齐 review group / diff / rollback / approve / maintenance 所依赖的 changeset/snapshot 语义，避免 adapter 层重新发明数据合同。
- 阶段 3 再实现薄 adapter 并用上游 web 做真实复用验证；在这之前，不把“页面能打通”视为成功。
