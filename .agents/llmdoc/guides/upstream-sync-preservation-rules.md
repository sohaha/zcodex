# 上游同步时的本地分叉保留规则

## 适用范围
- 把 `openai/codex` 的变更同步到当前分叉仓库，且需要判断“跟上游”还是“保留本地”时。

## 先做的判断
- 先区分这次冲突属于哪一类：
  - 本地分叉独有功能：当前仓库自己加的能力，上游没有长期同等实现。
  - 上游原生功能：最初来自 `openai/codex`，当前仓库只是跟随同步进来。

## 默认规则
- 本地分叉独有功能，默认不能被上游直接覆盖。
- 上游测试或重构如果触碰到本地分叉行为，先验证本地真实输出，再决定是否接上游断言。
- 只有在“同一功能出现两套实现且必须二选一”时，才把冲突升级成需要用户拍板的问题。

## 上游删除时的特殊规则
- 如果确认某项能力是上游原生功能，且 upstream 已删除或回滚，本地不能静默保留。
- 这类情况必须明确提醒用户，并把决策收敛成两项：
  - 跟随上游删除
  - 继续作为本地分叉保留
- 用户未明确保留前，不能把“当前仓库里还留着旧实现”当作默认答案。

## 同步记录的可审计性规则
- 优先保留标准 merge 结构，让 upstream commit 直接成为父提交之一。
- 若最终只能用 squash、cherry-pick 或人工整理式提交，提交正文必须显式写出：
  - `Previous upstream baseline`
  - `Upstream target sha`
  - `Actual upstream range`
- `STATE.md` 只记录最近一次**可准确审计**的已落地基线；若代码已经吸收了更晚同步内容，但 target SHA 仍无法精确确认，不要伪造 `last_synced_sha`，而是在 `notes` 中明确说明原因。

## 本地特性基线应独立外置
- 不要把本地分叉特性台账长期堆在技能正文里；应拆成：
  - 权威基线 `json`
  - 展示报告 `md`
  - discover / promote / render / check 脚本
- 当前仓库的 upstream sync 使用：
  - `.codex/skills/sync-openai-codex-pr/references/local-fork-features.json`
  - `.codex/skills/sync-openai-codex-pr/references/local-fork-features.md`
  - `.codex/skills/sync-openai-codex-pr/scripts/local_fork_feature_audit.mjs`
- 权威基线至少要能表达：
  - 特性 ID / 作用区域 / 为什么要保留
  - “什么情况下算被更好的实现替换”
  - 可脚本化的存在性或文本证据检查

## 先 discover，再 promote
- 自动发现和最终批准不是同一个动作。
- 正确顺序：
  1. `discover` 先扫描本地提交历史，输出 commit / path / 既有特性命中 / 未覆盖路径
  2. 子代理并发分析 discover 结果，只产出 candidate ops
  3. 主代理先把多个 candidate 文件合并，再审阅后 `promote` 到权威基线
  4. `render` 生成新的展示报告
  5. `check` 作为 merge-back gate 审查当前 repo 或 worktree
- 不要让子代理直接并发改权威基线；那会把“候选结论”和“最终批准”混在一起，容易把误判永久写进同步基线。
- 更稳的做法是让子代理各自写独立 candidate 文件，再由主代理用脚本合并；同一 feature id 一旦出现互相矛盾的 upsert/remove，应该在合并阶段就失败，而不是靠人工肉眼发现。
- `discover` 默认只能从 `STATE.md:last_sync_commit` 推断范围，而且这个提交必须仍是 `HEAD` 的祖先。
- 不要把 `last_synced_sha` 当成“我们自己的提交范围”默认起点；它表示 upstream 基线，不等于本地已落地同步提交。
- 如果 `last_sync_commit` 因 rebase、cherry-pick 或人工整理式同步而不再是祖先，必须显式改用 `--base-ref <trusted-local-commit>`，或在接受更宽噪音的前提下使用 `--merge-base-ref <ref>`。

## merge-back 前必须做两次特性审查
- 第一次：worktree 内冲突解决和定向验证之后
- 第二次：回到当前分支，`git merge --no-ff --no-commit "$branch"` 之后
- 两次都应该跑权威基线的 `check`，不要只凭肉眼 diff 或记忆判断“应该没丢”

## 缺失项的处理顺序
- 先找原因，再决定动作；不要一看到 `check` 失败就立即把特性重新抄回去。
- 合理原因只有四类：
  - 真丢了
  - rename / move 了
  - 冲突解决时被覆盖了
  - 已被更好的等效实现替代
- 只有在“行为不回退”且“特性清单定义已经更新到新实现”时，才允许把缺失项判定为等效替换。
- 否则，一律按本地功能回归处理并修复。

## 本次同步验证到的例子
- `codex-rs/account` 属于上游原生功能，不是本地分叉独有能力。
- 它由 `9f2a58515` 引入，又被 `930e5adb7` 回滚，因此本地应先按“是否跟随 upstream 删除”来处理，而不是直接保留。

## 最小验证闭环
- 先查看功能的引入/回滚提交历史，确认来源归属。
- 再检查本地是否真的有额外分叉语义，而不是只是在旧同步点上滞留。
- 接受或删除后，跑受影响 crate 的定向测试，不把环境噪音误判成同步逻辑问题。
