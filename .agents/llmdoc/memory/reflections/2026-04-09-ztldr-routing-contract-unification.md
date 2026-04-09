# 2026-04-09 ztldr 路由 contract 一次性收敛反思

## 背景
- 前一轮已经强化了 tool description 与 shell interception 文案，但问题本质不是“文案不够多”，而是路由决策仍分散在多个入口。
- 用户明确要求避免继续内嵌大量提示词，改成内嵌 tools 可复用的一次性方案。

## 本轮有效做法
- 在 `core/src/tools/rewrite/` 新增统一模块 `tldr_routing.rs`，把以下规则集中：
  - search/read passthrough 原因
  - symbol/semantic 路由分类
  - reason 模板（search/extract/shell intercept）
  - shell interception 的短解释模板
  - language 映射公共函数
- 三入口 (`auto_tldr` / `read_gate` / `shell_search_rewrite`) 只消费统一 contract，不再各自维护长文本与重复条件。

## 关键收益
- 决策一致性：同一输入在不同入口得到同一类 route/reason。
- 维护成本下降：后续调整路由边界只改一处。
- 提示词可控：解释文本保持短且结构化，不再持续膨胀。

## 踩坑
- `just fix -p codex-core` 会修复并改写仓库内其他非本任务文件；执行后必须回退与任务无关改动，再继续仅提交本任务文件。
- shell interception 测试在替换文案后，原先匹配转义 JSON 子串的断言容易失效；应断言关键结构片段而不是过度依赖转义形式。

## 后续建议
- 后续若继续优化命中率，优先改 `tldr_routing.rs` 的分类与边界，而不是在各入口追加自然语言描述。
- 新增路由策略时，先补 contract 单元测试，再改入口接线，避免回归由入口差异引起。
