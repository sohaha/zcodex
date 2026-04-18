# Responses replay reasoning content 必须剥离

## 背景

2026-04-18 在排查一次 `git commit 中文提交` 的失败时，`/root/.codex/log/codex-tui.log` 显示 `/responses` 请求中的 `input[11]` 是一条 `type: "reasoning"` 历史项，并且带了：

- `summary`
- `content: [{"type":"reasoning_text","text":"..."}]`
- `encrypted_content: null`

服务端随后返回：

`Invalid 'input[11].content': array too long. Expected an array with maximum length 0, but got an array with length 1 instead.`

这说明 replay 历史到 Responses API 时，`reasoning.content` 不能继续原样回传。

## 结论

- `ResponseItem::Reasoning` 在本地历史/UI 中可以保留 raw reasoning，便于诊断和展示。
- 但发起下一次模型请求前，必须把 `reasoning.content` 从出站输入里剥离。
- 不能只依赖协议层 `serde skip_serializing_if` 作为唯一防线；运行时日志已经证明，坏 payload 仍可能逃到出站层。

## 落点

- 在 `core/src/client_common.rs` 的 `Prompt::get_formatted_input()` 里统一清空 `ResponseItem::Reasoning.content`，让所有 transport 共用同一层防线。
- 在请求链路测试里直接断言出站 JSON 的 reasoning item 不含 `content`，避免以后回归成“本地对象看似正常，但最终发出的 JSON 非法”。

## 验证与边界

- `RUSTC_WRAPPER= cargo check -p codex-core --lib` 可通过，说明主代码编译正常。
- 针对性测试被仓库现有问题阻塞：
  - `just fmt` 在当前环境失败。
  - `cargo test -p codex-core --test all ...` 被 `core/tests/suite/shell_command.rs` 现存未闭合定界符阻塞。

## 后续提醒

- 若再次看到 `invalid_request_error` 指向 `input[n].content`，优先先抓出实际请求体里的该项类型，不要只凭错误文案猜测。
- 对 `reasoning`、`compaction`、tool output 这类“本地历史可见，但服务端输入约束更严”的 item，优先在出站整理层做防御式清洗。
