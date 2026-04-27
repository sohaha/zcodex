# upstream sync 后 CLI shared options 不能在 TUI 重复声明

## 背景
- 上游同步后，`codex` debug build 在 clap Command 构建阶段 panic：
  `Argument names must be unique, but 'oss_provider' is in use by more than one argument or group`。
- 根因是 `--local-provider` / `oss_provider` 已经存在于 `codex-rs/utils/cli/src/shared_options.rs` 的 `SharedCliOptions`，但本地 `codex-rs/tui/src/cli.rs` 仍保留了同名字段。
- 旧的 `local-fork-features.json` 基线还要求 `tui/src/cli.rs` 里存在 `pub oss_provider: Option<String>`，导致本地特性 gate 会错误地保护导致 panic 的坏状态。

## 结论
- 对 interactive CLI 参数，不能只检查“字段还存在”或“help 还能显示”。
- 如果参数已收敛到 `SharedCliOptions`，TUI 不应再次声明同名 clap 字段；子命令 merge 应通过 `shared.apply_subcommand_overrides(shared.into_inner())` 透传。
- `resume` / `fork` / `zoffsec resume` 这类复用 `TuiCli` 的子命令需要同时覆盖：
  - root / 子命令 help 不 panic
  - `Command::debug_assert()` 级别的重复参数防护
  - bridge 行为测试，确保 `provider` / `local-provider` 等参数不会在 merge 阶段丢失
  - `local_fork_feature_audit` 的权威基线指向正确参数所有者，而不是旧字段位置

## 后续规则
- 同步触及 `codex-rs/cli/src/main.rs`、`codex-rs/tui/src/cli.rs` 或 `codex-rs/utils/cli/src/shared_options.rs` 时，必须检查同一 clap arg id 是否被多个 flatten/struct 重复注册。
- 更新本地特性基线时，要表达“参数归属在哪里”和“如何透传”，不要把旧文件位置当成永久不变量。
- CLI 中文化同步后，完整 `cargo test -p codex-cli` 很容易继续暴露测试断言漂移；这类失败应按真实输出逐个修正，不能只修当前截图里的 panic。
