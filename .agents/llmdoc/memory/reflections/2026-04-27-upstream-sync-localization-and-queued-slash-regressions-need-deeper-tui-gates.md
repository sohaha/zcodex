# 同步后汉化与 queued slash 回归需要更深 TUI gate

## 现象
- 上游同步后 `codex-tui` 编译能过，但完整 TUI 测试暴露出大量用户可见回归：guardian 权限请求摘要、cyber notice、状态/底部栏、模型 reasoning popup 和模型目录子描述出现英文或旧快照。
- queued slash 命令存在更隐蔽行为回归：queued `/rename name` 从 composer 重新取参数，实际拿到下一条排队输入；`dispatch_command(/side)` 直调没有经过 side/review guard，先报 session 未开始。
- `/copy` 回滚历史相关测试必须覆盖真实非 replay agent event，否则不会记录 copy source，容易把 copy-history 行为误判成回归。

## 根因
- 同步 gate 只覆盖了模型主描述和少量高频 UI 文案，没有覆盖 `supported_reasoning_levels.description` 这类嵌套目录文本，也没有覆盖 `history_cell` / guardian / model verification 的汉化锚点。
- queued slash 与 live slash 共用 dispatch 分支时，某些分支仍默认依赖 `bottom_pane.prepare_inline_args_submission()`，这对 queued source 不成立；queued source 必须使用已解析出的 `PreparedSlashCommandArgs.args`。
- 只跑定向测试容易漏掉快照层的宽字符、状态行和模型目录漂移；完整 `codex-tui` 能把这些 user-visible 回归集中暴露出来。

## 经验
- 同步后检查汉化不能只 grep 顶层 description；模型目录要同时检查主描述、reasoning 子描述、upgrade/migration 文案和对应 TUI popup 快照。
- slash command 回归要分别覆盖 live input、queued input、inline args 和 direct `dispatch_command()`；这些路径的 guard 与参数来源不同。
- 接受快照前先修硬断言，再清理旧 `.snap.new` 并复跑完整 crate；确认剩余只是预期中文/模型变化后再接受快照。
