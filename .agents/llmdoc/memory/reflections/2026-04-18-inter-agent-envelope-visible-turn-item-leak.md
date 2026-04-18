# Inter-agent envelope 不应泄露到可见 assistant 文本层

## 背景

2026-04-18 排查“子代理结果直接在界面里打印出 `{\"author\":...,\"recipient\":...,\"content\":...}`”时，问题表面看像是子代理报错，实际不是。

根因是：

- `InterAgentCommunication::to_response_input_item()` 会把邮箱消息序列化成 assistant `Message` 的单段 JSON 文本。
- 这条 assistant message 会继续走普通的 `parse_turn_item()` / `handle_non_tool_response_item()` / `last_assistant_message_from_item()` 链路。
- turn-item 提取层没有把它识别成“内部 envelope”，于是 TUI / app-server 历史把整段 JSON 当普通 assistant 正文显示了。

## 结论

- `InterAgentCommunication` 是模型上下文与线程协作用的结构化 envelope，不是用户可见 assistant 文本。
- 修复点不该落在 mailbox 序列化层；原始 JSON 仍要保留给历史、边界判断和后续 turn 使用。
- 正确的收口层是“可见文本提取”：
  - `core/src/event_mapping.rs` 的 `parse_turn_item()`
  - `core/src/stream_events_utils.rs` 的 `last_assistant_message_from_item()`

## 落点

- 当 assistant message 的 `content` 能被 `InterAgentCommunication::from_message_content()` 解析时，`parse_turn_item()` 直接返回 `None`，不再为 UI 生成可见 `TurnItem::AgentMessage`。
- 同样的 envelope 在 `last_assistant_message_from_item()` 中返回 `None`，避免 turn-complete fallback、桌面通知或后续摘要链路把 JSON 当最终回复。
- 保留 legacy 非 JSON 的旧格式 assistant inter-agent 文本作为兼容行为，不一刀切屏蔽所有包含 `author/recipient` 的消息。

## 验证与边界

- `cargo check -p codex-core --lib` 通过，说明生产代码编译正常。
- 自动格式化和 lib test 仍被仓库现有问题阻塞：
  - `just fmt` / `cargo fmt` 会被当前仓库里 `core/tests/suite/shell_command.rs` 的未闭合定界符挡住。
  - `cargo test -p codex-core --lib ...` 会被 `core/src/config/config_tests.rs` 等现有测试断点挡住，和本次改动无关。
- 由于仓库级 `rustfmt` 被整 crate 解析阻塞，本次只对改动文件单独执行了 `rustfmt`。

## 后续提醒

- 以后只要看到 assistant 正文里出现完整结构化 envelope，优先检查“内部消息是否越过了 turn-item / summary 提取边界”，不要先去改 UI 渲染。
- 对 mailbox / hook / diagnostic 这类“内部可持久化，但不应直接展示”的数据，优先在“生成可见 turn item”这一层做过滤，而不是破坏底层原始记录。
