# Handoff: TUI buddy status visibility validation

## Session Metadata
- Created: 2026-04-28 13:17:25 UTC
- Project: .
- Branch: web
- Commit: 36aa5d14b

## Current State
当前处于 TUI 底部面板宠物与 status 共显改动的验证前状态。仓库当前真实 diff 只显示 `codex-rs/tui/src/bottom_pane/mod.rs` 有未暂存修改；上一份 handoff 中提到的 `codex-rs/tui/src/app.rs` 启动排空补丁当前未出现在工作区 diff 中，因此恢复时应以 `git status` / `git diff` 的真实状态为准，不假设 `app.rs` 仍有未提交改动。

## Work Completed
- [x] 读取 `proactive-handoff` 技能、handoff 模板和 resume checklist。
- [x] 确认当前分支为 `web`，HEAD 为 `36aa5d14b docs(cadence): add tui buddy rarity visual differentiation plan and handoff`。
- [x] 检查当前工作区状态：`codex-rs/tui/src/bottom_pane/mod.rs` 修改，`.agents/handoffs/2026-04-28-131504-tui-startup-buddy-fix.md` 未跟踪。
- [x] 读取当前 `bottom_pane/mod.rs` diff：buddy 渲染不再受 `self.status.is_none()` 门禁限制，相关测试改为断言 status 与 buddy 同时可见，并新增 snapshot 名 `buddy_and_status_visible_snapshot`。

## In Progress
- [ ] 验证并收口 `bottom_pane/mod.rs` 的 buddy/status 共显改动。
- 当前进度：代码 diff 已确认，但尚未运行 TUI 定向测试、尚未生成/接受对应 snapshot，未提交。

## Immediate Next Steps
1. 运行 `git status --short --untracked-files=all` 和 `git diff -- codex-rs/tui/src/bottom_pane/mod.rs`，确认恢复后的工作区仍与本 handoff 一致。
2. 在隔离 Cargo 目录中运行 `cargo test -p codex-tui buddy_remains_visible_while_status_indicator_is_visible -- --exact --nocapture`，避免共享 `target/` 锁竞争。
3. 如果测试生成 `*.snap.new`，只审阅并接受 `buddy_and_status_visible_snapshot` 对应 snapshot，不做全量 snapshot accept。
4. 跑 `just fmt`；若 TUI 测试和 snapshot 通过，再做最小自审并只暂存本任务相关文件。

## Key Files
| File | Why It Matters | Notes |
|---|---|---|
| codex-rs/tui/src/bottom_pane/mod.rs | 当前唯一源码 diff，控制 status、buddy、footer 等底部渲染组合 | 已取消 status 存在时隐藏 buddy 的条件，并更新同文件测试 |
| codex-rs/tui/src/bottom_pane/snapshots/ | TUI snapshot 输出目录 | 预期需要新增或更新 `buddy_and_status_visible_snapshot` |
| .agents/handoffs/2026-04-28-131504-tui-startup-buddy-fix.md | 上一份相关 handoff | 内容提到 `app.rs`，但当前真实 diff 未显示 `app.rs` 修改 |

## Decisions & Rationale
| Decision | Rationale | Impact |
|---|---|---|
| 以当前 git 状态为事实源，而不是沿用上一份 handoff 的全部叙述 | 当前 `git diff --name-status` 只显示 `bottom_pane/mod.rs` 修改 | 恢复时避免追不存在的 `app.rs` 改动 |
| buddy 与 status 共显在 `BottomPane` 渲染门禁处修 | 当前 diff 直接移除 `self.status.is_none()` 条件，命中宠物提交后因 status 出现而隐藏的根因 | 底部高度和 snapshot 会变化，需要 snapshot 验证 |
| 验证使用隔离 Cargo 目录 | 仓库存在长跑 Cargo/target 锁竞争历史 | 降低恢复后测试被无关构建阻塞的概率 |

## Risks / Gotchas
- 当前已有未跟踪 `.agents/handoffs/2026-04-28-131504-tui-startup-buddy-fix.md`；提交时不要误把不相关 handoff 混入，除非明确决定一起提交。
- `bottom_pane/mod.rs` 改动会影响 UI 高度和 snapshot，必须用 `insta` 结果确认视觉输出。
- 如果恢复时 `codex-rs/tui/src/app.rs` 又出现 diff，需要重新核对它是否来自上一份 handoff 的启动排空工作，而不是本次 buddy/status 收口。
- 使用 `codex ztok shell` 跑 Rust 测试可能注入环境变量导致 sccache/incremental 冲突；必要时直接使用 shell 并设置隔离 `CARGO_TARGET_DIR`。

## Validation Snapshot
- Commands run: `codex ztok shell git status --short --branch --untracked-files=all`, `codex ztok shell git diff --name-status && git diff --cached --name-status`, `codex ztok shell git diff -- codex-rs/tui/src/bottom_pane/mod.rs`
- Result: partial; 当前只完成状态确认与 handoff 落盘，未执行 Rust/TUI 测试。
- Remaining checks: `cargo test -p codex-tui buddy_remains_visible_while_status_indicator_is_visible -- --exact --nocapture`, snapshot 审阅/接受，`just fmt`，最终自审。
