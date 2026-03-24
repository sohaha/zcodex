# AGENTS.md

AGENTS.md 说明见 [此文档](https://developers.openai.com/codex/guides/agents-md)。

## 分层 agents 消息

当启用 `child_agents_md` 功能标记（在 `config.toml` 的 `[features]` 中配置）时，Codex 会在用户指令消息中追加关于 AGENTS.md 作用域与优先级的说明，即使没有 AGENTS.md 也会发送该消息。
