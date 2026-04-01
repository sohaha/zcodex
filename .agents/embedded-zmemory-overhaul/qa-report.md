# QA 测试报告

## 报告信息
- **功能名称**：embedded-zmemory-overhaul
- **创建日期**：2026-03-30
- **更新日期**：2026-03-31
- **状态**：通过（M1 已闭环，M2 文档与治理说明已同步）

---

## 验证范围

- recall gate / root-subagent memory 协议
- `system://defaults` / `system://workspace` 事实视图
- CLI `zmemory export defaults|workspace`
- core tool spec / e2e 对 defaults-vs-workspace 语义的消费
- 默认项目库路径策略与显式全局路径覆盖
- 治理/桥接文档：README、config、architecture、QA 同步

## 执行记录

所有 Rust 验证均使用独立锁与独立目标目录：

```bash
export CARGO_HOME=/tmp/codex-cargo-home-zmemory-a3
export CARGO_TARGET_DIR=/tmp/codex-cargo-target-zmemory-a3
```

已通过：

```bash
cargo test -p codex-zmemory --quiet
cargo test -p codex-core --test all zmemory_prompt --quiet
cargo test -p codex-core --test all zmemory_ --quiet
```

2026-04-01 默认项目库策略补充验证：

```bash
cargo test -p codex-zmemory --quiet
cargo test -p codex-core --test all suite::zmemory_e2e:: --quiet
```

未通过但已确认与本次 `zmemory` 改动无关：

```bash
cargo test -p codex-cli export_uri_supports_defaults_and_workspace_views
```

- 失败原因：预存的 `codex-tui` 编译错误阻塞 `codex-cli` 构建（`tui/src/chatwidget.rs:5566-5567`），不是当前默认项目库改动引入

## 文档一致性检查

已人工核对并同步：

- `codex-rs/zmemory/README.md`
  - 补充 `system://defaults` / `system://workspace`
  - 补充 `export defaults` / `export workspace`
  - 明确“没有记忆” vs “可检索性不足”的判别路径
  - 记录旧节点 bridge 策略与治理入口
- `docs/config.md`
  - 补充 defaults/workspace 事实视图与验证命令
  - 说明 `dbPathDiffers` / `bootHealthy` / `defaultWorkspaceKey` 等 runtime facts 的用途
  - 明确默认 DB 现在是项目级路径，跨项目共享需显式配置 `[zmemory].path`
- `.agents/embedded-zmemory-overhaul/architecture.md`
  - 将 system view 从“增强方向”更新为“已落地合同”
  - 记录当前 bridge 是显式治理而非自动迁移
  - 记录默认路径已从旧的全局/工作区讨论收敛为项目级默认库
- `.agents/embedded-zmemory-overhaul/qa-report.md`
  - 回填验证结论与剩余风险

## 结论

- 当前 embedded zmemory 已能区分产品默认事实与当前 workspace 实际事实
- 默认 DB 已收敛为项目级路径 `$CODEX_HOME/zmemory/projects/<project-key>/zmemory.db`
- 若用户需要跨项目共享同一份库，必须显式配置 `[zmemory].path`
- boot / alias / disclosure / deprecated-or-orphaned memory 的治理信号已可通过 `stats` / `doctor` / `system://alias` / `system://workspace` 复核
- 旧节点 bridge 目前是可执行的人工治理流程，不是自动迁移；这与本里程碑边界一致

## 剩余风险

- 当前没有单独的 markdown lint / docs-check 自动化，本轮文档校验以人工一致性审查为主
- `codex-cli` 的定向验证仍会被与本任务无关的 `codex-tui` 编译错误阻塞；当前已确认 `codex-zmemory` 与 `codex-core` 相关验证通过
