---
name: sync-openai-codex-pr
description: 在独立 worktree（从 web 迁出）同步 openai/codex main 到当前仓库：本地优先解决冲突；先做 code-review 评估改动范围与冲突性质；只有遇到“同功能两套实现必须二选一”才在 PR comment 阻塞并请求选择；CI 全绿后才允许 merge。
---

# sync-openai-codex-pr（上游同步 PR）

## 目标
- 拉取 `https://github.com/openai/codex.git` 的 `main` 并同步到当前仓库。
- 在独立 `git worktree` 内完成合并、修冲突、推送、创建/更新 PR；不污染当前分支/工作区。
- 默认策略：**本地代码/行为优先**（除非你明确选择采纳上游实现）。
- 同步完成后，对新增/变更的用户可见文案执行**汉化处理**：优先自然中文表达，保留不适合硬翻译的专有名词（命令名、协议名、产品名、crate 名、API 字段名等）。
- 门禁：**CI required checks 全绿** 才能合并。

## 前置检查（开始前 2 分钟）
- 确认 `gh auth status` 已登录且有权限创建 PR。
- 确认当前仓库无未提交改动（避免误混入到 worktree 的提交）。
- 若 `openai` remote 已存在但指向错误，先修正地址。

## 决策规则（先 code-review 再动手）
- 先做**改动范围审计**：哪些目录/模块变了、风险点在哪（比如多 agent/agent teams、hooks/cleanup、TUI 命令 `/clear` `/theme` 等）。
- 冲突分类（按优先级）：
  1) **机械冲突**（format/import/rename/move/并行改同一段但语义一致）：直接合并，保持最小 diff。
  2) **逻辑可融合**（两边改动互补，功能不重复）：融合到一个实现，默认保持本地行为不回退。
  3) **同功能双实现（必须二选一）**：不要私自拍板；必须在 PR comment 里写清楚差异并 @你选择，然后**阻塞流程**等待决定。
 - 对每个冲突做最小化修复后，记录关键决策点到 PR 描述或 comment（方便复盘与评审）。

## 工作流

### 1) 从 `web` 创建 worktree（不污染当前分支）
在仓库根目录执行：

```bash
ts="$(date +%Y%m%d-%H%M%S)"
branch="sync/openai-codex-$ts"
path=".worktrees/sync-openai-codex-$ts"
git fetch origin web
git worktree add -b "$branch" "$path" origin/web
```

进入 worktree：

```bash
cd "$path"
```

### 2) 拉取上游并确认最新 commit（再合并）

```bash
git remote add openai https://github.com/openai/codex.git 2>/dev/null || true
git fetch openai main
openai_sha="$(git rev-parse openai/main)"
echo "openai/codex main: $openai_sha"
```

### 3) 先做改动范围审计（冲突前/后都做一次）

```bash
git diff --name-status origin/web...openai/main
git diff --stat origin/web...openai/main
```

如果你怀疑某些用户可见能力被“同步时舍弃”，在这里就能直接定位文件范围（例如 `/clear`、`/theme`、agent teams 等）。

### 4) 合并上游并处理冲突

```bash
git merge --no-edit openai/main
```

如果出现冲突：
1) 先列出冲突文件：`git status`  
2) 对每个冲突做快速 code-review 分类（机械 / 逻辑可融合 / 同功能二选一）  
3) 机械冲突、逻辑可融合：直接解决并继续  
4) 只有“同功能二选一”才进入下一节的 PR comment 阻塞流程  

### 5) 仅在“同功能二选一”时写阻塞性 PR comment
当且仅当你确认两边是**同一个功能**的两套实现，且无法合理融合：
- 在 PR comment 里写清楚：
  - 文件路径 + 关键函数/结构体
  - 行为差异（接口、边界条件、失败模式）
  - trade-off（复杂度、可维护性、性能、安全性、测试覆盖）
  - 需要你拍板的选项：**保留本地** / **采用上游**
- 在你选择前，停止继续推进（避免“默认本地优先”把上游同功能实现静默删掉）。

### 6) 最小化修复 + 格式化 + 目标测试
Rust（改 Rust 代码后）：

```bash
cd codex-rs
just fmt
```

优先跑最窄的相关测试（例）：

```bash
cargo test -p codex-core
```

如果改了 `Cargo.toml` / `Cargo.lock`：

```bash
cd ..
just bazel-lock-update
just bazel-lock-check
```

### 7) 同步后汉化（新增必做）
在完成冲突解决后、提交前，检查上游引入的用户可见文本（CLI/TUI 文案、帮助信息、提示语、错误提示、文档说明）并进行汉化：
- 目标：中文读起来自然，不做逐词直译。
- 保留原文的项目内专有名词，不强行翻译（如 `agent`、`hook`、`session`、`sandbox`、命令/子命令、配置键、协议字段）。
- 保持行为语义不变，不修改非文案逻辑。
- 汉化后重新检查相关快照/测试，确保输出稳定。

### 8) 提交 + 推送

```bash
git status
git add -A
git commit -m "sync: openai/codex @ <sha>"
git push -u origin HEAD
```

### 9) 创建/更新 PR
创建 PR：

```bash
gh pr create --base web --head "$branch" --title "sync: openai/codex @ <sha>" --body "Sync upstream openai/codex main. Local code prioritized; see commit(s) for conflict resolutions."
```

PR 已存在时，直接 push 新 commit 即可触发更新。

### 10) CI 门禁（必须）
只在 required checks 全绿后才允许 merge：

```bash
gh pr checks --repo <owner/repo> --watch <PR>
```

如果出现同功能二选一的阻塞 comment：等你选择后再继续修复/重跑 CI。

## PR comment 模板（仅二选一场景）
```
同功能双实现冲突，需要选择：
- 位置：<file>:<symbol>
- 行为差异：<差异点 1/2/3>
- 影响：复杂度 / 可维护性 / 性能 / 安全 / 测试覆盖
- 选项：保留本地 / 采用上游
```
