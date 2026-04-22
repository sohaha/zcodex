# ztldr Responses schema 与工具描述必须共享同一份 language 契约

日期：2026-04-22

## 背景
- 这次要彻底修 `ztldr` 被 agent 错误使用的问题，表面现象是 agent 调 `semantic` 时缺少 `language`，随后还把失败当成“做过语义分析”。
- 初看像是 shell interception 提示不够好，但深挖后发现至少有两条独立误导链：
  1. shell interception 会生成缺少必填 `language` 的建议参数。
  2. Responses API 主链路会把 `ztldr` 顶层 `oneOf` 压平成 object，丢掉 action 级必填约束。

## 这次学到的
- `ztldr` 这类 action-by-action 契约工具，不能只在 MCP schema 修 `oneOf`；还必须检查 Responses API 的共享工具序列化入口最终发给 provider 的是不是纯 object 顶层。当前 provider 连顶层 `oneOf` 本身都不接受，所以“顶层 object + 保留 oneOf”也还是错的。
- shell interception 即使已经不再生成缺参 JSON，工具 description 依然可能继续把模型带回错误路径。描述里的“下一步建议”也必须和 `tool_api.rs` 的真实参数约束完全一致。
- 对 `semantic` 缺 `language` 的补救建议，正确顺序是：
  1. 先补 `language` 重试；
  2. 如果手头只有文件路径，再改用 `extract` / `imports` / `slice` / `diagnostics` 这类可按 `path` 推断语言的 action；
  3. 若是 regex 或逐字核对，明确退回 raw grep/read。

## 这次改动落点
- `codex-rs/core/src/tools/rewrite/shell_search_rewrite.rs`
  - 对必须显式 `language` 但当前推断不出的 action，不再生成 `Suggested ztldr arguments` JSON。
- `codex-rs/core/src/tools/rewrite/tldr_routing.rs`
  - shell interception 文案改成只提示“先补 `language`”，并明确缺参 `structuredFailure` 不是有效分析。
- `codex-rs/tools/src/tool_spec.rs`
  - `ztldr` 参数 schema 仍保留 action 级 `oneOf` 作为内部 richer contract。
  - Responses API 序列化时必须把它彻底压平成纯 object 顶层，不能再保留任何顶层 combinator。
  - 工具描述不再推荐同样缺 `language` 也跑不通的 action。
- `codex-rs/mcp-server/src/tldr_tool.rs`
  - MCP `inputSchema` 也要同步扁平成纯 object 顶层，不能只做到“顶层 object + oneOf”。

## 之后遇到同类问题时的检查顺序
1. 先看 `codex-rs/native-tldr/src/tool_api.rs`，确认真实 action 与 `language` 约束。
2. 再看 `codex-rs/tools/src/tool_spec.rs` 和 `codex-rs/mcp-server/src/tldr_tool.rs`，确认 Responses/MCP 暴露给模型的 schema 与描述没有漂移。
3. 再看 `codex-rs/core/src/tools/rewrite/shell_search_rewrite.rs` 与 `tldr_routing.rs`，确认入口提示不会产出缺参模板。
4. 最后用请求级测试核对真实外发 `tools[].parameters` 已经没有顶层 `oneOf` / `anyOf` / `allOf` / `enum` / `not`。

## 验证备注
- `cargo test -p codex-tools create_tldr_tool_exposes_decision_guidance_and_current_action_surface -- --exact`
- `cargo test -p codex-tools create_tools_json_for_responses_api_flattens_tldr_to_plain_object -- --exact`
- `cargo test -p codex-mcp-server verify_tldr_tool_json_schema -- --exact`
- `cargo test -p codex-core shell_search_rewrite --lib`
- `cargo test -p codex-core --test all tldr_tool_request_exposes_ztldr -- --exact`
  - 仍被仓库既有 `core/tests/suite/zmemory_e2e.rs` 的 `rusqlite` / type inference 编译错误阻塞，未能作为本次改动的有效成败信号。
