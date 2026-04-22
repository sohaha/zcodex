# app-server thread history 必须过滤 inter-agent envelope

## 背景

2026-04-22 排查 v2 子代理输出里出现 `{"author":...,"recipient":...,"other_recipients":...,"content":...}` 时，`core` 主链本身并没有把内部 mailbox envelope 当作可见 assistant 正文。

真正漏边界的是 app-server v2 的线程历史 reducer：

- `core/src/event_mapping.rs::parse_turn_item()` 已经会在 assistant message 命中 `InterAgentCommunication::from_message_content()` 时返回 `None`。
- `core/src/stream_events_utils.rs::last_assistant_message_from_item()` 也会把同类 envelope 排除在 `last_agent_message` 之外。
- 但 `app-server-protocol/src/protocol/thread_history.rs` 直接消费 `EventMsg::AgentMessage`，把文本塞进 `ThreadItem::AgentMessage`，没有复用这层过滤。

结果就是：

- 实时或重建出来的 v2 线程历史会把内部 envelope 当普通 assistant message 保存；
- TUI / app-server adapter 再把它原样渲染出来，用户就会看到原始 JSON。

## 结论

- `InterAgentCommunication` 的“不可见”约束不能只放在 `core` 的 turn-item 映射链。
- 任何直接消费 `EventMsg::AgentMessage`、绕过 `parse_turn_item()` 的 reducer / adapter，都必须显式同步这条 envelope 过滤规则。
- 修复点应优先落在历史归并层，而不是 TUI 渲染层；否则只是 UI 打补丁，thread history 里仍会保留脏数据。

## 实践提醒

- 以后看到 app-server/TUI 里出现内部 JSON envelope，优先检查“是否存在旁路 reducer 直接消费 `EventMsg::AgentMessage`”。
- 对 mailbox、hook、diagnostic 这类“可持久化但不应可见”的结构化文本，要把边界锁在共享 reducer 或 turn-item 归并层，不要等到视图层再兜底。
