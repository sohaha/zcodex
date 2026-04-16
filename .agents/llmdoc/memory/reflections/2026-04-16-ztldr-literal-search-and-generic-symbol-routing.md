# 2026-04-16 ztldr literal search 与通用名路由反思

## 背景
- `ztldr search` 之前把输入直接当 regex 编译，`resolveProjectAvatar(` 这类精确代码片段会因为未闭合分组直接失败。
- `createOrAttach` 这类通用实现名虽然能被结构检索识别成 symbol，但结果通常区分度不够，模型容易拿到一组“结构上合法、定位上无用”的候选。

## 本轮有效做法
- 把 search 语义下沉到 `native-tldr`：显式区分 `literal` 和 `regex` 两种匹配模式，默认走 `literal`，只有显式 `regex` 才按正则编译。
- 非法 regex 不再只返回自由文本错误，而是通过 MCP 层暴露稳定的 `structuredFailure.error_type = invalid_regex` 和明确的 `retry_hint`。
- 在 `core/src/tools/rewrite/tldr_routing.rs` 增加通用 symbol 降级：对未带路径/成员限定的泛化名字（如 `createOrAttach`）直接回退到 raw exact-text 路径，而不是继续强推 context route。
- 调整 shell/auto_tldr/read_gate 的 `TldrToolCallParam` 构造，给新增字段留出明确默认值，避免调用链新增参数后继续出现散点编译错误。

## 关键收益
- 精确文本检索默认可用，不再要求调用方自己先手动 escape regex 元字符。
- agent 在遇到非法 regex 时能拿到机器可读的恢复信号，便于自动切换到 literal 模式或提示用户转义。
- broad grep/read 的自动路由对“低区分度 symbol”更保守，减少“用了 ztldr 但没有帮助”的假阳性。

## 验证与阻塞
- 已通过：
  - `cargo nextest run -p codex-native-tldr`
  - `cargo test -p codex-mcp-server run_tldr_tool_with_mcp_hooks_preserves_search_payload_contract --lib -- --exact`
  - `cargo test -p codex-mcp-server run_tldr_tool_with_mcp_hooks_surfaces_invalid_regex_error --lib -- --exact`
  - `cargo test -p codex-cli search_command_defaults_to_literal_match_mode --bin codex -- --exact`
  - `cargo test -p codex-cli search_command_parses_regex_match_mode --bin codex -- --exact`
  - `cargo test -p codex-cli render_search_response_text_includes_match_mode --bin codex -- --exact`
  - `cargo check -p codex-core --lib`
- 未能跑通的不是本次回归：
  - `cargo nextest run -p codex-cli -p codex-mcp-server` 被现有用例阻塞：一个是 CLI 的 sqlite migration 唯一键失败，一个是 MCP “feature disabled 时不应列出 ztldr” 的断言与当前工具暴露状态不一致。
  - `cargo test -p codex-core --lib ...` 会先撞上大量既有测试编译错误（`ModelProviderInfo` / provider name / shell runtime 类型签名等），因此本次只能先用 `cargo check -p codex-core --lib` 保证实现面可编译。

## 后续建议
- 如果继续优化 `ztldr` 路由，优先把真实误判 query 继续补进 `REAL_QUERY_MATRIX`，不要只改启发式。
- 若后续要进一步降低误判，可把“generic symbol” 判定从硬编码 token 表继续收敛到真实 trace，而不是盲目扩大词表。
