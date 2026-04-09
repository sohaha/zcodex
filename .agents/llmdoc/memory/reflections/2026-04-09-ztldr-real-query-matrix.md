# 2026-04-09 ztldr 真实 query 矩阵反思

## 背景
- 前几轮已经补了 signal 日志、metrics 和少量边界测试，但单个样例不足以稳定覆盖真实用户常见问法。
- 继续优化时，最容易回归的不是底层 wiring，而是 query 分类边界本身。

## 本轮有效做法
- 在 `tldr_routing.rs` 增加表驱动矩阵测试，把一组接近真实提问的 query 同时断言到 `route`、`signal`、`search_reason`、`shell_intercept_reason`。
- 样例覆盖了 bare symbol、wrapped symbol、member symbol、path-like 和 natural language 五类高频输入。
- 让 `shell` 拦截 reason 与核心分类共用同一批断言，避免 search/read/shell 三条链路再次漂移。

## 关键收益
- 后续如果调整 `looks_like_*` 规则，能更快看到是哪个真实 query 被重新分类了。
- 这类矩阵测试比单点测试更适合承担“真实输入回归样本库”的角色。

## 后续建议
- 如果后面拿到真实 trace，再把高频误判 query 逐步并入这个矩阵，而不是只改实现不留样本。
- 保持矩阵规模克制，只放高频、代表性、容易误判的 query，避免把测试变成重复样例堆积。
