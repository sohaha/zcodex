## 背景
- 排查 `codex -P glm -m glm-5.1` 的 TUI 启动慢时，先前已经修掉了自定义 provider `/models` 刷新兼容问题。
- 新日志显示 `thread/start` 主路径只剩约 `214ms`，但体感仍慢。

## 观察
- `codex-rs/tui/src/app.rs` 会在首帧调度后继续触发若干启动期后台请求；其中 `skills/list` 量级较小，但 `account/rateLimits/read` 可能因为 ChatGPT 后端或 Cloudflare 挑战拖到数秒。
- 这条 `account/rateLimits/read` 并不是会话启动必需，只是为了让第一次 `/status` 更快显示限额缓存。
- `codex-rs/tui/src/lib.rs` 还会在进入 TUI 前触发 `tooltips::announcement::prewarm()`，命中 `raw.githubusercontent.com`；而公告 tip 本身只是 best-effort，启动通常在它 2 秒超时之前就已经走完，因此这条预热很容易落成纯启动噪音。

## 结论
- 当 `thread/start` 已经收敛后，启动优化要把“首帧前阻塞”和“首帧后预取造成的慢体感”分开看，不能继续把锅都甩给 session init。
- 启动期的非必需预取应默认延后到首帧之后，或改成用户显式触发时再拉取；不要为了缓存热身把高延迟远端请求继续留在 startup 邻近路径。

## 本次处理
- 去掉 `RateLimitRefreshOrigin::StartupPrefetch`，不再在 TUI 启动时自动请求 `account/rateLimits/read`；限额只在用户执行 `/status` 时刷新。
- 把 `tooltips::announcement::prewarm()` 从 `run_ratatui_app()` 早期移到 `App::run()` 里首帧调度之后，避免 `raw.githubusercontent.com` 预热继续挤进最早启动阶段。

## 可复用经验
- 日志里看到 `thread/start` 很快但启动仍慢时，继续看：
  - 首帧后的后台 RPC
  - 只为 UI 缓存预热的远端请求
  - 与启动文案/tooltip/插件列表有关的 best-effort 网络预热
- 这类路径如果不影响“能否进入可交互状态”，应优先后移，而不是继续在 session/core 里做重构。
