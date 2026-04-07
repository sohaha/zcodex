# 将 zmemory + tldr 作为 MCP 服务能力对外提供

> 不适用项写 `无`，不要留空。
> 只写已确认事实；若信息不清楚或需要假设，先回到对话澄清，不要把未确认内容写进计划。

## 背景
- 当前状态：`codex mcp-server` 已可作为 MCP 服务启动；`tldr` 已有 mcp-server 侧接线并通过 `tldr` feature 选择性暴露；`zmemory` 已有内置动作层、MCP 风格工具 schema 与 core handler，但尚未接入 `codex mcp-server` 的 `tools/list` / `tools/call`。
- 触发原因：用户希望在接受“每个项目一个 `codex mcp-server`”运行模型的前提下，让 `zmemory + tldr` 都可以作为 MCP 服务能力对外提供。
- 预期影响：其他 MCP 客户端在连接项目级 `codex mcp-server` 时，可以调用 `zmemory` 相关工具；`tldr` 继续保持现有 MCP 暴露能力与 feature gate。

## 目标
- 目标结果：在项目级 `codex mcp-server` 中，对外稳定提供 `zmemory` MCP 工具，并保持 `tldr` 现有 MCP 服务能力可用。
- 完成定义（DoD）：
  - `codex mcp-server` 的 `tools/list` 可列出对外承诺的 `zmemory` MCP 工具。
  - `codex mcp-server` 的 `tools/call` 可成功调用这些 `zmemory` 工具，并返回与现有动作层一致的 `content` / `structuredContent`。
  - `tldr` 的现有 MCP 暴露与 feature gate 行为不回退。
  - 文档明确说明“每个项目一个 `codex mcp-server`”与 `system://workspace` 自检方式。
  - 补齐最小闭环测试，覆盖 `tools/list`、`tools/call` 与关键负例。
- 非目标：
  - 不支持单个 `codex mcp-server` 在多个项目之间动态切换 `zmemory` 上下文。
  - 不把 `zmemory` 单独拆成独立 server 进程。
  - 不在第一版中扩展新的 `zmemory` 存储后端、REST API 或 daemon。

## 范围
- 范围内：
  - `codex-rs/mcp-server` 中 `zmemory` 的 MCP 工具导出、调用接线与返回包装。
  - `codex-rs/tools` / `codex-rs/core` 中可复用的 `zmemory` schema 与参数适配抽取。
  - `codex-rs/docs/codex_mcp_interface.md` 与相关 README 的文档更新。
  - `mcp-server` 与现有 `zmemory` 相关测试更新。
- 范围外：
  - 单 server 多项目复用。
  - `delete_memory`、`add_alias`、`manage_triggers` 的第一版对外暴露承诺。
  - 与 HTTP 传输、远程部署、外部托管服务有关的扩展。

## 影响
- 受影响模块：
  - `codex-rs/mcp-server/src/message_processor.rs`
  - `codex-rs/mcp-server/src/` 下新增或调整的 zmemory MCP 适配层
  - `codex-rs/tools/src/zmemory_tool.rs`
  - `codex-rs/core/src/tools/handlers/zmemory.rs`
  - `codex-rs/docs/codex_mcp_interface.md`
  - `codex-rs/mcp-server/tests/suite/codex_tool.rs`
- 受影响接口/命令：
  - `codex mcp-server`
  - MCP `tools/list`
  - MCP `tools/call` 中的 `read_memory` / `search_memory` / `create_memory` / `update_memory`
- 受影响数据/模式：无新增存储模式；继续复用现有 `zmemory.db` 路径解析与 `structuredContent` 形状。
- 受影响用户界面/行为：MCP 客户端可在连接项目级 `codex mcp-server` 时直接调用 `zmemory` 工具；调用前后可通过 `read_memory(system://workspace)` 观察当前工作区绑定的运行时事实。

## 约束与依赖
- 约束（时间/兼容性/安全/性能/发布窗口等）：
  - 必须保持 `tldr` 现有 feature gate 与返回契约不回退。
  - 必须保持 `zmemory` 路径解析遵循现有“项目级默认 + 显式 `[zmemory].path` 覆盖”的规则。
  - 必须避免在 `core` 与 `mcp-server` 各维护一份独立的 `zmemory` 参数映射逻辑。
- 外部依赖（系统/人员/数据/权限等）：
  - 依赖现有 `codex-zmemory` 动作层与 `codex-native-tldr` 的现有能力。
  - `tldr` 的 MCP 暴露仍依赖 `codex-mcp-server --features tldr` 或等价透传构建方式。

## 实施策略
- 总体方案：
  - 采用“每个项目一个 `codex mcp-server`”模型；在服务端直接复用现有 `zmemory` schema 与动作层，为 `tools/list` 和 `tools/call` 增加 `zmemory` MCP 工具接线；`tldr` 保持现有 MCP 集成方式。
- 关键决策：
  - 第一版仅对外承诺 4 个 `zmemory` MCP 工具：`read_memory`、`search_memory`、`create_memory`、`update_memory`。
  - 不为这些工具新增 `cwd` / `projectRoot` / `dbPath` 参数；server 进程级别绑定项目，调用级别只传业务参数。
  - 对外文档统一要求调用方先使用 `read_memory(system://workspace)` 确认当前项目与 DB。
- 明确不采用的方案（如有）：
  - 不在 `mcp-server` 中手写第二份 `tool name -> ZmemoryToolCallParam` 映射逻辑。
  - 不在第一版里把 7 个内部支持的 `zmemory` MCP 名称全部公开承诺。
  - 不通过单一全局 server 承担多项目 `zmemory` 访问。

## 阶段拆分
> 可按需增减阶段；简单任务可只保留一个阶段。

### 共享契约收敛
- 目标：统一 `zmemory` 的对外 schema、工具名集合与参数适配来源，消除 core / mcp-server 双份映射风险。
- 交付物：共享适配层设计与确定的对外工具集合（4 个工具）。
- 完成条件：`mcp-server` 与 core 共用同一套 `zmemory` tool name -> call param 语义；不再依赖复制粘贴式分叉。
- 依赖：`codex-rs/tools/src/zmemory_tool.rs`、`codex-rs/core/src/tools/handlers/zmemory.rs`。

### mcp-server 接线
- 目标：在 `codex mcp-server` 中完成 `zmemory` 的 `tools/list` 与 `tools/call` 接线，并保持 `tldr` 行为不回退。
- 交付物：`mcp-server` 侧 `zmemory` 适配文件、`message_processor` 接线改动、返回包装逻辑。
- 完成条件：MCP 客户端可列出并调用 4 个 `zmemory` 工具；`tldr` 相关分支继续按 feature gate 工作。
- 依赖：共享契约收敛结果、`run_zmemory_tool_with_context()`、现有 `tldr` MCP 接线模式。

### 文档与验证闭环
- 目标：补齐项目级 server 使用说明、运行时自检说明与最小测试闭环。
- 交付物：更新后的 MCP 文档与 README、`mcp-server` 测试、必要的现有测试更新。
- 完成条件：文档明确项目级绑定语义；测试覆盖 `tools/list`、`tools/call`、scope/负例；相关现有 `zmemory` 语义测试继续通过。
- 依赖：前两个阶段完成后的实际工具集合与返回结构。

## 测试与验证
> 只写当前仓库中真正可执行、可复现的验证入口；如果没有自动化载体，就写具体的手动检查步骤，不要写含糊表述。

- 核心验证：`cargo nextest run -p codex-mcp-server --test all`（若无 `cargo-nextest`，则 `cargo test -p codex-mcp-server --test all`）
- 必过检查：
  - `cargo nextest run -p codex-core --test all zmemory_ --quiet`
  - 若无 `cargo-nextest`，则 `cargo test -p codex-core --test all zmemory_ --quiet`
- 回归验证：
  - 构建不带 `tldr` feature 的 `codex-mcp-server`，确认 `tools/list` 不回归现有非 `tldr` 行为。
  - 构建带 `tldr` feature 的 `codex-mcp-server`，确认 `tldr` 仍出现在 `tools/list`。
- 手动检查：
  - 在目标项目根目录启动 `codex mcp-server`。
  - 用 MCP 客户端调用 `read_memory` 读取 `system://workspace`，确认 `dbPath` / `workspaceBase` / `source` 与目标项目一致。
  - 调用 `create_memory`、`search_memory`、`update_memory` 做一轮 smoke test。
- 未执行的验证（如有）：无

## 风险与缓解
- 关键风险：`mcp-server` 未正确绑定或传递项目上下文，导致 `zmemory` 写入错误的项目 DB。
- 触发信号：工具调用成功但在目标项目中读不到记忆，或 `system://workspace` 报告的 `workspaceBase` / `dbPath` 与目标项目不一致。
- 缓解措施：
  - 坚持项目级 server 语义，不新增调用级路径切换参数。
  - 将 `system://workspace` 作为对外自检入口写入文档与手动检查。
  - 保留并复用现有与路径解析相关的 `zmemory` 回归测试。
- 回滚/恢复方案（如需要）：若 `zmemory` MCP 接线出现不可接受回归，可先移除 `mcp-server` 中新增的 `zmemory` tool 暴露，保留原有 `codex` / `codex-reply` / `tldr` 能力，并继续使用本地 `codex zmemory` 与 core 内置工具。

## 参考
- `codex-rs/mcp-server/src/message_processor.rs:318`
- `codex-rs/mcp-server/src/message_processor.rs:344`
- `codex-rs/mcp-server/src/tldr_tool.rs:37`
- `codex-rs/tools/src/zmemory_tool.rs:8`
- `codex-rs/tools/src/zmemory_tool.rs:350`
- `codex-rs/tools/src/tool_registry_plan.rs:241`
- `codex-rs/core/src/tools/handlers/zmemory.rs:143`
- `codex-rs/core/src/tools/handlers/zmemory.rs:246`
- `codex-rs/core/templates/zmemory/write_path.md:24`
- `codex-rs/core/tests/suite/zmemory_e2e.rs:782`
- `codex-rs/core/tests/suite/zmemory_e2e.rs:1014`
- `codex-rs/core/tests/suite/zmemory_e2e.rs:1078`
- `codex-rs/core/tests/suite/zmemory_e2e.rs:1779`
- `codex-rs/mcp-server/Cargo.toml:13`
- `codex-rs/docs/codex_mcp_interface.md:1`
- `docs/zmemory.md:14`
- `docs/zmemory.md:124`
