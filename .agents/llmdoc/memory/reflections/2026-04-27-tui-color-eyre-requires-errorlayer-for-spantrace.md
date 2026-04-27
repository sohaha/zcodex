# TUI 的 color_eyre 报告必须配套 tracing_error::ErrorLayer

## 现象
- TUI 报错时出现 `Backtrace omitted`、`SpanTrace capture is Unsupported`，并提示检查 `tracing-error ErrorLayer` 和 semver 兼容性。

## 根因
- `codex-rs/tui/src/lib.rs` 已调用 `color_eyre::install()`，但初始化的 `tracing_subscriber::registry()` 只挂了文件日志、feedback、state db 和 otel layer，没有接入 `tracing_error::ErrorLayer`。
- `color-eyre` 默认会尝试在报告里捕获并渲染 `SpanTrace`；缺少 `ErrorLayer` 时，`SpanTrace::status()` 固定退化为 `UNSUPPORTED`，于是用户只能看到降级提示，而不是当前 span 上下文。

## 修复
- 在 TUI 的 subscriber 链追加 `tracing_error::ErrorLayer::default()`。
- 增加独立 integration test，用 `SpanTrace::capture().status() == SpanTraceStatus::CAPTURED` 直接锁住这个初始化契约，避免被现有 `lib test` 噪音掩盖。

## 经验
- 这类问题不要靠禁用 span trace 或吞掉提示来“修表象”；真正的修法是把 `ErrorLayer` 接到和 `color_eyre` 同一条 subscriber 链上。
- 当 crate 现有 `#[cfg(test)]` 单测面已经有大量基线编译噪音时，优先把新的回归保护落成独立 integration test，再用 `cargo check --lib` 补生产代码编译证据。
