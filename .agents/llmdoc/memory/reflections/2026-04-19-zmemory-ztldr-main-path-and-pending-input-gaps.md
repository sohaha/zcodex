# 2026-04-19 zmemory / ztldr 主路径与 pending-input 缺口反思

- 这次深入审查说明，`mise run build` 的 warning 虽然已经通过接线显著减少，但“能编过”和“主路径功能完整”不是一回事。
- 两类残余缺口尤其容易被误判为已经完成：
  1. `consumer` 已接上，但主 `producer` 还没接。
  2. 只接了 turn 起始 `input`，漏掉 mid-turn `pending_input` / mailbox 输入。

## 具体表现

- `build_initial_context()` 现在会消费 `pending_zmemory_recall_note`，但普通 `Op::UserInput` 主路径在启动 regular turn 前并不会生产这条 note；当前唯一生产者仍是 mailbox-triggered pending-work turn。
- `run_turn()` 现在会从初始 `input` 提取 `tool_routing_directives`，也会基于初始 `input` 执行 `capture_stable_preference_memories()`；但 mid-turn 期间接收并写入 history 的 `pending_input` 仍然绕过这两步。

## 结论

- 审查这类本地分叉功能时，不能只看“某个 helper 终于被调用了”；还要同时核对：
  - 主用户路径有没有生产它需要的 state
  - mid-turn steer / mailbox / queued-next-turn 这类旁路输入是否也走同一套派生逻辑
- 对 `turn` 级能力，最稳的检查框架是：
  - initial user input
  - active-turn pending input
  - mailbox-triggered empty-input turn
  - subagent/review one-shot turn
- 如果一个特性依赖 `UserInput -> derived state -> initial context/runtime dispatch` 这条链，必须逐段证明每种 turn 入口都覆盖到了，而不是只看编译 warning 是否消失。
