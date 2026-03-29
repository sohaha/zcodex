# sync-zmemory-upstream state

- upstream_repo: https://cnb.cool/zls_nmtx/sohaha/nocturne_memory
- upstream_ref: 912d1deb47334a4241d99d4aa0ce917ee62b9786
- last_synced_hash: selective-sync-boot-domain-disclosure-governance
- last_synced_at_utc: 2026-03-29T15:20:00Z
- notes: selective sync baseline plus governance follow-ups. Local `zmemory` now keeps the earlier update/create/export/review/alias parity slices, and this round advances the remaining boot/domain/disclosure gap without widening architecture: `VALID_DOMAINS` now gates writable/readable non-system domains, `system://boot` now follows configured `CORE_MEMORY_URIS` anchors (reporting missing anchors explicitly), and `stats`/`doctor`/memory-skill review flow now expose disclosure governance via `pathsMissingDisclosure` and `disclosuresNeedingReview`. The implementation still stays local-first: embedded Rust crate, CLI thin wrapper, no upstream daemon/REST admin surfaces.
