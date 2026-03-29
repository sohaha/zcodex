# sync-zmemory-upstream state

- upstream_repo: https://cnb.cool/zls_nmtx/sohaha/nocturne_memory
- upstream_ref: 912d1deb47334a4241d99d4aa0ce917ee62b9786
- last_synced_hash: selective-sync-alias-trigger-parity
- last_synced_at_utc: 2026-03-29T06:50:00Z
- notes: selective sync baseline plus four landed slices. Local `zmemory` now supports the prior update/create/export slices, review/admin parity (orphaned/deprecated metrics), and alias/trigger parity signals; `stats` exposes alias/trigger counts, `doctor` issues report alias nodes missing triggers, and the repo-root `memory` skill stays a minimal wrapper around existing actions without importing upstream daemon/REST admin surfaces.
- notes: selective sync baseline plus five landed slices. Local `zmemory` now supports update/create/export, review/admin parity, alias/trigger parity, and skill recipe parity. `stats`/`doctor` expose alias/trigger metrics & alarms, and `.codex/skills/memory/references` 里新增 project-init / recall/resume recipes that reuse the existing CLI flow.
