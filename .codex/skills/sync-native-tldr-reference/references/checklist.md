# Native TLDR Sync Checklist

Use this checklist for each upstream native-tldr reference sync.

## Baseline

- Read `/workspace/.codex/skills/sync-native-tldr-reference/STATE.md`
- Confirm upstream repo and exact ref/hash
- Confirm whether this round is:
  - selective port
  - behavior alignment
  - broader sync

## Audit

- Review upstream tldr behavior relevant to:
  - parsing / analysis model
  - daemon/session behavior
  - config knobs
  - CLI output
  - MCP output
- Review local Codex-specific behavior relevant to:
  - lock / liveness / stale handling
  - daemon startup policy
  - CLI/MCP compatibility

## Implementation

- Update `codex-rs/native-tldr` first
- Update `codex-rs/cli/src/tldr_cmd.rs` internal daemon path if needed
- Update `codex-rs/cli/src/tldr_cmd.rs` if CLI behavior changed
- Update `codex-rs/mcp-server/src/tldr_tool.rs` if MCP behavior changed
- Update docs/state:
  - `.agents/codex-cli-native-tldr/tasks.md`
  - `.agents/codex-cli-native-tldr/qa-report.md`
  - `.agents/codex-cli-native-tldr/.meta/execution.json`
  - `STATE.md`

## Validation

- `just fmt`
- targeted `cargo test -p codex-native-tldr`
- targeted `cargo test -p codex-cli --bin codex ...` when CLI touched
- targeted `cargo test -p codex-mcp-server ...` when MCP touched
- `just fix -p <crate>` for touched crates
- record if `just argument-comment-lint` remains blocked by missing repo script

## Final Summary

- previous recorded upstream hash
- new synced upstream hash
- ported reference behavior
- preserved local divergence
- validation completed
- remaining risk
