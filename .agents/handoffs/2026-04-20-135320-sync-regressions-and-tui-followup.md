# Handoff: sync-regressions-and-tui-followup

## Session Metadata
- Created: 2026-04-20 13:53:20 UTC
- Project: .
- Branch: web
- Commit: 5b9220336

## Current State
本轮已经确认并修复两条真实回归：`thread-store` 远端链路里的 `parent_model` 断层，以及 `tui` 里的 `ResetMemories` 入口/确认流断链。`codex-api` 当前已经全绿，但 `codex-tui` 完整测试集仍有大范围漂移，既包含大量 snapshot 变化，也包含若干真实逻辑失败，因此当前不适合提交，下一会话应继续收敛 `codex-tui` 的剩余回归。

## Work Completed
- [x] 已确认 `parent_model` 是同步上游后的真实回归，并在更早一轮修复和提交；相关提交为 `c8c41522a`。
- [x] 已清理一批 build warning，并在更早一轮提交；相关提交为 `06e6a451a`。
- [x] 已修复 `codex-api` 的测试漂移，包括 `ResponsesApiRequest.max_output_tokens`、`ContentItem::InputImage.detail`、`ModelInfo.skip_reasoning_popup`、`ResponsesClient::new/CompactClient::new` 鉴权参数，以及 Anthropic context-window 错误映射。
- [x] 已确认 `anthropic_stream_request_preserves_session_headers` 不是 runtime 回归，而是测试错误地用 `ResponsesClient` 断言 Anthropic `/messages`；现已改为使用 `AnthropicClient`。
- [x] 已补回 `tui` 的 memories reset 入口，在 `MemoriesSettingsView` 中恢复“重置所有记忆 -> 二次确认 -> 发送 ResetMemories”链路。
- [x] 已修复本轮阻塞 `codex-tui` 定向测试的一批编译漂移：`ProjectConfig` 类型迁移、`InProcessClientStartArgs` 字段更新、`ModelPreset.skip_reasoning_popup`、`HookRunSummary.source`、`PatchApplyUpdated` match 分支、`history_cell.rs` 的格式字符串。
- [x] 已让以下用例通过：`memories_reset_confirmation_sends_event_on_confirm`、`memories_reset_confirmation_snapshot`、`memories_settings_popup_snapshot`、`memories_settings_toggle_saves_on_enter`。
- [x] 已删除完整 `codex-tui` 测试跑出的大量 `.snap.new` / `.pending-snap` 噪音文件，避免污染后续提交边界。

## In Progress
- [ ] 继续收敛 `codex-tui` 的剩余大范围回归
- 当前进度：`codex-api` 已全绿，`memories` 这条回归线已闭合；`codex-tui` 仍是主阻塞。完整运行 `env -u RUSTC_WRAPPER cargo test -p codex-tui` 时，结果为 `1557 passed; 113 failed; 1 ignored`。失败不是单一问题，分成两类：一类是大量 snapshot 漂移；另一类是真实逻辑断言失败，例如 `buddy_is_visible_by_default`、`history_lookup_response_is_routed_to_requesting_thread`、`feedback_submission_for_inactive_thread_replays_into_origin_thread`、部分 guardian/feature-flag/apps/plugin popup 行为。

## Immediate Next Steps
1. 先按真实逻辑失败优先级处理 `codex-tui`：从 `chatwidget::tests::slash_commands::buddy_is_visible_by_default` 和 `app::tests::history_lookup_response_is_routed_to_requesting_thread` 两个代表性失败开始，定位断链原因，不要先批量接受 snapshot。
2. 在真实逻辑失败收敛后，再重新跑 `env -u RUSTC_WRAPPER cargo test -p codex-tui`，根据新的失败面区分“可批量接受的快照基线更新”和“仍需修代码的行为回归”。
3. 当 `codex-tui` 至少达到可提交状态后，再整理只属于本任务的改动并提交，注意不要混入用户并行修改的无关文件。

## Key Files
| File | Why It Matters | Notes |
|---|---|---|
| codex-rs/codex-api/tests/clients.rs | 修复 Anthropic client 测试错位 | 已将错误的 `ResponsesClient` 替换为 `AnthropicClient` |
| codex-rs/codex-api/src/api_bridge.rs | Anthropic context-window 错误映射 | 本轮已有未提交改动，`codex-api` 全测已通过 |
| codex-rs/codex-api/src/chat_completions/tests.rs | `ResponsesApiRequest` fixture 漂移修复 | 已补 `max_output_tokens: None` 等字段 |
| codex-rs/codex-api/src/endpoint/compact.rs | Anthrop ic 分支与 auth 相关测试漂移 | 本轮已有未提交改动 |
| codex-rs/codex-api/tests/models_integration.rs | `ModelInfo.skip_reasoning_popup` fixture 更新 | `codex-api` 已全绿 |
| codex-rs/tui/src/bottom_pane/memories_settings_view.rs | 真实回归修复点 | 已恢复 reset entry + confirm flow |
| codex-rs/tui/src/chatwidget.rs | `open_memories_popup()`、事件分发、`PatchApplyUpdated` 分支 | 当前大量 TUI 行为仍可能继续改这里 |
| codex-rs/tui/src/chatwidget/tests/popups_and_settings.rs | memories / popup / apps / reasoning 等测试集中地 | 已补 fixture 字段，部分逻辑失败仍在这里暴露 |
| codex-rs/tui/src/chatwidget/tests/slash_commands.rs | `buddy_is_visible_by_default` 等代表性逻辑失败 | 下一轮优先看 |
| codex-rs/tui/src/chatwidget/tests/status_and_layout.rs | `HookRunSummary.source` fixture 已补 | 仍有状态栏相关失败 |
| codex-rs/tui/src/history_cell.rs | 本轮修复格式字符串导致的 warning/编译问题 | 已改 `line![format!(...)]` |
| codex-rs/tui/src/lib.rs | `ProjectConfig` 类型迁移修复 | 已从 `codex_config::config_toml::ProjectConfig` 导入 |
| codex-rs/tui/src/onboarding/auth.rs | `InProcessClientStartArgs` 字段漂移修复 | 已移除不存在的 `log_db` 字段 |
| codex-rs/tui/src/status/helpers.rs | test-only 导入修复 | 已补 `AgentsMdManager` / `LOCAL_FS` |

## Decisions & Rationale
| Decision | Rationale | Impact |
|---|---|---|
| 将 `anthropic_stream_request_preserves_session_headers` 改为使用 `AnthropicClient` | 生产代码对 `WireApi::Anthropic` 本就走独立 `ApiAnthropicClient::new(...).stream_request(...)`，不是 `ResponsesClient` | 避免把测试错位误判为 runtime 回归 |
| 在 `MemoriesSettingsView` 内恢复 reset confirm 状态机 | 运行时仍保留 `AppEvent::ResetMemories` 和 app-server handler，但 UI 入口已经断链，测试和设计快照都表明原本应有此功能 | 闭合真实回归链，恢复用户可达路径 |
| `PatchApplyUpdated` 暂时保持 no-op | 该事件当前携带结构化 `changes`，而现有 TUI 没有对应增量渲染实现；本轮目标是先恢复编译与现有行为，不盲目新增 UI 语义 | 解决同步引入的 match 不全编译错误，不额外扩大范围 |
| 不接受完整 `codex-tui` 测试生成的所有 snapshot | 当前完整测试仍含大量真实逻辑失败，若直接收快照会掩盖行为回归 | 下一轮继续先修逻辑、后收快照 |

## Risks / Gotchas
- 仓库里存在并行脏改动，不要误混入：`codex-rs/app-server/src/codex_message_processor.rs` 不是本轮产物。
- 当前分支名显示为 `web`，HEAD 是 `5b9220336`；提交前需要再次确认是否仍在同一分支、是否有用户新改动插入。
- `codex-tui` 完整测试仍会生成大量 `.snap.new` 噪音文件；跑全量后要及时清理，只保留有意接受的快照。
- `protoc` 在当前环境不可用，这影响 `thread-store` 的自动 proto 生成；此前 `parent_model` 修复是手动同步 `.proto` 和 checked-in `.rs`。
- `RUSTC_WRAPPER=sccache` 在本环境可能干扰命令，当前稳定命令形式仍是 `env -u RUSTC_WRAPPER ...`。
- `git status` 中还有 `MODULE.bazel.lock` 变更；本轮未处理其来源，提交前要确认是否属于本任务。

## Validation Snapshot
- Commands run: `env -u RUSTC_WRAPPER just fmt`, `env -u RUSTC_WRAPPER cargo test -p codex-api`, `env -u RUSTC_WRAPPER cargo test -p codex-tui memories_reset_confirmation_sends_event_on_confirm -- --exact`, `env -u RUSTC_WRAPPER cargo test -p codex-tui memories_reset_confirmation_snapshot -- --exact`, `env -u RUSTC_WRAPPER cargo test -p codex-tui memories_settings_popup_snapshot -- --exact`, `env -u RUSTC_WRAPPER cargo test -p codex-tui memories_settings_toggle_saves_on_enter -- --exact`, `env -u RUSTC_WRAPPER cargo test -p codex-tui`
- Result: `codex-api` 全量通过；`memories` 相关 4 个 `codex-tui` 定向测试通过；`codex-tui` 全量部分失败（1557 passed; 113 failed; 1 ignored）
- Remaining checks: 下一会话需要继续收敛 `codex-tui` 的真实逻辑失败，再决定批量快照更新范围；之后再做本任务相关文件的精确提交
