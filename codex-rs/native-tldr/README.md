# codex-native-tldr

`codex-native-tldr` 是 `codex-cli` / `codex-mcp-server` 共用的本地代码上下文分析库。

当前已落地能力：

- 统一引擎入口 `TldrEngine`
- 16 种语言注册：C、C++、C#、Go、Java、JavaScript、Kotlin、Lua、Luau、PHP、Python、Ruby、Rust、Swift、TypeScript、Zig
- `structure` / `search` / `extract` / `imports` / `importers` / `context` / `impact` / `calls` / `dead` / `arch` / `change-impact` / `cfg` / `dfg` / `slice` / `diagnostics` / `doctor` 分析入口
- semantic phase-2：真实 dense embedding、系统运行时目录/临时目录下 `codex-native-tldr/<scope>/<project-hash>/cache/semantic/<language>/` 本地持久化、brute-force 向量检索、`warm` 触发 reindex
- daemon / session / health / status 生命周期闭环
- CLI `codex ztldr ...` 接入
- MCP `ztldr` tool 接入
- 项目级配置：`project/.codex/tldr.toml`

## 本地交付与启动

当前推荐把 native-tldr 视为一组本地二进制交付：

- `target/release/codex`
- `target/release/codex-mcp-server`

常用本地验证命令：

```bash
cargo build --release -p codex-cli -p codex-mcp-server
./target/release/codex ztldr languages
./target/release/codex ztldr daemon --project /path/to/project --json status

# repo 根目录下的快速回归
just tldr-test-fast
just tldr-daemon-test-fast
just tldr-semantic-test-fast

# 或用 mise 聚合入口自动拆分独立 slot
mise run test --slot tldr tldr
mise run test --slot tldr-daemon tldr-daemon
mise run test --slot tldr-semantic tldr-semantic
```

说明：

- `codex ztldr daemon ...` 在 Unix 下会走 daemon-first，并在允许时通过当前 `codex` 进程自动拉起内部 daemon 模式
- daemon 在空闲超出 `session.idle_timeout_secs` 后会自动退出，默认 1800 秒；`status` 会返回当前阈值
- 上面的 `just` / `mise` 入口会给 tldr 链路拆分独立的 `CARGO_HOME` / `CARGO_TARGET_DIR`，并固定 `CARGO_INCREMENTAL=0`，减少多会话并发时的 Cargo 锁与 `sccache` 冲突
- CLI 分析命令目前对应为：`structure -> structure`、`search -> search`、`extract -> extract`、`imports -> imports`、`importers -> importers`、`context -> context`、`impact -> impact`、`calls -> calls`、`dead -> dead`、`arch -> arch`、`diagnostics -> diagnostics`、`doctor -> doctor`
- `codex-mcp-server` 是 stdio MCP server；它会在可定位 `codex` 二进制时沿用同一套 daemon-first auto-start 策略
- 这是本地 CLI/MCP 交付，不提供 HTTP 端口或 Web UI

示例项目配置：

```toml
[daemon]
auto_start = true

[session]
idle_timeout_secs = 1800
```

## daemon artifacts

Unix 下 daemon artifacts 现在按“运行时目录 / 用户 / 项目”隔离：

- scope 根目录优先：`$XDG_RUNTIME_DIR/codex-native-tldr/<uid>/`
- 否则回退：`$TMPDIR/codex-native-tldr/<uid>/`
- `socket/pid` 位于：`.../<project-hash>/`
- `lock/launch.lock` 位于 scope 根目录，避免项目 artifact 目录被删时一并丢失互斥语义
- semantic cache 位于：`.../<project-hash>/cache/semantic/<language>/`

非 Unix 下回退到：

- scope 根目录：`$TMPDIR/codex-native-tldr/`
- `socket/pid` 位于：`.../<project-hash>/`
- `lock/launch.lock` 位于 scope 根目录
- semantic cache 位于：`.../<project-hash>/cache/semantic/<language>/`

文件名保持稳定：

- `codex-native-tldr-<hash>.sock`
- `codex-native-tldr-<hash>.pid`
- `codex-native-tldr-<hash>.lock`
- `codex-native-tldr-<hash>.launch.lock`

## 当前边界

- daemon-first 是 Unix 主路径；分析类与 semantic 在 daemon 不可用时可回退本地引擎，但 daemon action（`ping/warm/snapshot/status/notify`）仍要求 daemon 可用
- `status` 的配置摘要会暴露 `session_idle_timeout_secs`，便于观察当前空闲自退阈值
- `structure` 当前对应代码结构分析；`tree` 仍保留给未来的真实 file-tree contract，当前不会再作为 AST 别名
- semantic 默认开启，并在首次查询时按语言 lazy 建索引；首次 fresh build 会把 units/vector/manifest 落到系统运行时目录或系统临时目录下的 `codex-native-tldr/<scope>/<project-hash>/cache/semantic/<language>/`
- semantic / status 对外 schema 已收口到稳定 view；更激进的 payload 控制仍可继续增强
- semantic embedding 的 ONNX Runtime 现改为运行时动态加载；构建时不再静态链接预编译 ORT，但执行 semantic embedding 前需要让 `libonnxruntime.so` 可被动态加载器找到，或设置 `ORT_DYLIB_PATH=/path/to/libonnxruntime.so`
- 若当前环境暂时无法提供可加载的 ORT 动态库，`semantic` 现在会自动回退到非 embedding 路径；命令仍会成功返回，普通 CLI 输出不会额外打断用户
- 若希望始终禁用 embedding，可在项目级 `.codex/tldr.toml` 中显式设置 `[semantic.embedding] enabled = false`

## semantic embedding 安装与使用

### 它做什么

- `semantic` 的 embedding 路径由 `fastembed` + ONNX Runtime 在本地执行，不调用 Codex 远端向量服务
- embedding 打开时，`semantic` 会先把代码单元转成向量，再结合关键词分数做排序
- embedding 关闭或自动降级时，`semantic` 仍可运行，但结果更依赖关键词、符号名和文本命中

### 运行时需要什么

- Linux: 需要可加载的 `libonnxruntime.so`
- macOS/iOS: 需要可加载的 `libonnxruntime.dylib`
- Windows: 需要可加载的 `onnxruntime.dll`
- 模型权重由 `fastembed` 管理；ONNX Runtime 动态库本身需要由运行环境提供或随交付物一起分发

### 让 runtime 能被找到

优先级如下：

1. 显式设置 `ORT_DYLIB_PATH`
2. 若未设置，则按默认文件名查找系统动态库搜索路径
3. 若给的是相对路径，native-tldr 会先尝试按当前可执行文件目录解析

示例：

```bash
export ORT_DYLIB_PATH=/opt/onnxruntime/lib/libonnxruntime.so
./target/release/codex ztldr semantic --project /path/to/project --lang rust "query"
```

### 没有 runtime 时会怎样

- 不再 panic，也不会把整个 codex 进程打崩
- `semantic` 会自动回退到非 embedding 路径
- 默认的人类可读输出会继续按成功查询展示
- 若消费 JSON / structured payload，仍可从 `embeddingUsed = false` 看出当前未使用 embedding
- 如果你需要稳定、可预期地关闭 embedding，而不是依赖运行时自动降级，请显式配置：

```toml
[semantic.embedding]
enabled = false
```

### 推荐排障顺序

1. 若消费 JSON / structured payload，先看 `embeddingUsed`
2. 确认 `ORT_DYLIB_PATH` 是否指向真实文件
3. 若未设置 `ORT_DYLIB_PATH`，确认系统动态库搜索路径里是否能找到 ONNX Runtime
4. 若当前环境不方便提供 ORT，临时把 `[semantic.embedding] enabled = false`

## agent-first 指引

- 参考说明：`../../docs/tldr-agent-first-guidance/tool-description.md`
- 当问题属于结构化代码理解、依赖关系、影响分析、诊断、语义搜索时，应优先考虑 `ztldr`
- 当结果含有 `degradedMode` 时，说明当前结果是降级路径，不应当作 daemon 正常成功
- 当结果含有 `structuredFailure` 时，应读取：
  - `error_type`
  - `reason`
  - `retryable`
  - `retry_hint`

当前对外约定：

- `semantic` 在 `source = "local"` 时，会额外返回 `degradedMode`
- daemon 结果中只要带有 `daemonStatus` 且其处于不健康状态，就会额外返回 `structuredFailure` 与 `degradedMode`
- 这些字段属于稳定 wire contract，对 MCP/CLI JSON 消费方开放

## 后续方向

- 继续补 daemon 崩溃/残留 artifact/权限异常的压力回归
- 继续收紧 semantic payload 上限与截断策略
- 继续评估 Kotlin 支持下的语义/关系精度
- 按职责拆分 `daemon.rs` / `semantic.rs` / `tldr_cmd.rs`
