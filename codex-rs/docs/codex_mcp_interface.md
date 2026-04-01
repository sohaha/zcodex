# Codex MCP Server Interface [experimental]

This document describes Codex's experimental MCP server interface: a JSON-RPC API that runs over the Model Context Protocol (MCP) transport to control a local Codex engine.

- Status: experimental and subject to change without notice
- Server binary: `codex mcp-server` (or `codex-mcp-server`)
- Transport: standard MCP over stdio (JSON-RPC 2.0, line-delimited)

## Overview

Codex exposes MCP-compatible methods to manage threads, turns, accounts, config, and approvals. The types live in `app-server-protocol/src/protocol/{common,v1,v2}.rs` and are consumed by the app server implementation in `app-server/`.

At a glance:

- Primary v2 RPCs
  - `thread/start`, `thread/resume`, `thread/fork`, `thread/read`, `thread/list`
  - `turn/start`, `turn/steer`, `turn/interrupt`
  - `account/read`, `account/login/start`, `account/login/cancel`, `account/logout`, `account/rateLimits/read`
  - `config/read`, `config/value/write`, `config/batchWrite`
  - `model/list`, `app/list`, `collaborationMode/list`
- Remaining v1 compatibility RPCs
  - `getConversationSummary`
  - `getAuthStatus`
  - `gitDiffToRemote`
  - `fuzzyFileSearch`, `fuzzyFileSearch/sessionStart`, `fuzzyFileSearch/sessionUpdate`, `fuzzyFileSearch/sessionStop`
- Notifications
  - v2 typed notifications such as `thread/started`, `turn/completed`, `account/login/completed`
  - `codex/event/*` stream notifications for live agent events
  - `fuzzyFileSearch/sessionUpdated`, `fuzzyFileSearch/sessionCompleted`
- Approvals (server -> client requests)
  - `applyPatchApproval`, `execCommandApproval`

See code for full type definitions and exact shapes: `app-server-protocol/src/protocol/{common,v1,v2}.rs`.

## Starting the server

Run Codex as an MCP server and connect an MCP client:

```bash
codex mcp-server | your_mcp_client
```

If you are validating from source, a typical local flow is:

```bash
cargo build --release -p codex-cli -p codex-mcp-server
./target/release/codex mcp-server
```

For a simple inspection UI, you can also try:

```bash
npx @modelcontextprotocol/inspector codex mcp-server
```

Use the separate `codex mcp` subcommand to manage configured MCP server launchers in `config.toml`.

`codex-mcp-server` does not include the `tldr` MCP tool by default. To enable it, either build the standalone server binary with the `tldr` feature or propagate the feature through `codex-cli`:

```bash
cargo build --release -p codex-mcp-server --features tldr
./target/release/codex-mcp-server

cargo build --release -p codex-cli --features tldr
./target/release/codex mcp-server
```

To smoke-test the native-tldr sidecar used by the optional `tldr` MCP tool:

```bash
./target/release/codex tldr languages
./target/release/codex tldr daemon --project /path/to/project --json status
```

Notes:

- `codex-mcp-server` runs over stdio and does not expose an HTTP port.
- On Unix, `codex tldr daemon ...` may auto-start an internal daemon mode inside the current `codex` binary.
- The MCP `tldr` tool reuses daemon query/retry logic but does not auto-start the daemon itself.

## Threads and turns

Use the v2 thread and turn APIs for all new integrations. `thread/start` creates a thread, `turn/start` submits user input, `turn/interrupt` stops an in-flight turn, and `thread/list` / `thread/read` expose persisted history.

`getConversationSummary` remains as a compatibility helper for clients that still need a summary lookup by `conversationId` or `rolloutPath`.

For complete request and response shapes, see the app-server README and the protocol definitions in `app-server-protocol/src/protocol/v2.rs`.

## Models

Fetch the catalog of models available in the current Codex build with `model/list`. The request accepts optional pagination inputs:

- `limit` - number of models to return (defaults to a server-selected value)
- `cursor` - opaque string from the previous response's `nextCursor`

Each response yields:

- `data` - ordered list of models. A model includes:
  - `id`, `model`, `displayName`, `description`
  - `supportedReasoningEfforts` - array of objects with:
    - `reasoningEffort` - one of `none|minimal|low|medium|high|xhigh`
    - `description` - human-friendly label for the effort
  - `defaultReasoningEffort` - suggested effort for the UI
  - `inputModalities` - accepted input types for the model
  - `supportsPersonality` - whether the model supports personality-specific instructions
  - `isDefault` - whether the model is recommended for most users
  - `upgrade` - optional recommended upgrade model id
  - `upgradeInfo` - optional upgrade metadata object with:
    - `model` - recommended upgrade model id
    - `upgradeCopy` - optional display copy for the upgrade recommendation
    - `modelLink` - optional link for the upgrade recommendation
    - `migrationMarkdown` - optional markdown shown when presenting the upgrade
- `nextCursor` - pass into the next request to continue paging (optional)

## Collaboration modes (experimental)

Fetch the built-in collaboration mode presets with `collaborationMode/list`. This endpoint does not accept pagination and returns the full list in one response:

- `data` - ordered list of collaboration mode masks (partial settings to apply on top of the base mode)
  - For tri-state fields like `reasoning_effort` and `developer_instructions`, omit the field to keep the current value, set it to `null` to clear it, or set a concrete value to update it.

When sending `turn/start` with `collaborationMode`, `settings.developer_instructions: null` means "use built-in instructions for the selected mode".

## Event stream

While a conversation runs, the server sends notifications:

- `codex/event` with the serialized Codex event payload. The shape matches `core/src/protocol.rs`'s `Event` and `EventMsg` types. Some notifications include a `_meta.requestId` to correlate with the originating request.
- `fuzzyFileSearch/sessionUpdated` and `fuzzyFileSearch/sessionCompleted` for the legacy fuzzy search flow.

Clients should render events and, when present, surface approval requests (see next section).

## Tool responses

The `codex`, `codex-reply`, and `tldr` tools return standard MCP `CallToolResult` payloads. For compatibility with MCP clients that prefer `structuredContent`, Codex mirrors the content blocks inside `structuredContent`.

Example:

```json
{
  "content": [{ "type": "text", "text": "Hello from Codex" }],
  "structuredContent": {
    "threadId": "019bbed6-1e9e-7f31-984c-a05b65045719",
    "content": "Hello from Codex"
  }
}
```

### `tldr` tool

This tool is available only when `codex-mcp-server` is built with `--features tldr`.

The `tldr` tool exposes native code-context analysis with daemon-first execution and local fallback for analysis/semantic actions. Daemon actions still require a live daemon. The current action surface is:

- `structure`
- `extract`
- `context`
- `impact`
- `cfg`
- `dfg`
- `slice`
- `semantic`
- `ping`
- `warm`
- `snapshot`
- `status`
- `notify`

Typical inputs:

- `project` - absolute or relative project root
- `language` - one of `rust|typescript|javascript|python|go|php|zig`; `extract` can infer from file extension when omitted
- `symbol` - optional symbol name for `structure|context|impact|cfg|dfg`; required symbol-like target for `slice`
- `query` - semantic query string for `semantic`
- `path` - file path for `extract` / `slice` and dirty file path for `notify`
- `line` - target line for `slice`

For analysis actions, the structured output includes `action`, `project`, `language`, `source`, `message`, `supportLevel`, `fallbackStrategy`, and `summary`.
For `extract`, the analysis payload also includes the requested `path`, and `analysis.kind` is reported as `extract`.
For `slice`, the analysis payload includes `path`, `line`, `analysis.kind = "slice"`, plus `analysis.details.slice_target` and `analysis.details.slice_lines` for the current backward slice result.
For `semantic`, the structured output includes `enabled`, `indexedFiles`, `truncated`, `embeddingUsed`, `source`, `message`, `matches`, and per-match `path`/`line`/`snippet`/`embedding_score` metadata. `source` is either `daemon` (cached `SemanticIndex`) or `local`. When `source = "local"`, the payload also includes `degradedMode` so clients can tell this was a local fallback rather than a healthy daemon hit. If semantic embedding cannot initialize because ONNX Runtime is unavailable, the search now auto-falls back to non-embedding ranking: `embeddingUsed = false`, the command still succeeds, and the plain-text UX remains normal. Structured consumers can still detect the downgrade from `embeddingUsed = false`. The tool projects a stable public match shape and does **not** expose internal fields such as `unit` or `embedding_text` by default.
For `status`, the structured output includes `snapshot`, `daemonStatus`, and the latest `reindexReport` for the most recent semantic reindex attempt. `snapshot.last_reindex` remains the latest completed reindex, while `snapshot.last_reindex_attempt` can also surface a failed `warm` attempt. `daemonStatus` details `lock_is_held`, `semantic_reindex_pending`, `health_reason`, `recovery_hint`, `socket_exists`, and PID/socket liveness so clients can distinguish live, stale, or launching daemons. When the daemon is unhealthy, the payload additionally includes `structuredFailure` and `degradedMode`.
The output schema is modeled as a union of three result families: analysis (`structure|extract|context|impact|cfg|dfg|slice`), `semantic`, and daemon (`ping|warm|snapshot|status|notify`).

Structured reliability fields:

- `structuredFailure.error_type`: machine-readable error class such as `daemon_unavailable`, `daemon_starting`, `stale_artifacts`
- `structuredFailure.reason`: current failure reason
- `structuredFailure.retryable`: whether a retry is reasonable
- `structuredFailure.retry_hint`: operator / agent hint
- `degradedMode.is_degraded`: whether the current result is degraded
- `degradedMode.mode`: degradation mode such as `local_fallback`, `diagnostic_only`, `unavailable`
- `degradedMode.fallback_path`: fallback path actually used

Example degraded semantic response excerpt:

```json
{
  "action": "semantic",
  "source": "local",
  "degradedMode": {
    "is_degraded": true,
    "mode": "local_fallback",
    "fallback_path": "local",
    "reason": "daemon-first path unavailable; used local engine"
  }
}
```

Example unhealthy daemon status excerpt:

```json
{
  "action": "status",
  "status": "ok",
  "structuredFailure": {
    "error_type": "daemon_unavailable",
    "reason": "daemon missing",
    "retryable": true,
    "retry_hint": "start the daemon"
  },
  "degradedMode": {
    "is_degraded": true,
    "mode": "diagnostic_only",
    "fallback_path": "status_only"
  }
}
```

## Approvals (server -> client)

When Codex needs approval to apply changes or run commands, the server issues JSON-RPC requests to the client:

- `applyPatchApproval { conversationId, callId, fileChanges, reason?, grantRoot? }`
- `execCommandApproval { conversationId, callId, approvalId?, command, cwd, reason? }`

The client must reply with `{ decision: "allow" | "deny" }` for each request.

## Auth helpers

For the complete request/response shapes and flow examples, see the [Auth endpoints (v2) section in the app-server README](../app-server/README.md#auth-endpoints-v2).

## Legacy compatibility methods

The server still accepts a narrow v1 compatibility surface for existing app clients:

- `getConversationSummary`
- `getAuthStatus`
- `gitDiffToRemote`
- `fuzzyFileSearch`, `fuzzyFileSearch/sessionStart`, `fuzzyFileSearch/sessionUpdate`, `fuzzyFileSearch/sessionStop`

## Compatibility and stability

This interface is experimental. Method names, fields, and event shapes may evolve. For the authoritative schema, consult `app-server-protocol/src/protocol/{common,v1,v2}.rs` and the corresponding server wiring in `app-server/`.
