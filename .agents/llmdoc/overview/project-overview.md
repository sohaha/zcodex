# 项目概览

## 身份
- 这是 Codex CLI 的主仓库，包含本地 CLI/TUI、外部接口层、记忆系统和若干开发辅助能力。
- 仓库同时服务终端交互、IDE/客户端集成、MCP 集成、项目语义提炼和长期记忆治理。

## 边界
- 属于这里：Rust 主产品实现、顶层用户文档、构建/测试脚本、项目级记忆与任务编排产物。
- 不属于这里：云端 Codex Web 服务本体；`reference-projects/` 更像参考资料，不是主产品代码。

## 主要区域
- `codex-rs/`：Rust workspace，包含 `cli`、`tui`、`core`、`app-server`、`mcp-server`、`native-tldr`、`zmemory` 等主线 crate。
- `docs/`：安装、配置、贡献、特性说明等面向用户/贡献者文档。
- `.ai/`：项目级工作记忆，记录 handoff、风险、业务规则与后续可复用结论。
- `.agents/`：任务计划、issue、专项文档与本次新增的 `llmdoc` 稳定知识地图。

## 主要阅读路径
- 产品与安装入口：`README.md`、`docs/install.md`、`docs/config.md`
- Rust 总览：`codex-rs/README.md`
- 外部接口：`codex-rs/app-server/README.md`
- 长期记忆：`docs/zmemory.md`、`codex-rs/zmemory/README.md`
