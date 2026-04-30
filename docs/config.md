# 配置

基础配置说明见：[此文档](https://developers.openai.com/codex/config-basic)。

高级配置说明见：[此文档](https://developers.openai.com/codex/config-advanced)。

完整配置参考见：[此文档](https://developers.openai.com/codex/config-reference)。

## 连接 MCP 服务器

Codex 可以连接在 `~/.codex/config.toml` 中配置的 MCP 服务器。最新的 MCP 服务器选项以配置参考为准：

- https://developers.openai.com/codex/config-reference

MCP 工具默认按“串行”（一次只跑一个）调用。若要把某个服务器暴露的所有工具都标记为可并行工具调用，请在该服务器配置上设置 `supports_parallel_tool_calls`：

```toml
[mcp_servers.docs]
command = "docs-server"
supports_parallel_tool_calls = true
```

只应对“工具可以安全并行运行”的 MCP 服务器启用并行调用。如果工具会读写共享状态、文件、数据库或外部资源，请在启用该设置前先审视并处理潜在的读写竞态问题。

## MCP 工具审批

Codex 会把自定义 MCP 服务器的审批默认值与逐工具覆盖项存放在 `~/.codex/config.toml` 的 `mcp_servers` 下。可以在服务器上设置 `default_tools_approval_mode` 来给所有工具一个默认值，然后用逐工具的 `approval_mode` 为少数例外单独覆盖：

```toml
[mcp_servers.docs]
command = "docs-server"
default_tools_approval_mode = "approve"

[mcp_servers.docs.tools.search]
approval_mode = "prompt"
```

## Apps（连接器）

在 composer 中输入 `$` 可插入一个 ChatGPT 连接器；弹出层会列出你可访问的 apps。`/apps` 命令会列出可用与已安装的 apps。已连接的 app 会优先显示并标记为 connected；其余会标记为可安装。

## TUI 粘贴图片压缩

TUI 可以在上传前自动重新压缩通过 `Ctrl+V` 粘贴的图片。可在 `~/.codex/config.toml` 中配置：

```toml
[tui]
auto_compress_pasted_images = true
pasted_image_max_width = 1280
pasted_image_max_height = 720
pasted_image_jpeg_quality = 85
```

行为说明：

- 大于配置宽度或高度的图片会按比例缩放，
- 透明图片保持为 PNG，
- 非透明图片会同时编码为 PNG 和 JPEG，Codex 会保留体积更小的结果，
- 宽/高/质量配置值无效时会回退到内置默认值。

实现细节与 composer 行为见 `docs/tui-chat-composer.md`。

## ZTeam 入口

TUI 默认会暴露一个本地 `ZTeam` 协作入口。如果你只想要常规的单线程交互面，可以在 `~/.codex/config.toml` 中把它隐藏：

```toml
[tui]
zteam_enabled = false
```

禁用后，`/zteam` 会从命令面板和斜杠命令解析中移除，因此 TUI 不会初始化本地 ZTeam 入口路径。

启用时，`/zteam` 会打开本地工作台。当前底层仍固定复用两个本地 worker：

- `frontend`：前端协作者
- `backend`：后端协作者

当前推荐主路径是先给一个目标，再启动 ZTeam：

- `/zteam start <目标>`：推荐主路径。向主线程提交一条带目标的启动指令，后续由主线程继续通过 `spawn_agent` 创建 `frontend/backend` 两个长期 worker，并围绕这个目标进入协作上下文

其余相关命令的实际行为如下：

- `/zteam`：打开工作台，只读查看当前状态
- `/zteam start`：兼容入口。只提交 worker 启动指令，不带 mission 目标
- `/zteam status`：刷新并查看当前工作台状态
- `/zteam attach`：恢复最近一次、且仍归属当前主线程的 worker 状态，并尽量重新附着 live 会话
- `/zteam <frontend|backend> <任务>`：把一条任务分派给指定 worker；属于高级手动干预路径
- `/zteam relay <frontend|backend> <frontend|backend> <消息>`：让一个 worker 向另一个 worker 转发消息；属于高级手动干预路径

如果当前已经有任务在运行，TUI 只允许裸 `/zteam` 和 `/zteam status`；`start`、`attach`、任务分派和 `relay` 仍会被阻止，避免在运行中的主线程里插入新的协作动作。

工作台会区分以下几种常见状态：

- 尚未启动：还没有提交 worker 启动指令
- 等待注册：主线程已经提交启动指令，但还没收到 worker 的 `ThreadStarted` 回流
- 部分注册：只收到一个 worker，另一个仍在等待
- 需要再附着：最近线程已恢复，但当前没有 live 连接
- 已就绪：两个 worker 都已注册，可继续分派和 relay

如果 TUI 线程是通过 `--federation-*` 选项启动的，ZTeam 工作台还会显示准备好的外部 adapter 摘要。这里仅是在既有 federation bridge 之上的本地 adapter 接缝；它不会为 ZTeam workers 引入一个独立的公共身份空间。

更完整的命令用法、兼容说明和实际协作案例见 [Slash commands](./slash_commands.md#zteam)。

## ZTOK

`codex ztok` 和 `ztok` alias 都会读取同一份 `config.toml` 里的 `[ztok]` 配置：

```toml
[ztok]
behavior = "enhanced"
```

`behavior` 支持两个值：

- `enhanced`：默认值。保留当前增强压缩栈，包括共享压缩、session dedup、near-diff 和 `.ztok-cache` 会话缓存。
- `basic`：关闭增强压缩路径。`read`、`json`、`log`、`summary` 都不会再走 session dedup、near-diff 或 SQLite 会话写入。

`basic` 模式下的命令行为：

- `ztok read`：仍保留本地读取、过滤、窗口截断和行号能力，但不做会话去重或近重复压缩。
- `ztok json`：输出原始 JSON 文本；无效 JSON 仍会报解析错误；`--keys-only` 不受支持。
- `ztok log`：输出原始日志文本。
- `ztok summary`：仍保留本地测试/构建/列表/通用文本摘要；当输入更像 JSON 或日志时，会退回通用文本摘要，而不是专用压缩摘要。

如果你希望完全关闭增强行为，可以显式写成：

```toml
[ztok]
behavior = "basic"
```

## ZTLDR

`ztldr` 的用户配置边界不同于 `ztok`：`~/.codex/config.toml` 或项目 `.codex/config.toml` 中的 `[ztldr]` 提供全局开关、产物位置选择和语义模型默认值；daemon、semantic 和 session 参数仍来自项目根目录下的 `.codex/tldr.toml`。

```toml
[ztldr]
enabled = true
artifact_location = "project"
onnxruntime = false
model = "jina-code"
```

`enabled` 是总开关，默认 `false`。`artifact_location` 支持 `"temp"` 和 `"project"`，默认 `"temp"`；只有同时设置 `enabled = true` 且 `artifact_location = "project"` 时，ztldr 的本地产物才会写入项目根目录下的 `.tldr/`，例如 semantic cache 会落在 `.tldr/cache/semantic/`。`onnxruntime` 默认 `true`；设置为 `false` 后，ztldr 会全局关闭 ONNX Runtime backed embedding，不加载 ONNX Runtime，不预热 embedding 模型，并退回无 dense embedding 的语义索引路径。`model` 用作语义嵌入模型的默认值；若 `.codex/tldr.toml` 同时配置 `[semantic].model`，以 `.codex/tldr.toml` 为准。其他情况下继续使用默认 runtime/temp artifact 目录。

`.codex/tldr.toml` 最小示例：

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

完整命令、配置字段和 MCP 边界见 [`docs/ztldr.md`](./ztldr.md)。

## Notify

当 agent 完成一个 turn 时，Codex 可以运行一个通知 hook。最新的通知相关配置以配置参考为准：

- https://developers.openai.com/codex/config-reference

当 Codex 知道是哪个客户端启动了这个 turn 时，legacy notify 的 JSON payload 还会包含一个顶层 `client` 字段。TUI 会上报 `codex-tui`，app server 会上报初始化 `initialize` 中的 `clientInfo.name` 值。

## Memories / zmemory

Rust Codex CLI 现在会默认分别启用 native memory 与 `zmemory`（两者互相独立）：

- `native_memories`：控制内置只读 memory 管线与 `get_memory`
- `zmemory`：控制内嵌的“可写、基于 SQLite”的 memory 工具

如需在本次运行中显式禁用其中一个：

```shell
codex --disable native_memories
codex --disable zmemory
```

如需在 `~/.codex/config.toml` 中持久禁用：

```toml
[features]
native_memories = false
zmemory = false
```

`[memories]` 只用于配置 native memory 管线。`zmemory` 现在有自己独立的配置块：

```toml
[zmemory]
path = "./agents/memory.db"
namespace = "team-alpha"
valid_domains = ["core", "project", "notes"]
core_memory_uris = [
  "core://agent/coding_operating_manual",
  "core://my_user/coding_preferences",
  "core://agent/my_user/collaboration_contract",
]
```

`[zmemory]` 字段：

- `path`：可选，覆盖数据库路径
- `namespace`：可选，支持 namespace-aware 数据库时的运行时 namespace 覆盖
- `valid_domains`：可选，运行时可写 domain 白名单覆盖
- `core_memory_uris`：可选，运行时 boot anchor 覆盖

运行时优先级为：

1. `config.toml` 的 `[zmemory]`
2. 环境变量（`VALID_DOMAINS`、`CORE_MEMORY_URIS`）
3. 产品默认值

路径解析规则：

- 绝对路径会直接使用。
- 相对路径在 Codex 处于 git 仓库内时，会以活跃的 repo root 为基准解析；否则以当前工作目录为基准解析。
- 当 `[zmemory].path` 未设置时，Codex 会使用 project-scoped 的默认数据库：`$CODEX_HOME/zmemory/projects/<project-key>/zmemory.db`。
- 若你想跨项目共用一个全局数据库，请显式配置：

```toml
[zmemory]
path = "/absolute/path/to/.codex/zmemory/zmemory.db"
```

你可以用下面的命令验证当前实际生效的路径解析结果：

```shell
codex zmemory stats --json
codex zmemory doctor --json
codex zmemory read system://workspace --json
codex zmemory read system://defaults --json
```

稳定的诊断 payload 位于 `result.pathResolution`（同样的 `dbPath` / `workspaceKey` / `source` / `reason` 字段也会镜像到 `result` 顶层，便于快速检查）：

```json
{
  "dbPath": "/home/me/.codex/zmemory/projects/my-repo-a1b2c3d4e5f6/zmemory.db",
  "workspaceKey": "my-repo-a1b2c3d4e5f6",
  "source": "projectScoped",
  "reason": "defaulted to project scope /home/me/.codex/zmemory/projects/my-repo-a1b2c3d4e5f6/zmemory.db from repo root /workspace/my-repo"
}
```

`system://workspace` 是当前会话的运行时事实视图。它会增加诸如 `hasExplicitZmemoryPath`、`defaultDbPath`、`dbPathDiffers`、`defaultWorkspaceKey`、`bootHealthy` 以及内嵌的 `boot` 快照等字段，从而让你判断当前会话使用的是默认的项目数据库还是显式 override。它也会始终报告当前实际生效的 runtime profile，包括已配置的 `validDomains` 和 `coreMemoryUris`。

当启用 `Feature::Zmemory` 时，`codex-core` 可能会把“高置信度的命名/称谓偏好”等信息主动写入当前生效的 `zmemory` 数据库。该编排仍然使用标准的 `zmemory` action 层：先检查 `system://workspace`，再读写 canonical URI `core://my_user`、`core://agent`、`core://agent/my_user`，最后读回已写入的 URI 做验证。失败会以可观察的 warning 形式暴露，而不是静默成功。

`system://defaults` 是产品默认事实视图。它会报告默认的 `validDomains`、`coreMemoryUris` 以及默认的 DB 路径策略，并且不会把这些默认值与当前 workspace 状态混淆。用户配置会改变 workspace/runtime 视图，但不会改变 defaults 视图。

如果直接 `read <uri>` 失败，或 `search` 没有命中，请在下结论“确实不存在任何 durable memory”之前先使用 `system://workspace`、`stats`、`doctor` 与 `system://alias` 排查：boot 图不健康或 trigger 缺失时，问题可能是 recall 覆盖不足，而不一定是数据缺失。

关于 `zmemory` 的专用使用指南（涵盖命令、系统视图与排障）见 `docs/zmemory.md`。

## JSON Schema

`config.toml` 的自动生成 JSON Schema 位于 `codex-rs/core/config.schema.json`。

## 内置 Model Providers

Codex 内置了 `openai`、`anthropic`、`ollama`、`lmstudio` 等 model provider 条目。对于 Anthropic 兼容的接入方式，使用 `wire_api = "anthropic"` 并通过 `ANTHROPIC_API_KEY` 提供凭证（除非你覆盖了 provider 配置）。内置 `anthropic` provider 默认使用 `https://api.anthropic.com/v1`，你也可以用 `ANTHROPIC_BASE_URL` 或自定义的 `model_providers.<id>.base_url` 覆盖它。

用户自定义的 `model_providers` 也可以覆盖内置 ID（例如 `openai`），以便你改变默认 provider 的接线方式。

你还可以设置 `model_providers.<id>.model`，为某个 provider 提供它自己的默认模型；一旦设置，它在“通过该 provider 发送的请求”中会优先于全局 `model` 设置（包括使用 `-P`/`--model-provider` 选择该 provider 的情况），但显式的 CLI 模型覆盖（例如 `--model`）仍然优先。

示例：覆盖内置 OpenAI provider，使其使用 Chat Completions：

```toml
model_provider = "openai"

[model_providers.openai]
name = "OpenAI Chat"
base_url = "https://api.openai.com/v1"
env_key = "OPENAI_API_KEY"
wire_api = "chat"
```

当选择 `wire_api = "chat"` 时，Codex 会使用 `/v1/chat/completions`。该路径不支持仅 hosted 的工具（例如 `web_search`、`image_generation`），并且只有 `user` 消息允许包含图片输入。可以通过 `tool_choice = "required:<tool_name>"` 指定“命名工具选择”。这些限制来自 Chat Completions API 本身，并非 Codex 特有。需要 hosted 工具时请使用 `wire_api = "responses"`。

如果你希望在主请求失败后用另一个 provider 重试，可以把 `fallback_provider` 设置为 `model_providers` 中的某个 provider ID（或内置 provider），并可选地把 `fallback_model` 设置为 fallback 请求要使用的模型 slug。Codex 只会对“当前这次请求”做 fallback 重试；后续新请求仍会从主 `model_provider` 开始。

**注意**：`fallback_model` 是可选的。不指定时，Codex 会使用 fallback provider 的默认模型（如果配置了），否则回退到主请求指定的模型。这允许你在不改变模型的情况下切换 provider，或在需要时显式降级模型。

如需多级 fallback，可使用按优先级排序的 `fallback_providers`：

```toml
model_provider = "openai"

fallback_providers = [
  { provider = "anthropic", model = "claude-sonnet-4-5" },
  { provider = "openrouter", model = "openai/gpt-4.1" },
]
```

示例：主 provider 为 relay，并配合 OpenRouter 与备用 relay：

```toml
model = "gpt-5.1"
model_provider = "cn-relay"

[model_providers.cn-relay]
# 主 relay（OpenAI-compatible）
name = "CN relay"
model = "gpt-5.1"
base_url = "https://relay.example.com/v1"
env_key = "CN_RELAY_API_KEY"
wire_api = "responses"
request_max_retries = 0
stream_max_retries = 0
supports_websockets = false

[model_providers.openrouter]
name = "OpenRouter"
base_url = "https://openrouter.ai/api/v1"
env_key = "OPENROUTER_API_KEY"
wire_api = "responses"
request_max_retries = 0
stream_max_retries = 0
supports_websockets = false

[model_providers.cn-relay-backup]
name = "CN relay backup"
base_url = "https://backup-relay.example.com/v1"
env_key = "CN_RELAY_BACKUP_API_KEY"
wire_api = "responses"
request_max_retries = 0
stream_max_retries = 0
supports_websockets = false

fallback_providers = [
  # 主 relay 失败时，先尝试 OpenRouter。
  { provider = "openrouter", model = "openai/gpt-4.1" },
  # 再回退到备用 relay。
  { provider = "cn-relay-backup", model = "gpt-4.1" },
]
```

`fallback_provider` + `fallback_model` 仍支持“单级 fallback”。当两种写法同时存在时，Codex 会把它们视为同一个“仅针对当前请求”的有序 fallback 列表的一部分。

更多带中文注释的示例配置（包含）：

- `OpenRouter + relay`
- `relay + relay backup`
- `Azure OpenAI + OpenRouter`

见 `docs/fallback-providers.zh-example.md`。

## 重试与超时配置

Model provider 支持多种重试与超时选项：

- `request_max_retries`：对该 provider 的失败 HTTP 请求最多重试次数。
- `stream_max_retries`：流式响应连接中断时，重连的最大重试次数（超过后失败）。
- `stream_idle_timeout_ms`：流式响应的空闲超时（毫秒）。在该时间内无活动则认为连接已丢失。
- `websocket_connect_timeout_ms`：websocket 连接尝试的最大等待时间（毫秒）。超出则视为失败。
- `retry_base_delay_ms`：重试退避的基础延迟（毫秒）。实际重试间隔为该值乘以 `2^(attempt-1)` 并加 jitter。默认 `200`。

示例：

```toml
[model_providers.myprovider]
request_max_retries = 4
stream_max_retries = 5
stream_idle_timeout_ms = 300000
websocket_connect_timeout_ms = 15000
retry_base_delay_ms = 500  # 更慢网络可增大 base delay
```

## 自定义 Model Catalog

Codex 支持两个“仅在启动时读取”的配置项，用于覆盖可用模型列表：

- `model_catalog_json`：替换活跃 provider 的内置 catalog。
- `model_catalog_merge_json`：把额外模型合并进内置 catalog。

如果两者都设置了，Codex 会以 `model_catalog_json` 作为基础 catalog，然后在其上应用 `model_catalog_merge_json`。合并时按 `slug` 匹配；当同一个 slug 同时出现在两份 catalog 中时，以 merge 条目为准。

对于基于 Responses 的 provider，`model_catalog_merge_json` 不会禁用远端 `/models` 刷新；它会把额外条目叠加到“内置/远端”catalog 快照之上。

这对 Anthropic 兼容代理尤其有用：它们可能暴露了内置 Claude catalog 中不存在的模型 slug。

## 工具兼容性配置

部分第三方代理（如 manifest.build、本地网关等）不支持 OpenAI Responses API 的非标准 tool type（`custom`、`web_search`），会返回 HTTP 400 错误。Codex 默认会在首次失败后自动降级重试，但你可以通过 `skip_freeform_tools` 跳过首次失败，直接发送兼容的 `function` 工具：

```toml
[model_providers.manifest]
skip_freeform_tools = true
api_key = "mnfst_xxx"
base_url = "https://app.manifest.build/v1"
model = "manifest/auto"
wire_api = "responses"
```

设置为 `true` 时：
- 不发送 `custom` / `web_search` 等 tool type
- 将 `apply_patch` 从 freeform 格式转为标准 `function` 格式
- 避免浪费一次 HTTP 400 → 重试的往返

默认值为 `false`（保持原始行为，由自动重试机制兜底）。对 OpenAI 官方 API 无需设置此选项。

## SQLite 状态 DB

Codex 会把 SQLite 状态 DB 存放在 `sqlite_home`（配置项）或 `CODEX_SQLITE_HOME`（环境变量）指定的位置。当两者都未设置时，WorkspaceWrite 沙盒会话默认使用临时目录；其他模式默认使用 `CODEX_HOME`。

## 自定义 CA 证书

当企业代理或网关会拦截 TLS 时，Codex 支持为出站 HTTPS 与安全 websocket 连接信任自定义根 CA bundle。这适用于登录流程与 Codex 的其他对外连接，包括：通过共享的 `codex-client` CA 加载路径构建 reqwest client 或安全 websocket client 的 Codex 组件，以及使用该路径的远端 MCP 连接。

将 `CODEX_CA_CERTIFICATE` 设置为一个 PEM 文件路径（该文件可包含一个或多个证书块），即可让 Codex 使用专用的 CA bundle。若 `CODEX_CA_CERTIFICATE` 未设置，Codex 会回退到 `SSL_CERT_FILE`。若两者都未设置，Codex 使用系统根证书。

`CODEX_CA_CERTIFICATE` 的优先级高于 `SSL_CERT_FILE`。空值会被视为未设置。

PEM 文件可以包含多张证书。Codex 也能容忍 OpenSSL 的 `TRUSTED CERTIFICATE` 标签，并会忽略同一 bundle 中格式正确的 `X509 CRL` 区块。如果文件为空、不可读或格式损坏，受影响的 Codex HTTP 或安全 websocket 连接会报告用户可见的错误信息，并指向这些环境变量。

## Notices

Codex 会把部分 UI 提示的“不再显示”标记存放在 `[notice]` 表下。

## Plan 模式默认值

`plan_mode_reasoning_effort` 允许你为 Plan 模式设置一个“Plan 模式专用”的默认 reasoning effort 覆盖值。未设置时，Plan 模式会使用内置 Plan preset 的默认值（当前为 `medium`）。当显式设置时（包括 `none`），它会覆盖 Plan preset。字符串 `none` 表示“不推理”（对 Plan 的显式覆盖），而不是“继承全局默认”。目前没有单独的配置项可用于表达“Plan 模式跟随全局默认”。

## Realtime Start Instructions

`experimental_realtime_start_instructions` 允许你替换 Codex 在 realtime 变为 active 时插入的内置 developer message。它只影响 prompt history 中的 realtime start message，不会改变 websocket 后端的 prompt 设置，也不会改变 realtime end/inactive message。

使用 Ctrl+C/Ctrl+D 退出时，会有一个约 1 秒的双击提示（`ctrl + c again to quit`）。
