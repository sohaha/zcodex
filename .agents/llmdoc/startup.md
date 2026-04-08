# 启动

每次进入当前仓库，按顺序阅读：

1. `.agents/llmdoc/must/project-basics.md`
2. `.agents/llmdoc/must/engineering-guardrails.md`
3. `.agents/llmdoc/must/doc-routing.md`

出现以下情况时，继续升级阅读：

- 触及 CLI、TUI、app-server、MCP、native-tldr 或 `zmemory` 的职责边界：读 `.agents/llmdoc/architecture/runtime-surfaces.md`
- 需要判断 crate 归属、入口位置或是否应避免继续膨胀 `codex-core`：读 `.agents/llmdoc/architecture/rust-workspace-map.md`
- 需要处理 `.ai`、`zmemory`、project-scoped config、turn cwd 或文档/记忆写回：读 `.agents/llmdoc/architecture/memory-and-doc-systems.md`
- 进入具体实施回路：读 `.agents/llmdoc/guides/rust-change-loop.md`
- 需要查命令、schema、测试、Bazel 或 `mise`：读 `.agents/llmdoc/reference/build-and-test-commands.md`

编辑前，可用时优先补读相关 `guides/` 与 `memory/reflections/`。
