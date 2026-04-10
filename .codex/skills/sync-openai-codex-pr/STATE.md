# sync-openai-codex-pr state

- upstream_repo: https://github.com/openai/codex.git
- upstream_branch: main
- last_synced_sha: 7bbe3b6011ca66a20260a3a352cca1e98f532c4f
- last_synced_at_utc: 2026-04-10T12:46:09Z
- last_synced_base_branch: web
- last_sync_commit: c0bf2bc3d6cd075393572978c9af6dddea3a5ed8
- notes: 同步 openai/codex main (1de0085..7bbe3b6)，本次实际增量为 1 个上游提交 `Add output_schema to code mode render (#17210)`；冲突集中在 `tool_registry_plan*`，保留本地 `zmemory`/`ztldr`/`request_user_input` 工具注册与现有单工具描述行为，同时吸收上游 code-mode exec prompt 的 `outputSchema` 渲染与相关测试更新，之后已合并回 `web`。
