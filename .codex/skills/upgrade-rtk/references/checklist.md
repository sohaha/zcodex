# Upgrade RTK Checklist

Use this checklist before reporting completion.

## Version

- `.version/rtk.toml` matches the upstream version or PR you actually synced against.
- `integration_mode` still reflects repo reality.

## Runtime

- `codex rtk ...` still parses and runs.
- direct `rtk ...` invocation still routes through the same binary.
- Windows alias path still injects `rtk` explicitly instead of relying on leaked process environment.
- default argument behavior is updated only where intended.

## Prompting

- all four prompt files mention `rtk` consistently.
- prompt examples only mention commands the embedded practical subset really supports.

## Tests

- `codex-rs/cli/tests/rtk.rs` covers every changed behavior.
- non-zero exit code propagation is still tested for wrapper commands.
- alias coverage still exercises the real arg0 path.

## Validation

- `just fmt` passed.
- `cargo test -p codex-cli` passed.
- final summary states the new upstream baseline and the exact validation commands run.
