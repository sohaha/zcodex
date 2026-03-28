# Zmemory Upstream Sync Checklist

Use this checklist for each `codex-zmemory` upstream sync.

## Baseline

- Read `/workspace/.codex/skills/sync-zmemory-upstream/STATE.md`
- Confirm the upstream repo is `https://cnb.cool/zls_nmtx/sohaha/nocturne_memory`
- Confirm whether this round is:
  - selective sync
  - behavior alignment
  - broader model import
- Read the current design constraints:
  - `/workspace/.agents/zmemory/prd.md`
  - `/workspace/.agents/zmemory/architecture.md`
  - `/workspace/.agents/zmemory/tech-review.md`

## Audit

- Review upstream memory behavior relevant to:
  - node or path model
  - alias or trigger semantics
  - glossary or search behavior
  - doctor or rebuild workflows
- Review local Codex-specific behavior relevant to:
  - independent `codex-zmemory` crate boundary
  - CLI thin wrapper in `codex-rs/cli/src/zmemory_cmd.rs`
  - core tool handler in `codex-rs/core/src/tools/handlers/zmemory.rs`
  - state DB and `codex_home/memories/` isolation

## Implementation

- Update `codex-rs/zmemory` first
- Update CLI and core adapters only if the shared contract changes
- Extend `codex-rs/cli/tests/zmemory.rs` when CLI behavior changes
- Extend `codex-rs/core/tests/suite/zmemory_e2e.rs` when tool behavior changes
- Update `.agents/zmemory/tasks.md`, `.agents/zmemory/qa-report.md`, and `.agents/zmemory/.meta/execution.json` when sync scope or verification changes
- Update `STATE.md` after a landed sync

## Validation

- `cd /workspace/codex-rs && just fmt`
- `cd /workspace/codex-rs && cargo nextest run -p codex-zmemory` when available, otherwise `cargo test -p codex-zmemory`
- `cd /workspace/codex-rs && cargo nextest run -p codex-cli --test zmemory` when CLI changed
- `cd /workspace/codex-rs && cargo nextest run -p codex-core --test suite zmemory` when core tool behavior changed
- `cd /workspace/codex-rs && just fix -p zmemory` for larger landed Rust changes

## Final Summary

- previous recorded upstream baseline
- new synced upstream ref or hash
- ported memory behavior
- preserved local divergence
- validation completed
- remaining risk
