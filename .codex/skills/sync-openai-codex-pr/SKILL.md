---
name: sync-openai-codex-pr
description: 在独立 worktree 中把 `openai/codex` 的 `main` 同步到当前分支：先用本地分叉基线与候选发现流程审查我们自己的提交，再做范围审计和本地优先合并；仅在“同功能两套实现必须二选一”或“上游原生功能已被 upstream 删除/回滚”时阻塞询问；合并回当前分支前后都要用权威基线做审查，并更新同步基线与本地特性报告。
---

# Sync OpenAI Codex PR

仅在把 `openai/codex` 的 `main` 同步到当前分叉仓库时使用这个技能。

## 先读这些文件

- `/workspace/.codex/skills/sync-openai-codex-pr/STATE.md`
- `/workspace/.codex/skills/sync-openai-codex-pr/references/checklist.md`
- `/workspace/.codex/skills/sync-openai-codex-pr/references/local-fork-features.json`
- `/workspace/.codex/skills/sync-openai-codex-pr/references/local-fork-features.md`
- `/workspace/.agents/llmdoc/guides/upstream-sync-preservation-rules.md`

如果 `STATE.md` 不存在，先按 `references/checklist.md` 的模板初始化。

## 这套机制的 4 个层次

1. `STATE.md`
   - 记录最近一次已经落地、且可准确审计的 upstream 基线。
2. `local-fork-features.json`
   - 权威基线。
   - 只保存确认要长期保留的本地分叉特性。
   - 这是 merge-back gate 的事实源。
3. `discover` 产物
   - 来自本地提交历史的候选发现结果。
   - 适合并发子代理分析，但不直接覆盖权威基线。
4. `local-fork-features.md`
   - 由脚本从 `json` 渲染生成的人类可读报告。
   - 不手工编辑。

## 必用脚本

```bash
node /workspace/.codex/skills/sync-openai-codex-pr/scripts/local_fork_feature_audit.mjs discover --repo /workspace --base-ref <sha> --head-ref HEAD --output /tmp/sync-openai-codex-pr-discover.json
node /workspace/.codex/skills/sync-openai-codex-pr/scripts/local_fork_feature_audit.mjs merge-candidates --dir /tmp/sync-openai-codex-pr-candidates --output /tmp/sync-openai-codex-pr-candidate-ops.json
node /workspace/.codex/skills/sync-openai-codex-pr/scripts/local_fork_feature_audit.mjs promote --candidate /tmp/sync-openai-codex-pr-candidate-ops.json
node /workspace/.codex/skills/sync-openai-codex-pr/scripts/local_fork_feature_audit.mjs render --repo /workspace
node /workspace/.codex/skills/sync-openai-codex-pr/scripts/local_fork_feature_audit.mjs check --repo <repo-or-worktree>
```

补充：

- `refresh` 是 `render --repo <repo>` 的兼容别名。
- `discover` 的默认推断顺序是：
  - `--base-ref`
  - `--merge-base-ref`
  - `STATE.md:last_sync_commit`
- `discover` 不再隐式使用 `last_synced_sha`。
- 只有当 `last_sync_commit` 仍是 `HEAD` 的祖先时，才允许把它当作“我们自己的提交范围”默认起点。
- 如果 `last_sync_commit` 已经不是祖先，停止自动推断，改用：
  - `--base-ref <trusted-local-commit>`
  - 或显式广域模式 `--merge-base-ref <ref>`
- `discover`
  - 只收集事实：提交、文件、既有特性命中、未覆盖路径。
  - 不直接发明或修改权威特性。
- `merge-candidates`
  - 把多个子代理写出的 candidate ops 文件合并成一个待审阅文件。
  - 同一 feature id 只要出现互相矛盾的 upsert/remove，就直接失败，不允许静默覆盖。
- `promote`
  - 只应用已经审阅过的 candidate ops。
  - 不做“自动批准”。
- `render`
  - 从 `json` 权威基线生成 `md` 展示文件，并附带当前 repo 的最新审查报告。
- `check`
  - 用权威基线审查当前 repo 或 worktree。
  - 这是 merge-back gate。

## Candidate Ops 规则

子代理不要直接并发改 `local-fork-features.json`。

正确做法：

1. 主代理先跑一次 `discover`
2. 子代理并发阅读 `discover` 产物和相关提交
3. 子代理只输出 candidate ops
4. 主代理先用 `merge-candidates` 汇总多个 candidate 文件，再审阅后串行执行 `promote`
5. 再 `render`
6. 再 `check`

推荐 candidate ops 结构：

```json
{
  "operations": [
    { "action": "upsert", "feature": { "...": "full feature object" } },
    { "action": "remove", "id": "obsolete-feature-id", "reason": "why it is obsolete" }
  ]
}
```

适合并发的子代理分工：

- `core/config/protocol`
- `tui/localization/branding`
- `workspace/local-crates`

每个子代理只负责：

- 识别哪些提交或路径值得进入权威基线
- 判断既有特性是仍有效、rename/move，还是可能被更好的实现替换
- 产出 candidate ops

## 决策规则

### 默认优先级

1. 本地代码/行为不回退
2. 再吸收 upstream 新能力
3. 最后最小化 diff

### 上游删除反查 gate

合并冲突清完后、跑 `check` 前，必须反查 upstream 已删除但当前 worktree 仍保留的路径：

```bash
git -C "$path" diff --name-status "$previous_sha..$openai_sha" --diff-filter=D |
  awk '{print $2}' |
  while read -r deleted_path; do
    if git -C "$path" ls-files --error-unmatch "$deleted_path" >/dev/null 2>&1; then
      echo "$deleted_path"
    fi
  done
```

如果输出非空，逐项分类：

1. 已在 `local-fork-features.json` 中声明为本地长期特性：保留，并确认 checks 覆盖对应入口、资源、测试和文档。
2. 未在权威基线声明，且确认是 upstream 原生功能删除/回滚：默认跟随 upstream 删除，不能因为冲突解决时文件还在就静默保留。
3. 需要转成本地分叉特性继续保留：停止，先让用户明确确认；确认后再新增/更新 `local-fork-features.json`，并补齐 checks。

禁止只用 `local_fork_feature_audit.mjs check` 代替这个删除反查；`check` 只能证明已声明特性没丢，不能发现“上游已删但本地意外复活”的未声明旧功能。

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

只有下面两类情况才允许阻塞问用户：

- 同功能双实现，必须二选一
- 上游原生能力已被 upstream 删除/回滚，需要决定是否继续作为本地分叉保留

## 工作流

详细命令见 `references/checklist.md`。默认按这个顺序执行：

1. 先尝试用 `STATE.md:last_sync_commit` 跑 `discover`
   - 只有它仍是 `HEAD` 祖先时才允许自动成功
   - 若不是祖先，就显式改用 `--base-ref` 或 `--merge-base-ref`
2. 用并发子代理分析 discover 结果，生成 candidate ops
3. 主代理审阅后执行 `promote`
4. 在当前分支先 `render --repo /workspace` 与 `check --repo /workspace`
   - 如果这一步就失败，说明权威基线或候选晋升有问题，先修正再同步 upstream
5. 创建独立 worktree，并记录 `base_branch`
6. 读取 `STATE.md`，拉取 `openai/main`
   - 若 `openai` remote 已存在但 URL 错误，先修正
7. 做两次改动范围审计
8. 在 worktree 里 `git merge --no-edit openai/main`
9. 按冲突分类解决问题
10. 做定向验证
11. 在 worktree 里执行一次 `check --repo "$path"` 审查
12. 合并回当前分支，但先不要提交
13. 在当前分支工作区再执行一次 `check --repo /workspace`
14. 若审查通过，`render --repo /workspace`
15. 更新 `STATE.md`，把同步代码、权威基线、渲染报告和状态文件一起提交
   - `last_sync_commit` 必须写成当前分支真正落地的 sync 提交
   - 如果 upstream SHA 未推进，空同步轮次要保留上一次真实落地的 sync 提交，不能改写成空同步状态提交、后续本地修复提交或临时 worktree SHA

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

如果 upstream 在共享 struct 上新增字段，而本地又有 synthetic / fallback 构造，例如 `ModelInfo`：

- 不要只看编译通过的主路径
- 额外 grep 同类型的本地构造点，补齐新字段后再宣称同步完成

如果本次同步触及跨层共享字段，尤其是 `protocol/app-server/thread-store/state/proto/schema` 之间会重复出现的字段：

- 不要只补协议层 struct、serde 字段或单个调用点
- 额外审查所有镜像层：wire schema、protobuf、checked-in generated 文件、remote/local adapter、synthetic/fallback 构造、测试 fixture
- 发现某字段只在解构或构造时“占位为 `None`”而没有跨层透传时，按功能回归处理，不能仅用 `_field`、`#[allow(dead_code)]` 或局部消警结束
- 至少保留一条 round-trip 或端到端断言，锁定该字段跨层读写都不丢失
- 对 `thread-store` 这类有 checked-in proto 产物的模块，`.proto` 与生成的 Rust 文件必须同次更新，不能只改一边

如果本次同步触及 `codex-rs/protocol/src/error.rs` 或相关错误映射：

- 不要只看枚举本身的 diff
- 联动审查 `is_retryable()`、`to_codex_protocol_error()` 和 `codex-rs/core/src/session/turn.rs` 的调用方
- 如果只补了协议层枚举测试，没有覆盖 turn 级自动重试或协议映射断言，不要把它当成验证充分

如果本次同步触及 `codex-rs/cli/src/main.rs`、`codex-rs/tui/src/cli.rs`，或任何 interactive CLI 参数桥接（例如 root CLI / `resume` / `fork` / `zoffsec resume` 复用 `TuiCli`）：

- 不要只看 Clap 参数还存在
- 额外审查子命令到最终 `TuiCli` 的 merge/bridge 函数，例如 `merge_interactive_cli_flags()`
- 如果参数已经收敛到 `codex-rs/utils/cli/src/shared_options.rs` 的 `SharedCliOptions`，不要在 `codex-rs/tui/src/cli.rs` 重新声明同名字段；重复的 clap arg id 会在 debug build 的 Command 构建阶段 panic，例如 `oss_provider` / `--local-provider`
- 对新增或本地扩展的 interactive 参数，确认 root 路径和 `resume` / `fork` 等子命令路径都会透传
- 至少保留一条覆盖 bridge 行为的回归测试，并保留一条 `Command::debug_assert()` 级别测试覆盖重复参数注册；不能只靠 help 文案或解析测试宣称功能仍在
- 特别是 provider / local-provider、sandbox、approval、cwd、search 这类“能解析但可能在 merge 时被静默丢掉”的参数，要把 merge 赋值和回归测试一起纳入审查

如果本次同步触及 `codex-rs/core/src/session/mod.rs`、`codex-rs/core/src/session/turn_context.rs`、`codex-rs/app-server/src/codex_message_processor.rs`、`codex-rs/tui/src/app.rs`，或任何 `turn/steer` / warning 文案映射：

- 不要只检查某一层源码里的中文字符串还在
- 额外审查 `core -> app-server -> tui` 的错误/警告桥接是否仍然一致
- 对依赖字符串解析的路径，确认 `tui` 的 mismatch prefix / missing-active-turn 检测仍能命中
- 对 `turn_context.rs` 这类直接发出 warning 文案的源头，至少保留一条覆盖具体中文 warning 文案的回归测试；不能只靠静态 grep 判定“没有回归”
- 至少保留一条覆盖中文 warning 前缀和一条覆盖 steer 错误文案的回归测试；不能只靠静态 grep 判定“没有回归”

如果本次同步触及 `codex-rs/features/src/lib.rs` 或实验/开发中功能元数据与 warning：

- 不要只检查 `codex features enable` 的直出文案；TUI 启动页 warning 走 `unstable_features_warning_event()`，这是另一条源头路径
- `chinese-localization-sentinels` 必须覆盖 `codex-rs/features/src/lib.rs` 的中文 warning 文案、`codex-rs/features/src/tests.rs` 的源头单测，以及 `codex-rs/core/tests/suite/unstable_features_warning.rs` 的会话事件断言
- 若截图或用户反馈出现启动页英文 warning，先 grep `Under-development features enabled` / `Under-development features are incomplete`，不能只看 CLI feature 子命令测试是否通过

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

## Merge-Back Gate

- merge-back 前至少做两次 `check`
  - worktree 内一次
  - 当前分支 `git merge --no-ff --no-commit "$branch"` 之后再一次
- `check` 只要报缺失项，就不能提交
- worktree 审查时必须使用 worktree 自己的脚本与 `json` 基线副本
- 对 `local_surface` / `localized_behavior` 特性，检查点不能只停留在文案或模块存在性；要覆盖运行时桥接、事件 wiring、配置落盘等真实链路
- 对 `localized_behavior` 特性，如果用户可见文案实际来自 `FeatureSpec` 元数据、共享 helper、onboarding/history 组件、直接字符串断言或 snapshot，不要只检查视图入口；要把真正的文案源头和锁定这些文案的测试/快照一起纳入审查
- 对 `localized_behavior` 特性，如果文案跨 `core -> app-server -> tui` 传递，检查点还必须覆盖桥接和字符串解析路径；不能只看源头文案还在
- 对 `codex-rs/cli` 这类本地 CLI 面，检查点必须覆盖顶层 `Subcommand` 注册、dispatch 接线，以及 help/localization 哨兵；不能只看底层 crate 或模块目录还在
- 对 `resume` / `fork` 这类复用 `TuiCli` 的交互子命令，检查点还必须覆盖参数 bridge：不能只看到字段和 help 仍存在；要确认子命令 merge 后真的写入最终 interactive 配置，并有回归测试锁定
- 对 `workspace/local-crates` 这类本地 crate 面，检查点不能只保目录；还要覆盖 `codex-rs/Cargo.toml` 的 workspace members 和 workspace dependency 接线

对每个缺失/覆盖项，都必须给出原因：

- 被上游覆盖
- 本地冲突解决丢失
- 文件或符号被移动
- 已被更好的等效实现替代

只有在同时满足这两点时，才允许按“更好的等效替换”处理：

1. 行为不回退
2. 已先更新权威基线，再重新 `render` 和 `check`

否则，一律按功能回归处理并修复。

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
- 这轮 `discover` 与 `promote` 做了什么
- 哪些本地分叉特性被新增、修正、移除或保持
- 主要合并内容
- 关键冲突如何处理
- 是否发现功能丢失/覆盖，以及原因
- 是否存在“更好的等效替换”，以及因此更新了哪些基线定义
- 跑了哪些验证
- `STATE.md`、`local-fork-features.json` 和 `local-fork-features.md` 是否已更新
