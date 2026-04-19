# 2026-04-19 zmemory / ztldr 构建告警与半接线特性反思

- 这轮起因不是“构建失败”，而是 `mise run build` 有大量 `zmemory` / `ztldr` 相关 warning，用户追问这些 warning 是否意味着本地特性不完整。
- 真实情况不是 crate 缺失或 Cargo feature 没开，而是 `codex-core` 里存在一批“实现已写、测试已写、但生产调用链没接上”的本地分叉代码：
  - `build_zmemory_tool_developer_instructions()` / `build_ztok_tool_developer_instructions()` 已存在，但 `build_initial_context()` 之前没有注入它们。
  - `pending_zmemory_recall_note_for()` 和 `build_stable_preference_recall_note_from_texts()` 已存在，但 recall note 之前只写入 state，没有在首轮上下文里读出。
  - `capture_stable_preference_memories()` 已存在，但之前没有在正常 turn 流程里消费 `UserInput`。
  - `tools/rewrite/engine.rs` 与 `parallel_tests.rs` 已假定 runtime 会在 dispatch 前统一执行 `rewrite_call_for_dispatch()`，但生产路径仍直接走 `dispatch_any()`。
- 这类 warning 不能简单按“死代码噪音”处理；它们常常说明本地分叉功能在“提示层 / runtime seam / state bridge”三处有一处没收口。

## 本轮修正

- 在 `build_initial_context()` 中补回：
  - `Feature::Zmemory` 开启时的 `## Zmemory` developer instructions
  - 当前 turn 的 pending recall note
  - `## Embedded ZTOK` developer instructions
- 在 `run_turn()` 起手阶段把本轮 `UserInput` 解析为 `tool_routing_directives`，并在 `Feature::Zmemory` 打开时执行 `capture_stable_preference_memories()`。
- 在 `ToolCallRuntime` 里补上 `rewrite_call_for_dispatch()`，让 `ztldr` 的统一 rewrite engine 真正跑在生产 dispatch 前。

## 结论

- 以后看到 `zmemory` / `ztldr` warning，先判断它们属于哪一层：
  - crate 本体
  - prompt/context 注入
  - state 读写桥接
  - runtime dispatch seam
- 如果测试文件已经明确写出目标行为，而构建 warning 又集中出现在对应实现上，优先怀疑“测试和生产脱节”，不要先怀疑 feature flag。
- 对 `codex-core` 这类共享层，当前仓库里 `cargo test -p codex-core --lib ...` 可能先被大面积陈旧测试阻塞；确认本轮接线是否正确时，先用 `cargo check -p codex-core --lib` 验证生产代码，再把测试失败单独归类为既有漂移。
