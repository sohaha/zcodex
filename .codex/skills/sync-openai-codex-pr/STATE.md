# sync-openai-codex-pr state

- upstream_repo: https://github.com/openai/codex.git
- upstream_branch: main
- last_synced_sha: 3895ddd6b1caf80cd77d6fd44e3ce55bd290ef18
- last_synced_at_utc: 2026-04-12T21:09:09Z
- last_synced_base_branch: web
- last_sync_commit: 02959687a
- notes: 已在独立 worktree `sync/openai-codex-20260412-210909` 完成对 `openai/main@3895ddd6b1caf80cd77d6fd44e3ce55bd290ef18` 的 merge-parent 型同步，并保留本地中文化、buddy 命令与分叉能力；同步中补齐了 upstream 的 ToolName / namespaced MCP tools / guardian timeout / slash dispatch 等更新。验证上，受影响 crate 已分别通过 nextest；组合跑 core+tui+app-server 等包时有 9 个 codex-core 用例触发 nextest 默认 30 秒超时，但逐包串行与定向复测均通过，确认属于并发压测下的 runner 超时而非功能回归。
