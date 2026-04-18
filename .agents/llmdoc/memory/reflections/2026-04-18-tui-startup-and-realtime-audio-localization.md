# TUI 启动链路与实时音频文案汉化反思

## 背景

这次改动从 `codex-rs/tui/src/lib.rs` 开始，随后顺着真实用户可见链路继续补齐了两批残留英文：

1. 启动/恢复链路：`lib.rs`、`resume_picker.rs`
2. 实时音频链路：`app_event.rs`、`audio_device.rs`、`chatwidget.rs`、`chatwidget/realtime.rs`、`app.rs`

同时更新了对应 snapshot：

- `tui/src/chatwidget/snapshots/codex_tui__chatwidget__tests__realtime_*`
- `tui/src/snapshots/codex_tui__resume_picker__tests__resume_picker_{table,thread_names}.snap`

## 关键观察

### 1. TUI 汉化需要按“用户链路”收，不要按文件名收

最开始只看 `lib.rs`，很容易把任务理解成“翻掉当前文件里的报错字符串”。

但真正暴露给用户的是一条链路：

- `lib.rs` 的错误和退出提示
- `resume_picker.rs` 的会话选择 UI
- `chatwidget.rs` / `app.rs` 驱动的实时音频选择与重启提示
- `app_event.rs` / `audio_device.rs` 提供的设备名称与底层错误

如果只改入口文件，不顺着调用链往下收口，就会出现：

- 弹窗标题是中文，但选项还是 `Microphone`
- 主提示是中文，但错误里仍是 `failed to enumerate input audio devices`
- 会话表格大部分中文，但列头仍是 `CWD`

这类“半汉化”比纯英文更显得不一致。

### 2. `RealtimeAudioDeviceKind` 这种共享枚举应优先统一术语源头

实时音频相关 UI 同时依赖：

- `kind.title()`
- `kind.noun()`

如果只在某个渲染点手工翻译，其他提示、错误和快照还会继续带英文。

更稳的做法是先把共享术语源头改成中文，再补那些依赖英文语法拼接、在中文下读起来别扭的句子，比如：

- `实时 {} 已设置为 ...`
- `立即重启本地{}音频`

也就是：

1. 先改术语源头
2. 再改受影响的句子结构

### 3. snapshot 可以只更新当前任务真正触达的文件

这次没有做全仓 snapshot 清扫，而是只更新：

- 实时音频弹窗相关 snapshot
- `resume_picker` 当前仍在使用的表格/线程名 snapshot

旧的 `resume_picker_screen.snap` 虽然仍含英文，但对应测试当前是注释状态，不属于这次最小闭环必须项。处理 TUI 中文化时，优先清理“代码仍会执行、测试仍会跑”的 UI 面。

## 验证边界

### 已验证

- `rustfmt` 已对本次修改的 Rust 文件单独执行
- `RUSTC_WRAPPER= cargo check -p codex-tui --lib` 通过

### 未完成验证

- `just fmt` / 工作区级 `cargo fmt` 无法完成
  - 原因：`codex-rs/core/tests/suite/shell_command.rs` 当前存在未闭合分隔符，和本次改动无关
- `RUSTC_WRAPPER= cargo test -p codex-tui --lib --no-run -j 1` 无法完成
  - 原因：`codex-tui` 现有测试里 `memories_settings_toggle_saves_on_enter` 重复定义，和本次改动无关

## 后续建议

1. 继续做 TUI 汉化时，先按用户链路分批，而不是按目录机械扫文件。
2. 遇到 `title()` / `label()` / `noun()` 这类共享显示术语时，优先从源头统一。
3. 在测试恢复可编译之前，snapshot 只手动维护当前任务直接触达且仍在执行的用例，避免把无关遗留问题和本次文案改动混在一起。
