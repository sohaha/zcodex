# sync-native-tldr-reference state

- upstream_repo: https://github.com/parcadei/llm-tldr
- upstream_ref: main
- last_synced_hash: c6494afdbe617b56c04ce562651117f9dd437d38
- last_synced_at_utc: 2026-03-28T07:41:42Z
- notes: selective sync extended imports/importers/search/project-graph/diagnostics/doctor plus semantic phase-2 local cache reuse into codex-native-tldr, CLI, and MCP while preserving local daemon-first lifecycle behavior; real embedding provider is wired via fastembed, Kotlin remains deferred because of tree-sitter dependency conflicts.
