# OpenAI Codex Upstream Sync Checklist

按这份清单执行 `sync-openai-codex-pr`。命令中的 `/workspace` 是仓库根目录。

## 0. 先刷新本地分叉特性基线

```bash
node /workspace/.codex/skills/sync-openai-codex-pr/scripts/local_fork_feature_audit.mjs refresh --repo /workspace
```

要求：

- 若失败，先修当前分支基线或更新 `references/local-fork-features.md` 的特性定义
- 不要在特性清单已经失真的情况下继续同步 upstream

## 1. 创建独立 worktree

```bash
ts="$(date +%Y%m%d-%H%M%S)"
base_branch="$(git -C /workspace branch --show-current)"
branch="sync/openai-codex-$ts"
path="/workspace/.worktrees/sync-openai-codex-$ts"
git -C /workspace fetch origin "$base_branch"
git -C /workspace worktree add -b "$branch" "$path" "origin/$base_branch"
```

## 2. 读取状态并拉取 upstream

```bash
state_file="/workspace/.codex/skills/sync-openai-codex-pr/STATE.md"
if [ ! -f "$state_file" ]; then
  cat > "$state_file" <<'EOF'
# sync-openai-codex-pr state

- upstream_repo: https://github.com/openai/codex.git
- upstream_branch: main
- last_synced_sha: <none>
- last_synced_at_utc: <none>
- last_synced_base_branch: <none>
- last_sync_commit: <none>
- notes: initialized, no completed sync yet.
EOF
fi
previous_sha="$(sed -n 's/^- last_synced_sha: //p' "$state_file")"
previous_sha="${previous_sha:-<none>}"
openai_url="$(git -C "$path" remote get-url openai 2>/dev/null || true)"
if [ -z "$openai_url" ]; then
  git -C "$path" remote add openai https://github.com/openai/codex.git
elif [ "$openai_url" != "https://github.com/openai/codex.git" ]; then
  git -C "$path" remote set-url openai https://github.com/openai/codex.git
fi
git -C "$path" fetch openai main
openai_sha="$(git -C "$path" rev-parse openai/main)"
```

## 3. 做改动范围审计

```bash
git -C "$path" diff --name-status "origin/$base_branch"...openai/main
git -C "$path" diff --stat "origin/$base_branch"...openai/main
```

如果 `previous_sha` 不是 `<none>`，再看一次真实 upstream 增量：

```bash
git -C "$path" diff --name-status "$previous_sha..$openai_sha"
git -C "$path" diff --stat "$previous_sha..$openai_sha"
```

## 4. 合并 upstream

```bash
git -C "$path" merge --no-edit openai/main
```

冲突处理顺序：

1. 机械冲突：直接解
2. 逻辑可融合：融合并保住本地行为
3. 同功能双实现：阻塞并请用户选
4. 上游原生功能被删：阻塞并请用户决定是否跟随删除

## 5. 定向验证

Rust 改动后：

```bash
cd /workspace/codex-rs
just fmt
```

然后跑最窄相关测试。共享区域改动按仓库规则决定是否扩大。

如果改了依赖：

```bash
cd /workspace
just bazel-lock-update
just bazel-lock-check
```

## 6. Worktree 审查门

```bash
worktree_skill_dir="$path/.codex/skills/sync-openai-codex-pr"
node "$worktree_skill_dir/scripts/local_fork_feature_audit.mjs" check --repo "$path"
```

如果失败：

1. 记录缺失项和命中的失败检查
2. 找出原因：丢失 / rename / move / upstream 更好实现
3. 若不是更好实现，直接修复
4. 若是更好实现，更新 `references/local-fork-features.md` 中对应特性定义
5. 重新执行：

```bash
node "$worktree_skill_dir/scripts/local_fork_feature_audit.mjs" refresh --repo "$path"
node "$worktree_skill_dir/scripts/local_fork_feature_audit.mjs" check --repo "$path"
```

## 7. 合并回当前分支，但先不要提交

```bash
git -C /workspace checkout "$base_branch"
git -C /workspace merge --no-ff --no-commit "$branch"
```

再次做 merge-back gate：

```bash
node /workspace/.codex/skills/sync-openai-codex-pr/scripts/local_fork_feature_audit.mjs check --repo /workspace
```

要求：

- 只要这里还有缺失项，就不能提交
- 必须先修当前分支工作区里的实际结果，再继续

## 8. 刷新最终落地特性清单

```bash
node /workspace/.codex/skills/sync-openai-codex-pr/scripts/local_fork_feature_audit.mjs refresh --repo /workspace
```

这一步会把最新扫描结果写回 `references/local-fork-features.md`。
如果审查和刷新都通过，再把 worktree 中对 `local-fork-features.md` 的最终定义同步回当前分支对应文件。

## 9. 更新 `STATE.md`

在同步真正落地后再改。至少要回写：

- `last_synced_sha`
- `last_synced_at_utc`
- `last_synced_base_branch`
- `last_sync_commit`

如果 target SHA 无法准确核定，不要伪造，改写 `notes` 说明原因。

## 10. 提交要求

把这些一起提交：

- 同步代码
- `STATE.md`
- `references/local-fork-features.md`
- 技能目录里的其他辅助文件更新

提交正文至少写清：

- `Previous upstream baseline`
- `Upstream target sha`
- `Actual upstream range`
- 主要合并内容
- 关键融合点
- 本地分叉特性审查结果
- 验证命令

## 审查失败时的判断标准

只有在同时满足下面两点时，才允许把缺失项判定为“更好的等效替换”：

1. 功能行为没有回退，且目标实现已经完整覆盖旧特性意图
2. `references/local-fork-features.md` 里的检查方式、说明和 `better_when` 已更新到新实现

否则，一律按回归处理并修正。
