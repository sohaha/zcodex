# Anthropic / Claude API 使用文档

本文档介绍 Codex 对 Claude（Anthropic Messages API）接口的使用方式。

## 前置条件

- 已获得有效的 Anthropic API Key。
- 在终端环境中设置 `ANTHROPIC_API_KEY`。

## 基本配置

在 `~/.codex/config.toml` 中启用内置 `anthropic` 模型提供方，并选择 Claude 模型：

```toml
model_provider = "anthropic"

[model_providers.anthropic]
name = "Anthropic"
model = "claude-3-5-haiku-20241022"
wire_api = "anthropic"
max_output_tokens = 16384
# 默认 base_url 为 https://api.anthropic.com/v1
```

说明：

- `wire_api = "anthropic"` 会使用 `/v1/messages` 的 Claude 接口。
- `model_providers.anthropic.model` 需填写 Claude 模型名，例如 `claude-3-5-haiku-20241022`、`claude-sonnet-4-20250514`。
- 如果同时设置了全局 `model`，当前 provider 自己的 `model` 会优先生效；但命令行显式传入的 `--model` 仍然优先。
- 当 provider 配置了 `env_key` 时，Codex 会同时发送 `x-api-key` 与 `Authorization: Bearer ...`，
  以兼容官方 Anthropic API 以及 Claude Code 风格的兼容网关。
- `max_output_tokens` 控制 Anthropic 请求中的 `max_tokens` 参数（默认 8192）。如果模型支持更大的输出窗口（例如 Claude Sonnet 4 支持 16384），设置此值可以避免长回复被截断。
- `skip_reasoning_popup = true` 可在模型选择器中跳过推理级别选择弹窗，直接使用默认推理级别。

## 自定义 API 地址

若使用代理或私有网关，可通过环境变量或配置覆盖：

```bash
export ANTHROPIC_BASE_URL="https://your-proxy.example.com/v1"
```

或：

```toml
[model_providers.anthropic]
base_url = "https://your-proxy.example.com/v1"
```

## 自定义可用模型列表

如果你的 Anthropic 中转暴露了内置目录之外的模型，可以在
`~/.codex/config.toml` 中直接声明模型列表：

```toml
model_provider = "anthropic"
model_catalog = ["MiniMax-M2.5-higspeed"]
```

说明：

- `model_catalog = ["..."]` 会直接把数组里的字符串当作模型 slug 列表使用。
- 对已知模型，Codex 会复用内置元数据；对未知模型，会生成一份最小可用元数据。
- 如果你更希望从文件加载，也可以写成
  `model_catalog = "/path/to/anthropic-models.json"`。
- `model_catalog_merge_json` 会在当前 provider 的内置模型列表之上合并额外模型。
- 如果同时设置 `model_catalog_json`，则先使用它作为基础列表，再叠加
  `model_catalog_merge_json`。
- 如果未设置 `model_catalog` 或 `model_catalog_json`，Codex 会尝试从当前 Anthropic provider 的
  `/models` 拉取远端模型目录；如果拉取失败，则回退到内置 Claude catalog。
- 合并按模型 `slug` 匹配；相同 `slug` 时，以 merge 文件中的定义为准。
- 对 Responses provider 来说，`model_catalog_merge_json` 不会关闭远端
  `/models` 刷新；它只是在当前目录快照之上追加/覆盖条目。

## Provider 级配置项

以下配置项可以在 `[model_providers.xxx]` 中设置，影响该 provider 下所有模型的行为：

| 配置项 | 类型 | 默认值 | 说明 |
|--------|------|--------|------|
| `max_output_tokens` | `i64` | `8192` | Anthropic 请求中的 `max_tokens`；Responses API 的 `max_output_tokens` |
| `skip_reasoning_popup` | `bool` | `false` | 模型选择器中跳过推理级别弹窗 |
| `model_context_window` | `i64` | 内置 | 覆盖模型上下文窗口大小 |
| `model_auto_compact_token_limit` | `i64` | 自动 | 触发自动压缩的 token 阈值 |
| `request_max_retries` | `u64` | `4` | 请求最大重试次数 |
| `stream_idle_timeout_ms` | `u64` | `300000` | 流超时时间 |
| `retry_base_delay_ms` | `u64` | `200` | 重试基础延迟 |

## 当前实现限制

当前仓库对 Claude 的支持，主要是将对话请求适配到 Anthropic 的
`/v1/messages` 接口。

当前实现中：

- 会话压缩（compaction）已支持
- 记忆摘要（memory summarization）仍不支持

原因不是 Claude 完全不能完成这类任务，而是当前代码没有为
Anthropic provider 实现对应的专用接口适配。

具体来说：

- 对话流请求已适配到 Anthropic `messages` API。
- 会话压缩在当前实现中通过 `messages` 接口模拟实现：Claude 生成结构化摘要，再由 Codex 在本地包装为 compaction 结果。
- 记忆摘要仍依赖 `memories/trace_summarize`。

因此，Claude 原生 API 虽然没有 `responses/compact` 专用 endpoint，
但当前仓库已经通过 `/v1/messages` 为 compaction 补上了兼容实现。

不过 `memories/trace_summarize` 仍然没有映射到 Anthropic Messages API，
所以 `wire_api = "anthropic"` 时，记忆摘要仍会返回
`unsupported operation`。

## 常见问题

### 提示没有认证

确认已设置 `ANTHROPIC_API_KEY`，并重新启动终端或刷新环境变量后再运行 Codex。

### 模型回复被截断

Anthropic API 的 `max_tokens` 默认值为 8192。如果模型的输出被截断，可在 provider 配置中设置 `max_output_tokens`：

```toml
[model_providers.anthropic]
max_output_tokens = 16384
```

此值会作为 Anthropic `/v1/messages` 请求中的 `max_tokens` 参数发送。
