# 2026-04-09 ztldr grep corpus summary 反思

## 背景
- 之前已经有 classifier corpus 和 shell corpus，但中间的 `grep_files -> rewrite decision` 这一层还缺少基于当前项目真实样本的 summary 回归。
- 这层最容易出现“分类是对的，但最终 rewrite action / reason / signal 组合漂了”的问题。

## 本轮有效做法
- 在 `auto_tldr.rs` 增加当前项目 grep corpus summary，直接走 `rewrite_grep_files_to_tldr`。
- 样本覆盖 bare symbol、wrapped symbol、member symbol、natural language、path-like 和 regex passthrough。
- 同时汇总 `reason`、`action`、`signal` 三个维度，断言整体分布，而不是只测单条 rewrite。

## 关键收益
- 现在 classifier、grep rewrite、shell intercept 三层都有基于当前项目真实样本的 summary 回归。
- 以后再调 grep rewrite 时，可以更快判断是分类器变化、rewrite 逻辑变化，还是 shell 层变化。

## 后续建议
- 如果继续扩样本，优先往这三层各自的 corpus 补“高频误判 query”，不要把不同层的断言混在一个巨型测试里。
