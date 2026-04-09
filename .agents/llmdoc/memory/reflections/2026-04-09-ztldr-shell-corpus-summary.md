# 2026-04-09 ztldr shell corpus summary 反思

## 背景
- 前一轮已经基于当前项目补了 `classify_search_route` 的 corpus summary，但 shell 拦截链路还有自己一层解析与 reason/action 映射。
- 如果只测核心分类，不测 shell corpus，仍可能出现“分类对了、拦截建议不对”的漂移。

## 本轮有效做法
- 在 `shell_search_rewrite.rs` 增加一组基于当前项目真实命名风格的 shell corpus summary。
- 同时覆盖 symbol、wrapped symbol、member symbol、path-like、natural language 和 regex passthrough。
- 不是只断言单个 message，而是把 shell intercept 的 `reason` 与 `action` 汇总成稳定分布，再断言 summary。

## 关键收益
- 现在 search classifier 与 shell intercept 两层都有“基于当前项目真实样本”的 summary 回归。
- 后续如果 shell 命令解析或 reason/action 映射变化，更容易看出是整体分布变了，还是单个边界样例变了。

## 后续建议
- 如果后续做 trace 回放，优先分别保留“分类层 corpus”和“shell 层 corpus”，不要把两层责任混成一个超大测试。
