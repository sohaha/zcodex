# QA 测试报告

## 报告信息
- **功能名称**：codex-cli-native-tldr
- **创建日期**：2026-03-25
- **状态**：阶段 3 进行中（daemon 接线已完成）

## 中间验证进度（实时）

- **当前执行方式**：主线程已清理历史失败并继续收口 daemon 生命周期
- **最新代码提交**：`7773701e7` `fix: avoid duplicate native tldr daemon launches`（当前工作区还有未提交修复）

### 已完成验证
- `cargo test -p codex-cli --bin codex tldr_cmd::lifecycle_tests::query_daemon_with_hooks_retries_after_autostart -- --exact`：通过
- `cargo test -p codex-cli --bin codex tldr_cmd::lifecycle_tests::query_daemon_with_hooks_skips_retry_when_autostart_fails -- --exact`：通过
- `cargo test -p codex-cli --bin codex tests::tldr_help_renders -- --exact`：通过（daemon 子命令 auto-start 收口后复核）
- `cargo test -p codex-cli --bin codex tests::tldr_daemon_ping_parses -- --exact`：通过（daemon 子命令 auto-start 收口后复核）
- `cargo test -p codex-cli --bin codex tests::tldr_structure_parses_language_and_symbol -- --exact`：通过（公共 helper 抽取后复核）
- `just fix -p codex-cli`：通过（CLI daemon auto-start 收口后复核）
- `cargo test -p codex-mcp-server`：通过（历史失败已清理）
- `cargo test -p codex-mcp-server suite::codex_tool::test_shell_command_approval_triggers_elicitation -- --exact`：通过
- `cargo test -p codex-native-tldr`：通过（新增 stale socket 生命周期测试后复跑，共 11 个测试）
- `just fix -p codex-native-tldr`：通过（stale socket 清理后复核）
- `just fix -p codex-mcp-server`：通过（shell approval 测试修复后复核）
- `cargo test -p codex-cli --bin codex tests::tldr_structure_parses_language_and_symbol -- --exact`：通过（auto-start 变更后复核）
- `cargo test -p codex-cli --bin codex tests::tldr_daemon_ping_parses -- --exact`：通过（auto-start 变更后复核）
- `cargo test -p codex-mcp-server suite::codex_tool::test_tldr_tool_uses_daemon_when_available -- --exact`：通过
- `cargo test -p codex-mcp-server suite::codex_tool::test_tldr_tool_semantic_structured_content -- --exact`：通过
- `cargo test -p codex-mcp-server tldr_tool::tests::verify_tldr_tool_json_schema -- --exact`：通过
- `cargo test -p codex-mcp-server suite::codex_tool::test_tldr_tool_is_listed -- --exact`：通过
- `cargo test -p codex-mcp-server suite::codex_tool::test_tldr_tool_tree_falls_back_to_local_engine -- --exact`：通过
- `cargo test -p codex-mcp-server suite::codex_tool::test_tldr_tool_warm_returns_snapshot -- --exact`：通过
- `cargo test -p codex-mcp-server suite::codex_tool::test_tldr_tool_notify_includes_path -- --exact`：通过
- `cargo test -p codex-mcp-server suite::codex_tool::test_tldr_tool_snapshot_returns_snapshot -- --exact`：通过
- `just fmt`：通过
- `cargo check -p codex-native-tldr-daemon`：通过
- `cargo test -p codex-cli --bin codex tests::tldr_structure_parses_language_and_symbol -- --exact`：通过
- `cargo test -p codex-cli --bin codex tests::tldr_help_renders -- --exact`：通过
- `just fix -p codex-native-tldr-daemon`：通过
- `just fix -p codex-cli`：通过

### 当前验证结果
- `codex-native-tldr::daemon::query_daemon` 缺 socket 时返回 `None`，通过
- `query_daemon` Unix socket round-trip，`Ping -> pong`，通过
- `query_daemon` 遇到 stale socket 时会清理 socket 文件并返回 `None`，通过
- `codex tldr daemon` 子命令已复用 auto-start/重试路径，与 `structure/context` 行为对齐
- CLI `structure/context/daemon` 现在统一走 `query_daemon_with_autostart()` helper，daemon 生命周期入口一致
- CLI helper 已覆盖两条关键分支：auto-start 成功时会重试 query，auto-start 失败时不会多余重试
- MCP shell approval 历史失败已确认根因是当前环境 `bwrap` 无法创建 user namespace；测试已改为 `danger-full-access` sandbox 以验证 approval 流程本身
- `cargo test -p codex-cli --bin codex tests::tldr_daemon_ping_parses -- --exact`：通过
- `just fix -p codex-native-tldr`：通过
- `just fix -p codex-cli`：通过

### 当前下一批验证
- 本轮目标：继续补 daemon 生命周期与外部进程启动/回收策略
- 当前进行：补 CLI stale socket/auto-start 重试测试覆盖
- 后续：深化 CLI/MCP 共用 daemon 生命周期管理

### 当前遗留验证
- daemon auto-start 目前只在 Unix 路径启用，Windows 仍回退本地 engine
- `just argument-comment-lint` 仍因仓库缺脚本无法验证

### 已知环境问题
- `just argument-comment-lint` 失败：缺少 `./tools/argument-comment-lint/run-prebuilt-linter.sh`
- `just bazel-lock-check` 失败：缺少 `./scripts/check-module-bazel-lock.sh`
- 以上为仓库脚本缺失，非当前 native-tldr 逻辑回归
