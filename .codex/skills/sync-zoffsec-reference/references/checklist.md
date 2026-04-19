# zoffsec Reference Sync Checklist

Use this checklist for each `codex zoffsec` upstream sync.

## Baseline

- Read `/workspace/.codex/skills/sync-zoffsec-reference/STATE.md`
- Confirm upstream repo and exact ref/hash
- Confirm whether this round is:
  - selective sync
  - behavior alignment
  - broader sync
- Read the current local scope docs:
  - `/workspace/.agents/plan/2026-04-07-zoffsec-subcommand.md`
  - `/workspace/.agents/issues/2026-04-07-zoffsec-subcommand.toml`

## Audit

- Review upstream Codex-relevant behavior in:
  - `README.md`
  - `codex_session_patcher/core/formats.py`
- If the same concept changed upstream, also review:
  - `codex_session_patcher/core/detector.py`
  - `codex_session_patcher/core/patcher.py`
  - `codex_session_patcher/ctf_config/templates.py`
  - `codex_session_patcher/ctf_config/installer.py`
- Review local Codex-specific behavior in:
  - `codex-rs/cli/src/zoffsec_cmd.rs`
  - `codex-rs/cli/src/zoffsec_config.rs`
  - `codex-rs/rollout/src/patch.rs`
  - `codex-rs/tui/src/zoffsec_resume.rs`

## Implementation

- Prefer selective parity updates over broad porting
- Update Rust implementation first
- Preserve:
  - `codex zoffsec` subcommand entrypoint
  - marker-based session identification
  - explicit `zoffsec clean`
  - explicit clean-then-resume behavior
- Update plan/issue docs only when sync scope or recorded validation changes
- Update `STATE.md` after a landed sync

## Validation

- `cd /workspace/codex-rs && just fmt`
- `cd /workspace/codex-rs && cargo nextest run -p codex-rollout` when rollout cleanup changed
- `cd /workspace/codex-rs && cargo nextest run -p codex-tui` when resume behavior changed
- `cd /workspace/codex-rs && cargo nextest run -p codex-cli` when command or help behavior changed
- `cd /workspace/codex-rs && just fix -p <crate>` for larger landed Rust changes

## Final Summary

- previous recorded upstream baseline
- new synced upstream ref or hash
- ported Codex-specific behavior
- preserved local divergence
- validation completed
- remaining risk
