# Configuration

For basic configuration instructions, see [this documentation](https://developers.openai.com/codex/config-basic).

For advanced configuration instructions, see [this documentation](https://developers.openai.com/codex/config-advanced).

For a full configuration reference, see [this documentation](https://developers.openai.com/codex/config-reference).

## Connecting to MCP servers

Codex can connect to MCP servers configured in `~/.codex/config.toml`. See the configuration reference for the latest MCP server options:

- https://developers.openai.com/codex/config-reference

MCP tools default to serialized calls. To mark every tool exposed by one server
as eligible for parallel tool calls, set `supports_parallel_tool_calls` on that
server:

```toml
[mcp_servers.docs]
command = "docs-server"
supports_parallel_tool_calls = true
```

Only enable parallel calls for MCP servers whose tools are safe to run at the
same time. If tools read and write shared state, files, databases, or external
resources, review those read/write race conditions before enabling this setting.

## MCP tool approvals

Codex stores approval defaults and per-tool overrides for custom MCP servers
under `mcp_servers` in `~/.codex/config.toml`. Set
`default_tools_approval_mode` on the server to apply a default to every tool,
and use per-tool `approval_mode` entries for exceptions:

```toml
[mcp_servers.docs]
command = "docs-server"
default_tools_approval_mode = "approve"

[mcp_servers.docs.tools.search]
approval_mode = "prompt"
```

## Apps (Connectors)

Use `$` in the composer to insert a ChatGPT connector; the popover lists accessible
apps. The `/apps` command lists available and installed apps. Connected apps appear first
and are labeled as connected; others are marked as can be installed.

## TUI pasted image compression

The TUI can automatically recompress images pasted with `Ctrl+V` before upload. Configure it in `~/.codex/config.toml`:

```toml
[tui]
auto_compress_pasted_images = true
pasted_image_max_width = 1280
pasted_image_max_height = 720
pasted_image_jpeg_quality = 85
```

Behavior:

- images larger than the configured width or height are resized proportionally,
- transparent images stay PNG,
- non-transparent images are encoded as both PNG and JPEG and Codex keeps the smaller result,
- invalid width/height/quality values fall back to the built-in defaults.

For implementation details and composer behavior, see `docs/tui-chat-composer.md`.

## Notify

Codex can run a notification hook when the agent finishes a turn. See the configuration reference for the latest notification settings:

- https://developers.openai.com/codex/config-reference

When Codex knows which client started the turn, the legacy notify JSON payload also includes a top-level `client` field. The TUI reports `codex-tui`, and the app server reports the `clientInfo.name` value from `initialize`.

## Memories / zmemory

Rust Codex CLI now enables native memory and `zmemory` independently by
default:

- `native_memories`: controls the built-in read-only memory pipeline and
  `get_memory`
- `zmemory`: controls the embedded writable SQLite-backed memory tool

To explicitly disable one of them for the current run:

```shell
codex --disable native_memories
codex --disable zmemory
```

To disable one persistently in `~/.codex/config.toml`:

```toml
[features]
native_memories = false
zmemory = false
```

`[memories]` only configures the native memory pipeline. `zmemory` now has its
own config block:

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

`[zmemory]` fields:

- `path`: optional database path override
- `namespace`: optional runtime namespace override for namespace-aware databases
- `valid_domains`: optional runtime writable domains override
- `core_memory_uris`: optional runtime boot anchor override

Runtime precedence is:

1. `[zmemory]` in `config.toml`
2. environment variables (`VALID_DOMAINS`, `CORE_MEMORY_URIS`)
3. product defaults

- Absolute paths are used directly.
- Relative paths are resolved against the active repo root when Codex is
  inside a git repository, otherwise against the current working directory.
- When `[zmemory].path` is unset, Codex now uses the project-scoped default
  database at `$CODEX_HOME/zmemory/projects/<project-key>/zmemory.db`.
- If you want one shared global database across projects, configure it
  explicitly:

```toml
[zmemory]
path = "/absolute/path/to/.codex/zmemory/zmemory.db"
```

You can verify the active resolution with:

```shell
codex zmemory stats --json
codex zmemory doctor --json
codex zmemory read system://workspace --json
codex zmemory read system://defaults --json
```

The stable diagnostic payload is `result.pathResolution` (and the same
`dbPath` / `workspaceKey` / `source` / `reason` fields are mirrored at the top
level of `result` for quick checks):

```json
{
  "dbPath": "/home/me/.codex/zmemory/projects/my-repo-a1b2c3d4e5f6/zmemory.db",
  "workspaceKey": "my-repo-a1b2c3d4e5f6",
  "source": "projectScoped",
  "reason": "defaulted to project scope /home/me/.codex/zmemory/projects/my-repo-a1b2c3d4e5f6/zmemory.db from repo root /workspace/my-repo"
}
```

`system://workspace` is the runtime fact view for the active session. It adds
fields such as `hasExplicitZmemoryPath`, `defaultDbPath`, `dbPathDiffers`,
`defaultWorkspaceKey`, `bootHealthy`, and an embedded `boot` snapshot so you
can tell whether the current session is using the default project database or an explicit
override. It always reports the currently effective runtime profile, including
configured `validDomains` and `coreMemoryUris`.

When `Feature::Zmemory` is enabled, `codex-core` may proactively persist
high-confidence naming/addressing preferences into the active `zmemory`
database. That orchestration still uses the normal `zmemory` action layer:
inspect `system://workspace` first, then read/write the canonical URIs
`core://my_user`, `core://agent`, and `core://agent/my_user`, and finally read
back the written URI for verification. Failures are surfaced as observable
warnings rather than silent success.

`system://defaults` is the product-default fact view. It reports the default
`validDomains`, `coreMemoryUris`, and default DB path policy without conflating
those values with the current workspace state. User config changes the
workspace/runtime view, not the defaults view.

If a direct `read <uri>` misses or `search` returns no matches, use
`system://workspace`, `stats`, `doctor`, and `system://alias` before concluding
that no durable memory exists at all; an unhealthy boot graph or missing
triggers can mean the issue is recall coverage rather than missing data.

For a dedicated `zmemory` usage guide covering commands, system views, and
troubleshooting, see `docs/zmemory.md`.

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
Named tool choice is supported via `tool_choice = "required:<tool_name>"`.
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


## Retry and timeout configuration

Model providers support several retry and timeout options:

- `request_max_retries`: Maximum number of times to retry a failed HTTP request to this provider.
- `stream_max_retries`: Number of times to retry reconnecting a dropped streaming response before failing.
- `stream_idle_timeout_ms`: Idle timeout (in milliseconds) to wait for activity on a streaming response before treating the connection as lost.
- `websocket_connect_timeout_ms`: Maximum time (in milliseconds) to wait for a websocket connection attempt before treating it as failed.
- `retry_base_delay_ms`: Base delay (in milliseconds) for retry backoff. The actual delay between retries will be this value multiplied by 2^(attempt-1) with jitter. Defaults to `200`.

Example:

```toml
[model_providers.myprovider]
request_max_retries = 4
stream_max_retries = 5
stream_idle_timeout_ms = 300000
websocket_connect_timeout_ms = 15000
retry_base_delay_ms = 500  # Longer base delay for slower networks
```

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
