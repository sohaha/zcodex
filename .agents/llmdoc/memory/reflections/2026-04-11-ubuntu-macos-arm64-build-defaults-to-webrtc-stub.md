# 2026-04-11 Linux 交叉构建 macOS arm64 默认切 realtime-webrtc-stub 的反思

## 背景
- 用户执行 `mise run build-ubuntu-macos-arm64` 失败，需要直接修到可验证通过。
- 原始失败并不是 Rust 源码逻辑，而是 `codex-cli` 的 `aarch64-apple-darwin` 链接阶段在 Linux 主机上炸掉。
- 具体表现为 `libwebrtc_sys` 和 `libv8` 同时参与 Mach-O 链接时，都带入 Abseil 的 `arg.o`，`ld64.lld` 报大量 duplicate symbol。

## 本轮有效做法
- 先复现并锁定真正阻塞点：`SupportedLanguage::Kotlin` 的 `match` 缺臂只是早期一次性编译错误，当前主问题是交叉链接链路。
- 继续尝试“保留完整 WebRTC”的链接修补没有形成稳定结果；`-multiply_defined,suppress` 仍然压不住 `libwebrtc_sys` 与 `libv8` 的 Abseil 冲突。
- 将修复收敛到 `.mise/tasks/build-ubuntu-macos-arm64`：
  - Linux 交叉构建默认启用 `realtime-webrtc-stub`；
  - 保留显式覆盖口，用户仍可通过 `CODEX_UBUNTU_MACOS_ARM64_FORCE_WEBRTC_STUB=0` 强制尝试完整 WebRTC 构建；
  - 让最终链接继续走原有 `clang` + SDK 路线，避免 Zig 直接驱动最终链接时出现 SIGSEGV。

## 为什么这样修
- 这个 task 的目标是“在 Ubuntu/Linux 上稳定产出 macOS arm64 二进制”，优先级高于在 Linux 交叉环境里保留原生 WebRTC 能力。
- `codex-cli` 已经提供 `realtime-webrtc-stub` feature，并且 `build-ubuntu-macos-arm64` 之前就支持通过环境变量启用，说明仓库已经接受“在某些构建面用 stub 换可构建性”的策略。
- 相比继续在链接器参数上堆补丁，默认切到 stub 是最小且可验证的闭环。

## 关键收益
- `mise run build-ubuntu-macos-arm64` 在当前 Linux 环境下恢复可用，成功产出 `/workspace/dist/codex-aarch64-apple-darwin`。
- 构建脚本保留显式日志，用户能看见这次构建默认使用了 stub，而不是静默降级。
- 默认行为只影响 Linux -> macOS arm64 交叉构建，不波及其他本地构建入口。

## 剩余边界
- 产出的 macOS arm64 二进制默认不包含完整 realtime WebRTC 实现；如果后续必须恢复完整能力，需要单独解决 `libwebrtc_sys`/`libv8` 在 Mach-O 交叉链接下的 Abseil 重复符号问题。
- 当前日志里仍有 `codex-app-server` dead_code 与 `codex-tui` unreachable pattern 警告，它们不影响这次构建闭环，但不属于本次修复范围。

## 后续建议
- 如果未来要恢复完整 WebRTC 交叉构建，不要再优先试图用 `-multiply_defined,suppress` 之类参数硬压；应直接从依赖产物层处理 Abseil 重复对象来源。
- 对所有跨平台 `mise run build-*` 任务，优先明确“功能完整性”和“可交付构建产物”的优先级，再决定是否默认使用 stub/compat feature。
