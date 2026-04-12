# CLI 启动错误链可见性反思

## 背景
- Windows 下进入交互模式时，用户只看到 `Error: thread/start failed during TUI bootstrap`。
- TUI 会给 `thread/start` 再包一层上下文；CLI `main` 之前直接返回 `anyhow::Result<()>`，运行时默认只打印最外层 Display 文本。

## 这次修复
- 在 `codex-rs/cli/src/main.rs` 改为显式处理 `arg0_dispatch_or_else(...)` 的错误。
- 新增 `format_error_chain()`，把 `anyhow::Error::chain()` 全部展开，按：
  - `Error: <顶层错误>`
  - `Caused by:`
  - `  0: <第一层原因>`
  - `  1: <更深层原因>`
  的格式打印。

## 收获
- 启动阶段失败时，顶层上下文本身通常是对的，但单独打印它几乎无法定位根因。
- 对 `thread/start` 这类 RPC 包装错误，真正可操作的信息通常在更深一层，比如 required MCP 初始化失败、配置加载失败或 transport 失败。
- 这类问题优先改 CLI 入口的错误展示，比在每个调用点重复叠加更长的上下文字符串更稳。

## 后续建议
- 以后新增 startup/bootstrap 类上下文时，默认假设调用者需要看到完整 error chain，而不是只看最外层。
- 若再次出现“只剩包装文案、看不到底层原因”的问题，先检查入口是否吞掉了 `anyhow` 的 cause chain。
