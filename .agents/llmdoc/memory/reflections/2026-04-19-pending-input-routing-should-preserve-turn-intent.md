# 2026-04-19 pending_input 路由意图应保留 turn 基线并按最新 steer 决议

- 这次 `zmemory + ztldr` 深审暴露的不是“pending_input 没接上”，而是“接上后的语义不对”。
- `run_turn()` 在 turn 起始会先从初始 `input` 提取 `tool_routing_directives`，但 mid-turn 的 `pending_input` 进入 `apply_pending_user_input_side_effects()` 后，会直接覆盖整份 directives，而不是在当前 turn 既有意图上做增量更新。

## 具体表现

- 如果首条消息明确说“不要 ztldr”或“用 ripgrep”，后续一句中性的 `steer_input`（如“继续”“再看一下”）会把这份 routing state 重算回默认值，等于重新打开自动 `ztldr` 路由。
- 如果同一轮排队的多条 `pending_input` 自相矛盾，当前逻辑会把所有消息拼接后一次分类，而不是按时间顺序让最新 steer 覆盖旧意图；这会让“还是用 ztldr”被前面一句“不要 ztldr”吞掉。

## 结论

- 审查 turn 内 steer / pending_input / mailbox 输入时，不能只验证“side effect 被触发”，还要验证它是：
  - 保留当前 turn 的 routing 基线，还是整包覆盖
  - 按最新消息决议，还是把多条消息拼接后做静态分类
- 对这类 turn-local 派生状态，helper 级单消息测试不够。至少要补：
  - 初始输入建立 routing 基线
  - 中性 `pending_input` 不应清掉既有禁用/偏好
  - 冲突 `pending_input` 以最新 steer 为准
