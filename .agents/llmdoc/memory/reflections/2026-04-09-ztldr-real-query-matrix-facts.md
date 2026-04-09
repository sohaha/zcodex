# 2026-04-09 ztldr real query matrix facts 反思

## 背景
- `tldr_routing.rs` 里的 real query matrix 之前仍是手写 tuple，虽然已经有共享 corpus/helper，但这一组真实样本还没有进入统一事实源。

## 本轮有效做法
- 在 `core/src/tools/rewrite/test_corpus.rs` 新增 `REAL_QUERY_MATRIX`，集中保存真实 query matrix 的 `pattern`、`route`、`signal`。
- `tldr_routing.rs` 的 `search_route_real_query_matrix_stays_stable` 改为直接遍历共享 matrix，并通过现有 helper 推导 expected reason / shell intercept reason。

## 关键收益
- 真实 query matrix 不再与测试断言混写，后续新增或调整真实样本时只需要更新共享数据。
- route/signal 事实、reason 推导和 shell intercept 推导保持同一来源，降低漂移风险。

## 后续建议
- 如果后面要把更多真实 shell 命令或 grep 参数场景纳入回归，优先在 `test_corpus.rs` 继续扩展结构化样本，而不是回到散落 tuple。
