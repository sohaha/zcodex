# Anthropic 兼容流要同时识别 `[DONE]` 和顶层 `error`

## 背景
- 用户在 `wire_api = "anthropic"` 的兼容 provider 上持续看到 `stream closed before anthropic message_stop`，即使 `core` 已经触发了采样重试。
- 日志显示 provider 在失败时并不总发标准 Anthropic `{"type":"error",...}` 或 `message_stop`，而是会发顶层 `{"error":{"code":"1305","message":"The service may be temporarily overloaded, please try again later"}}`，随后直接发 `[DONE]`。

## 这次修复
- `codex-rs/codex-api/src/anthropic.rs`
  - 在 JSON 事件解析前先检查顶层 `error` payload；若命中，立即映射成 `ApiError`，不再等到 EOF 才报泛化的 `message_stop` 错误。
  - 把 `[DONE]` 当作显式终止信号处理，即便没有 `message_stop` 也允许正常 `finish()`。
  - `AnthropicErrorPayload` 补 `code` 字段，并把 `1305` / `server_is_overloaded` / `slow_down` 映射为 `ApiError::ServerOverloaded`。
- 补了回归测试，覆盖：
  - 有文本输出但只有 `[DONE]`、没有 `message_stop` 时仍应正常完成；
  - 顶层 overload `error` payload 应直接映射成 `ServerOverloaded`；
  - 旧式 overload code `1305` 的映射。

## 关键收获
- 兼容 provider 的“Anthropic SSE”不能只按官方 `type` 字段来判定错误和结束；有些网关会混入 OpenAI 风格的 `[DONE]`，也会把错误包成顶层 `error` 对象。
- 如果 parser 忽略了这些兼容信号，`core` 虽然还能把结果当成可重试的 `Stream` 错误，但最终用户看到的会是误导性的 `stream closed before anthropic message_stop`，丢掉真正的 overload 原因。
- 对兼容协议做 parser 时，要把“官方事件”、“网关兼容终止信号”、“顶层错误信封”三类输入都纳入显式分支，而不是只在 JSON 反序列化失败后静默跳过。
