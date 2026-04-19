# sync-openai-codex-pr state

- upstream_repo: https://github.com/openai/codex.git
- upstream_branch: main
- last_synced_sha: 996aa23e4ce900468047ed3ec57d1e7271f8d6de
- last_synced_at_utc: 2026-04-19T06:59:32Z
- last_synced_base_branch: web
- last_sync_commit: b7964491e8b2092356f6a7649bce274777a23994
- notes: |
    同步轮次：吸收 upstream 5bb193aa88fef0f5ef3fbbd2c6253ba93d3f6521..996aa23e4ce900468047ed3ec57d1e7271f8d6de。

    discover 结果（9bbf3a75d97a17ade5ad5258fb609dd73a1475e3..HEAD）：
    - 12 本地提交、81 文件
    - 触及 3 个已登记特性：models-manager-provider-overrides、buddy-surface、chinese-localization-sentinels
    - candidate ops 合并后为空；promote 未引入新权威特性定义

    合并过程：
    - worktree 分支：sync/openai-codex-20260419-144420
    - merge openai/main 时仅 1 处冲突：codex-rs/core/tests/suite/truncation.rs
    - 冲突按“保本地能力 + 吸收上游语义”处理：MCP image 输出断言改为 wall-time 文本项 + image 项（含 detail）
    - 额外修复编译适配：
      - codex-api InputImage 匹配补 `..`，兼容 upstream 新增 detail 字段
      - core/tests/suite/shell_command.rs 修复不闭合 if 块，恢复可编译状态

    gate 与验证：
    - promote/render/check（主分支前置）：11/11 通过
    - worktree check：11/11 通过
    - merge-back check：11/11 通过
    - just fmt：通过
    - 定向测试尝试：`RUSTC_WRAPPER= cargo test -p codex-core mcp_image_output_preserves_image_and_no_text_summary -- --nocapture`
      - 失败：仓库当前 `codex-core` 测试侧存在大量既有编译错误（config_tests / git_info_tests 等，163 errors），非本轮单点冲突文件可单独闭环解决
