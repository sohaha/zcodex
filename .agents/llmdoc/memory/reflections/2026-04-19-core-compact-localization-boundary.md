# core 压缩链路汉化边界反思

## 背景

本次任务从 `codex-rs/core/src/compact.rs` 的汉化开始，随后沿着同一条“上下文压缩”链路检查了：

- `compact_remote.rs`
- `session/turn.rs`
- `session/handlers.rs`
- 相关测试与快照归一化辅助

中途发现 `core/templates/compact/prompt.md` 与 `summary_prefix.md` 虽然也是英文，但它们属于内部模板，不应按 UI 文案处理。

## 关键结论

### 1. 先区分“用户提示”与“内部模板”

`core/src` 里的字符串并不都属于同一层：

- `WarningEvent`、`notify_stream_error`、任务错误前缀这类字符串会直接进入用户可见事件流，适合汉化。
- `core/templates/compact/*.md` 属于内部提示模板，尤其是交接压缩 prompt 和 summary prefix，会参与模型内部压缩语义，不应因为界面汉化就一起翻译。

如果不先分层，最容易把模板文件和 UI 文案一起改掉，造成任务边界误判。

### 2. 压缩链路要按调用路径收口，不要只改单文件

只改 `compact.rs` 会留下同一体验里的残余英文：

- 本地压缩的重连提示在 `compact.rs`
- 通用采样请求的重连/回退提示在 `session/turn.rs`
- 远程压缩错误前缀在 `compact_remote.rs`
- 发起压缩时的合成输入注释在 `session/handlers.rs`

这类文案应按“同一用户链路”一起看，否则会出现同功能一半中文、一半英文。

### 3. 当前仓库测试面存在与本任务无关的系统性阻塞

这次验证里：

- `RUSTC_WRAPPER= cargo check -p codex-core --lib` 通过
- `cargo test -p codex-core` 被 `core/src/config/config_tests.rs` 等现有测试错误阻塞
- `cargo test -p codex-core --test all` 也被大量现有集成测试 API 漂移阻塞

因此在 `core` 做局部文案改动时，应优先先拿到 `--lib` 编译证据，再明确区分“本次改动失败”与“仓库已有测试面失配”。

## 后续建议

1. 后续做 `core` 汉化时，先把目标限定为用户可见事件流，不要默认翻译模板目录。
2. 若任务起点是某个单文件文案，继续顺着同一功能链路扫描 `WarningEvent`、`ErrorEvent`、`notify_*` 调用点。
3. 在 `codex-core` 当前测试面未恢复前，把 `RUSTC_WRAPPER= cargo check -p codex-core --lib` 作为局部改动的最小可靠验证，再补充说明被哪些现有测试阻塞。
