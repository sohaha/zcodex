# codex-native-tldr

`codex-native-tldr` 是 `codex-cli` / `codex-mcp-server` 共用的本地代码上下文分析库。

当前已落地能力：

- 统一引擎入口 `TldrEngine`
- 7 种首批语言注册：Rust、TypeScript、JavaScript、Python、Go、PHP、Zig
- `tree` / `extract` / `context` / `impact` / `cfg` / `dfg` / `slice` 分析入口
- phase-1 semantic 本地检索、按语言缓存索引、`warm` 触发 reindex
- daemon / session / health / status 生命周期闭环
- CLI `codex tldr ...` 接入
- MCP `tldr` tool 接入
- 项目级配置：`project/.codex/tldr.toml`

## 本地交付与启动

当前推荐把 native-tldr 视为一组本地二进制交付：

- `target/release/codex`
- `target/release/codex-mcp-server`

常用本地验证命令：

```bash
cargo build --release -p codex-cli -p codex-mcp-server
./target/release/codex tldr languages
./target/release/codex tldr daemon --project /path/to/project --json status
```

说明：

- `codex tldr daemon ...` 在 Unix 下会走 daemon-first，并在允许时通过当前 `codex` 进程自动拉起内部 daemon 模式
- CLI 分析命令目前对应为：`structure -> tree`、`extract -> extract`、`context -> context`、`impact -> impact`、`cfg -> cfg`、`dfg -> dfg`、`slice -> slice`
- `codex-mcp-server` 是 stdio MCP server；它复用 native-tldr 的 query/retry 生命周期逻辑，但不负责 auto-start daemon
- 这是本地 CLI/MCP 交付，不提供 HTTP 端口或 Web UI

## daemon artifacts

Unix 下 daemon artifacts 现在按“运行时目录 / 用户 / 项目”隔离：

- scope 根目录优先：`$XDG_RUNTIME_DIR/codex-native-tldr/<uid>/`
- 否则回退：`$TMPDIR/codex-native-tldr/<uid>/`
- `socket/pid` 位于：`.../<project-hash>/`
- `lock/launch.lock` 位于 scope 根目录，避免项目 artifact 目录被删时一并丢失互斥语义

非 Unix 下回退到：

- scope 根目录：`$TMPDIR/codex-native-tldr/`
- `socket/pid` 位于：`.../<project-hash>/`
- `lock/launch.lock` 位于 scope 根目录

文件名保持稳定：

- `codex-native-tldr-<hash>.sock`
- `codex-native-tldr-<hash>.pid`
- `codex-native-tldr-<hash>.lock`
- `codex-native-tldr-<hash>.launch.lock`

## 当前边界

- daemon-first 是 Unix 主路径；分析类与 semantic 在 daemon 不可用时可回退本地引擎，但 daemon action（`ping/warm/snapshot/status/notify`）仍要求 daemon 可用
- MCP 复用 query/retry 生命周期逻辑，但**不负责 auto-start**
- semantic 默认开启，并在首次查询时按语言 lazy 建索引；`.codex/tldr.toml` 可用于覆盖默认行为
- semantic / status 对外 schema 已收口到稳定 view；更激进的 payload 控制仍可继续增强

## 后续方向

- 继续补 daemon 崩溃/残留 artifact/权限异常的压力回归
- 继续收紧 semantic payload 上限与截断策略
- 按职责拆分 `daemon.rs` / `semantic.rs` / `tldr_cmd.rs`
