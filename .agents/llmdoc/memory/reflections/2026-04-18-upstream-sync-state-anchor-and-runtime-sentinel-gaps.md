# 2026-04-18 上游同步的 state 锚点必须指向落地 sync 提交，local-surface 也要检查运行时桥接

## 背景

- 2026-04-18 的空同步轮次把 `STATE.md:last_sync_commit` 改成了一个没有落地到当前分支的 SHA。
- 同一轮之后又暴露了两类漏检：
  - `models-manager` 的本地 synthetic / fallback `ModelInfo` 构造没有跟上 upstream 新增字段 `max_context_window`
  - Buddy 的运行时桥接在 `app.rs` / `app_event.rs` / `app-server/src/bespoke_event_handling.rs` 中被冲突解决漏掉，但 `check` 仍然 11/11 通过

## 结论

- `STATE.md:last_sync_commit` 不能写“最后一个碰过同步相关文件的提交”，只能写“当前分支真正落地的 sync 提交”。
- 如果 upstream SHA 没有推进，空同步轮次继续保留上一次真实落地的 sync 提交；不要改成空同步状态提交、后续本地修复提交，更不能写临时 worktree / sync 分支上未落地的 SHA。
- `local_surface` / `localized_behavior` 不能只靠模块存在性或文案哨兵；如果真实行为依赖事件桥接、配置落盘、app-server notification 映射，这些链路也必须进基线检查。
- upstream 给共享 struct 新增字段时，不能只看主路径编译；只要本地还有 synthetic / fallback 构造，就要额外 grep 同类型构造点补齐字段。
- 如果同步触及 `protocol/src/error.rs`，不能只看枚举层 diff；`is_retryable()`、`to_codex_protocol_error()` 和 turn 级自动重试调用方要一起看，否则很容易把“错误分类”和“自动重试策略”改出互相矛盾的语义。

## 后续做法

- 修正 `STATE.md:last_sync_commit` 为真正落地的 sync 提交，再让下一轮 `discover` 从那里开始。
- 在 `local-fork-features.json` 中为 Buddy 补上 `app.rs` / `app_event.rs` / `bespoke_event_handling.rs` 的运行时检查点。
- 在 `local-fork-features.json` 中为 `models-manager-provider-overrides` 补上 synthetic `ModelInfo` 的字段完整性检查。
- 在技能和 checklist 中明确写出：
  - `last_sync_commit` 的落地语义
  - 共享 struct 增字段时的 synthetic constructor 审计要求
  - `protocol/error.rs` 变更时的 retryable / protocol mapping / turn 调用方联动审查
