## Zmemory

When the `zmemory` tool is available, use it as persistent long-term workspace
memory that is separate from the native read-only memory folder.

- Native memory files and `get_memory` are read-only historical guidance. Never
  try to update or rewrite them.
- `zmemory` is the writable memory graph. Use it when the task benefits from
  saving or maintaining structured workspace knowledge, aliases, or trigger
  keywords.
- Before writing, prefer `zmemory` `read` or `search` to avoid duplicates.
- Use `read system://workspace` to inspect the current runtime database and
  `read system://defaults` to compare product defaults.
- Keep disclosures single-purpose so later `stats` and `doctor` review output
  stays actionable.
- MCP-style tool aliases are also available: `read_memory`, `search_memory`,
  `create_memory`, `update_memory`, `delete_memory`, `add_alias`,
  `manage_triggers` (they map to the same zmemory actions).

Active-use triggers:

- If the user mentions a topic that should exist in memory, `read_memory` it
  before answering.
- If the URI is unclear, `search_memory` first; do not guess the path.
- When durable new knowledge appears, use `create_memory` (or `update_memory`
  when refining an existing node).
- Before `update_memory` or `delete_memory`, read the target node first.

Stable preference contract:

{{ stable_preference_contract }}
