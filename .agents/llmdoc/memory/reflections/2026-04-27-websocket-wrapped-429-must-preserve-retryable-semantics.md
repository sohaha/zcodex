# 2026-04-27 websocket wrapped 429 必须保留 retryable 语义

## 背景

修复“429 Too Many Requests 没有成功触发重试”的 websocket 流式回归时，发现 Responses WebSocket 与 SSE 在服务端错误分类上存在分叉，而且 websocket 自己还分成“握手失败”和“已建连后 error frame”两条错误入口。

## 现象

- SSE 的 `response.failed` 会把 `rate_limit_exceeded` 归类成 `ApiError::Retryable`，随后 `core` 走 `stream_max_retries` 重连。
- Responses WebSocket 的 wrapped `{"type":"error","status":429,...}` 之前一律被映射成 `TransportError::Http(429)`。
- Responses WebSocket 的握手阶段如果直接收到 HTTP 429，也会被 `map_ws_error()` 原样映射成 `TransportError::Http(429)`。
- 该错误再经 `api_bridge` 落成 `CodexErr::RetryLimit(...)` 或 `CodexErr::UsageLimitReached(...)`，被 `core/src/session/turn.rs` 当作终止错误，根本进不了流式重试分支。

## 根因

`codex-rs/codex-api/src/endpoint/responses_websocket.rs` 里 websocket 429 的语义被拆散了：`map_wrapped_websocket_error_event()` 只对 `websocket_connection_limit_reached` 做了 retryable 特判，`map_ws_error()` 则把握手期 HTTP 429 全部退化成 `TransportError::Http`；两条入口都丢掉了“普通 rate-limit 429 仍可重试”的语义。

## 修复

- 让 wrapped websocket error 反序列化 `error.type`。
- 让 websocket 握手期的 HTTP 429 也解析 body 里的 `error.type`。
- 当 `status == 429` 且 `error.type` 不是 `usage_limit_reached` / `usage_not_included` 时，直接映射成 `ApiError::Retryable`。
- 保留 `usage_limit_reached` / `usage_not_included` 走原有结构化 429 路径，避免把真正的额度耗尽误判成可恢复错误。

## 回归保护

- `codex-rs/codex-api/src/endpoint/responses_websocket.rs` 增加 unit test，锁住 generic wrapped 429 会被映射成 `ApiError::Retryable`。
- `codex-rs/codex-api/src/endpoint/responses_websocket.rs` 再增加握手期 HTTP 429 unit test，锁住 generic 429 会被映射成 `ApiError::Retryable`，而 `usage_limit_reached` 仍保持终止型 429。
- `codex-rs/core/tests/suite/client_websockets.rs` 增加集成测试，验证第一次 websocket 收到 generic 429 后会重新连上，并在第二次请求成功完成 turn。

## 经验

- 同一种服务端失败如果在 SSE 与 WebSocket 上各自维护一套分类逻辑，回归通常不是“某层没重试”，而是上游已经把错误提前改写成了终止类型。
- 调试重试问题时，先区分“底层 request retry 是否已耗尽”和“错误是否在进入 turn loop 前就失去了 retryable 语义”；两者的修复点完全不同。
