---
name: upgrade-rtk
description: Upgrade the Codex-curated ZTOK integration (local rename of upstream RTK). Use when syncing embedded `ztok` subcommands to a newer upstream `rtk-ai/rtk` release or PR, updating `.version/rtk.toml` (upstream baseline), prompt guidance, alias behavior, and `codex-rs/cli` regression coverage.
---

# Upgrade ZTOK (Upstream RTK)

Use this skill to upgrade the repository's embedded ZTOK baseline (renamed locally; upstream remains RTK) in a controlled way without re-importing upstream bootstrap, hook, analytics, or installer features.

## Workflow

1. Read `/workspace/.version/rtk.toml` to confirm the current recorded upstream version, source, and reference PR (upstream stays RTK).
2. Inspect the current embedded implementation before touching code:
   - `/workspace/codex-rs/ztok/src/lib.rs`
   - `/workspace/codex-rs/cli/src/main.rs`
   - `/workspace/codex-rs/arg0/src/lib.rs`
   - `/workspace/codex-rs/cli/tests/ztok.rs`
   - `/workspace/codex-rs/core/src/compact.rs`
   - `/workspace/codex-rs/core/templates/compact/prompt.md`
3. Compare the target upstream RTK version or PR against the recorded baseline. Focus on:
   - supported commands
   - default argument changes
   - output filtering changes
   - Codex-specific prompt or integration changes
4. First decide whether the upstream change affects Codex's curated command surface. Only sync the embedded commands used by this repo. Do not import upstream hook/bootstrap/analytics features unless explicitly requested.

## Codex-Curated Surface

Treat these as the supported embedded commands unless the user expands scope:

- `git`
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

Notes:

- `rg` is still safe to use as `ztok rg ...`, but in the current curated integration it routes through fallback instead of a dedicated help-listed subcommand.

This repo also embeds additional operational wrappers that are safe to keep aligned when useful, such as:

- `gh`
- `aws`
- `psql`
- `pnpm`
- `docker`
- `kubectl`
- `summary`
- `wget`
- `wc`
- `vitest`
- `prisma`
- `tsc`
- `next`
- `lint`
- `prettier`
- `format`
- `playwright`
- `cargo`
- `npm`
- `npx`
- `curl`
- `ruff`
- `pytest`
- `mypy`
- `pip`
- `go`
- `gt`
- `golangci-lint`

Do not reintroduce these upstream-only meta commands unless the user explicitly asks for them:

- `init`
- `gain`
- `cc-economics`
- `config`
- `discover`
- `learn`
- `proxy`
- `verify`
- `hook-audit`
- `rewrite`

Behavior details that should stay aligned with prompt guidance:

- `err` keeps error and warning related lines with one line of context on each side, capped at 40 lines; if nothing matches, it falls back to the last 40 lines.
- `log` keeps a broader set of log-worthy problem lines such as warnings, failures, timeouts, denials, and killed/refused events, capped at 80 lines; if nothing matches, it falls back to the last 40 lines.
- `err`, `log`, and `test` execute programs directly rather than through a shell, so shell syntax like pipes or redirection only works when wrapped explicitly via a real shell such as `bash -lc` on Unix or `powershell.exe -Command` / `cmd /C` on Windows.

When upstream changes behavior outside this set, ignore it unless it affects shared infrastructure or prompt wording.

## 估计

- 本地命名已改为 `ztok`，同步上游 RTK 时通常需要同时更新 `codex-rs/ztok`、CLI/arg0 入口、测试与提示文案；常规同步预计 0.5~1 天，具体以差异规模为准。

## Required Updates

When upgrading, update all applicable places together:

1. Version tracking:
   - `/workspace/.version/rtk.toml`
2. Runtime behavior:
   - `/workspace/codex-rs/ztok/src/lib.rs`
   - `/workspace/codex-rs/ztok/src/*`
   - `/workspace/codex-rs/cli/src/main.rs`
   - `/workspace/codex-rs/arg0/src/lib.rs`
3. Prompt guidance:
   - `/workspace/codex-rs/core/src/compact.rs`
   - `/workspace/codex-rs/core/templates/compact/prompt.md`
4. Regression tests:
   - `/workspace/codex-rs/cli/tests/ztok.rs`

## Upgrade Rules

- Preserve the current architecture: `codex ztok ...` plus direct `ztok ...` alias support.
- Keep the embedded `ztok` crate focused on operational wrappers; remove obsolete compatibility paths instead of leaving dead meta-command logic around.
- Prefer root-cause updates in filtering/default-arg behavior over adding special cases in tests.
- If upstream behavior changes but this repo intentionally diverges, keep the repo behavior and record the new upstream version only when the divergence is understood and intentional.
- If the new upstream target introduces a command you do not embed, do not mention that command in prompt guidance or tests.
- Do not make `core` prompt templates depend on `codex-rs/ztok` hook/init assets; keep Codex prompt guidance in `core/templates/compact/`.

## Validation

Always run these after changes:

```bash
cd /workspace/codex-rs
just fmt
cargo test -p codex-cli
```

If you add or change `ztok` command behavior, extend `/workspace/codex-rs/cli/tests/ztok.rs` so the upgraded behavior is locked in.

## Review Checklist

Read `/workspace/.codex/skills/upgrade-rtk/references/checklist.md` before finalizing to make sure you did not miss prompt files, alias routing, or regression coverage.
