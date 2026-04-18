---
name: sync-openai-codex-pr
description: 在独立 worktree 中把 `openai/codex` 的 `main` 同步到当前分支：先做范围审计，再本地优先解决冲突；仅在“同功能两套实现必须二选一”或“上游原生功能已被 upstream 删除/回滚”时阻塞询问；合并回当前分支前后都要用本地分叉特性清单做审查，并更新同步基线与特性清单。
---

# Sync OpenAI Codex PR

仅在把 `openai/codex` 的 `main` 同步到当前分叉仓库时使用这个技能。

## 先读这些文件

- `/workspace/.codex/skills/sync-openai-codex-pr/STATE.md`
- `/workspace/.codex/skills/sync-openai-codex-pr/references/checklist.md`
- `/workspace/.codex/skills/sync-openai-codex-pr/references/local-fork-features.md`
- `/workspace/.agents/llmdoc/guides/upstream-sync-preservation-rules.md`

如果 `STATE.md` 不存在，先按 `references/checklist.md` 里的模板初始化，再继续同步。

## 目标

- 在独立 `git worktree` 中完成 upstream merge、冲突解决、验证和审查。
- 默认保护本地分叉行为、中文化输出和社区分支 branding。
- 区分：
  - `本地分叉独有能力`：默认不能被上游直接覆盖。
  - `上游原生能力`：如果 upstream 已删除或回滚，不能静默保留，必须请求用户决定是否跟随删除。
- 通过 `STATE.md` 和本技能目录里的本地分叉特性清单保持同步过程可审计。

## 本技能的两个基线文件

- `STATE.md`
  - 记录最近一次已经落地、且可准确审计的 upstream 基线。
  - 只有在同步真正落地后才能更新。
- `references/local-fork-features.md`
  - 这是本地分叉特性的独立事实源。
  - 文件里同时包含：
    - 手工维护的特性规范
    - 脚本刷新出来的最新审查报告
  - 任何新增、删除、迁移或等效替换的本地分叉特性，都要回写这个文件。

## 必用脚本

用下面的脚本刷新或审查本地分叉特性清单：

```bash
node /workspace/.codex/skills/sync-openai-codex-pr/scripts/local_fork_feature_audit.mjs refresh --repo /workspace
node /workspace/.codex/skills/sync-openai-codex-pr/scripts/local_fork_feature_audit.mjs check --repo <repo-or-worktree>
```

规则：

- `refresh`
  - 重新扫描指定仓库，并把最新审查报告写回 `references/local-fork-features.md`。
  - 默认在当前分支开始同步前执行一次，最终合并落地后再执行一次。
- `check`
  - 只检查，不改文件。
  - 默认在 worktree 内冲突解决完成后执行一次。
  - 默认在把同步分支合并回当前分支、但尚未提交时再执行一次。
- 重要：
  - 审查 worktree 时，必须使用 worktree 自己的脚本和清单副本，而不是主工作区 `/workspace` 下的技能文件。
  - 否则你会在 worktree 阶段误改主工作区清单，导致审查和落地结果脱节。

如果脚本报错或返回缺失项：

1. 先定位缺失/覆盖发生在哪个文件和符号。
2. 再判断原因：
   - 真丢了
   - 被 rename / move
   - 被 upstream 用更好的等效实现替换了
3. 处理规则：
   - 如果不是“明确更好且行为不回退”的替换，必须修回来。
   - 如果确实已经换成更好的方式，就更新 `references/local-fork-features.md` 的特性定义和更优实现判定条件，再重新 `refresh`。

## 决策规则

### 默认优先级

1. 本地代码/行为不回退
2. 再吸收 upstream 新能力
3. 最后最小化 diff

### 冲突分类

1. `机械冲突`
   - import、rename、格式、同语义并行修改
   - 直接融合，保持最小 diff
2. `逻辑可融合`
   - 两边改动互补、不需要牺牲一方功能
   - 默认融合，并保住本地行为
3. `同功能双实现`
   - 同一能力有两套实现，无法合理融合
   - 停下，请求用户二选一
4. `上游原生功能已被删除/回滚`
   - 先查历史确认它最初来自 upstream，而不是本地分叉
   - 停下，请求用户决定是否跟随 upstream 删除

### 只有这两类情况才允许阻塞问用户

- 同功能双实现，必须二选一
- 上游原生能力已被 upstream 删除/回滚，需要决定是否继续作为本地分叉保留

## 工作流

详细命令见 `references/checklist.md`。默认按这个顺序执行：

1. 刷新当前分支的本地分叉特性基线。
   - 运行 `refresh --repo /workspace`
   - 如果失败，说明当前分支本身已偏离基线，先修正或更新特性定义，再开始同步
2. 创建独立 worktree，并记录 `base_branch`
3. 读取 `STATE.md`，拉取 `openai/main`
   - 如果 `openai` remote 已存在，先校验 URL；若不是 `https://github.com/openai/codex.git`，先修正再拉取
4. 做两次审计
   - `origin/$base_branch...openai/main`
   - 如果 `STATE.md` 里有 `last_synced_sha`，再看 `last_synced_sha..openai_sha`
5. 在 worktree 里 `git merge --no-edit openai/main`
6. 按冲突分类解决问题
7. 做定向验证
8. 在 worktree 里执行一次 `check --repo "$path"` 审查
9. 合并回当前分支，但先不要提交
10. 在当前分支工作区再执行一次 `check --repo /workspace`
11. 若审查通过，执行一次 `refresh --repo /workspace`，更新落地后的特性报告
12. 更新 `STATE.md`，把同步代码、技能改动、特性清单和状态文件一起提交

## Rust 验证要求

改了 Rust 代码后：

```bash
cd /workspace/codex-rs
just fmt
```

然后先跑最窄的相关测试。优先 `cargo nextest run -p <crate>` 或仓库已有 fast wrapper。

如果改了 `Cargo.toml` 或 `Cargo.lock`，额外执行：

```bash
cd /workspace
just bazel-lock-update
just bazel-lock-check
```

如果改动触及 `common`、`core` 或 `protocol` 这类共享区域，局部验证通过后再决定是否扩大。

## Responses / reasoning 相关专项检查

如果本次同步触及 Responses 输入序列化、Prompt 格式化、历史 replay 或 reasoning item 相关链路，例如：

- `codex-rs/core/src/client_common.rs`
- `codex-rs/protocol/src/models.rs`
- `codex-rs/codex-api/src/common.rs`
- `codex-rs/codex-api/src/endpoint/responses*.rs`

额外执行以下检查：

- 确认 replay 到 Responses API 的 `ResponseItem::Reasoning` 不会回传 raw `content`
- 允许保留 `summary` / `encrypted_content`，但出站输入里不能再带 `reasoning_text`
- 若看到 `invalid_request_error` 指向 `input[n].content`，先抓真实请求体再判断，不要只凭报错文案猜测
- 若仓库现有测试被无关编译错误阻塞，至少补一条靠近出站层的断言或单测，防止这个约束在同步时回归

## 合并回当前分支时的强制审查

这是本次优化后的核心规则：

- `references/local-fork-features.md` 不再只是备注，而是 merge-back gate
- 合并回当前分支之前，必须至少做两次审查：
  - worktree 内一次
  - 当前分支 `git merge --no-ff --no-commit "$branch"` 之后再一次
- 只要 `check` 报缺失项，就不能提交
- worktree 审查时要读写 worktree 自己的 `.codex/skills/sync-openai-codex-pr/references/local-fork-features.md`

对每个缺失/覆盖项，必须给出原因：

- 被上游覆盖
- 本地冲突解决丢失
- 文件或符号被移动
- 已被更好的等效实现替代

如果不能证明是“更好的等效实现”，就按功能回归处理并修复。

修复后，重新执行 `check`，直到通过为止。

## `STATE.md` 规则

- `STATE.md` 只记录最近一次已落地的真实 upstream 基线
- 如果这轮代码最后没有落地，不得留下新的 `last_synced_sha`
- 如果同步内容已经吸收，但无法精确核定 target SHA：
  - 不要伪造 `last_synced_sha`
  - 在 `notes` 中明确写出原因

优先保留标准 merge 结构，让 upstream commit 成为父提交之一。若最终只能 squash、cherry-pick 或人工整理式提交，提交正文必须写清：

- `Previous upstream baseline`
- `Upstream target sha`
- `Actual upstream range`

## 最终输出必须覆盖

- 上次基线 SHA
- 本次目标 upstream SHA
- 本次实际同步范围
- 主要合并内容
- 关键冲突如何处理
- 哪些本地分叉特性被审查
- 是否发现功能丢失/覆盖，以及原因
- 是否存在“更好的等效替换”，以及因此更新了哪些特性定义
- 跑了哪些验证
- `STATE.md` 和 `references/local-fork-features.md` 是否已更新
