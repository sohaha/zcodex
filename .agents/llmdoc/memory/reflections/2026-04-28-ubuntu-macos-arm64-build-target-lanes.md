# 2026-04-28 Ubuntu -> macOS arm64 构建需要隔离 Cargo target lane

## 背景
- 用户运行 `mise run build-ubuntu-macos-arm64` 后，先遇到 SDK metadata 解析阶段的 `141` 退出；修复后再次运行，后续命令卡在 `Blocking waiting for file lock on artifact directory`。
- 当时进程表里仍有被中断后遗留的 `bash /workspace/.mise/tasks/build-ubuntu-macos-arm64`、`cargo build -p codex-cli --bin codex ... --target aarch64-apple-darwin` 和 `sccache ... rustc --crate-name codex`。
- 后续新构建复用同一个命令形状和同一个 target slot，自然会等待旧 Cargo artifact lock。

## 根因
- `build-ubuntu-macos-arm64` 的默认 target base 是 `/workspace/.cargo-target-cross-macos-arm64`，`rs-ext` 再按命令形状生成稳定 slot。
- 这对缓存复用有利，但交叉 release 构建的最终 LTO 很慢；一旦会话中断但后台 Cargo 进程还活着，后续同命令会进入完全相同的 slot 并阻塞。
- 只修 SDK 下载或 wrapper 不能解决这个锁等待；锁等待的根因是默认 build lane 过于稳定。

## 修正
- `.mise/tasks/build-ubuntu-macos-arm64` 在未显式设置 `CODEX_CARGO_LANE` 时，默认生成 `macos-arm64-<timestamp>-<pid>`。
- `rs-ext` 仍使用既有 slot/target-dir 解析机制，但会把这次构建放进新的 lane，例如 `.cargo-target-cross-macos-arm64/macos-arm64-.../<slot>/...`。
- 用户若确实要复用固定 lane，仍可显式设置 `CODEX_CARGO_LANE`；若显式设置 `CARGO_TARGET_DIR` / `CODEX_CARGO_TARGET_DISABLE`，原有覆盖语义不变。

## 验证
- `bash -n .mise/tasks/build-ubuntu-macos-arm64 .mise/tasks/tests/build-ubuntu-macos-arm64.sh .mise/tasks/test` 通过。
- `bash .mise/tasks/tests/build-ubuntu-macos-arm64.sh` 通过，并断言默认 lane 包含 `macos-arm64-`。
- `mise run test build` 通过，且聚合入口现在包含 `build-ubuntu-macos-arm64.sh`。

## 经验
- 对这种慢速交叉 release 构建，默认命令不应复用同一 artifact lock 域；编译缓存可以交给 `sccache`，Cargo artifact target lane 则应偏向隔离。
- 看到 `Blocking waiting for file lock on artifact directory` 时，不要优先改 Rust 代码；先查是否有同 target-dir 的后台 `cargo build` / `rustc` / `sccache` 进程，以及命令是否落在同一 `CODEX_CARGO_LANE`。
