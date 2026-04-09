# 2026-04-09 ztldr query signal 可观测性反思

## 背景
- 前一轮已经把 `ztldr` 搜索分类下沉到 `core/src/tools/rewrite/tldr_routing.rs`，但运行时日志仍只记录 `reason` 和 `action`。
- 当 `bare symbol`、`wrapped symbol`、`path-like`、`natural language` 等不同输入共享同一个 `reason` 家族时，单看 route 日志难以判断分类边界是否符合预期。

## 本轮有效做法
- 在 `ToolRewriteDecision` 中显式携带 `signal: Option<SearchSignal>`，让 `auto_tldr`、`read_gate` 和 rewrite engine 共享同一份分类上下文。
- `read_gate` 明确写成 `signal: None`，避免把 search-only 分类误带到文件读取链路。
- 在 `codex_core::tool_route` 日志中增加 `signal` 字段，把分类结果落到统一可观测面，而不是在入口各自拼临时日志。
- 测试解构统一补 `..`，避免后续 `ToolRewriteDecision` 再加元数据时重复触发 `E0027`。

## 关键收益
- 调试 route 误判时，可以直接从日志区分“symbol 命中失败”还是“本来就被归到 semantic/path-like”。
- `decision -> engine log` 的元数据链路完整后，后续做 signal 级 metrics 或 prompt 收敛时不需要再改入口函数签名。
- 保持了“统一 contract + 薄入口”的方向，没有回退到追加大段 prompt 或入口特判。

## 踩坑
- 给 `ToolRewriteDecision` 新增字段后，单元测试里枚举解构很容易漏改；这类测试建议默认写 `..`，除非测试本身就是要断言新增字段。
- 这类纯元数据增强可以先用 `cargo check -p codex-core --lib` 排除编译问题，再跑定向测试，排查速度比直接跑全量更高。

## 后续建议
- 如果要继续评估命中率，优先在 `codex_core::tool_route` 基础上观察 `reason + signal + action` 组合，再决定是否真的需要改分类规则。
- 若后续接 metrics，建议直接复用这里的 `signal` 命名，不再引入第二套 query-kind 字段。
