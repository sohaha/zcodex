# inline resize 后 viewport 下移时要先按旧 origin 清屏

## 背景
- 任务：修复 `codex-cli` 在终端尺寸变化后出现的旧卡片残留与重复渲染。
- 现象：resize 后新的 viewport 已在更靠下的位置重绘，但旧 viewport 顶部附近的内容仍残留在屏幕上；用户截图里同一条消息会同时出现在旧位置和新位置。

## 结论
- 根因不在通用 diff buffer，而在 `codex-rs/tui/src/tui.rs` 的 pending viewport 重定位路径：
  - 当 `pending_viewport_area()` 让 viewport **向下移动**时，代码原先先 `set_viewport_area(new_area)`，再 `terminal.clear()`。
  - `terminal.clear()` 只会从**当前 viewport origin** 开始做 `ClearType::AfterCursor`，所以 old viewport 与 new viewport 之间那几行不会被清掉，旧内容就残留了。
- 修法不是扩大到全屏清理，而是按位移方向决定 clear 顺序：
  - viewport 下移：先按旧 origin `clear()`，再 `set_viewport_area(new_area)`。
  - viewport 上移或不变：维持先 `set_viewport_area()` 再 `clear()`，避免无谓扩大清理范围。

## 证据
- 代码路径：`codex-rs/tui/src/tui.rs` 中 resize draw 分支会先处理 `pending_viewport_area()`，然后才进入正常 `update_inline_viewport()`。
- 回归测试：`moving_viewport_down_clears_stale_rows_above_new_origin` 直接覆盖“viewport 下移后旧行必须被清掉”的场景。
- 验证：`env -u RUSTC_WRAPPER cargo test -p codex-tui moving_viewport_down_clears_stale_rows_above_new_origin -- --exact --nocapture` 与 `env -u RUSTC_WRAPPER cargo test -p codex-tui` 均通过。

## 下次遇到类似问题时
- 先判断是“新 frame 没画出来”还是“旧 frame 没被清掉”；截图里同时出现旧布局和新布局时，优先怀疑 clear origin 与 viewport 位移顺序。
- 检查所有 `clear()` / `invalidate_viewport()` 调用点时，不只看是否清了 buffer，也要看它们是从哪个屏幕坐标开始对真实终端发清理序列。
- 对 inline viewport，若位移后有区域离开 viewport，就要确认这些区域会被显式清理，而不是默认期待下一帧覆盖。
