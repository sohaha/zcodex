---
name: sync-zmemory-upstream
description: Sync the local `codex-zmemory` implementation against the upstream `nocturne_memory` reference repo with minimal, verifiable Rust-side parity updates. Use when the user asks to compare, port, or align `zmemory` with upstream memory behavior.
---

# Sync Zmemory Upstream

Use this skill to convert upstream `nocturne_memory` changes into a safe, minimal sync for the local `zmemory` line of work.

Canonical upstream for this skill:
- `https://cnb.cool/zls_nmtx/sohaha/nocturne_memory`

If a local checkout exists at `/workspace/nocturne_memory`, prefer auditing that tree first and use the CNB repository as the canonical upstream reference.

## Goal

- Pull useful upstream memory behavior into `codex-zmemory`.
- Preserve Codex-specific architecture:
  - embedded Rust crate, not a direct upstream import
  - CLI thin wrapper via `codex-rs/cli/src/zmemory_cmd.rs`
  - core tool integration via `codex-rs/core/src/tools/handlers/zmemory.rs`
  - no daemon, REST, MCP, or frontend scope unless the user explicitly expands scope
- Keep sync state auditable by updating the recorded upstream baseline after a landed sync.

## Required Baseline Tracking

Before editing, always read:
- `/workspace/.codex/skills/sync-zmemory-upstream/STATE.md`
- `/workspace/.codex/skills/sync-zmemory-upstream/references/checklist.md`
- relevant zmemory design docs under `/workspace/.agents/zmemory/`

If `STATE.md` does not exist, create it with the current upstream placeholder before doing a real sync.

After a completed sync, update `STATE.md` with the actual upstream ref or hash that the local implementation now matches. If only a subset of behavior was ported, record it explicitly as a selective sync.

## Primary Local Surface

Treat these as the normal sync surface:
- `/workspace/codex-rs/zmemory/`
- `/workspace/codex-rs/cli/src/zmemory_cmd.rs`
- `/workspace/codex-rs/cli/tests/zmemory.rs`
- `/workspace/codex-rs/core/src/tools/handlers/zmemory.rs`
- `/workspace/codex-rs/core/tests/suite/zmemory_e2e.rs`
- `/workspace/.agents/zmemory/tasks.md`
- `/workspace/.agents/zmemory/qa-report.md`
- `/workspace/.agents/zmemory/.meta/execution.json`

## Upstream Reference Surface

Audit only the upstream areas relevant to memory concepts and behavior:
- `docs/memory_skills.md`
- `.codex/skills/memory/`
- Python / Go / Rust memory storage or CLI code in the upstream repo that affects:
  - path or alias semantics
  - trigger management
  - search or glossary behavior
  - doctor or rebuild workflows

Do not copy upstream code mechanically. Sync behavior, terminology, and workflow where it helps the local Rust implementation.

## Workflow

1. Read the current local baseline first.
   - inspect `codex-rs/zmemory` implementation and CLI/core entrypoints
   - inspect `.agents/zmemory/*`
   - inspect `STATE.md`
2. Audit upstream before patching.
   - identify the exact upstream ref or hash
   - compare only memory-relevant behavior
   - ignore unrelated upstream frontend, HTTP, or daemon features unless the user explicitly asks
3. Decide sync shape.
   - `selective sync` is the default
   - broader imports require explicit user approval or a clearly bounded need
4. Implement local-first.
   - update `codex-rs/zmemory` first
   - then adapt CLI and core integration
   - preserve existing Codex DTO shape and feature boundaries
5. Validate with narrow checks first.
   - `just fmt`
   - targeted `cargo nextest run -p codex-zmemory` when available, otherwise `cargo test -p codex-zmemory`
   - targeted `cargo nextest run -p codex-cli --test zmemory` when CLI changed
   - targeted `cargo nextest run -p codex-core --test suite zmemory` only when core tool behavior changed
6. Update state and task docs.
   - update `STATE.md`
   - update `.agents/zmemory/*` when scope, verification, or remaining gaps changed
7. Summarize.
   - upstream ref or hash used
   - what was ported
   - what intentionally stayed local
   - validation run

## Decision Rules

- Preserve the local architecture from `.agents/zmemory/prd.md` and `.agents/zmemory/architecture.md`.
- Do not widen scope into daemon, REST, app-server, or MCP just because upstream has it.
- Keep `Feature::ZmemoryTool` independent from the existing startup memory pipeline.
- Prefer root-cause parity updates in `codex-rs/zmemory` over CLI-only patching.
- If upstream and local behavior are genuinely incompatible in the same feature area, stop and ask the user to choose.
- If a sync only informs future work and no code lands, do not claim the upstream baseline advanced.

## Guardrails

- Do not import upstream storage or API shapes wholesale when they make the Rust API less idiomatic.
- Do not write `zmemory` data into the state DB or `codex_home/memories/` paths.
- Do not skip `STATE.md` updates after a landed sync.
- Do not leave `.agents/zmemory/*` execution state behind the code when the sync changes scope or verification.

## Final Output Contract

Always report:
- upstream repo and exact ref or hash used
- previous recorded baseline
- what changed in `codex-rs/zmemory`
- what changed in CLI and core integration
- what intentionally stayed local
- which state or task docs were updated
- validation commands and results
