# 2026-04-23 ztldr `matchMode` 错值修复必须同时更新共享 parser 和 schema 文案

- 这次用户反馈不是字段名拼错，而是值域漂移：上一轮把 `ztldr search` 的 `matchMode` 写成了 `substring`。运行时真实枚举只有 `literal|regex`，所以如果只靠现有 schema/description，模型仍可能继续生成这个旧词。
- 之前 `native-tldr` 的共享 parser 只修字段 alias（`pattern -> query`、`match_mode -> matchMode` 等），没有修 value alias；同时 `TLDR_TOOL_MATCH_MODE_DESCRIPTION` 和 `TldrToolCallParam` 的 `schemars` 描述也没把合法值列全，导致“公共 description 已更新，但 struct schema 仍旧模糊”的双轨漂移风险。

## 本轮修正

- 在 `codex-rs/native-tldr/src/tool_api.rs` 的共享归一化层新增 value repair，把 `matchMode=substring` 统一折叠成 `literal`。
- 同步更新共享 tool description、`matchMode` 字段描述和 retry hint，显式说明合法值只有 `literal`（默认）与 `regex`。
- MCP 入口补回归测试，确认通过 `run_tldr_tool(...)` 走共享 parser 时，`substring` 会被修正后成功执行。
- agent-first 文档与 MCP 接口文档同步写清这条约定，避免提示词继续把旧词教给模型。

## 结论

- 对共享工具参数的“错值修复”，不能只修 parser 或只修 description；必须把 value repair、struct schema 描述、对外文档一起更新，否则不同入口会继续漂移。
- `schemars` 上的字段说明如果与共享常量重复，改动时必须检查两边是否同改；否则自动导出的 schema 会悄悄落回旧 contract。
