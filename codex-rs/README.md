# Codex CLI（Rust 实现）

我们提供独立的原生可执行文件形式的 Codex CLI，以保证零依赖安装。

## 安装 Codex

目前最简单的安装方式是通过 `npm`：

```shell
npm i -g @sohaha/zcodex
codex
```

你也可以通过 Homebrew（`brew install --cask codex`）安装，或直接从 [GitHub Releases](https://github.com/sohaha/zcodex/releases) 下载平台对应的发布包。

## 文档快速入口

- 第一次使用 Codex？从 [`docs/getting-started.md`](../docs/getting-started.md) 开始（包含提示词、快捷键与会话管理的引导）。
- 需要更深入的控制？查看 [`docs/config.md`](../docs/config.md) 与 [`docs/install.md`](../docs/install.md)，包含 Ubuntu 交叉编译 macOS arm64 与 Windows amd64/arm64 的说明。

## Rust CLI 有哪些新特性

Rust 实现已成为 Codex CLI 的主线版本与默认体验，提供旧版 TypeScript CLI 没有的能力。

### 配置

Codex 支持更完整的配置能力。注意 Rust CLI 使用 `config.toml` 而不是 `config.json`。详情见 [`docs/config.md`](../docs/config.md)。

### Model Context Protocol 支持

#### MCP 客户端

Codex CLI 作为 MCP 客户端，可在启动时让 Codex CLI 与 IDE 扩展连接到 MCP 服务。详见 [`配置文档`](../docs/config.md#connecting-to-mcp-servers)。

#### MCP 服务端（实验性）

运行 `codex mcp-server` 可将 Codex 作为 MCP _server_ 启动，让其他 MCP 客户端把 Codex 当作工具调用。

如果你是从源码本地验证，推荐先构建 release 二进制：

```shell
cargo build --release -p codex-cli -p codex-mcp-server
./target/release/codex mcp-server
```

可使用 [`@modelcontextprotocol/inspector`](https://github.com/modelcontextprotocol/inspector) 进行尝试：

```shell
npx @modelcontextprotocol/inspector codex mcp-server
```

使用 `codex mcp` 管理 `config.toml` 中定义的 MCP server launcher（添加/列出/查看/删除），使用 `codex mcp-server` 直接运行 MCP 服务端。

如果要一起验证 native-tldr 相关能力，可额外执行：

```shell
./target/release/codex tldr languages
./target/release/codex tldr daemon --project /path/to/project --json status
```

### 通知

你可以配置一个脚本，在代理完成一轮任务时触发通知。[通知文档](../docs/config.md#notify) 中提供了示例，说明如何在 macOS 上通过 [terminal-notifier](https://github.com/julienXX/terminal-notifier) 获取桌面通知。当 Codex 检测到在 Windows Terminal 的 WSL 2 环境中运行（设置了 `WT_SESSION`），TUI 会自动降级为原生 Windows toast 通知，即使 Windows Terminal 不支持 OSC 9，也能显示审批与完成提示。

### 使用 `codex exec` 进行程序化/非交互运行

要以非交互方式运行 Codex，可执行 `codex exec PROMPT`（也可通过 `stdin` 传入 prompt）。Codex 会处理任务直到完成并退出，输出会直接打印到终端。你可以设置 `RUST_LOG` 以查看更多日志。
使用 `codex exec --ephemeral ...` 可在不持久化会话产物的情况下运行。

### 体验 Codex 沙箱

我们提供以下子命令，用于测试命令在 Codex 沙箱中的行为：

```
# macOS
codex sandbox macos [--full-auto] [--log-denials] [COMMAND]...

# Linux
codex sandbox linux [--full-auto] [COMMAND]...

# Windows
codex sandbox windows [--full-auto] [COMMAND]...

# 旧别名
codex debug seatbelt [--full-auto] [--log-denials] [COMMAND]...
codex debug landlock [--full-auto] [COMMAND]...
```

### 使用 `--sandbox` 选择沙箱策略

Rust CLI 提供独立的 `--sandbox`（`-s`）参数，无需使用通用的 `-c/--config` 也能选择沙箱策略：

```shell
# 使用默认只读沙箱运行 Codex
codex --sandbox read-only

# 允许代理在当前工作区写入，同时继续阻断网络
codex --sandbox workspace-write

# 危险！彻底关闭沙箱（仅在容器或其他隔离环境中使用）
codex --sandbox danger-full-access
```

同样也可以在 `~/.codex/config.toml` 中用顶层 `sandbox_mode = "MODE"` 持久化设置，例如 `sandbox_mode = "workspace-write"`。
在 `workspace-write` 模式下，Codex 还会把 `~/.codex/memories` 加入可写目录，以避免记忆维护需要额外审批。

## 代码结构

该目录是一个 Cargo workspace 的根目录，包含部分实验性代码。核心 crate 如下：

- [`core/`](./core) 包含 Codex 的业务逻辑。我们希望它最终成为可复用的库 crate，供其他 Rust/原生应用使用。
- [`exec/`](./exec) 适用于自动化场景的“无界面”CLI。
- [`tui/`](./tui) 全屏 TUI（使用 [Ratatui](https://ratatui.rs/)）。
- [`cli/`](./cli) CLI 多功能入口，通过子命令提供上述功能。

如果你要贡献或深入排查行为，建议先阅读各 crate 下的 `README.md`，并从顶层 `codex-rs` 目录运行 workspace，以保证共享配置、特性与构建脚本对齐。
