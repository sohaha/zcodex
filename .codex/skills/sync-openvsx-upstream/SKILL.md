---
name: sync-openvsx-upstream
description: 在独立 worktree 中把 `eclipse-openvsx/openvsx` 的 `main` 同步到本仓库的 `third_party/openvsx` vendored 目录；先审计上游增量与本地补丁，再以本地改动优先完成融合、验证并更新同步基线。用户提到同步 openvsx 上游、刷新 vendored openvsx、更新 third_party/openvsx 时使用。
---

# sync-openvsx-upstream

用这个 skill 把上游 `eclipse-openvsx/openvsx` 的最新代码安全同步到本仓库的 `third_party/openvsx`。

规范上游：
- `https://github.com/eclipse-openvsx/openvsx.git`
- 默认分支：`main`

## 目标

- 在独立 `git worktree` 中完成同步，避免污染当前工作区。
- 把上游快照同步到 `/workspace/third_party/openvsx`。
- 默认保留本仓库已经存在的本地补丁；不要因为 vendoring 而把本地修补静默覆盖掉。
- 同步完成后更新 `/workspace/.codex/skills/sync-openvsx-upstream/STATE.md`，保证下次有真实基线可追踪。
- 最终输出必须说明：
  - 上次基线
  - 本次上游 SHA
  - 上游带来了什么
  - 本地保留了什么
  - 是否有潜在覆盖或丢失
  - 实际做了哪些验证

## 必读文件

开始前先读：
- `/workspace/.codex/skills/sync-openvsx-upstream/STATE.md`
- `/workspace/.codex/skills/sync-openvsx-upstream/references/checklist.md`

如果 `STATE.md` 不存在，先创建占位基线；不要跳过。

## 本地同步面

主要关注：
- `/workspace/third_party/openvsx`

如同步方式或说明发生变化，再补充更新：
- `/workspace/.codex/skills/sync-openvsx-upstream/SKILL.md`
- `/workspace/.codex/skills/sync-openvsx-upstream/STATE.md`
- `/workspace/.codex/skills/sync-openvsx-upstream/references/checklist.md`

## 工作流

### 1) 从当前分支创建 worktree

在仓库根目录执行：

```bash
ts="$(date +%Y%m%d-%H%M%S)"
base_branch="$(git branch --show-current)"
branch="sync/openvsx-upstream-$ts"
path=".worktrees/sync-openvsx-upstream-$ts"
git fetch origin "$base_branch"
git worktree add -b "$branch" "$path" "origin/$base_branch"
```

后续都在这个 worktree 里操作：

```bash
cd "/workspace/$path"
```

### 2) 读取同步基线

固定使用 skill 目录下的 `STATE.md` 记录真实已落地基线：

```bash
skill_dir="/workspace/.codex/skills/sync-openvsx-upstream"
state_file="$skill_dir/STATE.md"
cat "$state_file"
previous_sha="$(sed -n 's/^- last_synced_sha: //p' "$state_file")"
echo "previous upstream sha: ${previous_sha:-<none>}"
```

### 3) 准备上游缓存并导出待同步快照

不要把 `third_party/openvsx` 重新变成 Git 仓库。上游仓库放到缓存目录，真实同步靠导出的文件快照：

```bash
cache_dir="/workspace/.cache/openvsx-upstream"
stage_dir="/workspace/$path/.sync/openvsx-stage"

if [ -d "$cache_dir/.git" ]; then
  git -C "$cache_dir" fetch origin main --tags
else
  git clone https://github.com/eclipse-openvsx/openvsx.git "$cache_dir"
fi

openvsx_sha="$(git -C "$cache_dir" rev-parse origin/main)"
rm -rf "$stage_dir"
mkdir -p "$stage_dir"
git -C "$cache_dir" archive "$openvsx_sha" | tar -x -C "$stage_dir"
echo "eclipse-openvsx/openvsx main: $openvsx_sha"
```

如果 `previous_sha` 有值，后续总结必须明确写出：
- 上次基线：`$previous_sha`
- 本次目标：`$openvsx_sha`
- 实际上游增量：`$previous_sha..$openvsx_sha`

### 4) 先做两层审计

先看纯上游增量：

```bash
if [ "${previous_sha:-<none>}" != "<none>" ]; then
  git -C "$cache_dir" diff --name-status "$previous_sha..$openvsx_sha"
  git -C "$cache_dir" diff --stat "$previous_sha..$openvsx_sha"
else
  git -C "$cache_dir" show --stat --summary "$openvsx_sha"
fi
```

再看当前 vendored 目录相对新上游的本地偏移：

```bash
git diff --no-index --stat -- third_party/openvsx "$stage_dir" || true
git diff --no-index --name-status -- third_party/openvsx "$stage_dir" || true
```

这里的目标不是立刻覆盖，而是先识别：
- 上游新增/删除了什么
- 当前 vendored 目录是否有本地补丁
- 哪些文件会在同步时冲突

### 5) 决策规则

冲突按这三类处理：
1. 机械差异：直接跟上游，保持最小 diff。
2. 可重放的本地补丁：先同步上游，再把本地补丁按最小范围重放回去。
3. 同一能力两套实现、无法合理融合：停止并请求用户选择“保留本地”或“采用上游”。

默认规则：
- 本地补丁优先。
- 不要静默删除本地补丁。
- 只有确认本地补丁已过时或应被上游替代时，才移除它。

### 6) 执行同步

确认策略后，用上游快照覆盖 vendored 目录，再补回需要保留的本地改动：

```bash
rsync -a --delete --exclude '.git' "$stage_dir/" third_party/openvsx/
rm -rf third_party/openvsx/.git
```

如果需要保留本地补丁：
- 先从 `git diff --no-index` 的审计结果定位文件
- 再在当前 worktree 中做最小化手工修复
- 不要把整个目录回滚成“半旧半新”的混合状态

### 7) 验证

至少执行这些最小检查：

```bash
test ! -e third_party/openvsx/.git
git status --short -- third_party/openvsx
```

推荐补充：
- spot check 关键文件是否来自新上游版本
- 确认上游删除的文件确实被删除
- 确认没有意外引入缓存、构建产物或 Git 元数据

这是 vendored 源码同步 skill，不自带“翻译/汉化”步骤。除非用户明确要求，否则保留上游原始文案，不做额外本地化处理。

### 8) 更新同步基线

同步落地并验证后，回写 `STATE.md`：

```bash
sync_time_utc="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
cat > "$state_file" <<EOF
# sync-openvsx-upstream state

- upstream_repo: https://github.com/eclipse-openvsx/openvsx.git
- upstream_branch: main
- last_synced_sha: $openvsx_sha
- last_synced_at_utc: $sync_time_utc
- last_synced_base_branch: $base_branch
- last_sync_commit: <fill-after-commit>
- notes: 最近一次已落地的 openvsx vendored 同步基线；如仅选择性同步，必须在此写明保留的本地补丁或未跟进项。
EOF
```

注意：
- `STATE.md` 记录的是已经真正落地到当前仓库的基线。
- 如果本次最终没有提交成功，必须把 `STATE.md` 恢复成旧值。

### 9) 合回原分支并提交

回到主工作区后：

```bash
cd /workspace
git checkout "$base_branch"
git merge --no-ff --no-commit "$branch"
```

然后：
- 只提交本次 openvsx 同步相关文件
- 把 `STATE.md` 和 skill 文档变更一起提交
- 提交正文至少包含：
  - previous baseline
  - new upstream sha
  - merged upstream changes
  - preserved local patches
  - validation performed

提交后，把 `STATE.md` 里的 `last_sync_commit` 回填成真实提交 SHA；不要把占位符留在仓库里。

如无后续用途，清理 worktree：

```bash
git worktree remove "$path"
```

## Guardrails

- 不要把 `third_party/openvsx` 恢复成 Git 仓库。
- 不要在未审计的情况下直接覆盖 vendored 目录。
- 不要把“本地补丁被上游覆盖”当成正常结果而不报告。
- 不要引入额外的翻译、本地化或文案改写流程。
- 不要在 skill 里承诺无法验证的构建成功；如未运行验证，必须明确写出。

## 最终输出约定

最终至少汇报：
- previous recorded baseline
- new upstream sha
- upstream delta summary
- preserved local patches
- files or directories with notable conflicts
- validation commands and results
- remaining risk
