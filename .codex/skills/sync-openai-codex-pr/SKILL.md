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
- 在开始真正合并前，先确认这次准备产出的“同步提交”能否被未来审计：
  - 最好是标准 `merge`，让 upstream commit 直接成为父提交之一。
  - 若最终只能用 squash、cherry-pick 或人工整理式提交，必须提前准备把 upstream 基线和目标 SHA 明写进提交正文与总结，不能只写 `sync: merge openai/codex main` 这类无法自证的标题。

## 决策规则（先 code-review 再动手）
- 先做**改动范围审计**：哪些目录/模块变了、风险点在哪（比如多 agent/agent teams、hooks/cleanup、TUI 命令 `/clear` `/theme` 等）。
- 先区分两类能力来源：
  1) **本地分叉独有能力**：例如本地新增命令、协议事件、中文化交互、兼容层、额外恢复/路由逻辑。**默认不能被上游直接覆盖**；若上游改到同一区域，应先尝试融合并保住本地能力。
  2) **上游原生能力**：即功能最初来自 upstream，后来 upstream 又删除、回滚或重构。**不要因为“当前本地还有这段代码”就默认保留**；如果同步会让该能力消失，必须明确提醒用户这是“上游原生功能删除/回滚”，并请求是否跟随上游。
- 冲突分类（按优先级）：
  1) **机械冲突**（format/import/rename/move/并行改同一段但语义一致）：直接合并，保持最小 diff。
  2) **逻辑可融合**（两边改动互补，功能不重复）：融合到一个实现，默认保持本地行为不回退。
  3) **同功能双实现（必须二选一）**：不要私自拍板；必须汇总差异并请求你选择，然后**阻塞流程**等待决定。
  4) **行为变更检测（汉化恢复时必做）**：在恢复本地独有能力（尤其是中文化）时，对每个被恢复的区域，必须对比上游当前实现和本地恢复后的实现是否在**功能行为**上一致。
     - 典型反例：上游已把进度条改为百分比显示，本地恢复时只做了文案翻译但沿用了旧的行为实现（如进度条函数），导致功能行为与上游不一致。正确做法：先采纳上游行为实现，再做文案翻译。
     - 检查点：格式/展示方式、条件分支、默认值/阈值、函数签名、调用方行为。
     - 执行方式：对每个被恢复的文件区域，用 `git diff` 对比上游版本和本地恢复版本的**非文案差异**（排除纯字符串/注释变更），确认逻辑结构一致。
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
  5) **行为对齐检查**：翻译前先确认上游是否改了该区域的功能行为（格式、分支、阈值、签名等）；若有变更，先采纳上游行为再做文案翻译，不要沿用本地旧实现。
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
7) **可审计性说明**：
   - 本次同步是否是标准 merge-parent 型提交
   - 如果不是，是否已在提交正文中显式写出 `Previous upstream baseline`、`Upstream target sha` 和 `Actual upstream range`

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
- 如果这次同步已经落地代码，但你**无法准确确认**本次 upstream target SHA：
  - 不要伪造 `last_synced_sha`
  - 保持 `last_synced_sha` 指向最近一次可准确审计的基线
  - 在 `notes` 里明确写出“已有更晚同步提交落地，但因提交结构/记录不足，当前仍需重新核定 target SHA”
- 只要 `STATE.md` 中的 `last_synced_sha`、`last_sync_commit` 仍是旧值，就必须在 `notes` 中同步写明原因，避免后续把旧基线误当成“仓库当前已同步到的最新状态”

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
  - 若不是标准 merge-parent，同步范围是如何核定的
  - `STATE.md` 是否已更新到最新准确基线；若未更新，原因是什么
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

### 7.1) 合并前审查循环（新增 2026-04-17）

在合并回原分支之前，必须执行审查循环以确保本地特色功能未被覆盖：

1. **触发审查**：
   ```bash
   codex -P cch -m gpt-5.4 exec "审查最近的 upstream 同步合并，检查本地特色功能（汉化、fallback provider 等）是否完整保留"
   ```

2. **审查重点**：
   - 检查本地特色功能是否保留（汉化、本地化、fallback provider 等）
   - 确认没有本地代码被上游覆盖导致功能丢失
   - 验证冲突解决方案是否正确
   - 确认编译通过

3. **审查不通过的处理**：
   - 如果发现功能丢失或覆盖，重新处理冲突
   - 再次触发审查直到通过
   - 记录审查结果和修复过程

4. **模型回退策略**：
   - 如果 `-m gpt-5.4` 连续失败多次，切换到 `-m glm-5.1` 继续审查

5. **审查通过后才能合并**：
   - 只有审查通过后才能执行合并回原分支
   - 在技能文档中记录审查结果

**已验证通过的审查案例**：
- 2026-04-17: 上游 dd00efe78 合并，fallback provider 功能成功保留，编译通过


- 2026-04-17: 上游同步 API 适配（22→0 编译错误），codex-core 编译通过

### 7.1.1) 深度审查发现的功能退化修复（2026-04-17）

多 agent 并行深度验证发现并修复了两个功能退化：

| 退化项 | 根因 | 修复 |
|-------|------|------|
| `max_output_tokens` 被硬编码为 `None` | 上游移除了 `client.rs` 中从 provider 读取的逻辑，sync 时直接替换为 `None` | 恢复为 `self.client.state.provider.info().max_output_tokens.filter(\|v\| *v > 0)` |
| `auto_tldr_routing` 调用丢失 | 上游从 `Config` 移除该字段，sync 时误删了 `.with_auto_tldr_routing()` 调用 | 改用 `AutoTldrRoutingMode::default()` 保持 tldr 路由功能启用 |

**验证通过的完整检查清单**：
- ✅ Fallback provider：6 核心函数、4 config 字段、8 测试完整
- ✅ 汉化：cli 256行、tui 2100+行、core 360行中文，HEAD~5..HEAD diff 无中文删除
- ✅ 本地模块：ztok(2126行)、zmemory(12570行)、buddy(1920行)、compact_remote(344行)、agent/role(434行) 均未被修改
- ✅ 公共 API：lib.rs 40+ pub use、codex.rs 28 pub 函数、config/mod.rs 7 pub struct 全部保留
- ✅ max_output_tokens：已恢复从 provider 配置读取
- ✅ auto_tldr_routing：已恢复默认值调用
- ✅ 全量编译：`cargo check` 0 error 通过

### 7.2) 上游 API 适配经验库（新增 2026-04-17）

上游同步后常见的 API 变更模式及修复方法：

| 变更模式 | 典型案例 | 修复方法 |
|---------|---------|---------|
| 模块移除/重构 | `project_doc` → `agents_md` | 替换导入和调用，使用 `AgentsMdManager` |
| 函数重命名 | `merge_plugin_apps_with_accessible` → `merge_plugin_connectors_with_accessible` | 更新函数名 + 添加 `pub use` 导出 |
| 类型变化 | `AppConnectorId` → `String` | `.into_iter().map(\|id\| id.0).collect::<Vec<_>>()` |
| 结构体字段移除 | `file_system_sandbox_policy` 从 `FileSystemSandboxContext` 移除 | 删除字段赋值行 |
| 结构体字段类型变化 | `file_system_sandbox_policy: T` → `Option<T>` | 包装为 `Some(...)` |
| 方法签名变更 | `resolve_mcp_tool_info(&ToolName)` → `(&str, Option<&str>)` | 修改参数传递方式 |
| 新增枚举变体 | `PatchApplyUpdated`, `ToolCallInputDelta` | 在 match 中添加 `_ => {}` 或显式分支 |
| 函数参数增加 | `load_config_layers_state` 从 5→6 参数 | 添加缺失参数（通常是 `fs`） |
| 表达式不完整 | `max_output_tokens` 表达式被截断 | 补全表达式或用 `None` 替代 |
| 私有字段访问 | `ModelClientSession.client` 私有 | 添加 `pub(crate)` 代理方法 |
| 方法移除 | `token_usage_info()` 从 `Session` 移除 | 返回 `None` 或注释掉 |

**修复优先级**：
1. 先修复 `codex-core`（`cargo check -p codex-core`）
2. 再修复依赖 `codex-core` 的其他 crate
3. 最后修复测试和 CLI 相关

**验证命令**：
```bash
# 核心编译检查
RUSTC_WRAPPER="" cargo check -p codex-core
# 完整编译检查（可能暴露依赖问题）
RUSTC_WRAPPER="" cargo check
```

**注意事项**：
- 修复时优先使用 `apply_patch` 工具，避免 `sed` 导致的多行替换问题
- 每修复一个类别后立即验证编译，不要积累错误
- 保留所有本地特色功能（汉化、fallback provider、buddy 反应库等）
- ToolName 结构体字段：`.name`（String）和 `.namespace`（Option<String>），不是元组
