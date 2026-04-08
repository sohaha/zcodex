# 工程护栏

## 先验约束
- 先读文档和相邻实现，再改代码或写文档。
- 保持最小闭环：优先根因修复、最小必要改动、任务相关验证。
- 未验证前不要宣称成功。

## Rust workspace 约束
- 默认通过根目录 `justfile` 操作；它会把工作目录切到 `codex-rs/`。
- Rust crate 命名统一为 `codex-*`。
- 优先使用已有小 crate；没有充分理由时，不要把新概念继续堆进 `codex-core`。
- 共享配置、schema、lockfile 都有固定生成命令，按仓库约定刷新，不手工维护产物。

## 验证约束
- 常规改动先跑受影响 crate 的局部测试，再决定是否扩大。
- 若触及 `common`、`core` 或 `protocol` 这类共享区域，局部验证通过后再决定是否跑全量。
- TUI 用户可见改动必须补 snapshot 覆盖并审阅变更。

## 跨系统高风险点
- `turn` 级 cwd override 与 project-scoped config 解析曾出现耦合问题，`zmemory`、子线程和相关 handler 是重点回归面。
- 改公共配置、协议、记忆路径或客户端接口时，至少检查 CLI/TUI、app-server/MCP、相关诊断视图是否仍一致。
- `system://workspace` 和 `system://defaults` 是诊断当前配置/默认配置边界的首选事实来源。
