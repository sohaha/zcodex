---
name: upgrade-rtk
description: Upgrade the embedded practical-subset RTK integration used by this Codex repo. Use when syncing this repo's built-in `rtk` wrapper to a newer upstream `rtk-ai/rtk` release or PR, updating `.version/rtk.toml`, prompt guidance, alias behavior, and `codex-rs/cli` regression coverage.
---

# Upgrade RTK

Use this skill to upgrade the repository's embedded RTK baseline in a controlled way.

## Workflow

1. Read `/Users/so/Code/zcodex/.version/rtk.toml` to confirm the current recorded upstream version, source, and reference PR.
2. Inspect the current embedded implementation before touching code:
   - `/Users/so/Code/zcodex/codex-rs/cli/src/rtk_cmd.rs`
   - `/Users/so/Code/zcodex/codex-rs/cli/src/main.rs`
   - `/Users/so/Code/zcodex/codex-rs/arg0/src/lib.rs`
   - `/Users/so/Code/zcodex/codex-rs/cli/tests/rtk.rs`
   - `/Users/so/Code/zcodex/codex-rs/core/prompt.md`
   - `/Users/so/Code/zcodex/codex-rs/core/prompt_with_apply_patch_instructions.md`
   - `/Users/so/Code/zcodex/codex-rs/core/gpt_5_1_prompt.md`
   - `/Users/so/Code/zcodex/codex-rs/core/gpt_5_2_prompt.md`
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

When upstream changes behavior outside this set, ignore it unless it affects shared infrastructure or prompt wording.

## Required Updates

When upgrading, update all applicable places together:

1. Version tracking:
   - `/Users/so/Code/zcodex/.version/rtk.toml`
2. Runtime behavior:
   - `/Users/so/Code/zcodex/codex-rs/cli/src/rtk_cmd.rs`
   - `/Users/so/Code/zcodex/codex-rs/cli/src/main.rs`
   - `/Users/so/Code/zcodex/codex-rs/arg0/src/lib.rs`
3. Prompt guidance:
   - `/Users/so/Code/zcodex/codex-rs/core/prompt.md`
   - `/Users/so/Code/zcodex/codex-rs/core/prompt_with_apply_patch_instructions.md`
   - `/Users/so/Code/zcodex/codex-rs/core/gpt_5_1_prompt.md`
   - `/Users/so/Code/zcodex/codex-rs/core/gpt_5_2_prompt.md`
4. Regression tests:
   - `/Users/so/Code/zcodex/codex-rs/cli/tests/rtk.rs`

## Upgrade Rules

- Preserve the current architecture: `codex rtk ...` plus direct `rtk ...` alias support.
- Keep prompt wording aligned across all prompt variants.
- Prefer root-cause updates in filtering/default-arg behavior over adding special cases in tests.
- If upstream behavior changes but this repo intentionally diverges, keep the repo behavior and record the new upstream version anyway only if the divergence is understood and intentional.
- If the new upstream target introduces a command you do not embed, do not mention that command in prompt guidance.

## Validation

Always run these after changes:

```bash
cd /Users/so/Code/zcodex/codex-rs
just fmt
cargo test -p codex-cli
```

If you add or change `rtk` command behavior, extend `/Users/so/Code/zcodex/codex-rs/cli/tests/rtk.rs` so the upgraded behavior is locked in.

## Review Checklist

Read `/Users/so/Code/zcodex/.codex/skills/upgrade-rtk/references/checklist.md` before finalizing to make sure you did not miss prompt files, alias routing, or regression coverage.
