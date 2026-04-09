# 2026-04-09 ztldr shared summary helpers 反思

## 背景
- 共享 `PROJECT_QUERY_CORPUS` 之后，三层 summary 测试的样本字符串已收敛，但 `route` / `signal` / `reason` 的标签映射和期望计数仍重复散落。

## 本轮有效做法
- 在 `core/src/tools/rewrite/test_corpus.rs` 继续下沉共享测试 helper：
  - label 映射：`route_label`、`signal_label`
  - reason 映射：`structural_search_reason`、`structural_shell_intercept_reason`
  - summary 计数：`project_route_counts`、`project_signal_counts`、`project_structural_search_reason_counts`、`project_structural_shell_reason_counts`
- `tldr_routing.rs`、`auto_tldr.rs`、`shell_search_rewrite.rs` 的 summary 断言优先复用这些 helper，只在各层额外样本上做增量修正。

## 关键收益
- 共享样本和共享期望现在都集中在同一测试模块，修改 query 分类时更不容易漏改断言。
- 三层测试仍保留各自特有的附加样本，不会被一个过度抽象的大测试绑死。

## 后续建议
- 如果后面再引入 trace 回放或更大的真实样本集，优先沿 `test_corpus.rs` 扩充“样本 + 期望 helper”，不要把新的 reason/action 统计散回各测试文件。
