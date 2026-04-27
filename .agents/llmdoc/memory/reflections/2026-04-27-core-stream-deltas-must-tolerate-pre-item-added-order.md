# `core` 的流式 delta 处理必须容忍 `output_item.added` 之后才补到的顺序

## 背景

- 这轮问题表面上是 debug 构建在 `core/src/util.rs:97` 触发 panic，`SPANTRACE` 落在 `try_run_sampling_request -> run_sampling_request`。
- 真正的触发点不在 `util.rs`，而在 `core/src/session/turn.rs` 的流式事件状态机：
  - `ResponseEvent::OutputTextDelta`
  - `ResponseEvent::ReasoningSummaryDelta`
  - `ResponseEvent::ReasoningSummaryPartAdded`
  - `ResponseEvent::ReasoningContentDelta`
- 这些分支之前把“收到 delta 时一定已经有 `active_item`”当成硬不变量；一旦 provider / SSE 解析路径先送 delta、后送 `response.output_item.added`，debug 模式就会直接 `panic!`。

## 根因

- `core` 的 turn 层把 item-less delta 绑定到了“当前 active item”，但协议现实并不保证顺序永远满足这个假设。
- 同仓已经承认 `response.output_item.done` 可以独立成立，`handle_output_item_done(...)` 在 `previously_active_item.is_none()` 时仍会补发 started/completed 事件；这说明 turn 层原先对 delta 顺序的要求比实际流协议更强。
- `response.output_text.delta` 与 reasoning delta 都不携带 item id，遇到“delta 先到、added 后到”时，不能靠回溯关联修复，只能显式缓存并在 item 真正出现后再决定如何处理。

## 本轮有效做法

- 在 `core/src/session/turn.rs` 新增 turn 级 `PendingStreamDeltas`，分别缓存：
  - assistant text delta
  - reasoning summary delta
  - reasoning section break
  - reasoning raw delta
- 当 `active_item` 为空时，不再直接 panic，而是先缓存对应 delta。
- 等收到 `OutputItemAdded` 后：
  - 如果是空内容的 assistant message，就把缓存的 text delta 回放进现有 `AssistantMessageStreamParser`
  - 如果是空内容的 reasoning item，就回放缓存的 reasoning delta 事件
  - 如果 added item 自己已经带了内容，则丢弃同类型缓存，避免重复拼接
- 当只收到 `OutputItemDone`、始终没有 `OutputItemAdded` 时，沿用现有 done 路径产出最终 item，并在 done 时清掉同类型缓存，避免污染下一项。

## 验证经验

- 最小可靠回归不是只看 panic 消失，而是构造真实 SSE 顺序：
  - `delta -> output_item.added -> output_item.done`
  - reasoning `delta -> output_item.added -> output_item.done`
- 在 `core/tests/suite/client.rs` 补 e2e 测试比只写 unit test 更有价值，因为这里的问题来自 `codex-api -> core turn loop -> protocol events` 的整链路顺序假设。

## 后续默认做法

- 以后凡是消费 Responses 流里“不带 item id 的增量事件”，都不要默认依赖 `active_item` 已经建立；先确认协议是否真的保证顺序。
- 如果下游状态机已经允许 `done-without-added`，就要同步审查 delta 分支是否也具备相同鲁棒性，不要只在最终 item 路径上兜底。
