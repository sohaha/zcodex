# 2026-04-22 ztldr shell 拦截建议不能给出缺少 language 的 semantic 模板

- 这轮问题最初看起来像“agent 没把 `ztldr` 用深”，但真正的根因更靠前：`shell_search_rewrite` 在拦截广义 `rg`/`grep` 查询时，会生成一份建议的 `ztldr` 参数 JSON；当查询没有可推断语言的路径时，这份建议会把 `semantic` 暴露成一个缺少 `language` 的模板。
- `native-tldr` 的真实约束在 `codex-rs/native-tldr/src/tool_api.rs`：`semantic`、`structure`、`context`、`impact`、`calls` 等 action 运行时要求显式 `language`。只有 `extract`、`imports`、`slice`、`diagnostics` 这类带 `path` 的 action 才可能从扩展名推断语言。
- 之前的问题不只是“工具描述没写清楚”，而是共享 seam 自己在生成一个无效调用模板。模型拿到这份模板后，即便遵循了“先用 `ztldr`”的提示，也可能直接触发 `structuredFailure`，然后退回源码阅读，形成“用了工具但没真正得到结构证据”的假象。

## 本轮修正

- 在 `native-tldr` 的 `TldrToolCallParam.language` schema 描述里前置 action 级语言要求。
- 在 `codex-rs/tools/src/tool_spec.rs` 和 `codex-rs/mcp-server/src/tldr_tool.rs` 的主 tool description 中明确：
  - 哪些 action 要求 `language`
  - `semantic` 因缺少 `language` 失败时应重跑，不算有效分析
- 在 `codex-rs/core/src/tools/rewrite/tldr_routing.rs` 的 shell 拦截提示里补充缺参提醒：如果建议的 action 仍缺少显式 `language`，必须先补参，不得把缺参 `structuredFailure` 当作分析结果。
- 补测试锁住：
  - `tools` 主 description 和 `language` 参数描述
  - MCP tool schema 的 `language` 描述
  - shell 拦截在 `semantic` 且未推断出语言时必须提示补 `language`

## 结论

- 以后遇到 `ztldr` 误用，不要只检查文档和 tool description；还要检查共享 rewrite/interception 层是否在生成非法但看起来“像正确答案”的建议参数。
- 对共享工具的 agent-first 提示，必须同时验证三层：
  - 真实运行时约束
  - tool description / schema 暴露
  - rewrite/intercept 给模型的建议参数
- `structuredFailure` 需要分两类看：daemon/环境类失败和参数契约类失败。后者优先修提示与参数成形，不是继续扩写“遇到失败请汇报”。
