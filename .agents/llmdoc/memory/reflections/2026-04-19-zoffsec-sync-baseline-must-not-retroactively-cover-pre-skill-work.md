# 2026-04-19 zoffsec sync baseline 不能追认 skill 出现前的本地实现

## 背景

- `codex ctf` / `codex zoffsec` 的本地命令工作流，最初是在 sync skill 与 `STATE.md` 出现之前落地的。
- 直到 `b74d2512f` 才首次引入 `sync-codex-session-patcher` skill，并把状态初始化为 `last_synced_hash: <none>`。
- 后续 `fada696c3` 才对 rollout cleaner 做了真正带 upstream ref 的 selective sync，`2cb4d923e` 再把本地命令面改名为 `zoffsec`。

## 结论

- 当某个本地功能早于 sync skill / `STATE.md` 落地时，不能因为它“参考过 upstream”就追认成“已和某个 upstream ref 对齐”。
- 这类历史实现最多只能表述为：
  - 受 upstream 启发
  - 后续某些子能力做过 selective sync
  - 其余命令面与 UX 仍是本地分叉
- 对 `codex-session-patcher` 这条链路，当前能被审计确认的 upstream parity 只覆盖 rollout cleaner：
  - refusal 检测两级模型
  - `event_msg.agent_message`
  - `event_msg.task_complete.last_agent_message`
- `codex zoffsec` 启动入口、base-instructions marker、`zoffsec resume` clean hook 和本地模板体系，不应表述为“整体 upstream 对齐”。

## 落地做法

- `STATE.md` 要明确写出“对齐范围”，而不是只写一个 upstream hash。
- sync skill 要禁止对 pre-skill 本地实现做 retroactive full-parity 表述，除非补完逐文件审计证据。
- plan / issue 若同时承载“本地命令实现”和“后续 selective sync”，要把两者分开写，避免后人把它们合并理解。
