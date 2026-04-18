# 2026-04-18 上游同步的本地特性基线应拆成 json 权威源、discover 候选和 merge-back gate

## 背景
- `sync-openai-codex-pr` 之前把大量“本地分叉特性”和历史经验直接堆在技能正文里。
- 这样虽然信息都在一个地方，但实际执行时有两个问题：
  - 技能正文过长，规则、案例和当前基线混在一起，下一次同步很难快速定位“这轮必须保什么”
  - 本地特性清单只能靠人工阅读，缺少可重复刷新和 merge-back 前的机械审查

## 这次有效做法
- 把本地分叉特性从“技能正文里的长列表”升级成 4 段式：
  - `local-fork-features.json`：权威基线
  - `discover`：本地提交历史的候选发现
  - `promote`：主代理审阅后晋升候选
  - `local-fork-features.md`：从基线渲染的人类可读报告
- 用 `scripts/local_fork_feature_audit.mjs` 统一承载：
  - `discover`
  - `promote`
  - `render`
  - `check`
- 把 merge-back gate 固定在两个时点：
  - worktree 内冲突解决和定向验证之后
  - 回到当前分支、`git merge --no-commit` 之后再做一次

## 为什么这样更稳
- 权威基线和展示报告分离后，机器写入与人类阅读不再互相污染。
- `discover` 和 `promote` 分离后，自动发现与最终批准也被拆开，不会把子代理的误判直接写进长期基线。
- 在 merge-back 前强制再查一遍，可以兜住“worktree 里没丢，但合回当前分支时被本地并行改动覆盖”的情况。

## 这次暴露出的工作流结论
- 对上游同步这类长期重复任务，技能正文只该保留流程和判断规则，不该继续承载不断膨胀的本地特性台账。
- 子代理适合并发分析提交历史和提出 candidate ops，不适合直接并发改权威基线。
- `discover` 不能再把 `STATE.md:last_synced_sha` 当作默认起点；那个值是 upstream 目标基线，不是“本地已完成同步之后，我们自己继续提交的起点”。
- 更稳的默认规则是只认 `STATE.md:last_sync_commit`，而且它必须仍是当前 `HEAD` 的祖先；一旦祖先关系失效，就停止自动推断，改成显式 `--base-ref` 或带噪音预期的 `--merge-base-ref`。
- 并发子代理输出 candidate ops 时，不要靠人工粘贴汇总；应该让每个子代理单独落文件，再用脚本合并，并在同一 feature id 出现矛盾操作时立刻失败。
- 只要本地特性没有被明确证明为“更好的等效替换”，就不能仅因为 upstream 改了同一区域而删除旧特性。
- 如果本地特性已经 rename、move 或被新的实现覆盖，必须先更新 `json` 权威基线，再重新 `render` 和 `check`；否则下一轮同步还会把“新实现”误判成“特性丢失”。
