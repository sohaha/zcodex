# 401 应触发 request fallback，但不应重试同一 provider

## 背景
- request provider fallback 的目标是当主 provider 当前不可用时，尽快把请求切到下一个 provider。
- `401 Unauthorized` 对 API key / token provider 来说，常常意味着当前 provider 的凭证不可用，但并不代表整个请求必须失败。
- 同时，`401` 也不是瞬时流错误；继续在同一个 provider 上做 `stream_max_retries` 只会放大延迟和噪音。

## 结论
- `401` 应该允许触发 request fallback。
- `401` 不应该继续走同 provider 的 sampling retry。
- `404` 这类更像 endpoint 配置错误的状态码，默认仍不应触发 fallback，避免把主 provider 的配置错误静默掩盖掉。

## 实施要点
- 将“允许 request fallback 的状态码”和“允许同 provider sampling retry 的状态码”拆成两套判定。
- request fallback 侧允许：
  - `401`
  - `408`
  - `5xx`
- sampling retry 侧只允许：
  - `408`
  - `5xx`
- 回归测试不要只测 helper；要补一条真正经过 `run_sampling_request()` 的集成测试，显式断言：
  - primary `401` 只请求一次
  - fallback provider 接到后续成功请求
  - 不会在 primary provider 上把同一个 `401` 重试多次

## 验证提醒
- 如果顶层 `codex` 二进制因工作区其他 crate 的编译错误无法重链，先用 `codex-core` 集成测试验证 turn 行为，不要误把无关编译失败当成当前修复未生效。
