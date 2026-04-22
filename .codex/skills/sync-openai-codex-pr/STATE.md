# sync-openai-codex-pr state

- upstream_repo: https://github.com/openai/codex.git
- upstream_branch: main
- last_synced_sha: a978e411f628529e0f7c4095a5b5389622fca9b4
- last_synced_at_utc: 2026-04-22T09:53:07Z
- last_synced_base_branch: web
- last_sync_commit: e45818606f628529e0f7c4095a5b5389622fca9b4
- notes: |
    同步轮次：吸收 upstream 996aa23e4ce900468047ed3ec57d1e7271f8d6de..a978e411f628529e0f7c4095a5b5389622fca9b4。

    discover 结果（996aa23e4ce900468047ed3ec57d1e7271f8d6de..HEAD）：
    - 0 本地提交、0 文件
    - candidate ops 合并后为空；promote 未引入新权威特性定义

    合并过程：
    - worktree 分支：sync/openai-codex-20260422-095307
    - merge openai/main 时无冲突
    - 额外修复编译适配：
      - 补齐 `codex-rs/login` 的 `AgentIdentityAuthRecord` 导出与持久化逻辑
      - 补齐 `codex-rs/core/Cargo.toml` 的 `crypto_box`、`sha2` 依赖
      - 补齐 `codex-rs/core/src/session/session.rs` 的 `agent_identity_manager` 初始化
      - 补齐 `codex-rs/login/src/server.rs` 的 `agent_identity: None` 初始化

    gate 与验证：
    - promote/render/check（主分支前置）：17/17 通过
    - worktree check：17/17 通过
    - merge-back check：17/17 通过
    - just fmt：通过
    - 编译验证：`cargo check -p codex-core` 通过
    - 环境限制：
      - `sccache` 缺失，Rust 命令需 `env RUSTC_WRAPPER= ...`
      - `bazel` 缺失，无法本地权威刷新/校验 `MODULE.bazel.lock`
      - `codex-login` 测试超时，未跑完整测试套件
