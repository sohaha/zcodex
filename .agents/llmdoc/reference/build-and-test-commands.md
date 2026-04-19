# 构建与测试命令参考

## 范围
- 记录当前仓库最常用、最稳定的构建、格式化、lint、测试与 schema 命令。

## 常用命令
- `mise run dev-tools`：补齐本地 Rust 开发工具。
- `just fmt`：统一 Rust 格式化。
- `just fix -p <crate>`：对目标 crate 跑 clippy fix。
- `just core-test-fast`：`codex-core` 快速回路。
- `just app-server-test-fast`：`codex-app-server` 快速回路。
- `just native-tldr-test-fast`：`codex-native-tldr` 快速回路。
- `just test`：全 workspace nextest 回路。
- `mise run test <crate>`：通过项目包装器运行指定 crate 测试。

## 生成类命令
- `just write-config-schema`：刷新 `codex-rs/core/config.schema.json`。
- `just write-app-server-schema`：刷新 app-server schema fixture。
- `just bazel-lock-update`：依赖变化后更新 `MODULE.bazel.lock`。
- `just bazel-lock-check`：本地校验 Bazel lockfile 没漂移。
- `just argument-comment-lint`：检查 Rust 位置参数注释约定。

## 稳定事实
- 根目录 `justfile` 默认 `working-directory := "codex-rs"`，所以多数命令会自动在 Rust workspace 内运行。
- `justfile` 为多个 cargo 流程分配独立 `CARGO_HOME` 和 `CARGO_TARGET_DIR`，是仓库官方避免锁竞争的做法。
- 常规本地开发优先局部测试；只有确实需要时再跑 `just test` 或 Bazel 全量。
- 若直接手写 `cargo build` / `cargo nextest run`，在当前镜像环境优先加 `env -u CARGO_INCREMENTAL -u RUSTC_WRAPPER`，避免 `sccache: incremental compilation is prohibited` 这类环境噪音。
- 包级运行 `cargo nextest run -p codex-core` 时，如果 `cli_stream` 集成测试报 `codex_utils_cargo_bin::cargo_bin("codex")` 找不到二进制，先补 `cargo build -p codex-cli --bin codex` 再复跑。
- Clouddev 使用 Rust 工具链镜像时，不要把 `/root/.local/bin` 或 `/root/.local/share/mise` 挂成 `copy-on-write`；这会遮住镜像里预装的 `mise`、`lnk` 和对应工具链。
- `mise run build` 共享的 speed-first 构建脚本会在 `RUSTC_WRAPPER` 指向可选 `sccache` 但二进制缺失时显式取消 wrapper，避免半初始化环境直接卡死在 `sccache ... rustc -vV`。
- `mise run build-ubuntu-macos-arm64` 这类 Apple 交叉构建不能只设置 `CC_aarch64_apple_darwin` / `CXX_aarch64_apple_darwin`；有些第三方 `build.rs`（例如 `webrtc-sys`）会直接调用 PATH 上的 `cc --print-search-dirs`，所以任务脚本还需要兜住裸 `cc` / `c++` 并提供目标感知的 compiler-rt 搜索根。

## 事实来源
- `justfile`
- `mise.toml`
- `docs/install.md`
- `docs/contributing.md`
