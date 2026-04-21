# TUI 默认值回归要覆盖最终加载路径，异步 AppEvent 测试不要假设首个事件就是目标事件

## 背景

这次 `codex-tui` 回归里，表面上有三类失败：

- `buddy_is_visible_by_default` 断言默认可见失败
- `history_lookup_response_is_routed_to_requesting_thread` 断言第一个 `AppEvent` 就是 history 响应失败
- `feedback_submission_for_inactive_thread_replays_into_origin_thread` 断言 replay 后的成功文案失败

它们看起来分散在配置、线程事件和回放文案三个点，但实际都属于“上游同步后，默认路径和测试事实源脱节”。

## 关键观察

### 1. 配置默认值不能只在 `config_toml` 层保持正确

`codex-rs/config/src/types.rs` 与 `ConfigToml` 里的 TUI buddy 默认值仍然是开启，但
`codex-rs/core/src/config/mod.rs` 在组装最终 `Config` 时把：

- `tui_show_buddy`
- `tui_buddy_reactions_enabled`

都写成了 `unwrap_or(false)`。

如果测试只覆盖 TOML 反序列化，默认值漂移会漏过去；必须至少有一条走
`Config::load_default_with_cli_overrides_for_codex_home(...)` 的端到端默认路径测试。

### 2. app 测试 helper 应返回尽量安静的通道，但这还不够

`make_test_app_with_channels()` 复用真实 `ChatWidget` 初始化链路，会带来启动期 `AppEvent` / `Op`
噪音。先在 helper 内部清空已到达的通道，可以减少“测试起点不干净”的偶发断言失败。

但如果初始化期间还有异步任务稍后发事件，仅靠构造后立刻 drain 还不够。

### 3. 异步事件断言不要假设“第一个收到的事件”就是目标事件

对 `history lookup` 这种后台任务回调，测试应在时限内筛出目标事件，而不是：

- `recv()` 一次
- 直接断言该事件类型

只要运行面允许并发异步事件，这种“首事件即目标事件”的测试就会在同步或初始化链路变化后变脆。

### 4. 文案回归要先确认源码是否已本地化，再决定修实现还是修测试

`feedback_submission_for_inactive_thread_replays_into_origin_thread` 最终不是 replay 断链，而是测试仍断言旧英文：

- 运行时 `feedback_success_cell()` 已输出中文成功文案
- app 测试还在匹配英文 `"Feedback uploaded..."`

这类失败先追实际文案源头，再决定是否更新测试，避免把“断言过期”误判成行为回归。

## 验证边界

这次更稳妥的最小验证闭环是：

- `env -u RUSTC_WRAPPER just fmt`
- `env -u RUSTC_WRAPPER cargo test -p codex-tui buddy_is_visible_by_default -- --exact --nocapture`
- `env -u RUSTC_WRAPPER cargo test -p codex-tui history_lookup_response_is_routed_to_requesting_thread -- --exact --nocapture`
- `env -u RUSTC_WRAPPER cargo test -p codex-tui feedback_submission_for_inactive_thread_replays_into_origin_thread -- --exact --nocapture`
- `env -u RUSTC_WRAPPER cargo check -p codex-core --lib`

如果 `codex-core` 的 `lib test` 仍被仓库既有 test compile 漂移阻塞，要把这种阻塞和本次默认值修复分开记录，不要把整个失败面都算到当前改动上。
