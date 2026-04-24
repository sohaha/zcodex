# QA Review

- status: WARNING
- summary: 对 `zteam` 工作台、`/zteam start`、`/zteam attach`、`/zteam relay` 相关变更做了定向审查。发现 1 个已验证的中等级回归风险和 2 个低等级可用性/覆盖缺口。自动化验证方面，`cargo` 初次运行被环境里的 `CARGO_INCREMENTAL` + `sccache` 组合阻断；在 `env -u CARGO_INCREMENTAL` 下，`cargo check -p codex-tui --tests` 通过，`cargo nextest run -p codex-tui zteam` 通过（36 passed, 1687 skipped）。`cargo insta` 未安装，无法用 `pending-snapshots` 做额外快照巡检。
- files changed:
  - `/workspace/codex-rs/tui/src/zteam.rs`
  - `/workspace/codex-rs/tui/src/zteam/view.rs`
  - `/workspace/codex-rs/tui/src/zteam/worker_source.rs`
  - `/workspace/codex-rs/tui/src/slash_command.rs`
  - `/workspace/codex-rs/tui/src/chatwidget/tests/slash_commands.rs`
  - `/workspace/codex-rs/tui/src/chatwidget/snapshots/codex_tui__chatwidget__tests__zteam_workbench_empty_view.snap`
  - `/workspace/codex-rs/tui/src/chatwidget/snapshots/codex_tui__chatwidget__tests__zteam_workbench_active_view.snap`
  - `/workspace/codex-rs/tui/src/chatwidget/snapshots/codex_tui__chatwidget__tests__zteam_workbench_reattach_required_view.snap`
  - `/workspace/codex-rs/tui/src/chatwidget/snapshots/codex_tui__chatwidget__tests__zteam_workbench_partial_registration_view.snap`

## Acceptance Criteria Checklist

- [x] 已审查指定范围内的全部相关改动
- [x] 结论包含带 `file:line` 的已验证 findings
- [x] 已检查用户可见行为、文案契约、测试/快照覆盖与回归风险
- [x] 已运行定向自动化验证并记录受限项

## Review Result: WARNING

### CRITICAL
- 无

### HIGH
- 无

### MEDIUM
- `codex-rs/tui/src/zteam/worker_source.rs:77` — legacy fallback 现在只接受 `slot.display_name()` 或 `slot.task_name()` 的精确匹配，之前显式兼容的旧昵称别名（例如 frontend 的 `前端`/`android`/`android frontend`，backend 的 `server`）已被移除。`/zteam attach` 和 loaded auto-restore 都先经过 `local_thread_matches_slot()` 再筛选线程，所以缺少 `agent_path` 的旧 worker 线程会直接从恢复候选里消失，现有会话无法再附着或恢复。`worker_source.rs:155-176` 里的新测试已经把这个回归固化成当前行为。修复代码：
```rust
fn slot_matches_legacy_agent_nickname(slot: WorkerSlot, agent_nickname: &str) -> bool {
    let nickname = agent_nickname.trim();
    nickname == slot.display_name()
        || nickname.eq_ignore_ascii_case(slot.task_name())
        || matches!(
            slot,
            WorkerSlot::Frontend
                if nickname == "前端"
                    || nickname.eq_ignore_ascii_case("android")
                    || nickname.eq_ignore_ascii_case("android frontend")
        )
        || matches!(
            slot,
            WorkerSlot::Backend
                if nickname == "后端" || nickname.eq_ignore_ascii_case("server")
        )
}
```

### LOW
- `codex-rs/tui/src/zteam/view.rs:202` — 工作台底部快捷提示删掉了 root -> worker 的主分派语法，只保留了 `status/start/attach/relay`。但真正从主线程给 worker 派任务的命令仍是 `/zteam <frontend|backend> <任务>`，契约还写在 `codex-rs/tui/src/zteam.rs:726`，执行入口也还在 `codex-rs/tui/src/app.rs:2123`。结果是用户进入工作台、看到“worker 已就绪”后，界面上反而没有下一步分派命令。修复代码：
```rust
Paragraph::new(Line::from(vec![
    "Esc 关闭".dim(),
    " · ".dim(),
    "/zteam status".cyan(),
    " 查看状态".dim(),
    " · ".dim(),
    "/zteam <worker> <任务>".cyan(),
    " 分派任务".dim(),
    " · ".dim(),
    "/zteam relay".cyan(),
    " 协作中转".dim(),
]))
```

- `codex-rs/tui/src/app.rs:10543` — 当前 `app.rs` 的 `test_zteam_thread()` helper 强制给恢复候选塞入 `agent_path: Some(...)`，因此 app 层没有任何集成用例覆盖“`agent_path` 缺失、只能靠 legacy nickname/role 识别”的 `/zteam attach` 或 loaded auto-restore 场景。`chatwidget` 新增的快照只覆盖了工作台渲染，不覆盖恢复链路；这也是上面的兼容性回归没有在 app 级测试里暴露的直接原因。补测代码：
```rust
fn legacy_zteam_thread(
    thread_id: ThreadId,
    parent_thread_id: ThreadId,
    nickname: &str,
    role: &str,
) -> Thread {
    Thread {
        source: SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
            parent_thread_id,
            depth: 1,
            parent_model: None,
            agent_path: None,
            agent_nickname: Some(nickname.to_string()),
            agent_role: Some(role.to_string()),
        }).into(),
        agent_nickname: Some(nickname.to_string()),
        agent_role: Some(role.to_string()),
        ..test_zteam_thread(thread_id, parent_thread_id, WorkerSlot::Backend, ThreadStatus::Idle, 10)
    }
}
```

## Notes

- 这轮快照改动是合理同步的：空态、活动态、待再附着态都已更新，并新增了 `zteam_workbench_partial_registration_view`。
- 自动化命令需显式去掉 `CARGO_INCREMENTAL`，否则当前环境里的 `sccache` 会在依赖编译阶段直接失败，和本次代码无关。
