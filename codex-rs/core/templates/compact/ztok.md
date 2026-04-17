## Embedded ZTOK

`codex ztok` is the token-optimized CLI proxy for shell commands. Prefer
running shell commands through `codex ztok` so output is filtered or
summarized before it enters the context window.

When the shell rewrite layer can recognize a command shape, it will prefix the
command for you. If you need to force the proxy explicitly, run
`codex ztok <subcommand> ...`.

Use the generic wrappers when you need proxying for arbitrary commands:

- `codex ztok err -- <command> [args...]` runs any command and keeps only
  error and warning lines. Use this for noisy build, lint, or compile output.
- `codex ztok test -- <command> [args...]` runs any test command and keeps
  failures plus the summary. Use this for generic test runners that do not
  already have a dedicated ztok subcommand.
