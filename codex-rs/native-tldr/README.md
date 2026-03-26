# codex-native-tldr

`codex-native-tldr` 是 `codex-cli` / `codex-mcp-server` 共用的本地代码上下文分析库。

当前已落地能力：

- 统一引擎入口 `TldrEngine`
- 7 种首批语言注册：Rust、TypeScript、JavaScript、Python、Go、PHP、Zig
- `structure` / `context` 分析入口
- phase-1 semantic 本地检索、按语言缓存索引、`warm` 触发 reindex
- daemon / session / health / status 生命周期闭环
- CLI `codex tldr ...` 接入
- MCP `tldr` tool 接入
- 项目级配置：`project/.codex/tldr.toml`

## daemon artifacts

Unix 下 daemon artifacts 现在按“运行时目录 / 用户 / 项目”隔离：

- 优先：`$XDG_RUNTIME_DIR/codex-native-tldr/<uid>/<project-hash>/`
- 回退：`$TMPDIR/codex-native-tldr/<uid>/<project-hash>/`

非 Unix 下回退到：

- `$TMPDIR/codex-native-tldr/<project-hash>/`

目录内文件名保持稳定：

- `codex-native-tldr-<hash>.sock`
- `codex-native-tldr-<hash>.pid`
- `codex-native-tldr-<hash>.lock`
- `codex-native-tldr-<hash>.launch.lock`

## 当前边界

- daemon-first 是 Unix 主路径；daemon 不可用时 CLI/MCP 会回退本地引擎
- MCP 复用 query/retry 生命周期逻辑，但**不负责 auto-start**
- semantic 默认关闭，需在 `.codex/tldr.toml` 显式开启
- semantic / status 对外 schema 已收口到稳定 view；更激进的 payload 控制仍可继续增强

## 后续方向

- 继续补 daemon 崩溃/残留 artifact/权限异常的压力回归
- 继续收紧 semantic payload 上限与截断策略
- 按职责拆分 `daemon.rs` / `semantic.rs` / `tldr_cmd.rs`
