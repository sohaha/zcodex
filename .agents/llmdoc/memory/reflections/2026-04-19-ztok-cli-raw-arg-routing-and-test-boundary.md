# ztok CLI 原始参数入口与测试边界

## 背景
- 在升级 RTK/ztok 基线时，`codex-rs/cli/tests/ztok.rs` 几乎整套失败，表面现象是 `codex ztok ...` 被顶层 `codex` Clap 当成未知子命令。
- 起初容易怀疑是新同步的 `json` / `vitest` 行为改坏了 CLI，但失败覆盖面过大，且所有用例都卡在同一层解析前。

## 事实
- `codex-rs/README.md` 已明确说明：shell 路由显示为逻辑命令 `codex ztok ...`，但实际要回到当前 `codex` 二进制内部的 ztok 入口。
- `codex-rs/arg0/src/lib.rs` 只负责准备 `ztok` alias path，不会把 `argv0=ztok` 直接分发到 `codex-ztok`。
- 当前分支的 `codex-rs/cli/src/main.rs` 也没有 `Subcommand::Ztok`。
- `codex-rs/ztok/src/lib.rs` 已提供稳定入口 `run_from_os_args(args)`，适合同时承接：
  - `argv0=ztok`
  - `codex ztok ...`

## 结论
- 这个运行面不是靠顶层 Clap 声明式子命令实现的，而是要在 `cli_main` 解析 `MultitoolCli` 之前，先检查原始 `args_os`，把 `ztok` alias 或显式 `codex ztok` 前缀转发到 `codex_ztok::run_from_os_args(...)`。
- 当 `codex ztok` 整套测试突然全部失败时，优先检查“原始参数入口是否还接着”，不要先在 `codex-ztok` 内部子命令上做局部补丁。

## 验证经验
- 整套 `cargo test -p codex-cli --test ztok` 可能仍会被仓库既有失败阻塞；这次修完入口后，只剩 `ztok_summary_preserves_non_zero_exit_code` 的既有断言失败。
- 对本任务更可靠的最小验证闭环是：
  - `just fmt`
  - `cargo test -p codex-cli --test ztok ztok_alias_routes_to_ztok_parser -- --exact`
  - `cargo test -p codex-cli --test ztok ztok_json_keys_only_hides_values -- --exact`
  - `cargo test -p codex-cli --test ztok ztok_help_exposes_codex_curated_command_surface -- --exact`
