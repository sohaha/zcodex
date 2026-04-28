# rollout 持久化应先过滤非持久事件再查 live writer

## 背景

`codex exec` 正常完成后，shutdown 阶段可能出现：

`failed to record rollout items: thread <id> not found`

表面看是 `ShutdownComplete` 在 `live_thread.shutdown()` 之后写 rollout，但更深层原因是 `Session::persist_rollout_items()` 会先查 `LiveThread` / local live recorder，再交给 `RolloutRecorder::record_items()` 做持久化策略过滤。

## 经验

- `ShutdownComplete`、`SkillsUpdateAvailable` 等事件本身不会被 rollout policy 持久化。
- 如果先查 live recorder，再让 recorder 过滤，shutdown 后这些非持久事件仍会触碰已移除的 writer，产生噪音 error。
- 根因修复应把 rollout policy 判断前置到 session 层：没有任何 item 会被持久化时直接返回，不查 `live_thread`。
- 回归测试应断言 shutdown 后不会出现 `failed to record rollout items` 日志，而不是断言 `ShutdownComplete` 出现在 rollout 文件里，因为该事件按策略不落盘。

## 验证建议

- 用 core integration test 走真实 `Op::Shutdown`，配合 `tracing_test` 捕获日志。
- 额外跑 `codex-rollout` 测试，确认导出的 policy helper 与 recorder 行为一致。
