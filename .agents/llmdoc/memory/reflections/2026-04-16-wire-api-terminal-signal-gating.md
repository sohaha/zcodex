# wire_api 非 Responses 流式关闭要靠终止信号判定

## 背景
- 用户在 `wire_api = "anthropic"` 的兼容 provider 上遇到 `stream disconnected before completion: stream closed before anthropic message_stop`。
- 这轮顺手检查了 `wire_api = "chat"` 的流式 parser，发现它在 SSE 连接直接关闭时会无条件完成；而 `anthropic` 侧又过度依赖 `message_stop`，两边的“正常收尾 vs 异常断流”判定都不稳。

## 这次修复
- `codex-rs/codex-api/src/anthropic.rs`
  - 只在已经看到 `message_delta.stop_reason` 时，才把 transport close / SSE error 视为正常完成。
  - 若连接在 stop reason 之前关闭，继续显式报 `stream closed before anthropic message_stop`，不再因为已有局部文本就静默吞掉截断。
- `codex-rs/codex-api/src/chat_completions/stream.rs`
  - 新增 `saw_finish_reason`，只有在 provider 已经给出 `finish_reason` 时，才允许把连接关闭或尾部 transport error 当成正常完成。
  - 对没有 `finish_reason` 就结束的流，改为显式报 `stream closed before chat completions finish_reason`。
- 补了回归测试，分别覆盖：
  - 终止信号后断流仍应成功完成；
  - 终止信号前断流必须保留错误。

## 关键收获
- 对 `wire_api = "anthropic"` / `"chat"` 这类非 Responses 流式协议，不能把“连接断开”本身当成完成条件；真正可靠的是 provider 已经发出的终止信号（`stop_reason` / `finish_reason`）。
- 兼容 provider 常见的问题不是正文流不出来，而是尾部 `message_stop` / `[DONE]` 缺失；这种情况应该在“已看到终止信号”时降级为正常完成，而不是一刀切重试或报错。
- 反过来，如果连终止信号都没看到，就算已经收到了部分文本，也应保留错误，这样上层才能走 stream retry，而不是把截断响应误判成成功。

## 后续建议
- 以后改 `codex-api` 的任何流式 parser，都把“终止信号是否已出现”当成显式状态位，而不是只看 response id、是否收到了文本，或连接有没有自然关闭。
- 新增兼容 provider 时，优先补“终止信号后断流”“终止信号前断流”这对回归测试，避免再次在 parser 里把 transport 语义和协议终止语义混在一起。
