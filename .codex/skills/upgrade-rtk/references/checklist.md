# Upgrade ZTOK Checklist (Upstream RTK + sqz)

Use this checklist before reporting completion.

## Baselines

- `.version/rtk.toml` matches the upstream version or PR you actually synced against.
- `.version/sqz.toml` matches the `sqz` source / ref / commit actually used for the selective compression reference.
- each `integration_mode` still reflects repo reality and does not overclaim upstream parity.

## Runtime

- `codex ztok ...` still parses and runs.
- direct `ztok ...` invocation still routes through the same binary.
- Windows alias path still injects `ztok` explicitly instead of relying on leaked process environment.
- default argument behavior is updated only where intended.
- removed meta commands such as `init`/`gain`/`rewrite` do not still appear in help output or fallback logic.
- `sqz`-inspired work stays inside the existing `ztok` compression / dedup surface instead of expanding into hooks, proxy, plugins, or stats features unless explicitly requested.

## Prompting

- `codex-rs/core/src/compact.rs` only references prompt assets under `core/templates/compact/`.
- `core/templates/compact/prompt.md` only mentions commands the embedded curated surface really supports.
- `upgrade-rtk` skill text clearly distinguishes RTK command-surface sync from `sqz` selective compression reference.

## Tests

- `codex-rs/cli/tests/ztok.rs` covers every changed behavior.
- `codex-rs/ztok` tests cover shared compression / dedup changes when `sqz`-derived behavior moves.
- non-zero exit code propagation is still tested for wrapper commands.
- alias coverage still exercises the real arg0 path.
- help coverage reflects the curated Codex command surface rather than upstream full-sync.

## Validation

- if runtime changed: `just fmt`, `cargo test -p codex-cli --test ztok`, and `cargo test -p codex-ztok` passed.
- if only baseline / skill docs changed: the `rg` checks for `.version/sqz.toml` and `upgrade-rtk` dual-upstream wording passed.
- final summary states the RTK and/or `sqz` baseline actually advanced and the exact validation commands run.
