---
name: sync-native-tldr-reference
description: Sync the upstream reference implementation used for native-tldr into this repo. Use when comparing or porting `llm-tldr` or another chosen upstream reference into `codex-rs/native-tldr`, `codex-rs/cli/src/tldr_cmd.rs`, and `codex-rs/mcp-server/src/tldr_tool.rs`, while preserving Codex-specific daemon, lifecycle, CLI, and MCP behavior.
---

# Sync Native TLDR Reference

Use this skill only for the native-tldr line of work in this repo.

## Scope

- Upstream reference: usually `parcadei/llm-tldr`, or another explicitly named tldr reference.
- Local targets:
  - `/workspace/codex-rs/native-tldr/`
  - `/workspace/codex-rs/native-tldr-daemon/`
  - `/workspace/codex-rs/cli/src/tldr_cmd.rs`
  - `/workspace/codex-rs/mcp-server/src/tldr_tool.rs`
- Progress/state docs that must stay in sync:
  - `/workspace/.agents/codex-cli-native-tldr/tasks.md`
  - `/workspace/.agents/codex-cli-native-tldr/qa-report.md`
  - `/workspace/.agents/codex-cli-native-tldr/.meta/execution.json`

## Goal

- Pull useful upstream reference behavior into native-tldr.
- Preserve Codex-specific behavior:
  - daemon lifecycle
  - CLI command surface
  - MCP tool surface
  - lock/liveness/stale handling
  - repo-specific testing and validation rules
- After each completed sync, update the recorded upstream hash/state.

## Required Baseline Tracking

Before editing, always read:

- `/workspace/.codex/skills/sync-native-tldr-reference/STATE.md`

If it does not exist, create it with:

```md
# sync-native-tldr-reference state

- upstream_repo: <fill>
- upstream_ref: <none>
- last_synced_hash: <none>
- last_synced_at_utc: <none>
- notes: initialized, no completed sync yet.
```

After any completed sync/port round, update `STATE.md` with the actual upstream hash/ref that the local implementation now references. Do not leave a stale or placeholder hash behind.

## Workflow

1. Read the current local baseline first.
   - inspect `native-tldr`, daemon, CLI, and MCP integration points
   - inspect `.agents/codex-cli-native-tldr/*`
   - inspect `STATE.md`
2. Audit upstream before patching.
   - identify the exact upstream hash/ref
   - compare only the tldr-relevant behavior
   - ignore unrelated upstream bootstrap/tooling unless explicitly requested
3. Decide sync shape.
   - `selective port` is the default
   - only do a broader import if the user explicitly asks
4. Implement local-first.
   - port reference logic into `codex-rs/native-tldr`
   - then adapt CLI/MCP/daemon integration
   - keep Codex-specific lifecycle and validation conventions
5. Validate with narrow tests first.
   - native-tldr tests
   - CLI targeted tests
   - MCP targeted tests when touched
6. Update state/docs.
   - update `STATE.md` hash/ref
   - update `.agents/codex-cli-native-tldr/*`
7. Summarize:
   - upstream hash/ref used
   - what was ported
   - what intentionally stayed local
   - validation run

## Decision Rules

- Default to preserving local daemon/MCP/CLI architecture.
- Ask the user only when two incompatible tldr behaviors must be chosen.
- Do not silently drop existing local lifecycle or safety behavior.
- Do not claim an upstream sync if only a partial port landed; call it a selective sync explicitly.

## What To Inspect

- `codex-rs/native-tldr/src/*.rs`
- `codex-rs/native-tldr-daemon/src/main.rs`
- `codex-rs/cli/src/tldr_cmd.rs`
- `codex-rs/mcp-server/src/tldr_tool.rs`
- `.agents/codex-cli-native-tldr/*`
- `references/checklist.md`

## Guardrails

- Do not import upstream assumptions that bypass Codex daemon lifecycle controls.
- Do not overwrite local CLI/MCP UX without checking existing tests and help output.
- Do not skip updating the recorded upstream hash after a completed sync.
- Do not leave docs and execution state behind the code.

## Final Output Contract

Always report:

- upstream repo/ref/hash
- previous recorded hash
- what changed in native-tldr
- what changed in CLI/MCP/daemon
- what stayed intentionally local
- docs/state files updated
- validation commands and results
