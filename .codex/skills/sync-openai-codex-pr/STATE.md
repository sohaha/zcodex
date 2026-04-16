# sync-openai-codex-pr state

- upstream_repo: https://github.com/openai/codex.git
- upstream_branch: main
- last_synced_sha: bc969b6516f2fb7a4817b665ab1507f4f4beedc2
- last_synced_at_utc: 2026-04-16T09:30:00Z
- last_synced_base_branch: web
- last_sync_commit: a7e3f8d6e
- notes: 从 sync worktree 合并回 web。Cherry-pick 了 3 个上游 commit (7579d5ad7 memories API, 9402347f3 memories menu, bc969b651 dismiss stale requests)。保留本地 Buddy、memories、zmemory 和中文化。编译验证通过 (cargo check --workspace 0 errors)。
