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
