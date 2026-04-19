# 2026-04-19 upstream sync 的 CLI 本地特性基线必须覆盖入口接线与 help 汉化

- 这轮排查 `zmemory` / `ztldr` / `ztok` 时，真正的回归不在 crate 本体，而在 `codex-rs/cli/src/main.rs` 的顶层 CLI surface：子命令注册、dispatch 以及 help 汉化哨兵被上游 `main.rs` 覆盖掉了。
- 根因不是同步技能“没有要求优先保留本地特性”，而是权威基线把这块定义得过粗：
  - 只保了 `native-tldr` / `zmemory` / `ztok` crate 目录和 workspace member / dependency
  - 没保 `Subcommand::Ztok` / `Subcommand::Tldr` / `Subcommand::Zmemory`
  - 没保 `run_tldr_command` / `run_zmemory_command`
  - 没保 `localize_help_output()`、`显示帮助` / `显示版本` 这类 CLI help 本地化哨兵
  - 没保 `resume` 的 `r` 短别名这类本地 CLI 体验细节

## 结论

- 对 upstream sync 来说，“本地特性优先保留”如果只落在 crate 存在性层面，是不够的。
- 只要本地分叉功能是通过顶层 `main.rs` / `Subcommand` / `dispatch` / help 输出暴露给用户，就必须把这层 surface 直接写进权威基线。
- `check` 的保护面至少应覆盖：
  - 顶层子命令是否仍注册
  - 对应实现是否仍 dispatch
  - help / localization 哨兵是否仍在
  - 本地别名和命令名改造（如 `ztldr`、`r`）是否仍在

## 后续规则

- 新增或审查 `local_surface` 特性时，不要只问“目录还在不在”，还要问“用户入口还在不在”。
- 如果某项本地能力的真实价值是 CLI 行为而不是 crate 目录，那么基线里的 checks 也必须直指 CLI 行为来源文件。
