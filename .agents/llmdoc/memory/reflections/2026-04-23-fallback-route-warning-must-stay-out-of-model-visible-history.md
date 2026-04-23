# fallback route warning 必须留在可见事件层，不能写回模型可见 history

## 背景

- 这轮需求想在“当前渠道失败并切到 fallback provider/model”时，向用户显示一条类似“此请求已被路由到 z-ai/glm4.7 作为后备方案。”的提示。
- 首版实现除了发 `EventMsg::Warning`，还复用了 `record_model_warning()` 把提示写进 conversation history，并顺手把 `ModelRerouted` 在 TUI / exec 里也渲染成 warning。

## 发现

1. `run_turn()` 的 fallback 重试会在每轮请求前重新 `clone_history().for_prompt(...)`，所以在 fallback 切换点把 warning 记成 `ResponseItem::Message(role = "user")`，会让后备模型把这条提示当成新的用户输入读到。
2. 安全降级链路本来就同时发 `ModelReroute` 和 `Warning`；如果 UI 再把 `ModelRerouted` 单独渲染成 warning，会把同一次安全降级显示两遍。

## 结论

- fallback provider/model 的路由提示应停留在 `Warning` 这类用户可见事件层，必要时进入审计或 thread 事件流，但不要在同一 turn 重试前写回模型可见 history。
- `ModelRerouted` 更适合保留为协议/状态信号；若已经有配套 `Warning` 文案，TUI/exec 不应再额外生成第二条用户提示。

## 后续规则

- 以后给“重试、降级、回退、重路由”补用户提示时，先判断这条信息是否会进入下一轮 prompt。
- 如果提示的唯一目标是让用户看见，就优先走 `Warning` / app-server notification / UI surface；不要复用会写回 conversation history 的 helper。
- 涉及 `ModelRerouted` 的 UI 改动前，先核对同一路径是否已经有 `Warning` 或等价的最终用户文案，避免双提示。
