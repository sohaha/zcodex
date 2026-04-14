# Anthropic tool_result 历史消息必须和后续同角色消息合并

## 触发场景
- `MiniMax-M2.7` 通过 `wire_api = "anthropic"` 走 Anthropic 兼容链路时，用户在工具调用结束后再输入“继续”，provider 返回 `invalid_request_error: tool call result does not follow tool call (2013)`。

## 根因
- 问题不在 `app-server-protocol` 的 schema 或协议类型，而在 `codex-rs/codex-api/src/anthropic.rs` 的历史消息组装。
- 该路径此前按 `ResponseItem` 一项一条 message 生成 Anthropic `messages`，导致：
  - `assistant(tool_use)`
  - `user(tool_result)`
  - `user(text: 继续)`
- Anthropic 兼容端要求 `tool_result` 必须紧跟其对应 `tool_use`，且后续同角色内容应并入同一条 message 的 `content` block，而不是拆成连续的 `user` message。

## 修复
- 在 `codex-rs/codex-api/src/anthropic.rs` 新增 `push_message_blocks`，把相邻同角色 block 合并到上一条 message。
- `tool_use` / `tool_result` 不再先构造完整 message，再由调用方直接 `push`；改为先构造 block，再交给统一合并逻辑。
- 补回归测试，覆盖 `tool_result` 后紧跟用户“继续”的历史序列，确认输出只保留一条合并后的 `user` message。

## 后续规则
- 任何改动 Anthropic 历史回放逻辑时，都要把“相邻同角色消息是否需要合并”当作显式校验项，尤其是 `tool_use/tool_result`、reasoning 内联标签和兼容 provider 的文本续写场景。
- 如果 provider 端报的是 `tool call result does not follow tool call`，优先检查历史 message 边界，而不是先怀疑 protocol schema。
