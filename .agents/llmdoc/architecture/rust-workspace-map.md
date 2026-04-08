# Rust Workspace 地图

## 用途
- 提供 `codex-rs/` 的高价值 crate 路由，帮助在大型 workspace 里快速定位归属。

## 核心分层
- 入口层：`cli`、`tui`、`exec`
- 服务接口层：`app-server`、`app-server-protocol`、`mcp-server`、`protocol`
- 业务逻辑层：`core`
- 横切能力：`native-tldr`、`zmemory`、`hooks`、`skills`、`plugin`
- 基础设施与工具层：大量 `utils/*` crate、sandbox/proxy/provider 相关 crate

## 高价值入口
- `codex-rs/Cargo.toml`：workspace 成员全集与 crate 命名。
- `codex-rs/cli/Cargo.toml`：`codex` 命令面入口。
- `codex-rs/tui/Cargo.toml`：主 UI 依赖面和测试依赖。
- `codex-rs/core/Cargo.toml`：核心聚合点与依赖面。
- `codex-rs/app-server/README.md`：外部客户端生命周期与 RPC 能力。
- `codex-rs/protocol/README.md`：协议类型边界。

## 归属规则
- 新功能优先判断是否已有小 crate 可承载，而不是默认塞进 `codex-core`。
- 如果能力本质上是公共类型或契约，优先考虑 `protocol` 或专门的新 crate；不要把业务逻辑塞进类型层。
- 如果是平台能力、工具函数或封装，优先查 `utils/*`、sandbox、provider、proxy 相关 crate。

## 改动时的默认路由
- 改 CLI 子命令、启动逻辑、输出接线：先看 `cli`。
- 改终端交互与渲染：先看 `tui`。
- 改工具编排、模型请求、线程/turn 语义、配置解析：先看 `core` 与相关支持 crate。
- 改 IDE/客户端协议：先看 `app-server` 与 `app-server-protocol`。
- 改持久记忆：先看 `zmemory`、`docs/zmemory.md` 与相关 CLI 接线。

## 相关文档
- `.agents/llmdoc/architecture/runtime-surfaces.md`
- `.agents/llmdoc/reference/build-and-test-commands.md`
