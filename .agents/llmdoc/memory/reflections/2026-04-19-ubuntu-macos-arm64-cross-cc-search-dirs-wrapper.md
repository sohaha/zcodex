# 2026-04-19 Linux 交叉构建 macOS arm64 需要兜住 PATH 里的 cc/c++

## 背景
- 用户反馈 `mise run build-ubuntu-macos-arm64` 失败，需要直接修到可验证的闭环。
- 现有 task 已经把 `CC_aarch64_apple_darwin`、`CXX_aarch64_apple_darwin` 指向 Zig wrapper，也默认打开 `realtime-webrtc-stub`，但真实构建仍会在 macOS 交叉链路里卡住或失败。
- 继续往下查后，问题不在 task 自己的 shell 流程，而在第三方依赖 `webrtc-sys` 的 `build.rs`。

## 这次确认的根因
- `webrtc-sys` 在 Apple 目标上会直接执行 `cc --print-search-dirs`，再把返回的 `libraries` 根路径拼成 `.../lib/darwin`，并通过 `cargo:rustc-link-search=` 注入后续链接。
- 在 Linux 主机上，如果 PATH 里的 `cc` 仍是宿主 clang/gcc，这里拿到的就是 `x86_64-linux-gnu` 搜索路径；拼接后会变成一长串错误的 host linker path，末尾甚至出现 `/usr/lib//lib/darwin`。
- 这说明只设置 target-specific 的 `CC_aarch64_apple_darwin` / `CXX_aarch64_apple_darwin` 不够；只要依赖直接 shell 出 `cc` / `c++`，就会绕过这些环境变量。

## 本轮有效做法
- 在 `.mise/tasks/build-ubuntu-macos-arm64` 的交叉 wrapper 目录里新增 PATH 优先级更高的 `cc` / `c++` wrapper：
  - 普通编译参数继续委托给现有 `zig-cc` / `zig-cxx`；
  - 遇到 `--print-search-dirs` 时，返回一个任务自己准备的目标感知 search root，而不是宿主机 clang/gcc 的路径。
- 同时在 wrapper 目录下生成一个 synthetic `compiler-rt/lib/darwin/libclang_rt.osx.a`，把 `webrtc-sys` 预期的 Apple compiler runtime 搜索形状补齐。
- 为 `.mise/tasks/tests/build-ubuntu-macos-arm64.sh` 增加回归：
  - fake `rs-ext` 主动调用一次 `cc --print-search-dirs`；
  - 断言输出不再包含 `x86_64-linux-gnu`；
  - 断言返回的 `libraries:` 根落在 task 自己准备的 compiler-rt 目录；
  - 断言 synthetic `libclang_rt.osx.a` 已经落地。

## 为什么要记这条
- 这是跨平台构建里很容易漏掉的一层：很多 `build.rs` 不走 Cargo target env，而是直接信任 PATH 里的编译器名字。
- 以后再修 Linux -> macOS、Linux -> iOS 或其他 Apple 交叉链路时，只检查 `CC_<target>` / `CXX_<target>` 不够；必须一并检查 PATH 上裸调用的 `cc` / `c++` 是否仍然泄露宿主机行为。

## 验证与边界
- 脚本级回归 `bash .mise/tasks/tests/build-ubuntu-macos-arm64.sh` 已通过。
- 这次用隔离 `CARGO_TARGET_DIR` 做真实复跑时，提前撞到一个与本修复无关的 `sqlx_macros` host proc-macro 动态链接错误，因此没走到完整 `webrtc-sys` 链接收尾。
- 非隔离的真实 release 构建在当前环境里仍长期停留在 `codex` 最终 LTO/链接阶段，这次没有等到整条链路跑完；因此本轮真实验证证据以“脚本回归 + 根因定位 + wrapper 行为修正”为主。
