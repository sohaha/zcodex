# 2026-04-19 本地分析工具不能只保留 crate/CLI；还必须接入共享 tool plan 与 tests/all 聚合面

- 这轮 `zmemory` / `ztldr` 继续收口时，真正暴露的问题不是 crate 缺失，也不是 CLI 顶层子命令缺失，而是共享运行时接线不完整：
  - `codex-rs/tools/src/tool_registry_plan.rs` 保留了 `ToolsConfig.zmemory_tool_enabled`，但没有把 `create_zmemory_tool()` / `create_zmemory_mcp_tools()` 推入 tool plan，也没有注册 `ToolHandlerKind::Zmemory`。
  - 同一个 `tool_registry_plan.rs` 里也没有把 `create_tldr_tool()` 和 `ToolHandlerKind::Tldr` 推入 plan。
  - 结果是 CLI `codex zmemory` / `codex ztldr` 仍可见，但 `codex-core` 运行时给模型暴露的工具列表和 handler 映射是半断开的，实际表现就是 `unsupported call: zmemory`、`unsupported call: read_memory`、`unsupported call: ztldr` 这类占位回退。

## 结论

- 本地分叉功能是否“保住了”，不能只看：
  - crate 还在
  - 顶层 CLI help 还在
  - 单个 handler 文件还在
- 还必须同时核对两层：
  - `codex-rs/tools/src/tool_registry_plan.rs` 是否真的把 spec 和 handler 注册进共享 plan。
  - `codex-rs/core/tests/suite/mod.rs` 是否真的把对应 e2e 文件挂进 `tests/all` 聚合面。

## 本轮证据

- `core/tests/suite/zmemory_e2e.rs` 原先存在但没接进 `mod.rs`，所以长期没有被编译和执行。
- `core/tests/suite/tldr_e2e.rs` 也存在但同样没接进 `mod.rs`。
- 一旦把 `zmemory_e2e` 接进来，就会立刻暴露大面积 `unsupported call:*`，证明缺口在共享 tool registry，而不是测试本身。
- 把 `ztldr` / `zmemory` 真正注册进 tool plan 后：
  - `suite::zmemory_e2e` 31 个 case 全通过。
  - `suite::tldr_e2e` 通过，并额外验证请求体中的工具列表确实包含 `ztldr`。

## 后续规则

- 以后同步上游或做本地特性回归时，凡是“工具型本地能力”，至少检查这四个面：
  - CLI 入口是否仍注册并可 dispatch。
  - `tool_registry_plan.rs` 是否仍 push spec + register handler。
  - `tests/all` 聚合是否包含对应 e2e 模块。
  - 至少一条测试断言工具确实出现在模型请求的 `tools` 列表里，而不是只测 handler 能处理伪造来包。
