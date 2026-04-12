# 远程 compaction 卡在 build_initial_context 的反思

## 背景
- 这轮同步先把 `run_turn()` / `built_tools()` 改成了 `list_all_tools_nonblocking()`，想解决 `codex_apps` MCP server 启动竞态导致的远程 compaction 卡住问题。
- 但 `codex-rs/app-server/tests/suite/v2/compaction.rs` 里的 `auto_compaction_remote_emits_started_and_completed_items` 仍然间歇性卡死：日志只走到 pre-sampling compact，后续既没有发 `/responses` 请求，也没有写出 context update 完成日志。

## 根因
- 真正阻塞点不在 `run_turn()` 采样前拿工具列表的主链路，而在 `record_context_updates_and_set_reference_context_item()` 触发的 `build_initial_context()`。
- 当 `turn_context.config.include_apps_instructions` 为真且当前 turn 开启 apps 时，`build_initial_context()` 会调用 `connectors::list_accessible_and_enabled_connectors_from_manager()`。
- 这个 helper 之前仍然直接走阻塞版 `mcp_connection_manager.list_all_tools().await`。
- 结果是：即使主 turn 已经切到 nonblocking MCP 枚举，只要 `codex_apps` 还在 startup 且没有 startup snapshot，context 构建阶段还是会被重新卡住。

## 这次修正
- 把 `codex-rs/core/src/connectors.rs` 里的 `list_accessible_and_enabled_connectors_from_manager()` 改成使用 `list_all_tools_nonblocking().await`。
- 保留现有语义：若 `codex_apps` 还没 ready 且没有 startup snapshot，就临时视为“当前无可见 connector”，避免 turn 构建阻塞。
- 同时删除为定位这个竞态临时加入的 `run_turn` / `regular task` tracing 噪音，保留真正需要的行为修复。

## 验证
- `cargo nextest run -p codex-mcp list_all_tools_nonblocking_skips_pending_clients_without_startup_snapshot`
- `cargo nextest run -p codex-core build_initial_context_`
- `cargo nextest run -p codex-app-server auto_compaction_remote_emits_started_and_completed_items` 连跑 3 次通过

## 下次怎么做
- 只要为了“避免 turn 阻塞”引入 nonblocking MCP tool 枚举，就要顺手检查：
  - `build_initial_context()`
  - connector/app instructions 构建
  - plugin/app mention 注入前的 inventory 获取
- 如果日志显示 pre-sampling compact 已完成、但 context update 之后的采样日志完全缺失，优先怀疑“初始上下文构建阶段仍有阻塞 IO”，不要只盯着采样请求链路。
