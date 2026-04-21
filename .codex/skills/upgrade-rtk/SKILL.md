---
name: upgrade-rtk
description: Upgrade the Codex-curated ZTOK integration (local rename of upstream RTK) and keep the shared sqz-derived compression baseline auditable. Use when syncing embedded `ztok` subcommands to a newer upstream `rtk-ai/rtk` release or PR, or when advancing the recorded `sqz` reference used by `codex-rs/ztok` compression and dedup paths.
---

# Upgrade ZTOK (Upstream RTK + sqz)

Use this skill to upgrade the repository's embedded ZTOK baselines in a controlled way without re-importing upstream bootstrap, hook, analytics, proxy, installer, or plugin product surfaces.

Treat this as the single sync/documentation entrypoint for two different upstream references:

- `RTK`: command surface, wrapper behavior, help/prompt wording, and CLI integration.
- `sqz`: shared compression ideas used by `codex-rs/ztok` such as session dedup, near-duplicate handling, and generic content compression seams.

Recording a `sqz` baseline here does **not** mean the repo is in full upstream parity with `sqz`. It means the current `ztok` compression work selectively referenced that upstream snapshot.

## Workflow

1. Read both recorded baselines before touching code or docs:
   - `/workspace/.version/rtk.toml`
   - `/workspace/.version/sqz.toml` when the task touches shared compression / dedup behavior
2. Inspect the current embedded implementation before touching code:
   - `/workspace/codex-rs/ztok/src/lib.rs`
   - `/workspace/codex-rs/cli/src/main.rs`
   - `/workspace/codex-rs/arg0/src/lib.rs`
   - `/workspace/codex-rs/cli/tests/ztok.rs`
   - `/workspace/codex-rs/core/src/compact.rs`
   - `/workspace/codex-rs/core/templates/compact/prompt.md`
3. When the task involves `sqz`, inspect only the local surfaces that actually borrow `sqz` ideas:
   - `/workspace/codex-rs/ztok/src/compression.rs`
   - `/workspace/codex-rs/ztok/src/compression_json.rs`
   - `/workspace/codex-rs/ztok/src/compression_log.rs`
   - `/workspace/codex-rs/ztok/src/session_dedup.rs`
   - `/workspace/codex-rs/ztok/src/near_dedup.rs`
   - `/workspace/codex-rs/ztok/src/summary.rs`
4. Compare the target upstream RTK version / PR or target `sqz` snapshot against the recorded baseline. Focus on:
   - RTK: supported commands, default argument changes, output filtering changes, prompt and alias behavior
   - sqz: shared compression algorithms or contracts that this repo has actually adopted
5. First decide whether the upstream change affects Codex's curated command surface or the selective `sqz` reference surface. Only sync the embedded commands and compression seams used by this repo. Do not import upstream hook/bootstrap/analytics/proxy/plugin features unless explicitly requested.

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

## sqz Reference Surface

Use `sqz` as a selective reference only for the parts this repo has deliberately borrowed into `codex-rs/ztok`, for example:

- shared content compression routing
- JSON / log / summary compression heuristics
- session-scoped dedup contracts
- near-duplicate candidate filtering and diff-style fallback behavior

Do not interpret `sqz` as the default source of truth for these non-goals unless the user explicitly expands scope:

- `sqz init`, tool hooks, browser/IDE plugins, or MCP packaging
- `gain`, `stats`, `discover`, `resume`, `proxy`, dashboards, or telemetry surfaces
- full `sqz_engine` parity or wholesale module imports

## 估计

- 本地命名已改为 `ztok`，同步上游 RTK 时通常需要同时更新 `codex-rs/ztok`、CLI/arg0 入口、测试与提示文案；常规同步预计 0.5~1 天，具体以差异规模为准。

## Required Updates

When upgrading, update all applicable places together:

1. Version tracking:
   - `/workspace/.version/rtk.toml`
   - `/workspace/.version/sqz.toml` when the `sqz` reference baseline advances
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
- If a `sqz` task only informed selective local implementation rather than a true upstream sync, update `.version/sqz.toml` without claiming full parity in code comments, skill text, or commit messages.
- If the new upstream target introduces a command you do not embed, do not mention that command in prompt guidance or tests.
- Do not make `core` prompt templates depend on `codex-rs/ztok` hook/init assets; keep Codex prompt guidance in `core/templates/compact/`.

## Validation

Run the narrowest validation that matches the touched surface.

If runtime behavior changed:

```bash
cd /workspace/codex-rs
just fmt
cargo test -p codex-cli --test ztok
cargo test -p codex-ztok
```

If only the recorded baseline / skill docs changed:

```bash
cd /workspace
rg -n "source|ref|commit|integration" .version/sqz.toml
rg -n "sqz|RTK|ztok|双上游" .codex/skills/upgrade-rtk/SKILL.md .codex/skills/upgrade-rtk/references/checklist.md
```

If you add or change `ztok` command behavior, extend `/workspace/codex-rs/cli/tests/ztok.rs` so the upgraded behavior is locked in.

## Review Checklist

Read `/workspace/.codex/skills/upgrade-rtk/references/checklist.md` before finalizing to make sure you did not miss prompt files, alias routing, or regression coverage.
