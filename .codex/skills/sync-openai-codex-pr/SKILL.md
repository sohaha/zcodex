---
name: sync-openai-codex-pr
description: 在独立 worktree（从 web 迁出）同步 openai/codex main 到当前仓库：本地优先解决冲突；先做 code-review 评估改动范围与冲突性质；只有遇到“同功能两套实现必须二选一”才阻塞并请求选择；完成验证后直接合并回原分支，并汇总合并功能与影响。
---

# sync-openai-codex-pr（上游同步）

## 目标
- 拉取 `https://github.com/openai/codex.git` 的 `main` 并同步到当前仓库。
- 在独立 `git worktree` 内完成合并、修冲突、验证；不污染当前分支/工作区。
- 默认策略：**本地代码/行为优先**（除非你明确选择采纳上游实现）。
- 同步完成后，对新增/变更的用户可见文案执行**汉化处理**：优先自然中文表达，保留不适合硬翻译的专有名词（命令名、协议名、产品名、crate 名、API 字段名等）。
- 完成后**直接合并回原分支**，不走 PR 流程。
- 最终必须输出：**本次合并进来的功能**、**影响范围**、**是否有原功能丢失/覆盖/冲突**。

## 前置检查（开始前 2 分钟）
- 确认当前仓库无会被误带入的已跟踪改动；若存在未跟踪目录/文件，先确认是否忽略。
- 若 `openai` remote 已存在但指向错误，先修正地址。
- 记录当前分支名，后续同步完成后要合并回它。
- 固定使用技能目录下的 `STATE.md` 作为同步基线登记点；不要只依赖 commit message 追踪历史。

## 决策规则（先 code-review 再动手）
- 先做**改动范围审计**：哪些目录/模块变了、风险点在哪（比如多 agent/agent teams、hooks/cleanup、TUI 命令 `/clear` `/theme` 等）。
- 先区分两类能力来源：
  1) **本地分叉独有能力**：例如本地新增命令、协议事件、中文化交互、兼容层、额外恢复/路由逻辑。**默认不能被上游直接覆盖**；若上游改到同一区域，应先尝试融合并保住本地能力。
  2) **上游原生能力**：即功能最初来自 upstream，后来 upstream 又删除、回滚或重构。**不要因为“当前本地还有这段代码”就默认保留**；如果同步会让该能力消失，必须明确提醒用户这是“上游原生功能删除/回滚”，并请求是否跟随上游。
- 冲突分类（按优先级）：
  1) **机械冲突**（format/import/rename/move/并行改同一段但语义一致）：直接合并，保持最小 diff。
  2) **逻辑可融合**（两边改动互补，功能不重复）：融合到一个实现，默认保持本地行为不回退。
  3) **同功能双实现（必须二选一）**：不要私自拍板；必须汇总差异并请求你选择，然后**阻塞流程**等待决定。
- 额外规则：
  - **本地分叉独有功能**：默认保留；除非用户明确同意，不要为了贴 upstream 而直接删掉。
  - **上游原生功能被 upstream 删除/回滚**：必须单独提醒并询问是否跟随 upstream 删除；不能把“本地仍存在”误判成“必须保留的分叉功能”。
- 对每个关键冲突做最小化修复后，记录关键决策点，便于最后汇总影响。

## 工作流

### 1) 从当前分支创建 worktree（不污染当前分支）
在仓库根目录执行：

```bash
ts="$(date +%Y%m%d-%H%M%S)"
base_branch="$(git branch --show-current)"
branch="sync/openai-codex-$ts"
path=".worktrees/sync-openai-codex-$ts"
git fetch origin "$base_branch"
git worktree add -b "$branch" "$path" "origin/$base_branch"
```

进入 worktree：

```bash
cd "$path"
```

### 2) 读取上次基线，再拉取上游并确认最新 commit（再合并）

先读取技能目录中的状态文件；若不存在则新建：

```bash
skill_dir="/workspace/.codex/skills/sync-openai-codex-pr"
state_file="$skill_dir/STATE.md"

if [ ! -f "$state_file" ]; then
  cat > "$state_file" <<'EOF'
# sync-openai-codex-pr state

- upstream_repo: https://github.com/openai/codex.git
- upstream_branch: main
- last_synced_sha: <none>
- last_synced_at_utc: <none>
- last_synced_base_branch: <none>
- last_sync_commit: <none>
- notes: 初始化，尚未执行同步。
EOF
fi

echo "Current sync baseline:"
cat "$state_file"
previous_sha="$(sed -n 's/^- last_synced_sha: //p' "$state_file")"
echo "previous upstream sha: ${previous_sha:-<none>}"
```

再拉取上游：

```bash
git remote add openai https://github.com/openai/codex.git 2>/dev/null || true
git fetch openai main
openai_sha="$(git rev-parse openai/main)"
echo "openai/codex main: $openai_sha"
```

如果 `previous_sha` 不是 `<none>`，后续审计和最终总结都必须明确给出：
- 上次基线：`$previous_sha`
- 本次目标：`$openai_sha`
- 本次实际增量：`$previous_sha..$openai_sha`

### 3) 先做改动范围审计（冲突前/后都做一次）

```bash
git diff --name-status "origin/$base_branch"...openai/main
git diff --stat "origin/$base_branch"...openai/main
```

如果你怀疑某些用户可见能力被“同步时舍弃”，在这里就能直接定位文件范围（例如 `/clear`、`/theme`、agent teams 等）。
如果 `previous_sha` 有值，再额外审计一次“上次同步基线到本次上游”的真实增量，避免把仓库自身偏移误判为上游新增：

```bash
if [ "${previous_sha:-<none>}" != "<none>" ]; then
  git diff --name-status "$previous_sha..$openai_sha"
  git diff --stat "$previous_sha..$openai_sha"
fi
```

### 4) 合并上游并处理冲突

```bash
git merge --no-edit openai/main
```

如果出现冲突：
1) 先列出冲突文件：`git status`
2) 对每个冲突做快速 code-review 分类（机械 / 逻辑可融合 / 同功能二选一）
3) 机械冲突、逻辑可融合：直接解决并继续
4) 只有“同功能二选一”才停下来请求你选择

### 5) 仅在“同功能二选一”时阻塞并请求选择
当且仅当你确认两边是**同一个功能**的两套实现，且无法合理融合：
- 汇总并发给你：
  - 文件路径 + 关键函数/结构体
  - 行为差异（接口、边界条件、失败模式）
  - trade-off（复杂度、可维护性、性能、安全性、测试覆盖）
  - 选项：**保留本地** / **采用上游**
- 在你选择前，停止继续推进。

此外，遇到下面这个特殊场景也必须阻塞并请求选择：
- **上游原生功能被 upstream 明确删除/回滚，而本地当前还保留该实现**
  - 必须说明：
    - 该功能最初来自 upstream 还是本地分叉
    - upstream 删除/回滚它的提交和原因（如果能从 commit message 看出）
    - 当前本地保留会带来的持续分叉成本
  - 选项必须写成：**跟随上游删除** / **继续作为本地分叉保留**

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
- `执行闭环（默认）`：
  1) **先定位**：先只看本次上游同步实际改到的用户可见文件，不做全仓漫游。
  2) **先统一术语**：先写出本轮高频术语及目标译法，再开始改代码。
  3) **先改源码**：优先修改真实用户可见源码，不先改 snapshot。
  4) **再补镜像实现**：若 `tui` / `tui_app_server` 有平行实现，必须同步处理。
  5) **最后收测试**：统一 snapshot、断言、帮助输出、文档说明，再跑验证。
- `必做检查清单`：
  1) 文案是否“全中文优先”（含状态标签），避免半汉化（如 `[default]` 与中文混用）。
  2) 占位符与结构是否保持不变（如 `{thread_id}`、`{agent_role}`、Markdown/ANSI/快捷键）。
  3) 术语是否全局一致（同一词在全仓保持同一译法）。
  4) 是否仅改文案，不改行为逻辑。
- `翻译策略（默认）`：
  1) **先看平行实现**：若 `codex-rs/tui` 与 `codex-rs/tui_app_server` 存在平行实现，先对照已汉化的一侧，优先复用现有译法，避免同一功能两套文案风格不一致。
  2) **自然中文优先**：优先表达自然、完整、符合中文习惯的句子，不做逐词硬译；必要时可重组语序，但不得改变信息结构。
  3) **固定保留英文**：命令名/子命令、配置键、协议字段、crate 名、代码标识符、产品名、快捷键（如 `Enter`/`Esc`/`Ctrl+C`）默认保留英文。
  4) **先定术语表再批量改**：先从现有仓内文案抽取本轮高频术语，确定统一译法后再修改源码/快照/断言；不要边改边发明新译法，避免同一词在不同弹窗中漂移。
  5) **慎翻功能术语**：`Fast mode`、`Plan mode`、`Guardian Approvals`、`Windows sandbox` 这类仓内高频术语，优先遵循现有仓内叫法；若仓内未统一，先在本次改动范围内统一，不要一半中文一半英文。
  6) **先改源码，再补快照与断言**：优先修正真正的用户可见源码，再同步 snapshot、测试断言、帮助文本测试；避免只改 snapshot 掩盖源码仍是英文。
  7) **状态标签也算文案**：`(current)`、`[default]`、`Running` 这类状态标签与列表项标题同等重要，必须一起统一，避免正文中文、状态仍英文。
  8) **测试文案也要同步**：若源码文案变化会影响 snapshot、断言、帮助文本测试，必须同步更新测试期望，避免“代码已汉化、测试仍断言英文”。
  9) **避免放宽断言规避问题**：不要把测试改成“中英文任一即可”来绕过本地化；应更新为精确中文预期，或做稳定的规范化后再精确断言。
- `双层自检（默认执行）`：
  1) 固定关键词扫描：
     ```bash
     rg -n "Main \\[default\\]|\\[default\\]|\\bAgent spawn failed\\b|\\bAgent interaction failed\\b|\\bAgent resume failed\\b|\\bAgent close failed\\b|\\bAgent turn complete\\b|\\bSpawned\\b|\\bWaiting for\\b|\\bFinished waiting\\b|\\bResuming\\b|\\bResumed\\b|\\bClosed\\b|\\bNo agents completed yet\\b|\\bPending init\\b|\\bRunning\\b|\\bInterrupted\\b|\\bCompleted\\b|\\bNot found\\b" codex-rs/tui/src codex-rs/tui_app_server/src
     ```
  2) 增量差异扫描：
     ```bash
     git diff --unified=0 -- codex-rs/tui codex-rs/tui_app_server | rg -n "^\\+.*[A-Za-z]{4,}"
     ```
- `保留英文（不要硬翻）`：命令名/子命令、配置键、协议字段、crate 名、代码标识符、产品名。
- 目标：自然中文，不做逐词直译；汉化后重新检查相关快照/测试，确保输出稳定。

### 8) 汇总“合进来了什么”和“影响了什么”
在 worktree 内完成验证后，必须先写出一份结构化总结，至少覆盖：
1) **主要合并内容**：本次从上游带来了哪些功能/模块/基础设施变更。
2) **关键冲突如何处理**：哪些地方是融合、哪些地方是保留本地、哪些地方是跟随上游重构。
3) **影响范围**：协议、核心逻辑、TUI、测试、CI、依赖、Bazel 等哪些受影响。
4) **是否有功能丢失/覆盖**：
   - 明确写“未发现明显丢失/覆盖”，或
   - 明确列出存在风险的点。
5) **依据**：你实际跑过哪些验证、哪些没跑。
6) **同步基线变化**：
   - 上次基线 SHA
   - 本次上游 SHA
   - 是否已把新 SHA 回写到 `STATE.md`

默认输出结构：
```text
- 同步基线变化
- 主要合并内容
- 冲突与融合决策
- 影响范围
- 是否造成原逻辑丢失/覆盖
- 验证依据与剩余风险
```

### 9) 回写 `STATE.md`，再直接合并回原分支（不走 PR）
在 worktree 内验证完成后、回到主工作区前，先更新技能目录下的 `STATE.md`：

```bash
sync_time_utc="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
cat > "$state_file" <<EOF
# sync-openai-codex-pr state

- upstream_repo: https://github.com/openai/codex.git
- upstream_branch: main
- last_synced_sha: $openai_sha
- last_synced_at_utc: $sync_time_utc
- last_synced_base_branch: $base_branch
- last_sync_commit: <fill-after-commit>
- notes: 最近一次已完成同步的上游基线。若本次最终未提交，必须回滚此文件或改回真实已落地状态。
EOF
```

注意：
- `STATE.md` 记录的是**最近一次已落地同步的真实基线**，不是“准备同步到哪”。
- 如果后续合并回原分支失败、放弃提交、或你中止流程，必须把 `STATE.md` 恢复到旧值，不能留下虚假的已同步状态。

然后回到主工作区，将同步分支直接合回原分支：

```bash
cd <repo-root>
git checkout "$base_branch"
git merge --no-ff --no-commit "$branch"
```

然后：
- 确认工作区中只包含本次同步相关改动。
- 把**技能文档更新**、`STATE.md` 更新和**同步代码**一起纳入同一次提交。
- 提交信息必须写成**完整说明型**，而不是只写短标题；正文应至少包含：
  - 上次基线 SHA
  - 上游 commit
  - 合并的主要功能
  - 关键融合点
  - 是否发现功能丢失/覆盖
  - 实际验证情况

建议格式：

```bash
git commit -m "sync: merge openai/codex main into $base_branch" \
  -m "Previous upstream baseline: <previous_sha>" \
  -m "Upstream: <sha>" \
  -m "Merged features: <功能 1>; <功能 2>; <功能 3>." \
  -m "Conflict resolution: <关键融合点>." \
  -m "Impact: <影响范围>." \
  -m "Compatibility: <是否发现原功能丢失/覆盖>." \
  -m "Verified with: <命令列表>."
```

提交后立刻把 `STATE.md` 里的 `last_sync_commit` 从 `<fill-after-commit>` 改成真实提交 SHA，并执行一次补充提交（amend）或在首次提交前先拿到 commit SHA 后回填；总之不要把占位符留在仓库里。

如无后续用途，最后清理 worktree：

```bash
git worktree remove "$path"
```

## 阻塞模板（仅二选一场景）
```
同功能双实现冲突，需要选择：
- 位置：<file>:<symbol>
- 行为差异：<差异点 1/2/3>
- 影响：复杂度 / 可维护性 / 性能 / 安全 / 测试覆盖
- 选项：保留本地 / 采用上游
```
