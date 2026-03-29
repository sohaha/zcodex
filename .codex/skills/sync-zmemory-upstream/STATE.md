# sync-zmemory-upstream state

- upstream_repo: https://cnb.cool/zls_nmtx/sohaha/nocturne_memory
- upstream_ref: 912d1deb47334a4241d99d4aa0ce917ee62b9786
- last_synced_hash: selective-sync-create-update-export-cli
- last_synced_at_utc: 2026-03-29T01:00:00Z
- notes: selective sync baseline plus two landed slices. Local `zmemory` now supports upstream-inspired update patch/append/metadata semantics, create compatibility via `parentUri + title`, and a local CLI-only `export` command for system views. `export` intentionally stays a thin wrapper over `read system://...` rather than a new REST/daemon/admin surface. Verified with targeted crate/CLI/core-handler tests; broader upstream admin surfaces remain intentionally out of scope.
