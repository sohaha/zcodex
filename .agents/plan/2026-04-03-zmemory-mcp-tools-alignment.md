# zmemory MCP 工具对齐与主动使用

> 不适用项写 `无`，不要留空。
> 只写已确认事实；若信息不清楚或需要假设，先回到对话澄清，不要把未确认内容写进计划。

## 背景
- 当前状态：`zmemory` 已有内置动作层与工具 handler，支持 `read/search/create/update/delete-path/add-alias/manage-triggers/stats/doctor/rebuild-search`，并通过 core handler 调用。用户反馈当前 agent 不会主动使用 zmemory，且现有工具形态不足以支撑 agent 使用。
- 触发原因：需要参考 Nocturne Memory 的 MCP tool 形态，完善 Codex 的 zmemory 内置记忆系统。
- 预期影响：agent 可以通过更贴近 MCP 的工具面主动读写/检索记忆；提示与工具契约更清晰。

## 目标
- 目标结果：在 Codex 内提供与 Nocturne Memory MCP tools 对齐的工具接口与使用指引，并保持现有 zmemory 能力不回退。
- 完成定义（DoD）：
  - 提供 MCP 风格工具名（如 `read_memory` / `search_memory` / `create_memory` / `update_memory` / `delete_memory` / `add_alias` / `manage_triggers`）与参数契约，并映射到现有 zmemory 动作层。
  - agent 侧具备清晰的主动使用指引（读取、检索、写入触发条件）。
  - 相关文档与测试覆盖对齐更新。
- 非目标：引入新的存储后端、替换现有 CLI、重做图数据模型、引入远程 MCP 服务端。

## 范围
- 范围内：core 工具注册/handler、zmemory 工具契约与文档、agent 指令/模板中对记忆工具的使用指导、必要的回归/契约测试。
- 范围外：Nocturne Memory 的 Web/HTTP 管理面、外部 MCP 服务器部署能力、跨进程 SSE/HTTP 传输。

## 影响
- 受影响模块：`codex-rs/core/src/tools/handlers/zmemory.rs`、zmemory tool api/service、工具注册与说明模板、相关文档与测试。
- 受影响接口/命令：内置工具调用（agent 侧可见的 tool 名称与参数）、`codex zmemory` CLI（需保持兼容）。
- 受影响数据/模式：无（保持 SQLite + FTS5 与现有 schema）。
- 受影响用户界面/行为：agent 在对话中可主动 read/search/create/update 记忆。

## 约束与依赖
- 约束（时间/兼容性/安全/性能/发布窗口等）：保持现有 zmemory tool 行为与 CLI 不回退；不扩大到非 zmemory 体系。
- 外部依赖（系统/人员/数据/权限等）：参考 Nocturne Memory MCP tool 合同与使用指引文档。

## 实施策略
- 总体方案：基于现有 zmemory 动作层增加一层 MCP 风格工具适配（命名与参数对齐），并在 core 的工具注册/提示模板中写入“何时 read/search/create/update”的主动使用指引。
- 关键决策：复用现有 `zmemory` 动作层与系统视图；通过适配层实现 MCP 命名与参数对齐，避免引入新后端或重复实现。
- 明确不采用的方案（如有）：不新建独立 MCP 服务器进程；不替换现有 CLI；不引入远程传输协议实现。

## 阶段拆分
> 可按需增减阶段；简单任务可只保留一个阶段。

### 契约与映射梳理
- 目标：对齐 MCP 工具名与参数，并确定与现有 zmemory action 的映射关系。
- 交付物：工具映射表与参数契约说明；需要新增/调整的系统视图或行为清单。
- 完成条件：映射关系明确且不引入未验证行为。
- 依赖：Nocturne Memory MCP tools 文档与现有 zmemory tool api。

### 工具与提示接线
- 目标：在 Codex core 中注册 MCP 风格工具入口，并连接到 zmemory 动作层；完善 agent 主动使用指引。
- 交付物：工具注册/handler 代码改动；提示模板或工具说明改动。
- 完成条件：MCP 风格工具可调用并正确转发到 zmemory；agent 指引已更新。
- 依赖：现有工具注册机制与 zmemory handler。

### 测试与文档对齐
- 目标：补齐契约与回归测试，更新文档。
- 交付物：相关测试、`docs/zmemory.md`/相关 README 更新。
- 完成条件：关键测试通过，文档与实现一致。
- 依赖：现有测试框架与文档结构。

## 测试与验证
> 只写当前仓库中真正可执行、可复现的验证入口；如果没有自动化载体，就写具体的手动检查步骤，不要写含糊表述。

- 核心验证：`cargo nextest run -p codex-core --test all zmemory_ --quiet`（若无 cargo-nextest，则 `cargo test -p codex-core --test all zmemory_ --quiet`）
- 必过检查：`cargo nextest run -p codex-cli --test zmemory --quiet`（若无 cargo-nextest，则 `cargo test -p codex-cli --test zmemory --quiet`）
- 回归验证：`cargo nextest run -p codex-zmemory --quiet`（若无 cargo-nextest，则 `cargo test -p codex-zmemory --quiet`）
- 手动检查：调用新工具名执行 read/search/create 的 smoke 校验（记录返回结构是否含 `system://workspace` 等系统视图）。
- 未执行的验证（如有）：无

## 风险与缓解
- 关键风险：工具命名/参数对齐后与现有 `zmemory` 工具发生语义偏差。
- 触发信号：新工具调用结果与原 `zmemory` 行为不一致或文档冲突。
- 缓解措施：以现有动作层为单一事实来源，映射层只做参数规范化；补充契约测试覆盖。
- 回滚/恢复方案（如需要）：保留原 `zmemory` 工具入口；如 MCP 工具出现问题可先禁用新注册入口。

## 参考
- `docs/zmemory.md`
- `codex-rs/core/src/tools/handlers/zmemory.rs`
- `/workspace/_ext/nocturne_memory/README.md`
- `/workspace/_ext/nocturne_memory/docs/TOOLS.md`
