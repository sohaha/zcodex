# Configuration

For basic configuration instructions, see [this documentation](https://developers.openai.com/codex/config-basic).

For advanced configuration instructions, see [this documentation](https://developers.openai.com/codex/config-advanced).

For a full configuration reference, see [this documentation](https://developers.openai.com/codex/config-reference).

## Connecting to MCP servers

Codex can connect to MCP servers configured in `~/.codex/config.toml`. See the configuration reference for the latest MCP server options:

- https://developers.openai.com/codex/config-reference

## MCP tool approvals

Codex stores per-tool approval overrides for custom MCP servers under
`mcp_servers` in `~/.codex/config.toml`:

```toml
[mcp_servers.docs.tools.search]
approval_mode = "approve"
```

## Apps (Connectors)

Use `$` in the composer to insert a ChatGPT connector; the popover lists accessible
apps. The `/apps` command lists available and installed apps. Connected apps appear first
and are labeled as connected; others are marked as can be installed.

## Notify

Codex can run a notification hook when the agent finishes a turn. See the configuration reference for the latest notification settings:

- https://developers.openai.com/codex/config-reference

When Codex knows which client started the turn, the legacy notify JSON payload also includes a top-level `client` field. The TUI reports `codex-tui`, and the app server reports the `clientInfo.name` value from `initialize`.

## JSON Schema

The generated JSON Schema for `config.toml` lives at `codex-rs/core/config.schema.json`.

## Built-in model providers

Codex ships with built-in `openai`, `anthropic`, `ollama`, and `lmstudio`
model provider entries. For Anthropic-compatible setups, use
`wire_api = "anthropic"` and provide credentials with `ANTHROPIC_API_KEY`
unless you override the provider config. The built-in `anthropic` provider
defaults to `https://api.anthropic.com/v1`, and you can override that with
`ANTHROPIC_BASE_URL` or a custom `model_providers.<id>.base_url` entry.
User-defined `model_providers` entries may also override built-in IDs such as
`openai` when you want to change the default provider wiring.
You can also set `model_providers.<id>.model` to give that provider its own
default model; when present, it takes precedence over the global `model`
setting for requests sent through that provider.

Example overriding the built-in OpenAI provider to use Chat Completions:

```toml
model_provider = "openai"

[model_providers.openai]
name = "OpenAI Chat"
base_url = "https://api.openai.com/v1"
env_key = "OPENAI_API_KEY"
wire_api = "chat"
```

When `wire_api = "chat"` is selected, Codex uses `/v1/chat/completions`.
This path does not support hosted-only tools such as `web_search` or
`image_generation`, and only `user` messages may include image inputs.
Those are Chat Completions API limits, not Codex-only restrictions. Use
`wire_api = "responses"` when you need hosted tools.

To retry a failed primary request against another provider, set
`fallback_provider` to a provider ID from `model_providers` (or a built-in
provider) and optionally set `fallback_model` to the model slug that fallback
request should use. Codex retries the fallback only for the current request;
new requests still start with the primary `model_provider`.

For multi-step fallback, use `fallback_providers` in priority order:

```toml
model_provider = "openai"

fallback_providers = [
  { provider = "anthropic", model = "claude-sonnet-4-5" },
  { provider = "openrouter", model = "openai/gpt-4.1" },
]
```

Example with a relay primary provider plus OpenRouter and a backup relay:

```toml
model = "gpt-5.1"
model_provider = "cn-relay"

[model_providers.cn-relay]
# Primary relay (OpenAI-compatible)
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
  # Try OpenRouter first when the primary relay fails.
  { provider = "openrouter", model = "openai/gpt-4.1" },
  # Then fall back to a secondary relay.
  { provider = "cn-relay-backup", model = "gpt-4.1" },
]
```

`fallback_provider` + `fallback_model` remain supported for a single fallback.
When both styles are present, Codex treats them as part of the same ordered
fallback list for the current request only.

For Chinese-commented example configs, including:

- `OpenRouter + relay`
- `relay + relay backup`
- `Azure OpenAI + OpenRouter`

see `docs/fallback-providers.zh-example.md`.

## Custom model catalogs

Codex supports two startup-only config keys for overriding available models:

- `model_catalog_json` replaces the bundled catalog for the active provider.
- `model_catalog_merge_json` merges additional models into the bundled catalog.

If both are set, Codex uses `model_catalog_json` as the base catalog and then
applies `model_catalog_merge_json` on top. Merge entries match by `slug`; when
the same slug appears in both catalogs, the merge entry wins.

For Responses-based providers, `model_catalog_merge_json` does not disable
remote `/models` refreshes; it overlays additional entries on top of the
built-in/remote catalog snapshot instead.

This is especially useful for Anthropic-compatible proxies that expose model
slugs not present in the built-in Claude catalog.

## SQLite State DB

Codex stores the SQLite-backed state DB under `sqlite_home` (config key) or the
`CODEX_SQLITE_HOME` environment variable. When unset, WorkspaceWrite sandbox
sessions default to a temp directory; other modes default to `CODEX_HOME`.

## Custom CA Certificates

Codex can trust a custom root CA bundle for outbound HTTPS and secure websocket
connections when enterprise proxies or gateways intercept TLS. This applies to
login flows and to Codex's other external connections, including Codex
components that build reqwest clients or secure websocket clients through the
shared `codex-client` CA-loading path and remote MCP connections that use it.

Set `CODEX_CA_CERTIFICATE` to the path of a PEM file containing one or more
certificate blocks to use a Codex-specific CA bundle. If
`CODEX_CA_CERTIFICATE` is unset, Codex falls back to `SSL_CERT_FILE`. If
neither variable is set, Codex uses the system root certificates.

`CODEX_CA_CERTIFICATE` takes precedence over `SSL_CERT_FILE`. Empty values are
treated as unset.

The PEM file may contain multiple certificates. Codex also tolerates OpenSSL
`TRUSTED CERTIFICATE` labels and ignores well-formed `X509 CRL` sections in the
same bundle. If the file is empty, unreadable, or malformed, the affected Codex
HTTP or secure websocket connection reports a user-facing error that points
back to these environment variables.

## Notices

Codex stores "do not show again" flags for some UI prompts under the `[notice]` table.

## Plan mode defaults

`plan_mode_reasoning_effort` lets you set a Plan-mode-specific default reasoning
effort override. When unset, Plan mode uses the built-in Plan preset default
(currently `medium`). When explicitly set (including `none`), it overrides the
Plan preset. The string value `none` means "no reasoning" (an explicit Plan
override), not "inherit the global default". There is currently no separate
config value for "follow the global default in Plan mode".

## Realtime start instructions

`experimental_realtime_start_instructions` lets you replace the built-in
developer message Codex inserts when realtime becomes active. It only affects
the realtime start message in prompt history and does not change websocket
backend prompt settings or the realtime end/inactive message.

Ctrl+C/Ctrl+D quitting uses a ~1 second double-press hint (`ctrl + c again to quit`).
