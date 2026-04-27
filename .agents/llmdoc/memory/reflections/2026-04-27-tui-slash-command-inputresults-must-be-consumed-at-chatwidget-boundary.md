# TUI 的 slash command `InputResult` 必须在 ChatWidget 边界继续消费

## 现象
- 新会话已经稳定启动后，输入 `/model` 回车没有任何效果。
- 同一条输入链上的 bare slash command 与 inline slash command 相关测试也会失效，例如 `/goal` 不会打开菜单，`/rename Better title` 不会进入重命名流程。

## 根因
- `codex-rs/tui/src/bottom_pane/chat_composer.rs` 会把 slash command 解析成 `InputResult::Command` 或 `InputResult::CommandWithArgs`，并在 composer 内先暂存本地 command history。
- 但 `codex-rs/tui/src/chatwidget.rs` 的 `handle_key_event()` 只处理 `Submitted` 和 `Queued`，其余分支直接丢弃，导致 slash command 在清空输入框后被静默吞掉。
- 真正负责补完 dispatch 和 command-history 提交的 `handle_slash_command_dispatch()` / `handle_slash_command_with_args_dispatch()` 已经存在于 `codex-rs/tui/src/chatwidget/slash_dispatch.rs`，但当时没有从 `handle_key_event()` 接回去，编译期甚至会出现“方法未使用”的信号。

## 修复
- 在 `ChatWidget::handle_key_event()` 里显式消费 `InputResult::Command` 与 `InputResult::CommandWithArgs`，统一转发到现有 `handle_slash_command_dispatch*` 包装函数。
- 保持 dispatch 逻辑、`/goal` 的 pending submission drain，以及本地 slash command history 提交都继续走现有包装层，不在 `BottomPane` 或 composer 里重复拼接行为。

## 经验
- 这类输入链回归要先沿“composer 产物 -> 父层消费 -> app 级 dispatch”逐层核对，不要被“线程是否已初始化”之类更表层的门禁误导。
- 如果某个模块专门引入了“带副作用的包装 dispatch 函数”，同时编译器又提示它们未使用，往往说明接线在更高一层已经断了。
- slash command 的回归保护不能只测 `dispatch_command()` 直调；还要保留从真实输入路径 `set_composer_text(...) + Enter` 触发的测试，否则父层吞结果的 bug 会被绕过去。
