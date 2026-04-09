# 运行面架构

## 用途
- 描述仓库对外提供的主要运行面，以及它们如何共享核心逻辑。

## 核心组件
- `codex-rs/cli`：`codex` 主二进制入口，负责把多种子命令和运行面汇总到一个 CLI。
- `codex-rs/tui`：当前主交互界面，承担本地全屏终端体验。
- `codex-rs/core`：业务逻辑中枢，处理模型交互、工具调度、沙箱、配置与跨系统编排。
- `codex-rs/app-server`：面向 IDE/外部客户端的 JSON-RPC 服务层。
- `codex-rs/mcp-server`：把 Codex 作为 MCP server 暴露给其他客户端。
- `codex-rs/protocol`：内部与外部共享的轻量协议类型。
- `codex-rs/native-tldr`：项目结构提炼与守护进程能力。
- `codex-rs/zmemory`：独立 SQLite 长期记忆内核。

## 流程
- 用户通常从 `codex` 主命令进入，由 `cli` 选择进入 TUI、exec、app-server、mcp-server、zmemory 或 `ztldr` 等运行面。
- `tui`、`exec`、`app-server` 等上层都依赖 `core` 统一处理模型调用、工具、配置与执行策略。
- `app-server` 和 `mcp-server` 面向外部客户端，`protocol` 负责稳定类型边界。
- `native-tldr` 与 `zmemory` 是横切能力：前者提供代码结构分析，后者提供可写持久记忆。

## 重要不变量
- 外部客户端协议开发优先落在 app-server v2，而不是继续扩 v1。
- `protocol` 应保持“类型边界”角色，避免承载业务逻辑。
- `codex-core` 已是最大聚合点；新能力优先放到更合适的小 crate。

## 相关文档
- `.agents/llmdoc/architecture/rust-workspace-map.md`
- `.agents/llmdoc/architecture/memory-and-doc-systems.md`
- `.agents/llmdoc/reference/build-and-test-commands.md`
