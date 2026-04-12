# 如何在 `codex-rs` 做常规改动

## 前置条件
- 先读 `.agents/llmdoc/startup.md` 与相关架构文档。
- 先读 `.ai` 基础记忆；必要时用项目记忆召回脚本补充上下文。
- 进入实现前，优先查看目标 crate 的 `README.md`、`Cargo.toml` 和相邻测试。

## 主要步骤
1. 用根目录 `justfile` 或 `mise` 进入既有工作流，不手写新的本地套路。
2. 如果本地工具缺失，优先运行 `mise run dev-tools`。
3. 改动保持在任务相关 crate 和文件内；若发现公共能力更适合小 crate，不要默认堆进 `codex-core`。
4. 变更涉及 schema、lockfile、app-server fixture 或配置派生产物时，使用仓库既有生成命令同步更新。

## 验证
1. 先跑受影响 crate 的局部测试，优先用仓库提供的 fast wrapper。
2. Rust 改动完成后运行 `just fmt`；较大改动在结束前再跑 `just fix -p <crate>`。
3. 若是 TUI 用户可见改动，补 snapshot 并审阅 pending snapshots。
4. 触及共享区域时，再决定是否扩到全量测试。

## Snapshot 边界
- 在 dirty monorepo 里，不要从 `codex-rs/` 根目录直接执行 `cargo insta accept`，除非你已经确认整个工作区的 pending snapshots 都属于当前任务。
- 先把 pending snapshot 的范围收紧到目标目录；如果工具不能按路径安全收敛，就手动只接收目标 `*.snap.new`。

## 常见失败点
- 忽略 `justfile` 已经封装的独立 `CARGO_HOME`/`CARGO_TARGET_DIR`，导致并发锁竞争。
- 把项目范围配置问题误判为默认值问题，没有先看 `system://workspace`。
- 在 `codex-core` 继续叠加实现，导致边界进一步模糊。
- 在 dirty worktree 中直接 `cargo insta accept`，误把其他模块的 pending snapshots 一并接收。
- `codex-core` 里依赖 MCP tool ready 的测试如果只在 `nextest` 30 秒单 case timeout 下失败，先用 `cargo test -p codex-core --test all suite::... -- --exact --nocapture` 复跑，区分 runner 超时和真实回归。

## 相关文档
- `.agents/llmdoc/architecture/rust-workspace-map.md`
- `.agents/llmdoc/architecture/memory-and-doc-systems.md`
- `.agents/llmdoc/reference/build-and-test-commands.md`
