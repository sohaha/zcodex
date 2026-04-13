# inline resize 只在旧 viewport 已贴底时才回退到 screen delta

## 背景
- 任务：继续收敛 `codex-cli` 在终端 resize 后的错位/残留问题，补齐深度审查指出的遗漏场景。
- 现象：已有修复只处理了“cursor 行变化时 viewport 下移要先按旧 origin 清屏”的链路；但在 tmux / CPR 不稳定环境里，旧 viewport 若已贴底，resize 后 cursor 行可能不变，旧的 `pending_viewport_area()` 会直接返回 `None`，导致内部 viewport 不跟着实际终端内容移动。

## 结论
- `pending_viewport_area()` 不能继续把 cursor delta 当成唯一依据。
- 更稳妥的最小修法是：
  - 如果终端报告了非零 cursor delta，优先沿用它；
  - 如果 cursor 行未变化或 CPR 不可用，则只在旧 viewport 已贴底时回退到 `screen_size.height - last_known_screen_size.height` 的 screen delta。
- 这样可以覆盖“终端内容跟着底边移动，但 cursor 反馈没变”的 resize 场景，同时避免把 screen delta 错当成所有 resize 的通用位移来源。

## 证据
- 代码：`codex-rs/tui/src/tui.rs` 的 `pending_viewport_area_for_terminal()` 现在只在旧 viewport 已贴底时，才会在 cursor delta 为零时回退到 screen delta。
- 回归测试：
  - `moving_viewport_down_clears_stale_rows_above_new_origin`
  - `moving_viewport_up_clears_rows_between_new_and_old_origins`
  - `resize_uses_screen_delta_when_cursor_row_is_unchanged_for_bottom_aligned_viewport`
  - `resize_uses_negative_screen_delta_when_cursor_row_is_unchanged_for_bottom_aligned_viewport`
  - `resize_uses_screen_delta_when_cursor_query_fails_for_bottom_aligned_viewport`
  - `resize_without_cursor_query_keeps_non_bottom_aligned_viewport_in_place`
  - `resize_without_cursor_delta_keeps_non_bottom_aligned_viewport_in_place`
- 测试基建：`codex-rs/tui/src/test_backend.rs` 里 `VT100Backend::get_cursor_position()` 需要把 `vt100` 返回的 `(row, col)` 映射成 `Position { x: col, y: row }`；同时新增 `fail_cursor_position_queries()` 以显式覆盖 CPR 不可用时的 fallback 分支；如果直接 `.into()` 或无法模拟 query failure，依赖 cursor row 的 resize 回归都会失真或漏测。
- 验证：`env -u RUSTC_WRAPPER cargo test -p codex-tui moving_viewport -- --nocapture`、`env -u RUSTC_WRAPPER cargo test -p codex-tui resize_uses_screen_delta_when_cursor_row_is_unchanged -- --nocapture`、`env -u RUSTC_WRAPPER cargo test -p codex-tui resize_uses_negative_screen_delta_when_cursor_row_is_unchanged_for_bottom_aligned_viewport -- --nocapture`、`env -u RUSTC_WRAPPER cargo test -p codex-tui resize_uses_screen_delta_when_cursor_query_fails_for_bottom_aligned_viewport -- --nocapture`、`env -u RUSTC_WRAPPER cargo test -p codex-tui resize_without_cursor_query_keeps_non_bottom_aligned_viewport_in_place -- --nocapture`、`env -u RUSTC_WRAPPER cargo test -p codex-tui resize_without_cursor_delta_keeps_non_bottom_aligned_viewport_in_place -- --nocapture`、`env -u RUSTC_WRAPPER cargo test -p codex-tui` 均通过。

## 下次遇到类似问题时
- 如果 resize bug 依赖 cursor row，先核对测试 backend 暴露的 cursor 坐标轴是否和真实 terminal API 一致，再相信测试结论。
- 对 inline viewport，不要把 cursor delta 当成唯一事实源；但 screen delta 回退也必须带前置条件，至少确认旧 viewport 已贴底，否则很容易重新引入错误的通用 bottom-align。
