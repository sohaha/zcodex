# sync-openai-codex-pr state

- upstream_repo: https://github.com/openai/codex.git
- upstream_branch: main
- last_synced_sha: 5591912f0bf176257f71b3efbd37ee4479dfdfaf
- last_synced_at_utc: 2026-04-26T07:29:20Z
- last_synced_base_branch: web
- last_sync_commit: b7cd33bf154f6f052652ab05968f06f45df333a9
- notes: |
    同步轮次：吸收 upstream a978e411f628529e0f7c4095a5b5389622fca9b4..5591912f0bf176257f71b3efbd37ee4479dfdfaf。

    本地特性基线：
    - local_fork_feature_audit 更新到 25 项权威特性
    - pending-input-routing-and-zmemory-recall 哨兵对齐当前 routing_inputs -> user_inputs 实现
    - request-fallback-provider-chain 已确认是本地分叉功能，恢复 fallback_provider/fallback_model/fallback_providers 实现、测试与 gate
    - js_repl 按 upstream 删除方向清理；主动实现、schema、CLI/TUI/app-server 暴露面无残留，仅保留 legacy replay/feature key 忽略测试字符串

    合并过程：
    - worktree 分支：sync/openai-codex-20260426-121054
    - merge openai/main 后清理冲突并恢复本地 ztok/zmemory/buddy/TUI/side thread/zoffsec 等保留面
    - 对齐 upstream 删除：移除 js_repl 工具实现、handler、docs、schema/config feature 暴露
    - 额外修复编译适配：TUI/app-server/core config 结构字段、rollout resume、thread goal 与工具调用上下文字段

    gate 与验证：
    - worktree local_fork_feature_audit check：24/24 通过
    - 当前分支 post-sync repair local_fork_feature_audit check：25/25 通过
    - worktree render：local-fork-features.md 已由 json 生成
    - worktree 上游删除反查 gate：无残留路径
    - worktree js_repl 主动面/schema grep：无输出
    - worktree diff --check：通过
    - cargo fmt：通过（stable toolchain 对 imports_granularity 打印 nightly-only warning）
    - 编译验证：`cargo check -p codex-core -p codex-tui -p codex-cli -p codex-app-server-protocol -p codex-app-server` 通过，有 warnings
    - post-sync repair 验证：`cargo test -p codex-core --no-run --message-format=short` 通过，有 warnings
    - post-sync repair 验证：`cargo test -p codex-core --test all websocket_fallback -- --nocapture` 通过，10/10
    - post-sync repair 验证：`just fmt` 与 `just fix -p codex-core` 通过（通过 `mise exec -- just ...` 调用）
    - post-sync repair 提交：a27e95ea2 fix: preserve fallback provider sync behavior
    - 环境限制：
      - 本轮未改 Cargo.toml/Cargo.lock，未运行 `just bazel-lock-update` / `just bazel-lock-check`
