# sync-zmemory-upstream state

- upstream_repo: https://cnb.cool/zls_nmtx/sohaha/nocturne_memory
- upstream_ref: 912d1deb47334a4241d99d4aa0ce917ee62b9786
- last_synced_hash: selective-sync-alias-trigger-governance-plus-discoverability
- last_synced_at_utc: 2026-03-29T13:25:00Z
- notes: selective sync baseline plus follow-up governance slices. Local `zmemory` now supports update/create/export, review/admin parity, alias/trigger parity, alias governance output, and skill recipe parity; `stats`/`doctor` expose alias/trigger metrics and alarms, `system://alias` now reports coveragePercent, concrete `manage-triggers` recommendations, review priority scoring (`reviewPriority` / `priorityScore`), and review context (`priorityReason` / `suggestedKeywords`) for missing-trigger alias nodes. This round does not advance search/boot/disclosure semantics; it only closes discoverability parity by documenting `system://alias|alias/<n>` in the core tool spec, adding CLI `zmemory export alias [--limit N]`, and aligning the local memory skill/README to prefer that stable entrypoint without importing upstream daemon/REST admin surfaces.
