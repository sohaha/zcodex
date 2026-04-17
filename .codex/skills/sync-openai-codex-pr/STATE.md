# sync-openai-codex-pr state

- upstream_repo: https://github.com/openai/codex.git
- upstream_branch: main
- last_synced_sha: 92cf90277d4f3bcdee8457047c710db58d9fc715
- last_synced_at_utc: 2026-04-17T21:11:50Z
- last_synced_base_branch: web
- last_sync_commit: 6e9f21aca65025e25d4ac49c2015a8ffdf9fd2a6
- notes: |
    完整合并上游 openai/main (92cf90277) 到 web。包含：
    - Session/Codex 模块拆分（codex.rs → session/*.rs）
    - model-provider crate 新增
    - config filesystem abstraction 重构
    - Remote thread store
    - config aliases
    - PermissionRequest hooks
    - tool_search 动态工具搜索
    - apply_patch 流式支持
    - Guardian -> Auto-Review 重命名
    - glob deny-read sandbox 策略
    - 保留所有本地特色：WireApi streaming、中文汉化、buddy、zmemory、ztok、codex-api、URL替换
