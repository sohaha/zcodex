## Embedded ZTOK

The current Codex launcher path is exposed to shell commands as
`{{ codex_self_exe_env_var }}`. `ztok` is the token-optimized CLI proxy for
shell commands. Prefer direct shell commands first and let the rewrite layer
route them automatically when possible.

When the shell rewrite layer can recognize a command shape, it will prefix the
command for you. If you need to force the proxy explicitly, invoke the launcher
from `{{ codex_self_exe_env_var }}` with `ztok <subcommand> ...` instead of
assuming the executable name is `codex`. In POSIX shells, that looks like
`{{ posix_launcher_invocation }} ztok <subcommand> ...`.

Use the generic wrappers when you need proxying for arbitrary commands:

- `{{ posix_launcher_invocation }} ztok err -- <command> [args...]` runs any command and keeps only
  error and warning lines. Use this for noisy build, lint, or compile output.
- `{{ posix_launcher_invocation }} ztok test -- <command> [args...]` runs any test command and keeps
  failures plus the summary. Use this for generic test runners that do not
  already have a dedicated ztok subcommand.
- `{{ posix_launcher_invocation }} ztok log [file]` deduplicates noisy log streams into a compact
  error/warning-heavy summary.
- `{{ posix_launcher_invocation }} ztok json <file> --keys-only` shows JSON keys and types without
  echoing full values back into the context window.

`err` and `test` execute programs directly instead of through a shell. For
pipes, redirects, or shell operators, wrap the real shell explicitly, such as
`bash -lc 'cargo test 2>&1 | tail -n 40'`.
