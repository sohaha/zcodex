# 2026-04-19 launcher-agnostic ztok 验证应优先使用 tests/all 精确集成用例

- 这轮 launcher 无关改造里，`codex-core --lib` 的指定验证入口并不能代表本次改动是否正确，因为当前仓库的 `lib test` 面已经被大量无关漂移阻断。
- 真正和本次改动同链路的证据在 `core/tests/all.rs` 聚合面：
  - `suite::shell_command::*` 能验证 embedded ZTOK 的逻辑展示命令是否仍可用且不再写死 `codex`。
  - `suite::pending_input::*` 能验证初始上下文里的 Embedded ZTOK developer instructions 和相关 snapshots 是否随模板更新同步。
  - 需要时再用 `suite::tldr_e2e::*`、`suite::zmemory_e2e::*` 检查共享 tool 列表和运行时接线。

## 结论

- 对 `codex-core` 这类共享运行时改动，若 `cargo test -p codex-core --lib ...` 被仓库既有漂移卡住，不要把 issue 长期留在“实现完成但无法验证”。
- 先确认：
  - 生产代码能过 `cargo check -p codex-core --lib`
  - 真正承载当前行为的 `tests/all` 精确用例存在并可运行
- 然后改用 `cargo test -p codex-core --test all <full-test-name> -- --exact` 取得同链路证据。

## 这轮证据

- `suite::shell_command::shell_command_ignores_invalid_ripgrep_config_path_from_parent_env` 通过，说明 ztok 路由后的逻辑展示命令已允许任意 launcher 前缀。
- `suite::shell_command::shell_command_with_login_ignores_invalid_ripgrep_config_path_from_parent_env` 通过，说明 login=true 分支同样保持 launcher 无关。
- `suite::pending_input::queued_inter_agent_mail_triggers_follow_up_after_reasoning_item`、`suite::pending_input::queued_inter_agent_mail_triggers_follow_up_after_commentary_message_item`、`suite::pending_input::user_input_does_not_preempt_after_reasoning_item` 通过，说明 Embedded ZTOK prompt 与 snapshots 已一致收敛到 `CODEX_SELF_EXE` 表达。

## 后续规则

- 以后遇到“共享运行时代码正确，但 `lib test` 被仓库旧债阻断”的场景，优先寻找 `tests/all` 的精确集成用例，而不是把验证入口绑定死在 `--lib`。
- 当快照只因为字符串转义或模板字面变化失败时，先确认行为链路仍然正确，再定向更新受影响 snapshot，避免把快照失败误判为功能回归。
