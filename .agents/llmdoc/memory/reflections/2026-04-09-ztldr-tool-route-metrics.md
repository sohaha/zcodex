# 2026-04-09 ztldr tool_route metrics 反思

## 背景
- 前一轮已经把 `signal` 落到了 `codex_core::tool_route` 日志，但要回答“哪些 query signal 最常 rewrite / passthrough”仍需要人工扫日志。
- 用户希望继续推进一次性、内嵌 tools 原生方案，而不是再扩写提示词。

## 本轮有效做法
- 在 `core/src/tools/rewrite/engine.rs` 里直接复用既有 `SessionTelemetry`，新增 `codex.tool_route` counter。
- metric tags 与日志字段保持同一套语义：`decision`、`mode`、`source`、`from_tool`、`to_tool`、`reason`、`action`、`signal`。
- 对缺失值统一落成 `none`，避免后续聚合时混入空字符串分支。
- 用 in-memory metrics 测试直接断言 rewrite / passthrough 两种 tag 组合，避免只靠 tracing 文本回归。

## 关键收益
- 现在可以直接按 `reason + signal + action` 维度聚合 route 命中率，不需要先做日志清洗。
- telemetry contract 与 routing contract 同步演进，后续新增 signal 时更容易发现遗漏。

## 踩坑
- `ToolCallSource` 当前只有 `Direct` / `JsRepl` / `CodeMode`，不要想当然沿用别处的 source 命名。
- 测试环境下 `sccache` 可能在沙箱里报 `Operation not permitted`，可以用 `RUSTC_WRAPPER=` 跑定向测试，不必把问题误判为代码回归。

## 后续建议
- 下一步优先对真实 trace 做一次 query 聚合，确认 `path_like`、`natural_language`、`wrapped_symbol` 的分布是否符合预期。
- 如果后续要扩 metric，继续沿 `codex.tool_route` 加有限枚举 tag，不要引入第二套平行命名。
