# wire_api 非 Responses 断流兼容要区分无输出和可收敛输出

## 背景
- 用户在 `wire_api = "anthropic"` 的兼容 provider 上遇到 `stream disconnected before completion: stream closed before anthropic message_stop`。
- 之前刚收紧过一次逻辑，把 `anthropic` / `chat` 都改成“只有看到 `stop_reason` / `finish_reason` 才允许正常完成”，结果对一些正文已完整、只是尾部终止事件缺失的兼容 provider 仍然过严。

## 这次修复
- `codex-rs/codex-api/src/anthropic.rs`
  - 把 EOF / transport error 的正常完成条件放宽为：已经看到 `message_delta.stop_reason`，或已经积累出可收敛的文本 / reasoning / tool 输出。
  - 若 best-effort `finish()` 自己报错，不再静默吞掉，而是继续上抛流错误。
  - 对完全没有有效输出的提前关闭，仍显式报 `stream closed before anthropic message_stop`。
- `codex-rs/codex-api/src/chat_completions/stream.rs`
  - 把 EOF / transport error 的正常完成条件放宽为：已经看到 `finish_reason`，或已经积累出可收敛的文本 / reasoning / tool 输出。
  - 若 best-effort `complete()` 自己报错，不再静默吞掉，而是继续上抛流错误。
  - 对完全没有有效输出的提前关闭，仍显式报 `stream closed before chat completions finish_reason`。
- 补了回归测试，分别覆盖：
  - 终止信号后断流仍应成功完成；
  - 没有终止信号但已有可收敛输出时，断流仍应 best-effort 完成；
  - 完全没有有效输出时，断流必须保留错误。

## 关键收获
- 对 `wire_api = "anthropic"` / `"chat"` 这类非 Responses 流式协议，“连接断开”不是完成条件；完成判断要看 parser 手里是否已经有足够收敛成最终 `ResponseEvent` 的状态。
- 终止信号仍然是最强证据，但不是唯一证据。兼容 provider 常见的问题是正文和工具调用都已发完，只丢了尾部 `message_stop` / `[DONE]`；这时应做 best-effort complete，而不是把本可用输出一律打成错误。
- 反过来，如果还没有任何有效输出，只凭 response id、空 `content_block_start` 或连接自然关闭，仍不足以判定成功。
- 放宽断流兼容时，不能吞掉补完路径本身的解析错误；否则会把“协议不完整”误伪装成“正常结束”。

## 后续建议
- 以后改 `codex-api` 的任何流式 parser，都同时维护两类显式状态：终止信号是否已出现，以及当前是否已有可收敛输出。
- 新增兼容 provider 时，优先补三类回归测试：终止信号后断流、已有输出但无终止信号的断流、完全无输出的提前关闭。
