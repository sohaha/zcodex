# 2026-04-09 ztldr shared grep tool calls 反思

## 背景
- `auto_tldr.rs` 测试里虽然已经复用了 `grep_payload`，但 `ToolCall { tool_name: "grep_files", ... }` 的构造仍在多处重复，局部修改很容易漏掉字段。

## 本轮有效做法
- 在 `core/src/tools/rewrite/test_corpus.rs` 新增：
  - `grep_tool_call`
  - `grep_tool_call_from_arguments`
- `auto_tldr.rs` 测试统一通过共享 builder 生成 `grep_files` 调用，保留 payload fixture 与 call-id 差异即可。

## 关键收益
- `tool_name`、`tool_namespace`、`ToolPayload::Function` 这些固定样板不再散落在多处测试里。
- 后续如果 grep 工具调用结构再变化，只需要改一处 builder。

## 后续建议
- 如果后面还要给 read/search 其他 rewrite 测试补共享调用 builder，优先沿 `test_corpus.rs` 增加专用 builder，而不是在各测试模块继续复制 `ToolCall` 样板。
