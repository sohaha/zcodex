# QA 测试报告

## 报告信息
- **功能名称**：codex-cli-native-tldr
- **创建日期**：2026-03-25
- **状态**：阶段 3 进行中（daemon 接线已完成）

## 中间验证进度（实时）

- **当前执行方式**：本轮并行已整合完成（semantic MCP、daemon 生命周期、主线程复核）
- **最新代码提交**：`9c231e69d` `fix: harden native tldr daemon startup guards`

### 已完成验证
- `cargo test -p codex-cli --bin codex tests::tldr_structure_parses_language_and_symbol -- --exact`：通过（auto-start 变更后复核）
- `cargo test -p codex-cli --bin codex tests::tldr_daemon_ping_parses -- --exact`：通过（auto-start 变更后复核）
- `cargo test -p codex-native-tldr`：通过（当前 10 个测试，含 semantic indexer）
- `cargo test -p codex-mcp-server suite::codex_tool::test_tldr_tool_uses_daemon_when_available -- --exact`：通过
- `cargo test -p codex-mcp-server suite::codex_tool::test_tldr_tool_semantic_structured_content -- --exact`：通过
- `just fix -p codex-native-tldr`：通过
- `cargo test -p codex-mcp-server tldr_tool::tests::verify_tldr_tool_json_schema -- --exact`：通过
- `cargo test -p codex-mcp-server suite::codex_tool::test_tldr_tool_is_listed -- --exact`：通过
- `cargo test -p codex-mcp-server suite::codex_tool::test_tldr_tool_tree_falls_back_to_local_engine -- --exact`：通过
- `cargo test -p codex-mcp-server suite::codex_tool::test_tldr_tool_warm_returns_snapshot -- --exact`：通过
- `cargo test -p codex-mcp-server suite::codex_tool::test_tldr_tool_notify_includes_path -- --exact`：通过
- `cargo test -p codex-mcp-server suite::codex_tool::test_tldr_tool_snapshot_returns_snapshot -- --exact`：通过
- `just fix -p codex-mcp-server`：通过
- `just fmt`：通过
- `cargo check -p codex-native-tldr-daemon`：通过
- `cargo test -p codex-cli --bin codex tests::tldr_structure_parses_language_and_symbol -- --exact`：通过
- `cargo test -p codex-cli --bin codex tests::tldr_help_renders -- --exact`：通过
- `just fix -p codex-native-tldr-daemon`：通过
- `just fix -p codex-cli`：通过

### 当前验证结果
- `codex-native-tldr::daemon::query_daemon` 缺 socket 时返回 `None`，通过
- `query_daemon` Unix socket round-trip，`Ping -> pong`，通过
- `cargo test -p codex-cli --bin codex tests::tldr_daemon_ping_parses -- --exact`：通过
- `just fix -p codex-native-tldr`：通过
- `just fix -p codex-cli`：通过

### 当前下一批验证
- 评估 daemon 生命周期与外部进程启动/回收策略
- 深化 CLI/MCP 共用 daemon 生命周期管理
- 视情况补全量 `cargo test -p codex-mcp-server` 复跑

### 当前遗留验证
- `cargo test -p codex-mcp-server` 全量仍未复跑；已知历史遗留失败用例仍是 `test_shell_command_approval_triggers_elicitation`
- daemon auto-start 目前只在 Unix 路径启用，Windows 仍回退本地 engine
- `just argument-comment-lint` 仍因仓库缺脚本无法验证

### 已知环境问题
- `just argument-comment-lint` 失败：缺少 `./tools/argument-comment-lint/run-prebuilt-linter.sh`
- `just bazel-lock-check` 失败：缺少 `./scripts/check-module-bazel-lock.sh`
- 以上为仓库脚本缺失，非当前 native-tldr 逻辑回归
