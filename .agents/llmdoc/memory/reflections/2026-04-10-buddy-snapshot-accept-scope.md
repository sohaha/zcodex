# buddy snapshot 接收范围反思

## 背景
- 在 `codex-rs/tui/src/buddy/` 扩展参考 `reference-projects/claude-code-rev` 的额外宠物时，需要更新 buddy snapshot。
- 工作区本身已经带着其他 crate 和 TUI 区域的 pending snapshot 状态。

## 发生了什么
- 先通过 `cargo test -p codex-tui buddy::tests` 生成了 buddy 相关 `*.snap.new`。
- 随后在 `codex-rs/` 根目录直接执行 `cargo insta accept`，结果把工作区里其他 pending snapshot 也一并接收了。
- 虽然这些额外改动可以通过把无关 `.snap` / `.snap.new` 恢复到 `HEAD` 收回来，但这一步本身扩大了变更边界，也增加了审阅噪音。

## 根因
- `cargo insta accept` 默认处理当前工作区下所有 pending snapshots，不会自动限定到本次任务触及的子目录。
- 在 dirty monorepo 中，snapshot 工具的作用范围必须显式收紧；不能因为“当前只关心一个子系统”就默认工具也只会处理那个子系统。

## 下次怎么做
- 先用 `find <target-dir> -name '*.snap.new'` 或等价命令确认 pending snapshot 只在目标子目录。
- 如果仓库其他区域也有 pending snapshot，不要在 workspace 根目录直接跑 `cargo insta accept`。
- 只接受本任务目标目录下的 snapshot；如果工具本身不支持按 crate/路径收敛，就手动审阅并仅移动目标 `*.snap.new`。
- 一旦误接收，立刻把无关 `.snap` 和 `.snap.new` 恢复到 `HEAD`，再继续当前任务。

## 适用范围
- 所有在 `codex-rs/` 里做局部 snapshot 变更、同时工作区不是 clean 的任务。
