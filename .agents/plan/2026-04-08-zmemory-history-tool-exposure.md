# Zmemory “最新版本”可验证读取能力（history_memory）补齐

> 不适用项写 `无`，不要留空。
> 只写已确认事实；若信息不清楚或需要假设，先回到对话澄清，不要把未确认内容写进计划。

## 背景
- 当前状态：
  - 模型侧当前仅可调用 `read_memory` / `search_memory` / `create_memory` / `update_memory`。
  - `codex-zmemory` 底层动作层已支持 `history`（可输出版本列表与 `createdAt` / `deprecated` / `migratedTo` 等元信息），CLI 入口 `codex zmemory history <uri>` 可用。
  - `codex-core` 当前的 zmemory handler 未暴露 `history` 为模型可见工具名；`spec_tests` 也未把它列入默认工具集合。
- 触发原因：
  - 用户要求“找最新的称呼偏好”，但工具层缺少“版本历史/更新时间”信息，导致无法在模型侧可靠判定“最新”。
  - 现有协作契约记忆 `core://agent/my_user` 中存在多条互相冲突的称呼规则，进一步放大了“最新/有效”判定难度。
- 预期影响：
  - 补齐 `history_memory` 后，模型可在需要时读取某个 URI 的版本历史，从而在不猜测的前提下回答“最新是哪条/什么时候改的/是否已废弃”等问题。

## 目标
- 目标结果：
  - 新增模型可见工具 `history_memory`，将其映射到 zmemory 的 `history` action。
  - 使“找最新”具备可复现的验证入口（基于版本时间戳与 deprecated 标记）。
- 完成定义（DoD）：
  - `history_memory` 可被模型调用并返回结构化 JSON（与现有 `read_memory` 等一致的 `---BEGIN_ZMEMORY_JSON---` 包裹）。
  - 默认模型工具列表包含 `history_memory`，并有对应单测覆盖。
  - 至少补充一条最小的回归验证：对 `core://my_user` 之类 URI 能读到多个版本并含 `createdAt` 字段。
- 非目标：
  - 不在本轮强制治理/重写 `core://agent/my_user` 的内容（该项可作为后续独立 issue）。
  - 不改 DB schema、不引入新存储字段、不新增前端/REST API。

## 范围
- 范围内：
  - `codex-core`：暴露新工具名并解析参数；把该工具加入 model-visible tool specs；补测试。
  - `codex-zmemory`：不改动作层逻辑（仅按需补文档/注释或小的结构性适配）。
- 范围外：
  - 自动合并/去重/“选择一条有效称呼”的业务规则自动化（需要用户明确偏好策略）。
  - 对 `system://boot` 的锚点 URI 体系做破坏性迁移或默认值变更。

## 影响
- 受影响模块：
  - `codex-rs/core/src/tools/handlers/zmemory.rs`
  - `codex-rs/core/src/tools/spec_tests.rs`
  - 可能受影响：`codex-rs/core/src/memories/prompts*.rs`（仅当需要更新开发者指引列出工具名）
- 受影响接口/命令：
  - 新增：`history_memory`（function tool）
- 受影响数据/模式：
  - 无（仅读取现有版本表）
- 受影响用户界面/行为：
  - 模型在回答“最新/最近一次变更”类问题时可给出可验证依据（版本时间戳/废弃标记），减少推断。

## 约束与依赖
- 约束（时间/兼容性/安全/性能/发布窗口等）：
  - 变更需最小化：只新增一个工具名与对应解析；避免改动现有工具语义。
  - 不引入静默降级：若 `history_memory` 请求不存在的 URI，应显式返回错误（保持与 `read_memory` 一致的“memory not found”）。
- 外部依赖（系统/人员/数据/权限等）：
  - 无（本地仓库内可验证；不依赖联网资源）。

## 实施策略
- 总体方案：
  - 在 `codex-core` 的 zmemory handler 中增加 `history_memory` 分支，将参数反序列化为 `{ uri: String }`，并构造 `ZmemoryToolCallParam { action: History, uri: Some(uri), .. }`。
  - 在 tool spec 构建与单测中将 `history_memory` 纳入默认 model-visible 工具列表（与现有 `read_memory` 等保持同等级）。
- 关键决策：
  - “最新”默认判定策略不在工具层硬编码；由调用方（模型/上层逻辑）基于 `deprecated=false`、`createdAt` 等字段自行解释。
- 明确不采用的方案（如有）：
  - 不把 `createdAt/updatedAt` 强塞进 `read_memory` 回包（会扩散到所有读取场景，且需要讨论兼容性与 schema 稳定性）。

## 阶段拆分

### 阶段 1：暴露 history_memory 工具
- 目标：
  - 模型可调用 `history_memory` 并拿到版本列表。
- 交付物：
  - 代码变更：handler + tool specs + 最小测试。
- 完成条件：
  - 单测通过；本地执行 `codex zmemory history <uri>` 与工具返回内容一致性可人工比对。
- 依赖：
  - 无。

### 阶段 2：回归验证与文档对齐（最小）
- 目标：
  - 确保后续不会再次“有底层能力但 tool 未暴露”。
- 交付物：
  - 测试：覆盖默认工具列表包含 `history_memory`。
  - 如需要：更新 zmemory developer instructions 中“model-visible tools”清单（确保指引不误导）。
- 完成条件：
  - `cargo nextest run -p codex-core`（或等价命令）通过。
- 依赖：
  - 无。

## 测试与验证

- 核心验证：
  - 单测：默认工具列表包含 `history_memory`。
  - 单测/集成：调用 handler 的 `history_memory` 能解析参数并返回结构化 JSON。
- 必过检查：
  - `just fmt`（在 `codex-rs` 目录）。
  - 目标 crate 测试通过（优先 nextest）。
- 回归验证：
  - 继续确认 `read_memory` / `search_memory` / `create_memory` / `update_memory` 不受影响（至少编译与单测覆盖）。
- 手动检查：
  - 对同一 URI（例如 `core://my_user`）分别运行 `codex zmemory history core://my_user --json` 与模型调用 `history_memory` 的输出进行字段对齐检查（版本数量、`createdAt`、`deprecated`）。
- 未执行的验证（如有）：
  - 无（本计划要求完成上述最小验证）。

## 风险与缓解
- 关键风险：
  - 工具名新增可能影响默认模型工具顺序/列表，导致相关快照或断言变更。
  - `history` 输出体积在版本很多时可能变大，需确保有合理的 limit（若底层已有 limit，沿用；否则在 tool 参数层加默认上限）。
- 触发信号：
  - `spec_tests` 失败或任何工具列表相关断言失败。
  - `history_memory` 回包过大导致 token/输出截断。
- 缓解措施：
  - 为 `history_memory` 引入默认 limit（与 `codex-zmemory` 的 `history` 行为保持一致）。
  - 保持输出结构与现有 zmemory handler 一致（同样的 JSON 包裹与成功/失败标记）。
- 回滚/恢复方案（如需要）：
  - 若发现工具列表变化影响面过大，可先将 `history_memory` 置于 feature gate（仅在 `Feature::Zmemory` 下暴露，且不影响非 zmemory 场景）。

## 参考
- `codex-rs/zmemory/README.md`
- `codex-rs/zmemory/src/tool_api.rs`
- `codex-rs/core/src/tools/handlers/zmemory.rs`
- `codex-rs/core/src/tools/spec_tests.rs`
- 复现命令：`codex zmemory history core://my_user --json`
