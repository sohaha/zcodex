# QA 测试报告

## 报告信息
- **功能名称**：codex-cli-native-tldr
- **创建日期**：2026-03-25
- **状态**：阶段 3 进行中（warm/reindex、跨进程 launcher 竞争、以及 semantic 索引缓存闭环已补齐）

## 中间验证进度（实时）

- **当前执行方式**：主线程已把 semantic phase-1、warm/reindex 实际执行闭环、以及跨进程 launcher 竞争测试一起落地
- **最新代码提交**：`29822e3ed` `feat: cache native tldr semantic indexes`

### 已完成验证
- `codex rtk cargo test -p codex-native-tldr`：通过（35 个测试；新增 engine 级 semantic cache/reindex 回归）
- `codex rtk cargo test -p codex-cli --bin codex`：通过（47 个测试；CLI semantic 输出接线保持通过）
- `codex rtk cargo test -p codex-mcp-server`：通过（28 个测试；MCP semantic `embeddingUsed` / `embedding_score` 断言通过）
- `just fmt`：通过
- `just fix -p codex-native-tldr`：通过
- `just fix -p codex-cli`：通过
- `just fix -p codex-mcp-server`：通过
- `just argument-comment-lint`：失败（当前仓库 `Justfile` 无此 recipe）

- `cargo test -p codex-native-tldr`：通过（35 个测试；含 `TldrEngine` 复用 `SemanticIndex` 缓存直到 reindex 的回归）
- `cargo test -p codex-cli --bin codex`：通过（47 个测试）
- `cargo test -p codex-mcp-server`：通过（28 个测试；含 semantic `embeddingUsed` / `embedding_score` 断言）
- `cargo test -p codex-native-tldr`：通过（31 个测试；含 warm->reindex report 回归）
- `cargo test -p codex-cli --bin codex`：通过（47 个测试；含跨进程 launcher 竞争测试）
- `cargo test -p codex-cli --bin codex tldr_cmd::lifecycle_tests::ensure_daemon_running_only_spawns_once_across_processes -- --exact --nocapture`：通过
- `cargo test -p codex-mcp-server`：通过（28 个测试；warm/status structuredContent 已兼容 reindexReport）
- `cargo test -p codex-native-tldr`：通过（31 个测试；含 semantic embedding text/ranked matches 回归）
- `cargo test -p codex-cli --bin codex`：通过（44 个测试）
- `cargo test -p codex-mcp-server`：通过（28 个测试；含 semantic enabled 匹配返回）
- `cargo test -p codex-native-tldr`：通过（22 个测试；含 daemon health reason/hint 与 stale cleanup 断言）
- `cargo test -p codex-cli --bin codex`：通过（41 个测试；含 stale cleanup 在 lock-held 时不误删 metadata）
- `cargo test -p codex-mcp-server`：通过（27 个测试；含 ping success/missing-daemon 回归）
- `cargo test -p codex-cli --bin codex tldr_cmd::lifecycle_tests::cleanup_stale_daemon_artifacts_keeps_files_while_lock_is_held -- --exact -q`：通过
- `cargo test -p codex-mcp-server suite::codex_tool::test_tldr_tool_ping_reports_status -- --exact -q`：通过
- `cargo test -p codex-mcp-server suite::codex_tool::test_tldr_tool_ping_errors_when_daemon_missing -- --exact -q`：通过
- `cargo test -p codex-native-tldr daemon::tests::daemon_health_marks_stale_socket_without_live_pid -- --exact -q`：通过
- `cargo test -p codex-native-tldr daemon::tests::daemon_health_reports_lock_hint_when_lock_is_held -- --exact -q`：通过
- `just fix -p codex-native-tldr`：通过
- `just fix -p codex-cli`：通过
- `just fix -p codex-mcp-server`：通过
- `just argument-comment-lint`：失败（仓库缺少 `./tools/argument-comment-lint/run-prebuilt-linter.sh`）
- `cargo test -p codex-native-tldr daemon::tests::daemon_health_marks_stale_socket_without_live_pid -- --exact -q`：通过
- `just fix -p codex-native-tldr-daemon`：通过（shared config 接入 daemon 入口后复核）
- `cargo test -p codex-native-tldr tests::daemon_status_reports_config_and_reindex_state -- --exact -q`：通过
- `cargo test -p codex-cli --bin codex tests::tldr_daemon_status_parses -- --exact -q`：通过
- `cargo test -p codex-mcp-server suite::codex_tool::test_tldr_tool_status_returns_daemon_status -- --exact -q`：通过
- `just fix -p codex-native-tldr`：通过（status/session 可观测性变更后复核）
- `just fix -p codex-cli`：通过（CLI status 接线后复核）
- `just fix -p codex-mcp-server`：通过（MCP status 接线后复核）
- `cargo test -p codex-native-tldr`：通过（含 daemon lock query 新测试，共 16 个测试）
- `cargo test -p codex-mcp-server tldr_tool::tests -- --nocapture`：通过（MCP shared lifecycle 4 条单测）
- `cargo test -p codex-mcp-server suite::codex_tool::test_tldr_tool_uses_daemon_when_available -- --exact`：通过（MCP daemon 可用路径回归）
- `cargo test -p codex-cli --bin codex tldr_cmd::lifecycle_tests::query_daemon_with_hooks_retries_after_autostart -- --exact -q`：通过（CLI lifecycle 回归）
- `just fix -p codex-cli`：通过（launcher lock-aware 变更后复核）
- `just fix -p codex-mcp-server`：通过（MCP shared lifecycle 接线后复核）
- `cargo test -p codex-native-tldr`：通过（新增 daemon lock path 后复跑，共 15 个测试）
- `cargo test -p codex-cli --bin codex tests::tldr_daemon_ping_parses -- --exact`：通过（daemon 进程级 lock 接入后复核）
- `just fix -p codex-native-tldr`：通过（daemon lock 变更后复核）
- `cargo test -p codex-native-tldr lifecycle::tests -- --nocapture`：通过（shared lifecycle manager 3 条单测）
- `cargo test -p codex-cli --bin codex tldr_cmd::lifecycle_tests -- --nocapture`：通过（CLI 生命周期 4 条单测）
- `just fix -p codex-native-tldr`：通过（shared lifecycle 抽取后复核）
- `just fix -p codex-cli`：通过（CLI 切 shared manager 后复核）
- `cargo test -p codex-native-tldr`：通过（新增 pid path/hash 测试后复跑，共 12 个测试）
- `cargo test -p codex-cli --bin codex tldr_cmd::lifecycle_tests::daemon_metadata_requires_live_pid_and_socket -- --exact`：通过
- `cargo test -p codex-cli --bin codex tldr_cmd::lifecycle_tests::cleanup_stale_daemon_artifacts_removes_socket_and_pid -- --exact`：通过
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
- `cargo test -p codex-native-tldr`：通过（31 个测试；含 semantic embedding text / ranked matches / disabled gate）
- `cargo test -p codex-cli --bin codex`：通过（45 个测试，当时基线）
- `cargo test -p codex-mcp-server`：通过（全量复跑成功；另定点验证两条 semantic 用例）
- `cargo test -p codex-mcp-server suite::codex_tool::test_tldr_tool_semantic_structured_content -- --exact --nocapture`：通过
- `cargo test -p codex-mcp-server suite::codex_tool::test_tldr_tool_semantic_returns_matches_when_enabled -- --exact --nocapture`：通过
- `just fix -p codex-native-tldr`：通过（semantic phase-1 落地后复核）
- `just fix -p codex-cli`：通过（CLI semantic 接线后复核）
- `just fix -p codex-mcp-server`：通过（MCP semantic 接线后复核）
- `cargo fmt --all`：通过（`just fmt` 先因 `tldr_cmd.rs` 语法错误暴露问题，修正后手动格式化完成）
- `just argument-comment-lint`：失败（仓库仍缺少 `./tools/argument-comment-lint/run-prebuilt-linter.sh`）

### 当前验证结果
- semantic search 现已围绕显式 `SemanticIndex` build/query 边界运行；同一 `TldrEngine` 后续查询会复用缓存，直到 `semantic_reindex()` 重建
- daemon 每个连接现复用共享 `TldrEngine`，不会再因为新建默认 engine 而丢失配置或缓存状态
- CLI JSON / 文本输出与 MCP structuredContent 均已显式暴露 `embeddingUsed`，MCP e2e 还断言了 `embedding_score`
- semantic disabled 路径现在返回显式启用提示，不再冒充“已启用但未实现”
- `SemanticIndex` 现已成为明确的 build/query 边界；同一 `TldrEngine` 内首次查询后会缓存对应语言索引，`semantic_reindex()` 会重建并替换缓存
- daemon 连接处理现复用共享 `TldrEngine`，不再在每个 socket 连接里重建默认 engine 丢失项目配置/缓存
- CLI 与 MCP semantic 输出现在显式包含 `embeddingUsed`，MCP e2e 还校验了 `matches[*].embedding_score`
- semantic enabled 路径现在会扫描对应语言源码，返回 ranked matches、embedding-unit metadata 与 preview snippet
- CLI 与 MCP 都走统一 `engine.semantic_search(...)`，避免两端结果结构继续漂移
- MCP `tldr` 文档已补 `status` action 与 semantic 输出字段，代码/文档状态重新对齐
- launcher stale 清理已改为“只在未持锁且确认 stale 时清理”，相关 CLI 生命周期测试已通过
- daemon health/status 的 `health_reason` / `recovery_hint` 已在 native-tldr、CLI、MCP 路径验证通过
- MCP 已补齐 `ping` 成功 structuredContent 和 daemon missing 错误路径
- `codex-native-tldr::daemon::query_daemon` 缺 socket 时返回 `None`，通过
- `query_daemon` Unix socket round-trip，`Ping -> pong`，通过
- `query_daemon` 遇到 stale socket 时会清理 socket 文件并返回 `None`，通过
- daemon 进程启动后会为每个 project 写 pid file，退出时清理 pid file
- `codex tldr daemon` 子命令已复用 auto-start/重试路径，与 `structure/context` 行为对齐
- CLI `structure/context/daemon` 现在统一走 `query_daemon_with_autostart()` helper，daemon 生命周期入口一致
- CLI helper 已覆盖两条关键分支：auto-start 成功时会重试 query，auto-start 失败时不会多余重试
- CLI 现在要求 socket + pid 同时有效才认定 daemon 存活，stale artifact 清理也会同时删除 pid
- MCP shell approval 历史失败已确认根因是当前环境 `bwrap` 无法创建 user namespace；测试已改为 `danger-full-access` sandbox 以验证 approval 流程本身
- `cargo test -p codex-cli --bin codex tests::tldr_daemon_ping_parses -- --exact`：通过
- `just fix -p codex-native-tldr`：通过
- `just fix -p codex-cli`：通过

### 当前开发内快照
- semantic phase-1 已不再是 placeholder：native-tldr 现会为函数/类型级别块生成最小 embedding text，并按 query 做本地打分
- semantic 输出当前仍是 lightweight lexical ranking，而非真正 embedding / ANN 检索；这是有意保留的 phase-1 范围
- stale/liveness/lock phase-1 闭环继续收紧：launcher 现在不会在 lock-held 场景误删 socket/pid
- daemon status 已可向 CLI/MCP 给出更具体的恢复提示，便于排查“等待已有 daemon”与“需要清理 stale metadata”两类场景
- 已新增 native-tldr 专用上游同步技能：`.codex/skills/sync-native-tldr-reference/`
- 最小 shared config 已通过 `project/.codex/tldr.toml` 接入 CLI / daemon / MCP
- daemon health/status 现已显式暴露 `healthy` / `stale_socket` / `stale_pid`
- `native-tldr` daemon 现已支持 `Status` 命令，开始暴露配置摘要、锁状态、pid/socket 存活与 reindex pending
- `codex-native-tldr` 新增 `lifecycle.rs`，开始承载共用 query-retry / launch dedupe / backoff
- `codex-cli` 已删除本地重复生命周期状态实现，切到 shared manager
- `native-tldr` daemon 启动路径已开始持有 project 级 lockfile，降低跨独立进程重复拉起多个 daemon 的概率
- `codex-cli` launcher 已能感知 project lock：别的进程正在启动 daemon 时，本进程先等待 daemon 就绪，不再立即重复 spawn
- `mcp-server` 已接到 shared lifecycle manager，但仍不承担 auto-start，只等待外部 daemon 就绪后重试 query
- `just fmt` 与本轮定向测试已完成；`just argument-comment-lint` 仍受仓库缺脚本阻塞

### 当前下一批验证
- 本轮目标：把 semantic phase-1 与 lifecycle phase-1 同步收口到可稳定回归的状态
- 当前进行：继续补 daemon 外部进程启动路径与 semantic/daemon 协同的端到端覆盖
- 后续：把 daemon 生命周期状态判断/自动启动统一到单一抽象，并继续把进程级锁从“降低概率”推进到“更强唯一性保证”

### 当前遗留验证
- daemon auto-start 目前只在 Unix 路径启用，Windows 仍回退本地 engine
- shared lifecycle manager 已覆盖 CLI + MCP，但跨独立进程场景仍未做到严格“全局唯一启动”保证
- `just argument-comment-lint` 仍因仓库缺脚本无法验证

### 已知环境问题
- `just argument-comment-lint` 失败：缺少 `./tools/argument-comment-lint/run-prebuilt-linter.sh`
- `just bazel-lock-check` 失败：缺少 `./scripts/check-module-bazel-lock.sh`
- 以上为仓库脚本缺失，非当前 native-tldr 逻辑回归
