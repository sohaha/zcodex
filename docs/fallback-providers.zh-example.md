# 回退渠道配置示例（中文注释）

本文提供 3 套可直接改造的模板：

- 模板 A：`OpenRouter + 国内中转`
- 模板 B：`纯国内中转双备线`
- 模板 C：`Azure OpenAI + OpenRouter`

把其中一套按需改成你自己的域名、部署名、模型名和环境变量后，再放进 `~/.codex/config.toml`。

## 通用说明

- 回退只作用于“当前请求”，下一次新请求仍优先从主渠道开始。
- `fallback_model` 是可选的：不指定时会自动选择备用渠道的默认模型或主模型
- `request_max_retries = 0` / `stream_max_retries = 0` 更适合“尽快切线路”的场景。
- 如果备用渠道不支持主模型，请在 `fallback_providers` 里单独指定它自己的模型。
- 多级回退使用 `fallback_providers`；如果你只需要一级回退，也可以继续使用 `fallback_provider` + `fallback_model`。

## 模板 A：OpenRouter + 国内中转

适合：

- 主渠道走国内中转
- 第一备用走 OpenRouter
- 第二备用走另一条国内中转备线

```toml
# 默认主模型
model = "gpt-5.1"

# 主渠道：先走国内中转
model_provider = "cn-relay"

# -----------------------------
# 主渠道：国内中转（OpenAI 兼容）
# -----------------------------
[model_providers.cn-relay]
name = "国内中转主线"
base_url = "https://your-relay.example.com/v1"
env_key = "CN_RELAY_API_KEY"
wire_api = "responses"
request_max_retries = 0
stream_max_retries = 0
supports_websockets = false

# -----------------------------
# 备用渠道 1：OpenRouter
# -----------------------------
[model_providers.openrouter]
name = "OpenRouter"
base_url = "https://openrouter.ai/api/v1"
env_key = "OPENROUTER_API_KEY"
wire_api = "responses"
request_max_retries = 0
stream_max_retries = 0
supports_websockets = false

# -----------------------------
# 备用渠道 2：国内中转备线
# -----------------------------
[model_providers.cn-relay-backup]
name = "国内中转备线"
base_url = "https://your-backup-relay.example.com/v1"
env_key = "CN_RELAY_BACKUP_API_KEY"
wire_api = "responses"
request_max_retries = 0
stream_max_retries = 0
supports_websockets = false

fallback_providers = [
  # 主渠道失败后，先切 OpenRouter
  { provider = "openrouter", model = "openai/gpt-4.1" },

  # OpenRouter 再失败后，切到备用中转线路
  { provider = "cn-relay-backup", model = "gpt-4.1" },
]
```

建议准备的环境变量：

```bash
export CN_RELAY_API_KEY="你的主中转 Key"
export OPENROUTER_API_KEY="你的 OpenRouter Key"
export CN_RELAY_BACKUP_API_KEY="你的备线中转 Key"
```

## 模板 B：纯国内中转双备线

适合：

- 不想走 OpenRouter
- 只想在多条国内中转线路之间切换

```toml
# 默认主模型
model = "gpt-4.1"

# 主渠道：国内中转主线
model_provider = "relay-a"

# -----------------------------
# 主渠道：中转 A
# -----------------------------
[model_providers.relay-a]
name = "中转 A"
base_url = "https://relay-a.example.com/v1"
env_key = "RELAY_A_API_KEY"
wire_api = "responses"
request_max_retries = 0
stream_max_retries = 0
supports_websockets = false

# -----------------------------
# 备用渠道 1：中转 B
# -----------------------------
[model_providers.relay-b]
name = "中转 B"
base_url = "https://relay-b.example.com/v1"
env_key = "RELAY_B_API_KEY"
wire_api = "responses"
request_max_retries = 0
stream_max_retries = 0
supports_websockets = false

# -----------------------------
# 备用渠道 2：中转 C
# -----------------------------
[model_providers.relay-c]
name = "中转 C"
base_url = "https://relay-c.example.com/v1"
env_key = "RELAY_C_API_KEY"
wire_api = "responses"
request_max_retries = 0
stream_max_retries = 0
supports_websockets = false

fallback_providers = [
  # A 失败后切 B
  { provider = "relay-b", model = "gpt-4.1" },

  # B 再失败后切 C
  { provider = "relay-c", model = "gpt-4.1" },
]
```

建议准备的环境变量：

```bash
export RELAY_A_API_KEY="中转 A Key"
export RELAY_B_API_KEY="中转 B Key"
export RELAY_C_API_KEY="中转 C Key"
```

## 模板 C：Azure OpenAI + OpenRouter

适合：

- 主渠道走 Azure OpenAI
- 失败后回退到 OpenRouter

```toml
# 主模型按你的 Azure 部署对应能力来选
model = "gpt-4.1"

# 主渠道：Azure OpenAI
model_provider = "azure-openai"

# -----------------------------
# 主渠道：Azure OpenAI
# 说明：
# 1. 这里走的是 OpenAI 兼容 responses 接口
# 2. base_url 需要替换成你的 Azure OpenAI endpoint
# 3. query_params 里通常要带 api-version
# 4. 如果你的网关要求 api-key 请求头，可以用 http_headers / env_http_headers
# -----------------------------
[model_providers.azure-openai]
name = "Azure OpenAI"
base_url = "https://your-resource.openai.azure.com/openai"
env_key = "AZURE_OPENAI_API_KEY"
wire_api = "responses"
request_max_retries = 0
stream_max_retries = 0
supports_websockets = false

[model_providers.azure-openai.query_params]
api-version = "2025-01-01-preview"

# 如果你的 Azure 接入层需要额外 header，可以改成下面这种：
# [model_providers.azure-openai.env_http_headers]
# "api-key" = "AZURE_OPENAI_API_KEY"

# -----------------------------
# 备用渠道：OpenRouter
# -----------------------------
[model_providers.openrouter]
name = "OpenRouter"
base_url = "https://openrouter.ai/api/v1"
env_key = "OPENROUTER_API_KEY"
wire_api = "responses"
request_max_retries = 0
stream_max_retries = 0
supports_websockets = false

fallback_providers = [
  # Azure 主线路失败后切 OpenRouter
  { provider = "openrouter", model = "openai/gpt-4.1" },
]
```

建议准备的环境变量：

```bash
export AZURE_OPENAI_API_KEY="你的 Azure OpenAI Key"
export OPENROUTER_API_KEY="你的 OpenRouter Key"
```

## 单级回退的旧写法

如果你只需要一级回退，也可以继续使用：

```toml
model_provider = "cn-relay"
fallback_provider = "openrouter"
fallback_model = "openai/gpt-4.1"
```

**模型选择优先级**：
1. 如果配置了 `fallback_model`，使用指定的模型
2. 否则使用备用渠道的默认模型（如果配置了）
3. 最后才使用主请求的模型
