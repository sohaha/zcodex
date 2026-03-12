---
name: upgrade-rtk
description: Upgrade the embedded practical-subset RTK integration used by this Codex repo. Use when syncing this repo's built-in `rtk` wrapper to a newer upstream `rtk-ai/rtk` release or PR, updating `.version/rtk.toml`, prompt guidance, alias behavior, and `codex-rs/cli` regression coverage.
---

# Upgrade RTK

Use this skill to upgrade the repository's embedded RTK baseline in a controlled way.

## Workflow

1. Read `/workspace/.version/rtk.toml` to confirm the current recorded upstream version, source, and reference PR.
2. Inspect the current embedded implementation before touching code:
   - `/workspace/codex-rs/cli/src/rtk_cmd.rs`
   - `/workspace/codex-rs/cli/src/main.rs`
   - `/workspace/codex-rs/arg0/src/lib.rs`
   - `/workspace/codex-rs/cli/tests/rtk.rs`
   - `/workspace/codex-rs/core/prompt.md`
   - `/workspace/codex-rs/core/prompt_with_apply_patch_instructions.md`
   - `/workspace/codex-rs/core/gpt_5_1_prompt.md`
   - `/workspace/codex-rs/core/gpt_5_2_prompt.md`
3. Compare the target upstream RTK version or PR against the recorded baseline. Focus on:
   - supported commands
   - default argument changes
   - output filtering changes
   - Codex-specific prompt or integration changes
4. Only sync the practical subset used by this repo. Do not try to import the full upstream project unless explicitly requested.

## Practical Subset

Treat these as the supported embedded commands unless the user expands scope:

- `git`
- `rg`
- `grep`
- `read`
- `ls`
- `tree`
- `find`
- `json`
- `log`
- `err`
- `test`
- `env`
- `deps`

Behavior details that should stay aligned with prompt guidance:

- `err` keeps error and warning related lines with one line of context on each side, capped at 40 lines; if nothing matches, it falls back to the last 40 lines.
- `log` keeps a broader set of log-worthy problem lines such as warnings, failures, timeouts, denials, and killed/refused events, capped at 80 lines; if nothing matches, it falls back to the last 40 lines.
- `err`, `log`, and `test` execute programs directly rather than through a shell, so shell syntax like pipes or redirection only works when wrapped explicitly via a real shell such as `bash -lc` on Unix or `powershell.exe -Command` / `cmd /C` on Windows.

When upstream changes behavior outside this set, ignore it unless it affects shared infrastructure or prompt wording.

## Required Updates

When upgrading, update all applicable places together:

1. Version tracking:
   - `/workspace/.version/rtk.toml`
2. Runtime behavior:
   - `/workspace/codex-rs/cli/src/rtk_cmd.rs`
   - `/workspace/codex-rs/cli/src/main.rs`
   - `/workspace/codex-rs/arg0/src/lib.rs`
3. Prompt guidance:
   - `/workspace/codex-rs/core/prompt.md`
   - `/workspace/codex-rs/core/prompt_with_apply_patch_instructions.md`
   - `/workspace/codex-rs/core/gpt_5_1_prompt.md`
   - `/workspace/codex-rs/core/gpt_5_2_prompt.md`
4. Regression tests:
   - `/workspace/codex-rs/cli/tests/rtk.rs`

## Upgrade Rules

- Preserve the current architecture: `codex rtk ...` plus direct `rtk ...` alias support.
- Keep prompt wording aligned across all prompt variants.
- Prefer root-cause updates in filtering/default-arg behavior over adding special cases in tests.
- If upstream behavior changes but this repo intentionally diverges, keep the repo behavior and record the new upstream version anyway only if the divergence is understood and intentional.
- If the new upstream target introduces a command you do not embed, do not mention that command in prompt guidance.

## Validation

Always run these after changes:

```bash
cd /workspace/codex-rs
just fmt
cargo test -p codex-cli
```

If you add or change `rtk` command behavior, extend `/workspace/codex-rs/cli/tests/rtk.rs` so the upgraded behavior is locked in.

## Review Checklist

Read `/workspace/.codex/skills/upgrade-rtk/references/checklist.md` before finalizing to make sure you did not miss prompt files, alias routing, or regression coverage.
