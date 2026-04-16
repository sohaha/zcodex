# sync-openai-codex-pr state

- upstream_repo: https://github.com/openai/codex.git
- upstream_branch: main
- last_synced_sha: bc969b6516f2fb7a4817b665ab1507f4f4beedc2
- last_synced_at_utc: 2026-04-16T09:30:00Z
- last_synced_base_branch: web
- last_sync_commit: a7e3f8d6e
- notes: 从 sync worktree 合并回 web。Cherry-pick 了 3 个上游 commit (7579d5ad7 memories API, 9402347f3 memories menu, bc969b651 dismiss stale requests)。保留本地 Buddy、memories、zmemory 和中文化。编译验证通过 (cargo check --workspace 0 errors)。
- 本轮额外处理:
  - 还原中文 README.md (上游覆盖)
  - 还原中文 slash_command.rs (上游覆盖)
  - 解决 memories_settings_view.rs 中 18 个合并冲突 (保留中文)
  - 汉化 chatwidget.rs 新增上游消息 (计划/子代理/记忆相关 UI 常量)
  - 汉化 history_cell.rs 新增上游消息 (审批/会话历史)
  - 替换所有 openai/codex → sohaha/zcodex (install.sh, install.ps1, announcement_tip.toml, update_action.rs, codex-pr-body SKILL.md)
  - 更新 update_prompt_modal 快照 URL
  - 更新 session_header_indicates_yolo_mode 快照汉化
