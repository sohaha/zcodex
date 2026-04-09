# 2026-04-09 ztldr 项目内 corpus summary 反思

## 背景
- 用户明确追问“不能基于当前项目做一次测试吗”，说明仅有抽象分类测试还不够，需要一个更贴近仓库真实符号与路径的回归样本。

## 本轮有效做法
- 在 `tldr_routing.rs` 里增加一组基于当前仓库真实命名风格的 corpus：
  - `rewrite_tool_call`
  - `` `emit_tool_route_metric()` ``
  - `decision.signal`
  - `ToolCallSource::Direct`
  - `core/src/tools/rewrite/engine.rs`
  - `codex-rs/otel/src/metrics/names.rs`
  - 自然语言结构问题与 regex passthrough 样例
- 不只断言单个 query，而是把 route/signal/passthrough 聚合成 summary，再断言整体分布。

## 关键收益
- 现在已经有一个“基于当前项目真实符号/路径风格”的最小回放集，而不是只靠抽象示例。
- 后续新增真实误判样本时，可以继续往这个 corpus 里加，而不必另起一套测试机制。

## 后续建议
- 如果后面开始做 trace 回放，把真实 query 先归并成这种小规模 summary，再决定是否提升为长期回归样本。
