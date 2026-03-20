# 回退渠道配置示例（中文注释）

下面这份示例适合：

- 主渠道走国内中转
- 第一备用走 OpenRouter
- 第二备用走另一条国内中转备线

把它按需改成你自己的域名、模型名和环境变量后，再放进 `~/.codex/config.toml`。

```toml
# 默认主模型
model = "gpt-5.1"

# 主渠道：先走国内中转
model_provider = "cn-relay"

# -----------------------------
# 主渠道：国内中转（OpenAI 兼容）
# -----------------------------
[model_providers.cn-relay]
# 渠道显示名称
name = "国内中转主线"

# 你的中转地址，通常是 OpenAI 兼容接口
base_url = "https://your-relay.example.com/v1"

# 从环境变量读取主线 key
env_key = "CN_RELAY_API_KEY"

# OpenAI 兼容接口一般用 responses
wire_api = "responses"

# 当前渠道失败后尽快切下一个，不在本渠道内部继续重试
request_max_retries = 0
stream_max_retries = 0

# 建议先关闭 websocket，减少多渠道切换时的链路复杂度
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

# -----------------------------
# 回退链
# 按顺序尝试：
# 1. 主渠道 cn-relay
# 2. openrouter
# 3. cn-relay-backup
# -----------------------------
fallback_providers = [
  # 主渠道失败后，先切 OpenRouter
  { provider = "openrouter", model = "openai/gpt-4.1" },

  # OpenRouter 再失败后，切到备用中转线路
  { provider = "cn-relay-backup", model = "gpt-4.1" },
]
```

如果你只需要单级回退，也可以继续使用旧配置：

```toml
model_provider = "cn-relay"
fallback_provider = "openrouter"
fallback_model = "openai/gpt-4.1"
```

建议同时准备好这些环境变量：

```bash
export CN_RELAY_API_KEY="你的主中转 Key"
export OPENROUTER_API_KEY="你的 OpenRouter Key"
export CN_RELAY_BACKUP_API_KEY="你的备线中转 Key"
```

说明：

- 回退只作用于“当前请求”，下一次新请求仍优先从主渠道开始。
- `request_max_retries = 0` / `stream_max_retries = 0` 更适合“尽快切线路”的场景。
- 如果备用渠道不支持主模型，请在 `fallback_providers` 里单独指定它自己的模型。
