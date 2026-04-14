# 2026-04-14 上游同步时 TUI 本地化与 snapshot 的收敛回路

## 背景
- 这轮把 `openai/codex` 的大量 TUI 变更同步到本地 `web` 分叉，同时要保留中文化输出。
- 代码层冲突解决后，`codex-tui` 仍会在 full nextest 里持续暴露多轮回归：先是 plan/history cell，再是 slash command、runtime metrics、MCP history cell，最后是 status snapshot。

## 这轮踩到的坑
- 只看“代码能编译”不足以证明同步完成；很多回归只会在 snapshot 或文本断言里暴露。
- 如果先大批量接收 snapshot，再回头补中文文案，容易把上游英文输出误当成新基线，扩大审阅噪音。
- 解决 worktree 内冲突后，索引里的 `UU` 可能只是“内容已干净但尚未 `git add`”，不能把这种状态误判成还有真实冲突。

## 这轮有效做法
- 先用失败用例把问题按文案面分组，优先修真正的中文回归，再跑定向测试，最后再回到 full `cargo nextest run -p codex-tui`。
- 对 TUI 同步，代码、测试断言、snapshot 三者要一起收敛；只改其中一层通常会在下一轮 nextest 继续爆同类问题。
- 对 `git diff --name-only --diff-filter=U` 里的文件，先检查是否还存在 `<<<<<<<` 标记；若内容已干净，就直接 `git add` 收口 merge state。
- 在同步分支里先把 `codex-tui` 全量跑绿，再回主分支做最终 merge，可以把“逻辑回归”和“主分支已有本地提交”两个问题拆开。

## 下次怎么做
- 遇到本地中文化仓库的 upstream sync，默认把 `history_cell.rs`、`status/`、`chatwidget/tests/` 和相关 snapshot 当成同一收敛面处理。
- full nextest 失败时，不要一次性接收所有 `.snap.new`；先确认失败是否源于本地化遗漏，再只接收本轮已经确认正确的 snapshot。
- merge 冲突清理完成后，额外跑一次 `git diff --name-only --diff-filter=U`，避免把“索引未收口”拖到后续主分支合并阶段。
