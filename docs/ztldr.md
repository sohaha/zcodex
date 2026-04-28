# ztldr

`ztldr` 是 Codex 内置的本地代码结构分析工具，用于在大仓库中快速建立符号、调用关系、影响范围、诊断和语义检索视图。它面向代码理解，不替代逐字文本核对；需要确认精确字符串、复杂 regex 或原始文件内容时，仍应使用 `rg`、`grep` 或直接读文件。

## 命令入口

常用入口是 `codex ztldr`：

```shell
codex ztldr languages
codex ztldr structure --project /path/to/repo --language rust
codex ztldr context --project /path/to/repo --language rust MyType
codex ztldr impact --project /path/to/repo --language rust my_function
codex ztldr change-impact --project /path/to/repo --language rust path/to/file.rs
codex ztldr semantic --project /path/to/repo --language rust "where is config loaded"
codex ztldr diagnostics --project /path/to/repo path/to/file.rs
codex ztldr daemon --project /path/to/repo status
codex ztldr daemon --project /path/to/repo reindex
```

支持的分析类子命令包括：

- `structure`：项目结构概览。
- `extract`：单文件结构摘要。
- `imports`：单文件 import 列表。
- `importers`：查找导入指定模块的符号或文件。
- `slice`：指定行的 backward slice。
- `context`：指定符号或项目的上下文概览。
- `impact`：指定符号影响分析。
- `change-impact`：按变更文件评估影响范围。
- `calls`：调用图中的调用边。
- `dead`：死代码候选项。
- `arch`：调用拓扑结构统计。
- `cfg`：控制流概览。
- `dfg`：数据流概览。
- `search`：索引内匹配搜索。
- `semantic`：语义检索。
- `diagnostics`：运行语言诊断工具集合。
- `doctor`：探测诊断工具可用性。

Daemon 子命令包括 `start`、`stop`、`ping`、`warm`、`reindex`、`snapshot`、`status` 和 `notify <path>`。`warm` 会预热 daemon 缓存并在 dirty 阈值触发时重建索引；`reindex` 会显式请求运行中的 daemon 立即重建当前项目语义索引。

`codex zinit ztldr` 首次安装 ONNX Runtime 并预热默认模型后，会检测当前目录所在 Git 仓库是否已有运行中的 ztldr daemon；如果存在，会向该 daemon 发送 `reindex` 请求。显式传入 `codex zinit --model <模型> ztldr` 时也会执行同样的运行中 daemon 检测与索引重建请求。该检测不会自动启动 daemon。若全局 `[ztldr].onnxruntime = false`，`zinit` 会跳过 ONNX Runtime 安装和模型预热。

## 语言与降级

`ztldr` 的多语言关系分析能力按语言分级。Rust 使用 dedicated extractor，结构化 analysis 输出中的 owner、trait impl 和关系信息更精确。TypeScript、JavaScript、Python、Go、PHP、Zig 等语言当前更多依赖 heuristic extractor，结果应视为启发式分析。

当输出中出现 `degradedMode`、`structuredFailure` 或 `source=local` 时，说明本次结果并非完整 daemon 成功路径。调用方应把它当作降级结果处理，并在需要时补充源码阅读或原始搜索验证。

## 配置边界

`~/.codex/config.toml` 支持一个 `[ztldr]` 配置表：

```toml
[ztldr]
enabled = true
artifact_location = "project"
onnxruntime = false
model = "jina-code"
```

`enabled` 是总开关，默认 `false`。`artifact_location` 支持 `"temp"` 和 `"project"`，默认 `"temp"`。只有同时设置 `enabled = true` 且 `artifact_location = "project"` 时，ztldr 的本地产物才会写入当前项目根目录下的 `.tldr/`，对齐上游 `llm-tldr` 的项目内产物路径；典型路径包括 `.tldr/cache/semantic/`。`onnxruntime` 默认 `true`；设置为 `false` 后，ztldr 会全局关闭 ONNX Runtime backed embedding，不加载 ONNX Runtime，不预热 embedding 模型，并退回无 dense embedding 的语义索引路径。`model` 用作语义嵌入模型的默认值，优先级低于 `.codex/tldr.toml` 的 `[semantic].model`。其他情况下继续使用默认 runtime/temp artifact 目录。

`ztldr` 的 daemon、semantic 和 session 参数来自项目根目录下的 `.codex/tldr.toml`。如果该文件不存在，Codex 使用内置默认值和 `[ztldr]` 中的全局语义模型默认值。

`.codex/tldr.toml` 示例：

```toml
[daemon]
auto_start = true
socket_mode = "auto"

[semantic]
enabled = true
model = "bge-m3"
auto_reindex_threshold = 20
ignore = ["generated.rs"]

[semantic.embedding]
enabled = true
dimensions = 64

[session]
dirty_file_threshold = 20
idle_timeout_secs = 1800
```

字段含义：

- `[daemon].auto_start`：查询 daemon 时是否允许自动启动；默认 `true`。
- `[daemon].socket_mode`：daemon socket 模式；默认 `"auto"`。
- `[semantic].enabled`：是否启用语义索引能力；默认 `true`。
- `[semantic].model`：语义嵌入模型；默认 `"minilm"`。支持的模型包括 `minilm`、`all-minilm-l6-v2`、`bge-small-en-v1.5`、`bge-base-en-v1.5`、`bge-m3`、`jina-code`、`jina-embeddings-v2-base-code`。如果 `[ztldr].model` 也存在，则 `.codex/tldr.toml` 的 `[semantic].model` 优先。
- `[semantic].auto_reindex_threshold`：dirty 文件达到多少时触发自动重建索引；默认 `20`。
- `[semantic].ignore`：语义索引忽略模式列表；默认空列表。
- `[semantic.embedding].enabled`：是否启用 embedding；默认 `true`。
- `[semantic.embedding].dimensions`：embedding 维度；默认 `64`。
- `[session].dirty_file_threshold`：session 中 dirty 文件阈值；默认 `20`。
- `[session].idle_timeout_secs`：session 空闲超时秒数；默认 `1800`。

## MCP 工具

`ztldr` 也可以作为 MCP tool 暴露，但 `codex-mcp-server` 默认不编译该工具。需要使用 `tldr` Cargo feature 构建：

```shell
cargo build --release -p codex-mcp-server --features tldr
cargo build --release -p codex-cli --features tldr
```

MCP `ztldr` tool 复用 daemon 查询与重试逻辑，但不会自行自动启动 daemon；daemon 相关 action 仍需要已有 live daemon。

MCP 接口细节见 [`codex-rs/docs/codex_mcp_interface.md`](../codex-rs/docs/codex_mcp_interface.md#ztldr-tool)。
