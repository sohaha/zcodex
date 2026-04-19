## Embedded ZTOK

Codex exposes the current launcher path to shell commands as
`{{ codex_self_exe_env_var }}` for internal execution plumbing. Treat that
variable as internal only: do not show `"$CODEX_SELF_EXE"` in user-facing
commentary or command examples. `ztok` is the token-optimized CLI proxy for
shell commands. Prefer direct shell commands first and let the rewrite layer
route them automatically when possible.

When the shell rewrite layer can recognize a command shape, it will prefix the
command for you. If you need to force the proxy explicitly, use the logical
launcher form `{{ logical_launcher_invocation }} ztok <subcommand> ...`. The
runtime will resolve that logical form through `{{ codex_self_exe_env_var }}`
internally when needed.

Use the dedicated shell entrypoint for arbitrary commands:

- `{{ logical_launcher_invocation }} ztok shell <command> [args...]` runs a generic command and
  preserves normal stdout/stderr.
- `{{ logical_launcher_invocation }} ztok shell --filter err <command> [args...]` keeps only error
  and warning lines. Use this for noisy build, lint, or compile output.
- `{{ logical_launcher_invocation }} ztok shell --filter test <command> [args...]` keeps failures
  plus the summary. Use this for generic test runners that do not already have
  a dedicated ztok subcommand.
- `{{ logical_launcher_invocation }} ztok log [file]` deduplicates noisy log streams into a compact
  error/warning-heavy summary.
- `{{ logical_launcher_invocation }} ztok json <file> --keys-only` shows JSON keys and types without
  echoing full values back into the context window.

`shell --filter err` and `shell --filter test` execute programs directly
instead of through a shell. For pipes, redirects, or shell operators, wrap the
real shell explicitly, such as
`{{ logical_launcher_invocation }} ztok shell bash -lc 'cargo test 2>&1 | tail -n 40'`.
