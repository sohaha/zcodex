# 2026-04-09 ztldr routing switches 反思

## 背景
- 真实 query 矩阵已经覆盖了常见输入形态，但路由行为还会受 `prefer_context_search`、`force_tldr`、`problem_kind` 这类开关影响。
- 这类开关最容易在重构时被忽略，因为 query 本身没变，只有 directives 变了。

## 本轮有效做法
- 在 `tldr_routing.rs` 直接补两类测试：
  - `prefer_context_search = false` 时，symbol 仍保留 `BareSymbol` signal，但 route 切到 `SemanticQuery`。
  - `problem_kind = Factual` 默认 passthrough，只有 `force_tldr = true` 才继续分类并生成 factual reason。
- 让“signal 保持稳定、route 随开关变化”成为显式回归点，而不是隐含行为。

## 关键收益
- 后续再调 route 策略时，能更快区分“分类器坏了”还是“开关语义变了”。
- `problem_kind`、`force_tldr`、`prefer_context_search` 这些控制面现在有了更清晰的测试边界。

## 后续建议
- 如果后面需要做 trace 聚合，除了看 query 文本，也要把 directives 一并纳入样本上下文，否则会误判 route 是否异常。
