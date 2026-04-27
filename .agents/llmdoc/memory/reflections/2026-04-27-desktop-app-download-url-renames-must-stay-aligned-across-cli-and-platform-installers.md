# Desktop app 下载 URL 改名必须同时更新 CLI、平台分发器和平台实现

## 现象

- `mise run build-ubuntu-macos-arm64` 在 `codex-tui` 修完一个过期 `UpdateAction::StandaloneUnix` 分支后，继续在 `codex-cli` 失败。
- 具体报错集中在桌面 app 打开/安装链路：
  - `cli/src/app_cmd.rs` 仍引用不存在的 `DEFAULT_CODEX_DMG_URL`
  - `AppCommand` 已把字段命名成 `download_url`，但调用点还在读 `cmd.download_url_override`
  - `cli/src/desktop_app/mac.rs` 函数签名保留了 `download_url_override: Option<String>`，实现体却直接使用未定义的 `download_url`

## 根因

- 这是一次未收口的半截重命名：CLI 参数层试图把“可选覆盖 URL”改成普通字符串默认值，但平台分发器和 macOS 具体实现仍保留旧的 `Option<String>` 语义。
- 这类问题在本机 Linux 默认构建路径里不一定立刻暴露，但一旦跑 `build-ubuntu-macos-arm64` 这种会真正编到 `codex-cli` 的交叉构建任务，就会在后半程炸掉。

## 修正

- `cli/src/app_cmd.rs` 把 `download_url` 恢复为 `Option<String>`，维持“仅在显式传参时覆盖”的接口语义。
- macOS/Windows 入口统一直接透传 `cmd.download_url`。
- `cli/src/desktop_app/mac.rs` 在运行时根据宿主是否 Apple Silicon 选择默认 DMG URL，并仅在用户显式传入时覆盖默认值。
- 顺带删掉 `tui/src/updates.rs` 中已经失效的 `UpdateAction::StandaloneUnix` 匹配分支，避免交叉构建被更早的编译错误挡住。

## 经验

- 只要 CLI 参数、平台分发器和平台实现之间共享一个“覆盖值/默认值”契约，字段改名或语义改动就必须整条链路一起核对，不能只改最外层 struct。
- 对这类跨平台入口，`mise run build-ubuntu-macos-arm64` 不是单纯发包命令，而是能暴露宿主构建不常走到的编译路径；做完 CLI/TUI 入口调整后，值得把它当成一个高价值回归面。
