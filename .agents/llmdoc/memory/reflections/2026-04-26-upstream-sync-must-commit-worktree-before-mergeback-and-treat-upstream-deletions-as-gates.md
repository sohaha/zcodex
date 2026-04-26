# 2026-04-26 upstream sync 必须先提交 worktree 再 merge-back，并把 upstream 删除当 gate

## 背景

本轮同步 `openai/codex` 到本地 `web` 分支时，同步 worktree 分支和主工作区 `HEAD` 一度指向同一个提交，但大量 upstream 合并结果仍停留在 worktree 的 staged/unstaged 状态中。直接在主工作区 merge 该分支会是 no-op，无法带回实际同步内容。

## 经验

- 如果同步 worktree 分支与主工作区 `HEAD` 相同，先在同步 worktree 内把 staged 与 unstaged 的完整结果提交成真实 sync commit，再从主工作区 `git merge --no-ff --no-commit <sync-branch>`。
- `STATE.md:last_sync_commit` 可以记录同步 worktree 的真实 sync commit，用于后续 discover 起点；最终 merge-back commit 可作为落地提交存在，但不适合作为 worktree 内审查基线。
- 上游删除的功能不能只看代码是否还编译。必须跑“upstream 删除反查 gate”，并对高风险功能做主动面 grep，包括工具注册、schema、CLI/TUI/app-server 暴露面。
- `js_repl` 这类上游已删除功能应默认跟随 upstream 移除，除非用户明确要求作为本地分叉保留；保留 legacy replay 或“忽略已删除 key”的测试字符串不等于主动功能残留。
- 大范围同步后，`cargo check` 通过不代表测试 fixture 已对齐。`cargo test -p codex-core --no-run` 能更早暴露旧 fixture 漂移，但修复范围可能大于同步收口本身，应在总结中明确列为剩余风险。

## 后续动作

- 下次同步前先确认 `sync-openai-codex-pr` 技能仍要求 worktree gate 与 merge-back gate 各跑一次。
- 如果 `just` 不可用，应记录用 cargo bin 替代生成 schema 的命令，以及未运行的 `just bazel-lock-update/check` 风险。
