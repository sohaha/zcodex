# 2026-04-12 上游同步后 ToolName 迁移的验证回路反思

## 背景
- 这轮同步把 upstream 的 `ToolName` 结构、namespaced MCP tool 行为和 guardian / slash dispatch 变更一起带入了本地分叉。
- 冲突解决后，最主要的回归面集中在 `codex-core` 的工具路由、code-mode、MCP tool call，以及依赖 CLI 二进制的 `cli_stream` 集成测试。

## 这轮踩到的坑
- 当前环境里直接沿用镜像注入的 `CARGO_INCREMENTAL` / `RUSTC_WRAPPER=sccache` 跑 `cargo`，会撞上 `sccache: incremental compilation is prohibited`，导致看起来像编译问题，其实只是环境变量组合不兼容。
- `cargo nextest run -p codex-core` 只跑 core package 时，`cli_stream` 测试里的 `codex_utils_cargo_bin::cargo_bin("codex")` 不一定能拿到 `codex` 二进制；先单独 `cargo build -p codex-cli --bin codex` 后，这组测试即可恢复。
- 把 `codex-core`、`codex-tui`、`codex-app-server*` 等 crate 混在一个高并发 nextest 命令里跑时，少量依赖 RMCP/plugin ready 的 core 用例会被 nextest 默认 30 秒单 case timeout 误杀；逐包串行或定向复测可以证明它们并非功能回归。

## 这轮有效做法
- 直接用 `env -u CARGO_INCREMENTAL -u RUSTC_WRAPPER ...` 包住手写的 `cargo build` / `cargo nextest run`，把问题先收敛到真实编译或测试逻辑。
- 在 `codex-core` 包级 nextest 里，若失败集中在 `cli_stream::*` 且报 `NotFound { name: "codex" }`，先执行 `cargo build -p codex-cli --bin codex`，再复跑失败用例。
- 对只在组合 nextest 中超时、但定向复测能通过的 core 用例，按 runner 超时记录，而不是继续在业务逻辑层叠补丁。

## 之后怎么避免
- 做 upstream sync 或共享工具层迁移时，优先按 crate 串行验证，而不是先把多个重包混进同一条 nextest 命令。
- 只要看到 `cargo_bin("codex")` 相关失败，就先确认 `codex` 可执行是否已构建，而不是误判成 ToolName 或 MCP 行为坏了。
- 若后续继续使用“手写 cargo 命令 + 当前镜像环境”的回路，把 `env -u CARGO_INCREMENTAL -u RUSTC_WRAPPER` 当作默认前缀。
