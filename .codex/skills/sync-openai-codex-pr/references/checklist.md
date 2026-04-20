# OpenAI Codex Upstream Sync Checklist

按这份清单执行 `sync-openai-codex-pr`。命令中的 `/workspace` 是仓库根目录。

## 0. 初始化 `STATE.md`

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
last_sync_commit="$(sed -n 's/^- last_sync_commit: //p' "$state_file")"
last_sync_commit="${last_sync_commit:-<none>}"
```

## 1. 先做本地提交发现

确定 discover 范围：

- 默认只允许用 `STATE.md:last_sync_commit`
- 只有当它仍是 `HEAD` 祖先时，脚本才会自动推断
- 不要再隐式回退到 `last_synced_sha`
- 如果默认基线不可用，必须显式二选一：
  - `--base-ref <trusted-local-commit>`：本地提交范围
  - `--merge-base-ref openai/main`：广域审计模式，可能带进更多 upstream 噪音

```bash
ts="$(date +%Y%m%d-%H%M%S)"
discover_out="/tmp/sync-openai-codex-pr-discover-$ts.json"
if [ "$last_sync_commit" != "<none>" ] && git -C /workspace merge-base --is-ancestor "$last_sync_commit" HEAD; then
  node /workspace/.codex/skills/sync-openai-codex-pr/scripts/local_fork_feature_audit.mjs discover \
    --repo /workspace \
    --output "$discover_out"
else
  openai_url="$(git -C /workspace remote get-url openai 2>/dev/null || true)"
  if [ -z "$openai_url" ]; then
    git -C /workspace remote add openai https://github.com/openai/codex.git
  elif [ "$openai_url" != "https://github.com/openai/codex.git" ]; then
    git -C /workspace remote set-url openai https://github.com/openai/codex.git
  fi
  git -C /workspace fetch openai main
  echo "discover 默认基线不可用：STATE.md:last_sync_commit 缺失或已不是 HEAD 祖先" >&2
  echo "显式选择其一后再继续：" >&2
  echo "  node /workspace/.codex/skills/sync-openai-codex-pr/scripts/local_fork_feature_audit.mjs discover --repo /workspace --base-ref <trusted-local-commit> --head-ref HEAD --output \"$discover_out\"" >&2
  echo "  node /workspace/.codex/skills/sync-openai-codex-pr/scripts/local_fork_feature_audit.mjs discover --repo /workspace --merge-base-ref openai/main --head-ref HEAD --output \"$discover_out\"" >&2
  exit 1
fi
```

## 2. 并发子代理分析 discover 结果

默认分成 3 组：

- `core/config/protocol`
- `tui/localization/branding`
- `workspace/local-crates`

要求：

- 子代理只读分析 `discover_out` 和相关提交
- 子代理只输出 candidate ops，不直接改 `local-fork-features.json`

推荐输出结构：

```json
{
  "operations": [
    { "action": "upsert", "feature": { "...": "full feature object" } },
    { "action": "remove", "id": "obsolete-feature-id", "reason": "why it is obsolete" }
  ]
}
```

主代理把多个子代理结果汇总成一个候选文件，例如：

```bash
candidate_dir="/tmp/sync-openai-codex-pr-candidates-$ts"
mkdir -p "$candidate_dir"
# 子代理各自写入：
#   "$candidate_dir/core-config-protocol.json"
#   "$candidate_dir/tui-localization-branding.json"
#   "$candidate_dir/workspace-local-crates.json"
candidate_ops="/tmp/sync-openai-codex-pr-candidate-ops-$ts.json"
node /workspace/.codex/skills/sync-openai-codex-pr/scripts/local_fork_feature_audit.mjs merge-candidates \
  --dir "$candidate_dir" \
  --output "$candidate_ops"
```

## 3. 审阅后晋升到权威基线

```bash
if [ -f "$candidate_ops" ]; then
  node /workspace/.codex/skills/sync-openai-codex-pr/scripts/local_fork_feature_audit.mjs promote \
    --candidate "$candidate_ops"
fi
node /workspace/.codex/skills/sync-openai-codex-pr/scripts/local_fork_feature_audit.mjs render --repo /workspace
node /workspace/.codex/skills/sync-openai-codex-pr/scripts/local_fork_feature_audit.mjs check --repo /workspace
```

要求：

- 如果这里 `check` 不通过，先修权威基线或候选定义，不要继续同步 upstream

## 4. 创建独立 worktree

```bash
base_branch="$(git -C /workspace branch --show-current)"
branch="sync/openai-codex-$ts"
path="/workspace/.worktrees/sync-openai-codex-$ts"
git -C /workspace fetch origin "$base_branch"
git -C /workspace worktree add -b "$branch" "$path" "origin/$base_branch"
```

## 5. worktree 内拉取 upstream 并做范围审计

```bash
openai_url="$(git -C "$path" remote get-url openai 2>/dev/null || true)"
if [ -z "$openai_url" ]; then
  git -C "$path" remote add openai https://github.com/openai/codex.git
elif [ "$openai_url" != "https://github.com/openai/codex.git" ]; then
  git -C "$path" remote set-url openai https://github.com/openai/codex.git
fi
git -C "$path" fetch openai main
openai_sha="$(git -C "$path" rev-parse openai/main)"
git -C "$path" diff --name-status "origin/$base_branch"...openai/main
git -C "$path" diff --stat "origin/$base_branch"...openai/main
if [ "$previous_sha" != "<none>" ]; then
  git -C "$path" diff --name-status "$previous_sha..$openai_sha"
  git -C "$path" diff --stat "$previous_sha..$openai_sha"
fi
```

## 6. 合并 upstream

```bash
git -C "$path" merge --no-edit openai/main
```

冲突处理顺序：

1. 机械冲突：直接解
2. 逻辑可融合：融合并保住本地行为
3. 同功能双实现：阻塞并请用户选
4. 上游原生功能被删：阻塞并请用户决定是否跟随删除

## 7. 定向验证

Rust 改动后：

```bash
cd /workspace/codex-rs
just fmt
```

然后跑最窄相关测试。共享区域改动按仓库规则决定是否扩大。

如果本次同步碰到共享 struct 新增字段：

- grep 本地同类型的 synthetic / fallback 构造点
- 同时 grep 直接字段读取点，确认没有绕过新的 helper / resolved 字段语义

如果本次同步碰到 `codex-rs/protocol/src/error.rs`：

- 一并审查 `is_retryable()`、`to_codex_protocol_error()`、`codex-rs/core/src/session/turn.rs`
- 不要只补协议枚举级单测；至少确认 turn 级自动重试和对外错误分类没有互相打架

如果本次同步碰到本地中文化 surface：

- 不要只看视图入口文件；先追到真正的文案源头，例如 `FeatureSpec` 元数据、共享 helper、onboarding/history 组件
- 同时检查直接字符串断言和相关 snapshot，避免“源码已改回英文但测试/快照仍没覆盖”或“只改了视图层，元数据源头仍是英文”
- 如果文案会跨 `core -> app-server -> tui` 透传或被字符串解析，必须把桥接层和解析层一起纳入检查

如果本次同步碰到本地 workspace crate 面：

- 不要只检查目录是否还在
- 额外审查 `codex-rs/Cargo.toml` 里的 members 和 workspace dependency path 接线

如果本次同步碰到 `codex-rs/cli/src/main.rs`、`codex-rs/tui/src/cli.rs`，或任何 interactive CLI 参数桥接：

- 不要只检查参数定义、help 文案或 parse 测试
- 必查 root CLI 到 `resume` / `fork` / 其他复用 `TuiCli` 子命令的 merge/bridge 函数
- 对新增或本地扩展的 interactive 参数，确认至少两层都过：
  - 字段存在且能 parse
  - merge 后真正写入最终 `TuiCli`
- 对 provider / local-provider、sandbox、approval、cwd、search 这类容易在 merge 时静默丢失的参数，必须同时看赋值语句和回归测试
- 若只找到 help/localization 哨兵，没有找到 bridge 赋值或 round-trip 测试，不得把这类 CLI 能力判为已保留

如果本次同步碰到 `codex-rs/core/src/session/mod.rs`、`codex-rs/core/src/session/turn_context.rs`、`codex-rs/app-server/src/codex_message_processor.rs`、`codex-rs/tui/src/app.rs`，或任何 `turn/steer` / warning 文案映射：

- 不要只检查某一个文件里的中文文案还在
- 必查 `core -> app-server -> tui` 的错误/警告桥接是否同步更新
- 对依赖字符串解析的路径，确认 `tui` 的 active-turn race / mismatch prefix 仍能命中当前文案
- 对 `turn_context.rs` 这类直接发出 warning 文案的源头，至少保留一条覆盖具体中文 warning 文案的回归测试
- 至少保留一条覆盖中文 warning 前缀和一条覆盖 steer 错误文案的回归测试
- 若只看到源头文案存在，但下游映射、解析或测试未更新，不得把这类本地中文化行为判为已保留

如果改了依赖：

```bash
cd /workspace
just bazel-lock-update
just bazel-lock-check
```

## 8. Worktree 审查门

```bash
worktree_skill_dir="$path/.codex/skills/sync-openai-codex-pr"
node "$worktree_skill_dir/scripts/local_fork_feature_audit.mjs" check --repo "$path"
```

如果失败：

1. 记录缺失项和命中的失败检查
2. 找出原因：丢失 / rename / move / upstream 更好实现
3. 若不是更好实现，直接修复
4. 若是更好实现，先更新 worktree 里的 `local-fork-features.json`
5. 重新执行：

```bash
node "$worktree_skill_dir/scripts/local_fork_feature_audit.mjs" render --repo "$path"
node "$worktree_skill_dir/scripts/local_fork_feature_audit.mjs" check --repo "$path"
```

## 9. 合并回当前分支，但先不要提交

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
- 如果 worktree 里已经更新了 `local-fork-features.json`，要先把对应变更带回当前分支，再重新 `render` 和 `check`

## 10. 渲染最终报告并更新 `STATE.md`

```bash
node /workspace/.codex/skills/sync-openai-codex-pr/scripts/local_fork_feature_audit.mjs render --repo /workspace
```

然后更新 `STATE.md`：

- `last_synced_sha`
- `last_synced_at_utc`
- `last_synced_base_branch`
- `last_sync_commit`
- `last_sync_commit` 必须是当前分支真正落地的 sync 提交
- 如果 upstream SHA 没变，空同步轮次继续保留上一次真实落地的 sync 提交
- 不要把 `last_sync_commit` 改写成空同步状态提交、后续本地修复提交、补记 `STATE.md` 的提交，或临时 worktree / sync 分支上未落地的 SHA

如果 target SHA 无法准确核定，不要伪造，改写 `notes` 说明原因。

## 11. 提交要求

把这些一起提交：

- 同步代码
- `STATE.md`
- `references/local-fork-features.json`
- `references/local-fork-features.md`
- 技能目录里的其他辅助文件更新

提交正文至少写清：

- `Previous upstream baseline`
- `Upstream target sha`
- `Actual upstream range`
- 这轮 `discover` / `promote` 处理了什么
- 本地分叉特性审查结果
- 主要合并内容
- 关键融合点
- 验证命令

## 缺失项允许判定为“更好的等效替换”的前提

只有在同时满足下面两点时，才允许把缺失项按“更好的等效替换”处理：

1. 功能行为没有回退，目标实现已经完整覆盖旧特性意图
2. `local-fork-features.json` 已先更新为新实现，然后重新 `render` 和 `check`

否则，一律按回归处理并修正。
