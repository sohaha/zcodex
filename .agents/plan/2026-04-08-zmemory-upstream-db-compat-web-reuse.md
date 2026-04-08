


# zmemory 上游数据库兼容与 web 复用

> 不适用项写 `无`，不要留空。
> 只写已确认事实；若信息不清楚或需要假设，先回到对话澄清，不要把未确认内容写进计划。

## 背景
- 当前状态：
  - 本地 `codex-zmemory` 已形成嵌入式 Rust crate + CLI/core handler 的分叉路线，并在文档中明确“不扩 REST / daemon / remote admin service”。
  - 本地底层已支持 `history/export/stats/audit/doctor/rebuild-search` 等动作，但默认模型可见工具仅有 `read_memory/search_memory/create_memory/update_memory`。
  - 当前激活库为 `/workspace/.agents/memory.db`，`system://boot` 显示 3 个默认 boot anchors 全部缺失，当前库未完成 bootstrap。
  - 上游仓库 `nocturne_memory` 当前 HEAD 为 `a574c2d92bcfe377441e35d27f883fe1cb39e1e1`，其 frontend 通过 `/api/review`、`/api/browse`、`/api/maintenance` 等 HTTP 端点工作，而不是直接读数据库。
  - 本地 schema 与上游 Rust schema 已确认存在差异：本地 `paths/search_documents/glossary_keywords` 仍是无 `namespace` 版本，且本地额外有 `audit_log`；上游 Rust schema 已把 `namespace` 纳入主键/唯一约束，并把 `node_uuid` 纳入 `search_documents_fts`。
  - 上游 web 的核心治理流程还依赖 review/snapshot 契约：`/review/groups`、`/review/groups/{node_uuid}/diff`、`/review/groups/{node_uuid}/rollback`、`DELETE /review/groups/{node_uuid}`、`/maintenance/stats`、`/maintenance/doctor`、`/maintenance/rebuild-search` 等链路背后依赖 changeset/snapshot 聚合与回滚语义，而不只是主表结构。
- 触发原因：
  - 用户希望“最大化对齐上游”，并明确提出把“数据库兼容”作为复用上游 web 的主前提。
  - 现有本地实现与上游的差异已经不只是 tool exposure，还涉及 DB schema、runtime contract、service surface 与 review/admin 流程。
- 预期影响：
  - 若 DB/schema/数据语义能与上游兼容，本地可把“复用上游 web”从架构阻塞问题降为“接线与适配”问题。
  - 若继续保留当前 embedded-only 决策且不调整 schema/runtime contract，将无法稳定复用上游 web，也会继续放大“不同入口连到不同库”的混乱感。

## 目标
- 目标结果：
  - 制定一条以“数据库兼容优先”为主线的上游对齐方案，使本地 zmemory 具备复用上游 web 的现实前提。
- 完成定义（DoD）：
  - 明确记录本地与上游在 DB schema、数据语义、runtime contract、tool/admin/service surface 上的差异清单与取舍。
  - 形成分阶段实施方案：先对齐 DB/schema 与数据语义，再补 upstream-compatible service surface，最后验证 web 复用。
  - 明确哪些本地增强必须保留，以及它们在兼容方案中以“保留扩展”还是“上游兼容字段/端点”存在。
  - 明确验证口径：至少能验证本地 DB 与上游预期 schema/语义兼容，并能用上游 web 对接到同一 runtime DB。
  - 明确 web 可复用的验收不只包含“页面能打开”，还必须覆盖 review groups、group diff、rollback/approve、maintenance stats/doctor/rebuild-search 等主链路。
- 非目标：
  - 不在本计划中直接承诺一次性整包迁入上游 frontend/backend。
  - 不在未做兼容策略确认前，直接删除本地 `system://workspace`、`system://defaults`、`system://alias`、`system://paths` 等分叉增强。
  - 不把“只补 `history_memory`”误当成 web 复用方案的完整替代。

## 范围
- 范围内：
  - `codex-rs/zmemory` 的 schema/runtime/profile 对齐评估与后续改造。
  - 本地 tool/admin/service surface 与上游 frontend 复用前提之间的差异收口。
  - 上游 web 复用所需的最小适配层策略。
  - review/snapshot/changeset 相关数据契约与 HTTP contract 的兼容策略。
  - 本地分叉能力保留清单与兼容原则。
- 范围外：
  - 与 zmemory 无关的 Codex UI/TUI 功能同步。
  - 无关模块的全仓上游同步。
  - 在未确认阶段 0 决策前，直接推进大规模 web 接线实现。

## 影响
- 受影响模块：
  - `codex-rs/zmemory/src/schema.rs`
  - `codex-rs/zmemory/src/service/*`
  - `codex-rs/tools/src/zmemory_tool.rs`
  - `codex-rs/core/src/tools/handlers/zmemory.rs`
  - 可能新增的 upstream-compatible service adapter
  - 文档与规划文件：`.agents/zmemory/*`、`docs/zmemory.md`、相关 Cadence 文档
- 受影响接口/命令：
  - 现有 zmemory function tools / hidden union tool
  - 可能新增或调整的 admin/service adapter 接口
  - 与上游 web 对接所需的 HTTP surface
- 受影响数据/模式：
  - SQLite schema（尤其 `paths`、`search_documents`、`glossary_keywords`、FTS、audit/review 相关表）
  - runtime DB path / namespace / boot contract
  - review changeset / snapshot 聚合、分组、diff、rollback 所需的数据语义
- 受影响用户界面/行为：
  - 模型侧记忆治理与“最新版本”判定能力
  - 未来的上游 web 复用能力
  - 不同入口是否稳定连接到同一数据库/同一 runtime profile
  - review/maintenance 页面是否能真实完成分组、diff、回滚、批准与治理动作

## 约束与依赖
- 约束（时间/兼容性/安全/性能/发布窗口等）：
  - 当前仓库已有明确分叉决策，不能在未更新架构决策的前提下偷偷引入与其冲突的 service/web 形态。
  - 必须优先保证 DB/runtime contract 单一事实来源，避免 CLI/core/web 连接不同库。
  - 需要最小化对现有本地增强的破坏，尤其是 runtime introspection 与 alias/path/disclosure 治理信号。
- 外部依赖（系统/人员/数据/权限等）：
  - 上游参考仓库：`/workspace/nocturne_memory`
  - 当前本地显式 DB：`/workspace/.agents/memory.db`
  - 需要用户确认是否正式放开“no daemon / no REST”历史约束，或接受单独的兼容适配层

## 实施策略
- 总体方案：
  - 采用“分层对齐”路线：先对齐 DB/schema 与数据语义，再补一层 upstream-compatible service surface，最后验证上游 web 复用。
  - 按用户要求，把“数据库兼容”视为 web 复用主前提；同时承认上游 frontend 实际仍通过 `/api/...` 工作，因此需要一层与上游兼容的接线，但这应是薄适配层，而不是新的分叉核心。
- 关键决策：
  - 是否把当前 embedded-only 决策升级为“内核 + upstream-compatible service adapter”。
  - 哪些本地增强必须保留：`system://workspace`、`system://defaults`、`system://alias`、`system://paths`、`stats/doctor` 扩展信号。
  - 是否让 schema 直接向上游收敛，还是引入兼容迁移/视图/双读写过渡层。
- 明确不采用的方案（如有）：
  - 不在 schema/runtime contract 未对齐前，直接尝试硬接上游 web。
  - 不继续维持“嵌入式-only”文档口径，同时又实际引入 web/service 复用链路。
  - 不把当前问题缩减成单一 `history_memory` 暴露问题。

## 阶段拆分
> 可按需增减阶段；简单任务可只保留一个阶段。

### 阶段 0：架构与兼容目标定版
- 目标：
  - 明确本轮对齐目标是“数据库兼容优先 + upstream-compatible service adapter”，并更新分叉保留清单。
- 交付物：
  - 新的架构/技术评审结论
  - 本地增强保留清单
  - 对齐边界说明（哪些必须兼容，哪些允许保留扩展）
- 完成条件：
  - `embedded-only` 与 `web 复用` 之间的冲突被显式决策，而不是继续隐含并存。
- 依赖：
  - 无

### 阶段 1：DB schema 与数据语义对齐
- 目标：
  - 对齐本地与上游在 `namespace`、FTS、版本链、review/admin/snapshot 所需数据语义上的差异。
- 交付物：
  - 差异矩阵
  - schema 迁移方案
  - 数据兼容/迁移/回退策略
  - review/snapshot 数据契约映射说明
- 完成条件：
  - 能明确说明本地 DB 与 changeset/review 语义何时可被上游 web/backend 安全消费。
- 依赖：
  - 阶段 0

### 阶段 2：兼容 service surface
- 目标：
  - 在不把上游服务逻辑直接塞进 `codex-zmemory` 内核的前提下，补齐上游 web 所需的最小兼容接口与 review/admin 主链路。
- 交付物：
  - adapter 设计
  - 端点映射清单
  - runtime DB / namespace / boot contract 对齐策略
  - review groups / diff / rollback / approve / maintenance contract 映射
- 完成条件：
  - frontend 所需 `/api/review`、`/api/browse`、`/api/maintenance` 等能力有清晰接线方案，且 review/snapshot 主链路未被遗漏。
- 依赖：
  - 阶段 1

### 阶段 3：web 复用验证
- 目标：
  - 用上游 web 对接本地兼容层，验证是否真正复用成功。
- 交付物：
  - 手动验证记录
  - 回归风险清单
  - 后续收敛建议
- 完成条件：
  - 能确认 web/CLI/core 连接的是同一 dbPath / runtime profile，且核心浏览/review/maintenance 流程可用。
- 依赖：
  - 阶段 2

## 测试与验证
> 只写当前仓库中真正可执行、可复现的验证入口；如果没有自动化载体，就写具体的手动检查步骤，不要写含糊表述。

- 核心验证：
  - `cd /workspace/codex-rs && cargo nextest run -p codex-zmemory`（或等价 targeted test）验证 schema/service 语义
  - `cd /workspace/codex-rs && cargo nextest run -p codex-core --test all zmemory_` 验证 tool handler 与 runtime contract
  - 针对 schema 差异新增兼容测试：本地 DB 初始化后，核对关键表/索引/FTS 结构是否满足上游消费前提
- 必过检查：
  - `cd /workspace/codex-rs && just fmt`
  - 若阶段涉及较大 Rust 变更：`cd /workspace/codex-rs && just fix -p codex-zmemory`（必要时按模块扩展）
- 回归验证：
  - `system://workspace`、`system://defaults`、`system://boot` 继续正确反映当前 runtime profile
  - turn cwd 覆盖后，CLI/core/web 仍连接同一目标库
  - 本地保留增强未因兼容层引入而静默失效
- 手动检查：
  - 用上游 frontend 的 `/api` 调用链路，确认 review/browse/maintenance 页面能读到同一 DB 数据
  - 人工走通 `review groups -> diff -> rollback/approve` 主链路，以及 `maintenance stats/doctor/rebuild-search` 主链路
  - 人工核对 `dbPath`、namespace、boot anchor、recent/review/admin/snapshot 数据是否一致
- 未执行的验证（如有）：
  - 当前尚未实施，web 复用只完成了规划级分析，尚无真实接线验证

## 风险与缓解
- 关键风险：
  - schema 直接收敛到上游后，可能破坏本地现有 DB 与运行时视图合同
  - web/CLI/core 连接不同库，造成“看起来像记忆混乱”的假象
  - 上游同步覆盖本地分叉增强
  - service adapter 范围失控，反而把核心 crate 再次膨胀
- 触发信号：
  - `system://workspace.dbPath` 与 web 实际连接库不一致
  - namespace/review/admin 页面读取异常或数据缺列
  - 现有 `system://alias`、`system://paths`、`stats/doctor` 信号丢失或退化
- 缓解措施：
  - 先做 schema/runtime 差异矩阵，再做迁移
  - 为 dbPath / namespace / boot contract 建立跨入口一致性测试
  - 在阶段 0 先冻结“本地增强保留清单”
  - 把上游兼容层放在内核外侧，避免直接把 web/service 复杂度塞进 crate
- 回滚/恢复方案（如需要）：
  - schema 迁移采用可回滚或可导出恢复的策略
  - 兼容层与内核改造分步提交，必要时可先回退 adapter 层而保留内核改进

## 参考
- `/workspace/.agents/zmemory/architecture.md`
- `/workspace/.agents/zmemory/tech-review.md`
- `/workspace/.agents/zmemory/tasks.md`
- `/workspace/.agents/zmemory/qa-report.md`
- `/workspace/.ai/memory/known-risks.md`
- `/workspace/.ai/memory/decisions/2026-04-04-上游同步前先做分叉功能保留审计.md`
- `/workspace/.agents/plan/2026-04-08-zmemory-history-tool-exposure.md`
- `/workspace/codex-rs/zmemory/src/schema.rs`
- `/workspace/codex-rs/zmemory/src/tool_api.rs`
- `/workspace/codex-rs/tools/src/zmemory_tool.rs`
- `/workspace/codex-rs/tools/src/tool_registry_plan.rs`
- `/workspace/codex-rs/tools/src/tool_registry_plan_tests.rs`
- `/workspace/codex-rs/core/src/tools/handlers/zmemory.rs`
- `/workspace/nocturne_memory/rust/migrations/0001_core.sql`
- `/workspace/nocturne_memory/rust/migrations/0004_namespace.sql`
- `/workspace/nocturne_memory/rust/src/db.rs`
- `/workspace/nocturne_memory/rust/src/mcp.rs`
- `/workspace/nocturne_memory/backend/api/review.py`
- `/workspace/nocturne_memory/backend/api/maintenance.py`
- `/workspace/nocturne_memory/backend/db/snapshot.py`
- `/workspace/nocturne_memory/backend/tests/api/test_api_routes.py`
- `/workspace/nocturne_memory/frontend/src/lib/api.js`
