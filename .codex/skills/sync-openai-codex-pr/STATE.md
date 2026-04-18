# sync-openai-codex-pr state

- upstream_repo: https://github.com/openai/codex.git
- upstream_branch: main
- last_synced_sha: 5bb193aa88fef0f5ef3fbbd2c6253ba93d3f6521
- last_synced_at_utc: 2026-04-18T09:56:14Z
- last_synced_base_branch: web
- last_sync_commit: 70110f01165b80c2e45943bb5e3fcdba5f008ab1
- notes: |
    完整合并上游 openai/main (5bb193aa88fef0f5ef3fbbd2c6253ba93d3f6521) 到 web。
    
    本次同步范围：
    - 上游基线：92cf90277..5bb193aa88fef0f5ef3fbbd2c6253ba93d3f6521 (1253 个提交)
    - 变更文件：168 个，新增 10057 行，删除 1539 行
    
    主要变更内容：
    - Python SDK: 新增 SDK runtime foundation 和生成类型 (v2 schemas)
    - 修复 tmux/Linux 环境下 getpwuid 并发崩溃
    - 为 feedback 表单添加 ChatGPT user ID 日志
    - Plugin API: 新增 forceRemoteSync 参数
    - App-server protocol: 新增 WarningNotification、SendAddCreditsNudgeEmail
    - Rate limits: 新增 granular metrics
    - TUI: 插件市场增强 (新增 tab、marketplace entry)
    - Bazel: 更新 rules_rs patches
    
    冲突处理：
    - bespoke_event_handling.rs: 融合 Warning 事件（上游）+ Buddy 事件（本地）
    - app-server/lib.rs: 上游逻辑 + 中文文案
    - tui/app.rs: 上游逻辑 + 中文文案
    - plugins.rs: 采纳上游（新增 import 和常量）
    - trust_directory.rs: 中文文案
    - Snapshot/测试文件: 采纳上游
    - CLI main.rs: 采纳上游（新增 plugin marketplace 测试）
    
    本地分叉特性审查：
    - 11/11 特性全部保持完好
    - 无功能丢失或覆盖
    - 无"更好的等效替换"
    
    验证：
    - Worktree 审查：11/11 通过
    - Merge-back gate: 11/11 通过
    - 未发现功能回归
