# 2026-04-10 build 在缺少 sccache 时自愈的反思

## 背景
- 用户在 Clouddev 容器里执行 `mise run build`，构建还没真正开始就失败在 `sccache /root/.rustup/.../rustc -vV`。
- 当前环境里 `RUSTC_WRAPPER=sccache` 是由 `.cnb.yml` 预设的，但 `PATH` 上并没有 `sccache`，所以 Cargo 直接把不存在的 wrapper 当成可执行文件调用。
- `.mise/tasks/build` 原先只会在 `RUSTC_WRAPPER` 为空时补齐 `sccache` 路径；一旦环境里已经有了失效的 `sccache` 值，脚本就不会再修正。

## 本轮有效做法
- 先确认不是 Rust 工具链或 `rustc` 本身丢失，而是 wrapper 层先炸掉：`RUSTC_WRAPPER=sccache`、`command -v sccache` 为空、Cargo 报错是 wrapper 不存在。
- 把修复收敛到 `.mise/tasks/lib-speed-first-build`，让所有 source 这个库的构建入口在配置 speed-first 缓存时统一归一化 `RUSTC_WRAPPER`。
- 只把“可选的 sccache wrapper”当成可自愈对象：
  - `RUSTC_WRAPPER` 为空且 `sccache` 可用时，补成绝对路径；
  - `RUSTC_WRAPPER` 是 `sccache` 或指向 `sccache` 的失效路径，但当前环境没有可用 `sccache` 时，打印显式日志并 `unset RUSTC_WRAPPER`；
  - 其他自定义 wrapper 保持原样，不擅自吞掉用户显式配置。

## 关键收益
- `mise run build` 在 Clouddev 或其他半初始化环境里不再因为缺少 `sccache` 直接失败，而是自动退回普通 `rustc` 构建。
- `build`、`build-ubuntu-linux-amd64`、`build-ubuntu-macos-arm64`、`build-ubuntu-win-amd64`、`build-ubuntu-win-arm64` 共用同一套修复，不需要在各脚本重复补丁。
- 当 `sccache` 后续可用时，脚本仍会优先使用它，不影响 speed-first 构建链路。

## 踩坑
- 看到 `RUSTC_WRAPPER` 已经有值时，不能默认认为它有效；在容器/Clouddev 场景里，环境变量和实际二进制可用性可能短暂失配。
- 这个问题发生在 Cargo 真正编译前的 `rustc -vV` 探测阶段，所以表面上像是工具链坏了，实际上是 wrapper 抢先失败。
- 自愈逻辑要只覆盖仓库默认的可选 `sccache` 场景；如果把任意失效 wrapper 都静默清掉，会掩盖用户的显式自定义错误。

## 后续建议
- 以后再遇到 “could not execute process `sccache ... rustc -vV`” 这类错误，先同时检查 `RUSTC_WRAPPER` 和 `command -v sccache`，不要先去怀疑 Rustup 或 Cargo。
- 继续把可选加速层的容错收敛在共享脚本里，避免某个 build 入口自愈、另一个入口继续保留相同坑位。
