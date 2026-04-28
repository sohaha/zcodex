# Responses custom tool provider 兼容错误必须在流消费边界重试

## 背景
- `wire_api = "responses"` 的兼容 provider 可能接受首个 HTTP 200 / WebSocket 建连成功，但在流内部通过 `response.failed` 或等价错误事件返回 `invalid_request_error`。
- `apply_patch` 的 freeform/custom tool 在这类 provider 上会出现 `tools[N].type: unknown variant 'custom', expected 'function'`。
- 如果只在 `client.stream_request()` 调用点附近做“同 provider 降级后重试”，这类错误会漏过该分支，最后在 `map_response_stream()` 之后才变成 `CodexErr::InvalidRequest`，从而误触发本地 `fallback_providers`。

## 结论
- 对 Responses provider 兼容性做本地自愈时，不能只拦 request 建立阶段的 `ApiError`。
- 还要在真正消费 `ResponseStream` 的边界补一层“尚未产生实际输出时的同 provider 一次性重试”。
- 对 `apply_patch` 这类能力学习，真相源应来自 provider 实际返回的错误文本，而不是 base URL/域名是否官方。

## 实施要点
- 先把 `unknown variant 'custom'` + `tools[` + `.type` 收敛成共享判定函数。
- `ModelClientSession` 既要能识别 request-level `ApiError`，也要能识别 stream-level `CodexErr::InvalidRequest` / `CodexErr::Stream`。
- `try_run_sampling_request()` 这类流消费主循环里，只在还没进入实际输出阶段时允许一次本地重试；一旦出现真实输出 item/delta，就不能静默重放整个请求。
- 重试前要记录 provider 已知“不支持 responses custom apply_patch”，并重置当前 websocket session，避免沿用带毒连接状态。

## 验证提醒
- 若要验证“不再掉进 fallback provider”，测试应显式配置 `fallback_providers`，并断言：
  - primary provider 收到两次请求
  - fallback provider 零次请求
  - 第一次请求里的 `apply_patch` tool type 为 `custom`
  - 第二次请求里的 `apply_patch` tool type 为 `function`
