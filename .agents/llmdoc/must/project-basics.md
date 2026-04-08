# 项目基础

## 仓库身份
- 这是 `zcodex` 的 monorepo，面向本地运行的 Codex CLI 及其相关运行面。
- 对外安装入口在顶层 `README.md`；源码主线实现集中在 `codex-rs/`。
- 根目录 `package.json` 只承担仓库级维护脚本，不是产品主实现。

## 主要交付面
- `codex` CLI / TUI：本地交互式主体验。
- `codex exec`：非交互式执行面。
- `codex app-server`：给 IDE/外部客户端的 JSON-RPC 服务面。
- `codex mcp-server`：把 Codex 暴露为 MCP server。
- `codex zmemory`：可写长期记忆工具链。
- `codex tldr` / native-tldr：项目语义提炼与守护进程能力。

## 主要目录
- `codex-rs/`：Rust workspace，仓库核心实现。
- `docs/`：用户和贡献者文档。
- `.ai/`：项目级工作记忆与风险记录。
- `.agents/plan`、`.agents/issues` 及专题目录：任务规划与专项交付产物，不是稳定索引系统。

## 起手阅读
- 顶层理解先看 `README.md` 和 `codex-rs/README.md`。
- 进入实现前，再按任务读取对应 crate README 与本目录下的架构文档。
