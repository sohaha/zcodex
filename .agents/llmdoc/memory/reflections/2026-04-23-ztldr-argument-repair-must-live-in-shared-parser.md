# 2026-04-23 ztldr 错参修复必须收敛到共享解析层，而不是分散在各入口补 alias

- 这次用户直接点出了真实痛点：agent 不是不会用 `ztldr`，而是容易在 `query` / `pattern`、`matchMode` / `match_mode`、`path` / `file`、`paths` / 单一路径之间来回混淆。之前如果只在单个入口补 description 或 serde alias，看起来能止血，但 `core`、Responses、MCP 仍会继续漂。
- `ztldr` 的外部入口至少有三层：`native-tldr` 真实执行契约、`tools`/Responses 暴露给模型的 function schema，以及 `mcp-server` 手写的 MCP `inputSchema`。如果“错参修复”没有下沉到共享 parser，任何一个入口都可能继续复发“这边能跑、那边报错”的不一致。

## 本轮有效做法

- 在 `codex-rs/native-tldr/src/tool_api.rs` 增加共享的 `parse_tldr_tool_call_*` 入口，让参数归一和错误提示成为单一事实源。
- 兼容修复不要只做字段 alias；还要做 action-aware shape repair：
  - `pattern -> query`
  - `match_mode -> matchMode`
  - `file/filePath -> path`
  - `change-impact` 的单一路径请求自动归一成 `paths`
- 运行时契约要和 schema 同步演进：既然 `change-impact` 已能从 `paths` 推断语言，`tools` 和 MCP schema 的 required 列表也必须同步去掉 `language`，否则模型仍会被旧 required 约束误导。
- `core` 的 handler 不应继续走通用 `parse_arguments<T>()`，而应复用 `native-tldr` 的共享 parser；这样模型从 function tool 直接调用时也能享受同一套修复。

## 结论

- 对多入口共享工具，优先收敛“解析 + 修复 + retry hint”到能力 crate，再让外层入口复用；不要在每个入口局部打补丁。
- schema/description 的唯一职责是把 canonical contract 教给模型；兼容修复属于 parser，不能反过来让 description 继续漂成多个版本。
- 验证时要同时覆盖：
  - `native-tldr` 的共享 parser / 真实执行
  - `mcp-server` 的 MCP tool 调用
  - `core` handler 到 parser 的接线
