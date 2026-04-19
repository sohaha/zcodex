---
name: sync-zoffsec-reference
description: Sync the local `codex zoffsec` implementation against the upstream `ryfineZ/codex-session-patcher` reference repo with minimal, auditable parity updates. Use when the user asks to compare, port, or align `codex zoffsec`, rollout cleaning, or zoffsec resume behavior with the upstream reference workflow.
---

# Sync zoffsec Reference

Use this skill only for the `codex zoffsec` command line in this repo.

Canonical upstream for this skill:
- `https://github.com/ryfineZ/codex-session-patcher`

## Goal

- Pull useful upstream Codex Session Patcher behavior into the local `codex zoffsec` workflow.
- Preserve local architecture:
  - native `codex zoffsec` subcommand instead of an external Python installer
  - base-instructions injection via `codex-rs/cli/src/zoffsec_config.rs`
  - rollout cleanup in `codex-rs/rollout/src/patch.rs`
  - explicit clean-then-resume integration in `codex-rs/tui/src/zoffsec_resume.rs`
- Keep sync state auditable by updating the recorded upstream baseline after each landed sync.

## Required Baseline Tracking

Before editing, always read:
- `/workspace/.codex/skills/sync-zoffsec-reference/STATE.md`
- `/workspace/.codex/skills/sync-zoffsec-reference/references/checklist.md`
- `/workspace/.agents/plan/2026-04-07-zoffsec-subcommand.md`
- `/workspace/.agents/issues/2026-04-07-zoffsec-subcommand.toml`

If `STATE.md` does not exist, create it with the current upstream placeholder before doing a real sync.

After a completed sync, update `STATE.md` with the exact upstream ref or hash that the local implementation now matches. If only part of the upstream behavior was ported, record it explicitly as a selective sync.

## Primary Local Surface

Treat these as the normal local sync surface:
- `/workspace/codex-rs/cli/src/zoffsec_cmd.rs`
- `/workspace/codex-rs/cli/src/zoffsec_config.rs`
- `/workspace/codex-rs/rollout/src/patch.rs`
- `/workspace/codex-rs/rollout/src/lib.rs`
- `/workspace/codex-rs/tui/src/zoffsec_resume.rs`
- `/workspace/.agents/plan/2026-04-07-zoffsec-subcommand.md`
- `/workspace/.agents/issues/2026-04-07-zoffsec-subcommand.toml`

Audit adjacent tests before changing behavior:
- `codex-rs/cli` tests for `zoffsec`
- `codex-rs/rollout` tests covering `clean_zoffsec_rollout`
- `codex-rs/tui` tests covering `zoffsec_resume`

## Upstream Reference Surface

Start with the current upstream facts:
- `README.md`
- `codex_session_patcher/core/formats.py`

Read these only when the sync round touches the same concept:
- `codex_session_patcher/core/detector.py`
- `codex_session_patcher/core/patcher.py`
- `codex_session_patcher/ctf_config/templates.py`
- `codex_session_patcher/ctf_config/installer.py`

Default audit mapping:
- upstream session format handling -> local rollout patch behavior
- upstream refusal detection / replacement flow -> local `clean_zoffsec_rollout`
- upstream Codex offensive-security injection workflow -> local `codex zoffsec` template and marker behavior

Do not copy upstream Python code mechanically. Port behavior into the local Rust command surface.

## Workflow

1. Read the current local baseline first.
   - inspect the local zoffsec command, config, rollout, and TUI resume entrypoints
   - inspect the current issue and plan docs
   - inspect `STATE.md`
2. Audit upstream before patching.
   - identify the exact upstream ref or hash
   - compare only Codex-relevant behavior first
   - ignore upstream Web UI, Claude Code, OpenCode, AI rewriting, and installer UX unless the user explicitly expands scope
3. Decide sync shape.
   - `selective sync` is the default
   - broader imports require explicit user approval or a clearly bounded need
4. Implement local-first.
   - update the Rust implementation first
   - keep the existing `codex zoffsec`, `codex zoffsec clean`, and `codex zoffsec resume` surface unless the user asks for command changes
   - preserve explicit, observable cleanup behavior
5. Validate with narrow checks first.
   - `cd /workspace/codex-rs && just fmt`
   - `cd /workspace/codex-rs && cargo nextest run -p codex-rollout` when rollout cleanup changed
   - `cd /workspace/codex-rs && cargo nextest run -p codex-tui` when resume behavior changed
   - `cd /workspace/codex-rs && cargo nextest run -p codex-cli` when command parsing or help output changed
   - for larger landed Rust changes, run `cd /workspace/codex-rs && just fix -p <crate>`
6. Update state/docs.
   - update `STATE.md`
   - update `.agents/plan/2026-04-07-zoffsec-subcommand.md` or `.agents/issues/2026-04-07-zoffsec-subcommand.toml` only when the sync changes the recorded baseline, scope, or verification status
7. Summarize.
   - upstream ref or hash used
   - what was ported
   - what intentionally stayed local
   - validation run

## Decision Rules

- Preserve the local `codex zoffsec` UX and Rust-first architecture by default.
- Prefer root-cause changes in `rollout/src/patch.rs` or `zoffsec_config.rs` over CLI-only string patches.
- Keep the zoffsec session marker contract stable unless the sync explicitly requires a compatible migration.
- Keep cleanup explicit and observable. Do not add hidden automatic cleanup.
- If upstream and local behavior are incompatible in the same feature area, stop and ask the user only when one behavior must be dropped.
- If a sync round only informed future work and no code landed, do not advance the recorded baseline.

## Guardrails

- Do not import upstream Web UI, FastAPI, Vue, Claude Code, or OpenCode flows into this skill's default scope.
- Do not replace the local fixed replacement flow with upstream AI rewriting unless the user explicitly asks for that expansion.
- Do not reintroduce installer-style `ctf profile/global mode` UX over the current `codex zoffsec` subcommand without explicit approval.
- Do not retroactively describe pre-skill local features as “already synced to upstream” unless you have contemporaneous state tracking or a fresh file-by-file audit that proves the exact parity scope.
- Do not skip `STATE.md` updates after a landed sync.
- Do not leave plan or issue docs behind the code when a sync round changes scope or verification status.

## Final Output Contract

Always report:
- upstream repo and exact ref or hash used
- previous recorded baseline
- what changed in `zoffsec_cmd.rs`, `zoffsec_config.rs`, `patch.rs`, or `zoffsec_resume.rs`
- what intentionally stayed local
- which state or task docs were updated
- validation commands and results
