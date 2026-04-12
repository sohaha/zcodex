# 上游同步时让提交与基线状态保持可审计的反思

## 背景
- 这轮 `openai/codex` 同步已经把一批更晚的 upstream 变更落到了 `web`，但 `.codex/skills/sync-openai-codex-pr/STATE.md` 仍停在上一轮 `cbffb8c32` / `824ec94e...`。
- 承载主要同步内容的 `ea467a787` 标题写成 `sync: merge openai/codex main`，但它本身是单父提交，无法像标准 merge commit 那样直接从 git 结构读出本次 upstream target SHA。

## 问题本质
- 冲突决策规则本身没有失效；真正失效的是“同步记录闭环”。
- 当同步提交不是 merge-parent 型，若提交正文和 `STATE.md` 又没有补写 upstream 基线与目标 SHA，后续就只能靠文件级特征倒推同步范围。
- 这会让下次同步把“最近一次可准确审计的 upstream 基线”和“当前分支已经吸收的较新 upstream 内容”混在一起，增加误判增量和重复审计的风险。

## 这次有效做法
- 不伪造 `last_synced_sha`；保留 `STATE.md` 的可准确审计基线为 `824ec94e...`。
- 在 `STATE.md` 的 `notes` 里明确写出：`web` 上已经落地了更晚同步提交，但其精确 upstream 上界仍需重新核定。
- 把技能补强到“若不是标准 merge-parent，同步提交正文必须显式写出 `Previous upstream baseline`、`Upstream target sha` 和 `Actual upstream range`”。

## 以后怎么避免
- 优先保留标准 merge 结构，让 upstream commit 成为父提交之一；这样 `git show` 和 `git merge-base` 就能直接提供事实来源。
- 如果必须 squash 或人工整理，同步提交正文必须承担审计职责，不能只写模糊标题。
- `STATE.md` 只记录最近一次可准确审计的落地基线；一旦无法精确确认 target SHA，就在 `notes` 中显式说明，而不是静默沿用旧值。
