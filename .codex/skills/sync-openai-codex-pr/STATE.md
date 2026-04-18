# sync-openai-codex-pr state

- upstream_repo: https://github.com/openai/codex.git
- upstream_branch: main
- last_synced_sha: 5bb193aa88fef0f5ef3fbbd2c6253ba93d3f6521
- last_synced_at_utc: 2026-04-18T10:23:00Z
- last_synced_base_branch: web
- last_sync_commit: 9bbf3a75d97a17ade5ad5258fb609dd73a1475e3
- notes: |
    空同步轮次：upstream SHA 5bb193aa88fef0f5ef3fbbd2c6253ba93d3f6521 与上次相同。
    
    discover 结果（70110f011..HEAD，24 本地提交，170 文件）：
    - 触及 4 个本地分叉特性：zconfig-layer-loading、responses-reasoning-content-strip、
      reference-context-reinjection-baseline、buddy-surface
    - 当轮 check 11/11 通过，但后续审计发现 coverage 仍有盲区
    
    后续审计发现的同步遗漏：
    - upstream 5bb193aa8 (#18382) 为 ModelInfo 新增 max_context_window 字段
    - 本地 fallback ModelInfo 构造（manager.rs、model_info.rs）未同步补上
    - 已修复：两处均加 max_context_window: None
    - Buddy runtime 接线在冲突处理时被漏掉：app.rs / app_event.rs / bespoke_event_handling.rs
    - 已修复：恢复 Buddy event arms、AppEvent 变体与通知桥接
    - 这两类问题说明当轮基线没有覆盖 synthetic ModelInfo 构造和 Buddy 运行时桥接

    验证：
    - 本地 check：11/11 通过
    - models-manager 测试：25/25 通过
    - 无需 worktree（upstream 未推进）
    
    状态修正：
    - last_sync_commit 必须保持指向最近一次真正落地的 sync merge commit 9bbf3a75d97a17ade5ad5258fb609dd73a1475e3
    - b475adb7b 是空同步轮次的状态补记，不是新的 upstream 落地锚点
    - e2960cfd9 / fc7a5e229 / d231927bc 属于空同步后的本地修复与状态补记，不应覆盖 discover 的默认起点
