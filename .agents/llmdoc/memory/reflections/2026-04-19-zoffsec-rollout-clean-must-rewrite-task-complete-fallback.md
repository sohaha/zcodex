# 2026-04-19 zoffsec rollout 清理要同时改写 `task_complete.last_agent_message`，拒绝检测要区分强短语和弱前缀

## 背景

- 本仓 `codex zoffsec` 是参考 `ryfineZ/codex-session-patcher` 落地的，但之前没有做过一次真实的 upstream baseline sync。
- 这次按最新 upstream `af401d3e53f3836788c4326e01499d7d7946ceb1` 审计 `core/formats.py`、`core/detector.py` 和 `core/patcher.py` 后，发现本地 `rollout/src/patch.rs` 有两个具体缺口：
  - 只改了 `event_msg.agent_message`，没改 `event_msg.task_complete.last_agent_message`
  - 拒绝检测只做了少量英文全文匹配，没有 upstream 那种“强短语全文 + 弱关键词开头”分层

## 结论

- Codex rollout 的 replay 文本不能只盯 `agent_message`。如果 UI/replay 链路会把 `task_complete.last_agent_message` 当作最终兜底文案，清理 refusal 时必须一起改写这个字段，否则 resume 仍可能看到旧拒绝文本。
- refusal 检测不应该只做一层粗糙的全文 `contains`：
  - 强拒绝短语适合全文匹配
  - 弱关键词只适合在消息开头一小段内匹配
- 这样可以同时提高命中率，并减少把正文里的普通 `sorry`/`apologize` 误判成 refusal 的概率。

## 落地做法

- 在 `codex-rs/rollout/src/patch.rs` 中把 `task_complete` 纳入 event 副本改写范围。
- 把 refusal 检测改成两级：
  - `STRONG_REFUSAL_PATTERNS`：全文匹配
  - `WEAK_REFUSAL_PATTERNS`：只看前 150 个字符
- 补了定向测试，覆盖：
  - `task_complete.last_agent_message` 被同步替换
  - 强/弱拒绝规则的正反例
