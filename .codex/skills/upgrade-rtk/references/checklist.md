# Upgrade ZTOK Checklist (Upstream RTK)

Use this checklist before reporting completion.

## Version

- `.version/rtk.toml` matches the upstream version or PR you actually synced against.
- `integration_mode` still reflects repo reality.

## Runtime

- `codex ztok ...` still parses and runs.
- direct `ztok ...` invocation still routes through the same binary.
- Windows alias path still injects `ztok` explicitly instead of relying on leaked process environment.
- default argument behavior is updated only where intended.
- removed meta commands such as `init`/`gain`/`rewrite` do not still appear in help output or fallback logic.

## Prompting

- `codex-rs/core/src/compact.rs` only references prompt assets under `core/templates/compact/`.
- `core/templates/compact/prompt.md` only mentions commands the embedded curated surface really supports.

## Tests

- `codex-rs/cli/tests/ztok.rs` covers every changed behavior.
- non-zero exit code propagation is still tested for wrapper commands.
- alias coverage still exercises the real arg0 path.
- help coverage reflects the curated Codex command surface rather than upstream full-sync.

## Validation

- `just fmt` passed.
- `cargo test -p codex-cli` passed.
- final summary states the new upstream baseline and the exact validation commands run.
