# 内部协作消息的可见性边界应收敛到 `codex-protocol`，并让 `zteam` 复用同一套净化规则

## 背景

- 这轮问题表面上是 TUI 又把子会话内容打印给用户，进一步把 `/zteam start <目标>` 的 mission goal 和 Mission Board 一起污染了。
- 深挖后发现根因不是单一入口漏拦，而是三层各自维护了一套近似但不一致的隐藏判断：
  - `core` 只在部分 assistant 出站路径识别 `InterAgentCommunication`
  - `tui` 自己维护 `<subagent_notification>` / envelope 判断
  - `app-server-protocol` 的 thread history 也有一份重复实现

## 本轮有效做法

- 把“内部协作消息是否应对用户可见”的真相源上收至 `codex-rs/protocol/src/protocol.rs`，围绕 `InterAgentCommunication` 统一提供：
  - hidden envelope 判断
  - legacy `<subagent_notification>` 判断
  - 文本净化 helper
- `core` 的 `last_assistant_message_from_item(...)` 和 `event_mapping` 改为复用协议层 helper，而不是继续在本地只认某一种 carrier。
- `tui` 的 replay/live/completed/fallback 可见入口统一改走协议层净化，不再保留本地重复判断。
- `app-server-protocol` 的 thread history 也改成同一套 helper，避免 thread/read 与 TUI 再次漂移。
- `zteam` 不再信任原始 `<目标>` 输入，建 mission 前复用同一套协议层净化，避免任何未来漏网的内部消息再污染 Mission Board。

## 关键结论

- 这类“内部记录可以持久化，但不该对用户显示”的边界，不能放在单个 UI 或单个 reducer 里修；真相源必须放到所有运行面都依赖的共享层。
- 对当前仓库，最合适的共享层是 `codex-protocol`，因为 `core`、`tui`、`app-server-protocol` 都依赖它，而且 `InterAgentCommunication` 本体就在这里。
- 只做可见层过滤仍不够；像 `zteam` 这种会把文本再提升成产品状态的入口，也必须复用同一套净化规则做输入边界自保。

## 后续默认做法

- 以后再修内部协作消息泄露，先检查协议层是否已暴露统一 helper；没有就先补共享真相源，不要继续在 `tui`/`app-server` 各写一份。
- 对任何“文本会再次进入状态模型或产品文案”的功能，默认在输入边界复用协议层净化 helper，而不是假设上游显示层永远不会漏。
