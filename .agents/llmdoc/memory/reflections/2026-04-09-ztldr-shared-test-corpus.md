# 2026-04-09 ztldr shared test corpus 反思

## 背景
- 前几轮已经分别给 classifier、grep rewrite、shell intercept 补了 project corpus summary，但样本字符串散落在三个文件里，后续维护成本会上升。

## 本轮有效做法
- 新增 `core/src/tools/rewrite/test_corpus.rs`，把当前项目共用的高价值 query 样本集中成测试专用 corpus。
- 三层测试共用同一批样本：
  - `tldr_routing.rs`
  - `auto_tldr.rs`
  - `shell_search_rewrite.rs`
- 允许各层只补自己特有的附加样本或附加断言，避免把所有层硬塞进一个超大测试函数。

## 关键收益
- 新增或修正一个真实 query 样本时，不再需要在三处手动同步字符串。
- 三层 summary 更容易保持同一事实源，同时又保留各层自己的行为断言。

## 后续建议
- 后面如果引入 trace 回放样本，优先先更新共享 corpus，再决定哪些层需要补额外样本。
