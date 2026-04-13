# 2026-04-14 RTK 上游同步时全局 flag 收口与定向验证的反思

## 背景
- 这轮任务把本地嵌入的 ZTOK 基线从 upstream RTK `v0.31.0` 同步到 `v0.36.0`。
- 同步点同时覆盖 CLI 参数、wrapper 行为、若干命令的默认参数和 `cli/tests/ztok.rs` 回归用例。

## 这轮踩到的坑
- upstream 已移除了 ZTOK 全局短参数 `-u`，但本地除了版本测试外，还残留了 `first_subcommand_arg` 单测和 unknown-command fallback 集成测试把 `-u` 当作全局 flag，导致单测在 `first_subcommand_arg()` 上直接把 `-uvv` 识别成“首个子命令”。
- `cargo test -p codex-cli` 在当前仓库环境下会先失败在一个与本次同步无关的既有 lifecycle 用例 `tldr_cmd::lifecycle_tests::ensure_running_records_launcher_wait_in_two_process_race`，如果不拆分验证面，很容易误判成 RTK 同步引入的回归。
- 当前镜像环境仍会把 `CARGO_INCREMENTAL` / `RUSTC_WRAPPER` 注入到手写 cargo 命令里；若直接沿用默认环境跑检查，仍可能先撞到环境噪音而不是业务回归。

## 这轮有效做法
- 当 upstream 删除某个全局 flag 时，不要只改 help/version 用例；要同时全文检查解析辅助函数和“unknown command after global flags”这类 fallback 测试，确认没有把旧 flag 留在 parse matrix 里。
- 在 `cargo test -p codex-cli` 被既有非目标用例阻塞时，继续补跑与本次改动直接相关的两个闭环：
  - `cargo test -p codex-ztok`
  - `cargo test -p codex-cli --test ztok`
  这样可以把结论收敛到同步过的 wrapper 行为本身，而不是被无关 lifecycle 测试放大噪音。
- 手写 Rust 验证命令时，优先用 `unset RUSTC_WRAPPER CARGO_INCREMENTAL` 包住命令，再判断是真实编译/测试回归还是环境问题。

## 之后怎么避免
- 以后做 RTK/ztok 同步时，把“搜索残留 `-u` / 已删除全局 flag 的测试矩阵”当成固定 checklist 项，而不是只盯公开 help 输出。
- 若 `cargo test -p codex-cli` 失败，先判断失败是否落在当前改动链路；如果不是，明确记录为既有失败，并用受影响 crate/集成测试补齐本任务证据链。
