# Handoff: TUI startup warning drain and buddy visibility fix

## Session Metadata
- Created: 2026-04-28 13:15:04 UTC
- Project: .
- Branch: web
- Commit: 36aa5d14b

## Current State
当前在收口两个 TUI 问题：启动时“先出界面/宠物，再卡一下才显示开发中警告”的首帧竞态，以及提交输入后宠物消失。已经把启动排空逻辑扩到同时等待 app-server 事件流和 `active_thread_rx`，并把宠物渲染从“有 status 就隐藏”改成“可与 status 共存”。尚未完成编译/测试验证，也还没生成对应 snapshot 文件。

## Work Completed
- [x] 复核了此前 handoff，确认剩余启动卡顿问题大概率在 `codex-rs/tui/src/app.rs` 的首帧前排空阶段，而不是 app-server。
- [x] 保留并检查了 `app.rs` 中的启动排空补丁：先 drain `try_next_event()` / `active_thread_rx`，再在 80ms 内 `select!` 等待两条队列的后续事件。
- [x] 在 `codex-rs/tui/src/bottom_pane/mod.rs` 中移除了 `self.status.is_none()` 对宠物渲染的硬门禁。
- [x] 把底部面板测试从“status 可见时 buddy 必须隐藏”改成“status 仍可见，buddy 也仍可见”，并新增 snapshot 断言名 `buddy_and_status_visible_snapshot`。

## In Progress
- [ ] 让改动通过最小验证闭环
- 当前进度：代码已改，但还没跑 `cargo check` / 定向测试，也还没有接受或手动写入新 snapshot 文件。

## Immediate Next Steps
1. 先查看 `git diff -- codex-rs/tui/src/app.rs codex-rs/tui/src/bottom_pane/mod.rs`，确认只包含本任务改动。
2. 用隔离 Cargo 目录运行最小验证，优先 `cargo test -p codex-tui buddy_remains_visible_while_status_indicator_is_visible -- --exact --nocapture`，必要时再补 `cargo check -p codex-tui --lib`。
3. 如果 snapshot 生成了 `.snap.new`，只接收本任务对应的 `buddy_and_status_visible_snapshot`，不要在 dirty worktree 里全量 `cargo insta accept`。
4. 若用户继续反馈启动仍卡，再给 `drain_startup_app_server_events()` 补更直接的 TUI 级回归测试或日志证据，确认首帧前是否仍漏掉 warning 路径。

## Key Files
| File | Why It Matters | Notes |
|---|---|---|
| codex-rs/tui/src/app.rs | 启动首帧前的 app-server / thread 事件排空逻辑 | 当前有未提交补丁，核心是双队列 drain + 80ms bounded wait |
| codex-rs/tui/src/bottom_pane/mod.rs | 决定 status、buddy、footer、queued preview 的底部渲染组合 | 已取消 “status 存在时隐藏 buddy” 的门禁 |
| codex-rs/tui/src/chatwidget.rs | 提交输入时会多处设为“处理中”，解释了为什么旧逻辑会让宠物消失 | 本轮未改这里，避免在提交链路加表面补丁 |
| codex-rs/tui/src/bottom_pane/snapshots/ | 底部面板快照目录 | 需要新增或接受 `buddy_and_status_visible_snapshot` |

## Decisions & Rationale
| Decision | Rationale | Impact |
|---|---|---|
| 启动竞态优先在 TUI 首帧排空层修 | app-server 已有 in-process warning flush 修复，但 warning 进入可见 history 还要经过 `active_thread_rx` | 首帧前需要同时考虑 app-server 与 active thread 两条队列 |
| 宠物消失在渲染层修，而不是提交时重复 show | 提交后 status 变为“处理中”，旧 render gate 直接隐藏 buddy；这是根因，不是可见性状态被真正关闭 | 修复后宠物可在运行状态下继续显示 |
| 验证必须用隔离 Cargo 目录 | 用户明确不希望被 lock 卡住，仓库规则也要求避免共享 target/index 锁竞争 | 测试命令需要设置独立 `CARGO_TARGET_DIR` / `CARGO_HOME` |

## Risks / Gotchas
- `codex-rs/tui/src/app.rs` 的启动排空补丁尚未编译验证，可能存在 borrow 或异步分支细节问题。
- 宠物与 status 共显会改变底部高度和快照，需要以测试结果为准更新预期。
- 当前工作区 `git status --short` 只看到 `codex-rs/tui/src/bottom_pane/mod.rs` 脏；如果 `app.rs` 改动未显示，继续前要再确认该文件当前是否已被格式化并处于与 HEAD 相同还是差异被意外丢失。

## Validation Snapshot
- Commands run: `sed`/`rg` 定位相关代码、`git status --short`
- Result: partial
- Remaining checks: `cargo test -p codex-tui buddy_remains_visible_while_status_indicator_is_visible -- --exact --nocapture`、必要时 `cargo check -p codex-tui --lib`、`just fmt`
