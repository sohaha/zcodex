## Embedded ZTOK

Codex exposes the current launcher path to shell commands as
`{{ codex_self_exe_env_var }}` for internal execution plumbing. Treat that
variable as internal only: do not show `"$CODEX_SELF_EXE"` in user-facing
commentary or command examples. `ztok` is the token-optimized CLI proxy for
shell commands.

Use the logical launcher form `{{ logical_launcher_invocation }} ztok ...` in
all user-facing planning, commentary, and command examples. Do not plan around
automatic shell rewrite; rewrite is an internal runtime optimization, not the
model's default execution strategy.

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

Default priority:

1. For general shell commands, explicitly use `{{ logical_launcher_invocation }} ztok shell <command> [args...]`.
2. Use a dedicated `{{ logical_launcher_invocation }} ztok <subcommand> ...` form only when that subcommand
   directly matches the task.
3. Add `--filter err` or `--filter test` only when you specifically want filtered error or test output.

If the user explicitly asks to use `ztok`, commentary and executed commands
must explicitly use a `{{ logical_launcher_invocation }} ztok ...` form. Do not
say you will run a raw shell command first and let rewrite take over.

`shell --filter err` and `shell --filter test` execute programs directly
instead of through a shell. For pipes, redirects, or shell operators, wrap the
real shell explicitly, such as
`{{ logical_launcher_invocation }} ztok shell bash -lc 'cargo test 2>&1 | tail -n 40'`.
