# subagent notification 可见性治理必须覆盖 legacy user 事件和 TUI 多入口

## 背景

2026-04-24 排查“子 agent 完成通知直接在 TUI 里显示 `<subagent_notification>...</subagent_notification>` / `{"author":...}`”时，已有两条反思只覆盖了一半问题：

- 2026-04-18 已确认 assistant `InterAgentCommunication` JSON envelope 不该进入可见 assistant 文本层。
- 2026-04-22 已确认 app-server v2 的 thread history reducer 也必须过滤 assistant envelope。

这次暴露的新缺口是：

- 历史数据里还存在 legacy `EventMsg::UserMessage`，其正文整段包在 `<subagent_notification>...</subagent_notification>` 里；
- TUI 不只会消费 thread history replay，还会从 live completed message / resumed history fallback 等多个入口直接写可见消息。

如果只过滤 assistant JSON envelope，或者只修 app-server history，一样会在 replay 或 live 完成路径把内部协作通知显示给用户。

## 结论

- 内部协作消息的“不可见”边界要同时覆盖两种载体：
  - assistant `InterAgentCommunication` JSON envelope
  - legacy user `<subagent_notification>...</subagent_notification>` 文本
- 修复应落在“可见文本生成边界”，而不是删除底层原始记录；原始 envelope / notification 仍可保留给协作状态和历史诊断使用。
- TUI 侧不能只拦一个入口；至少要同时检查：
  - 历史 replay 注入
  - streaming / completed assistant message 收口
  - user message event 渲染入口

## 实践提醒

- 以后看到子 agent 内部通知出现在 UI，先按“消息载体 x 可见入口”二维排查：
  - 载体是否是 assistant envelope、legacy user notification，还是两者都有；
  - 泄露点是在 shared reducer、history replay、还是 live completion 收口。
- 对“内部可持久化但不应显示”的数据，验证不能只看单条主链 happy path；至少要补历史 replay 和 live 完成两类测试。
