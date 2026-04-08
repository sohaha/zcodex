# sync-zmemory-upstream state

- upstream_repo: https://cnb.cool/zls_nmtx/sohaha/nocturne_memory
- upstream_ref: a574c2d92bcfe377441e35d27f883fe1cb39e1e1
- last_synced_hash: selective-sync-compat-adapter-readonly-web-reuse
- last_synced_at_utc: 2026-04-08T09:58:45Z
- notes: local selective sync now covers namespace-aware runtime/db contracts, review diff service contracts, and a thin upstream-compatible HTTP adapter for web reuse without widening `codex-zmemory` into a second backend. New parity surface is `codex-rs/zmemory/src/compat/*` plus `codex zmemory serve-compat`, which exposes `/api/browse/*`, `/api/review/groups*`, and `/api/maintenance/*` against the same dbPath/namespace resolution as CLI/core. Read-only review inspection is implemented; review mutations (`rollback` / `approve`-adjacent deletes) intentionally stay explicit `501` until a later issue lands real write semantics. Validation for this selective sync used `cargo nextest run -p codex-zmemory`, `cargo nextest run -p codex-cli --test zmemory`, and a temp-`CODEX_HOME` curl check that compared `/api/browse/node`, `/api/review/groups`, `/api/review/groups/{uuid}/diff`, and `/api/maintenance/stats` with CLI `read` / `stats` against the same `system://workspace` dbPath.
