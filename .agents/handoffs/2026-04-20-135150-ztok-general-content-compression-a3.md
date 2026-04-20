# Handoff: ztok general content compression a3

## Session Metadata
- Created: 2026-04-20 13:51:50
- Project: .
- Branch: web
- Commit: 5b9220336

## Current State
`a2` 已完成真实收尾并回写到 `.agents/issues/2026-04-20-ztok-general-content-compression.toml`，包括 Bazel lock 收口与 Cadence 校验。当前停在 `a3` 开始前的实现勘察阶段：还没有写 `SimHash + LCS` 代码，只补齐了执行计划并开始读取 `ztok` 的共享压缩/会话 dedup 相关上下文。暂停原因是用户要求立即生成交接快照。

## Work Completed
- [x] 已补读 `llmdoc` 启动文档、Rust 改动回路与命令参考，确认本轮遵循仓库既有 Rust/Bazel 工作流。
- [x] 已确认没有残留 `bazel-lock-check` / `bazel mod deps` 相关后台进程。
- [x] 已确认本任务相关 lock 状态为 `MODULE.bazel.lock` 变更已存在，`codex-rs/Cargo.lock` 与 `tools/argument-comment-lint/Cargo.lock` 没有额外漂移。
- [x] 已再次确认 `just bazel-lock-check` / `scripts/check-module-bazel-lock.sh` 返回状态为 `0`。
- [x] 已将 issue `a2` 回写为 `status = "done"`、`validate_status = "passed"`，并写入验证结果与 lock 收口说明。
- [x] 已通过 Cadence 校验：
  - `node /root/.config/lnk/.agents/skills/using-cadence/scripts/cadence_validate.js execution-write --before /tmp/2026-04-20-ztok-general-content-compression.toml.before-a2 --after /workspace/.agents/issues/2026-04-20-ztok-general-content-compression.toml`
  - `node /root/.config/lnk/.agents/skills/using-cadence/scripts/cadence_validate.js issue /workspace/.agents/issues/2026-04-20-ztok-general-content-compression.toml`

## In Progress
- [ ] `a3`：补齐完整的 SimHash + LCS 近重复压缩路径
- 当前进度：只完成了执行面准备和 issue 状态收口；尚未正式阅读 `codex-rs/ztok/src/compression.rs`、`session_dedup.rs`、`read.rs` 的 `a3` seam，也还没有新增任何 `a3` 代码或测试。

## Immediate Next Steps
1. 读取 `codex-rs/ztok/src/compression.rs`、`codex-rs/ztok/src/session_dedup.rs`、`codex-rs/ztok/src/read.rs` 以及相关测试，确认近重复候选筛选和差分输出应插入的位置。
2. 设计并实现最小可审计的 `SimHash + LCS` 路径，优先服务 `read`，同时把阈值/判定参数做成可测试 seam。
3. 补 `codex-ztok` 测试覆盖高相似命中、低置信度回退、候选冲突/缺失快照回退，然后执行 `just fmt`、`cargo test -p codex-ztok`、必要时 `just fix -p codex-ztok`。

## Key Files
| File | Why It Matters | Notes |
|---|---|---|
| `.agents/issues/2026-04-20-ztok-general-content-compression.toml` | Cadence 执行 SSOT | `a2` 已回写为 done/passed，`a3` 仍是 todo |
| `.agents/plan/2026-04-20-ztok-general-content-compression.md` | 当前任务的执行计划与收尾顺序 | 已更新为执行态，包含 `a2` 收口和 `a3` 范围 |
| `codex-rs/ztok/src/session_dedup.rs` | 现有 exact dedup 缓存与会话 SQLite 载体 | `a3` 预计复用其缓存数据作为近重复候选来源 |
| `codex-rs/ztok/src/read.rs` | 当前 `read` 路径的共享压缩接入点 | `a3` 第一落点应继续优先放在这里 |
| `codex-rs/ztok/src/compression.rs` | 共享压缩合同与输出类型中心 | 需要确认是否已经存在 `Diff` 结果合同与显式回退结构 |
| `MODULE.bazel.lock` | 本轮依赖变更后的 Bazel lock 收口文件 | 已变更，不要回退 |

## Decisions & Rationale
| Decision | Rationale | Impact |
|---|---|---|
| 保留 `rusqlite + sha1` 方案，不切换实现路线 | 用户明确要求尽量复用已有依赖，仓库内本来就已有 `rusqlite` / `sha1` | `a2` 的技术栈稳定，可直接继续在 SQLite 会话缓存上扩展 `a3` |
| 缺失会话标识时禁用 dedup，而不是退化为粗粒度缓存 | 这是 issue/plan 的强约束，避免跨会话误命中 | `a3` 也必须复用同一 dedup-disable 合同，不能绕过 |
| 先完成 `a2` 的 lock/lint/Cadence 收尾，再进入 `a3` | 避免把近重复压缩建立在未闭环的依赖状态之上 | 当前代码面可以直接专注 `a3`，不用再纠缠 Bazel lock |

## Risks / Gotchas
- 仓库当前存在大量与本任务无关的脏改动，主要在 `codex-rs/tui`、`codex-rs/codex-api`、`codex-rs/app-server`；继续执行时必须显式收紧边界，不要误碰这些文件。
- `MODULE.bazel.lock` 已有本轮必要变更，不要因后续阅读或清理动作把它回退。
- 之前用 `just bazel-lock-check` 观察到输出顺序异常，像是 stdout 缓冲；但脚本退出码已明确为 `0`，不要把这个 warning 当失败重新折腾。
- `a3` 的完成标准不是“命中某种相似算法”，而是“输出可解释且可回退”；低置信度、候选冲突、缺失快照、不可读差分都必须显式回退。

## Validation Snapshot
- Commands run: `cd /workspace/codex-rs && just fmt`, `cd /workspace/codex-rs && env -u RUSTC_WRAPPER cargo test -p codex-cli --test ztok`, `cd /workspace/codex-rs && env -u RUSTC_WRAPPER cargo test -p codex-ztok`, `cd /workspace && just bazel-lock-update`, `cd /workspace && just bazel-lock-check`, `cd /workspace/codex-rs && env -u RUSTC_WRAPPER just fix -p codex-ztok`
- Result: `a2` 所需验证已通过；Cadence execution-write 与 issue 校验已通过；`a3` 尚未开始编码，因此没有新的实现验证结果
- Remaining checks: 恢复后先读 `a3` 相关代码 seam，再补实现与 `codex-ztok` 测试，之后按需回写 issue/plan
