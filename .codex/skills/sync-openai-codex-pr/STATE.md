# sync-openai-codex-pr state

- upstream_repo: https://github.com/openai/codex.git
- upstream_branch: main
- last_synced_sha: 5bb193aa88fef0f5ef3fbbd2c6253ba93d3f6521
- last_synced_at_utc: 2026-04-18T10:25:00Z
- last_synced_base_branch: web
- last_sync_commit: 62438362965c2e4c2ce9fb3b4e2bb5cf58e6d5d3
- notes: |
    空同步轮次：upstream SHA 5bb193aa88fef0f5ef3fbbd2c6253ba93d3f6521 与上次相同。
    
    discover 结果（70110f011..HEAD，24 本地提交，170 文件）：
    - 触及 4 个本地分叉特性：zconfig-layer-loading、responses-reasoning-content-strip、
      reference-context-reinjection-baseline、buddy-surface
    - 均为纯融合，本地分叉代码完整保留
    
    编译遗漏发现：
    - upstream 5bb193aa8 (#18382) 为 ModelInfo 新增 max_context_window 字段
    - 本地 fallback ModelInfo 构造（manager.rs、model_info.rs）未同步补上
    - 已修复：两处均加 max_context_window: None
    - 测试：models-manager 25/25 通过
    
    已存在 upstream 带入 bug：
    - app-server/src/bespoke_event_handling.rs:288 有 parse 错误
    - d53814c08 merge commit 带入，需后续修复
    
    验证：
    - 本地 check：11/11 通过
    - models-manager 测试：25/25 通过
    - 无需 worktree（upstream 未推进）
