## Embedded ZTOK

Use `{{ logical_launcher_invocation }} ztok ...` in user-facing commentary and
examples. Never show `"$CODEX_SELF_EXE"` or any absolute launcher path.

Default general commands to
`{{ logical_launcher_invocation }} ztok shell <command> [args...]`.

Use `--filter err` and `--filter test` only as filters, not as general
entrypoints. Use a dedicated `{{ logical_launcher_invocation }} ztok <subcommand> ...`
form only when that subcommand directly matches the task.

Do not rely on automatic shell rewrite when planning or explaining commands.
If the user explicitly asks to use `ztok`, commentary and executed commands
must explicitly use `{{ logical_launcher_invocation }} ztok ...`.

For compound shell syntax, wrap a real shell, for example:
`{{ logical_launcher_invocation }} ztok shell bash -lc '<command>'`

If a prior output is `[ztok dedup <ref>]` and the full body is needed, use
`{{ logical_launcher_invocation }} ztok cache expand <ref>`; it defaults to
the current session.

To temporarily disable session dedup while keeping compression, use
`{{ logical_launcher_invocation }} ztok --no-cache ...` or set
`CODEX_ZTOK_NO_DEDUP=1`.
### `read` subcommand

`{{ logical_launcher_invocation }} ztok read <file> [file...] [options]`

Reads one or more files with optional language-aware filtering to strip
comments and boilerplate, reducing token consumption.

| Flag | Values | Default | Effect |
|------|--------|---------|--------|
| `-l`, `--level` | `none`, `minimal`, `aggressive` | `none` | Filter level. `minimal` strips comments (keeps doc comments). `aggressive` also collapses function bodies. |
| `-m`, `--max-lines` | `<N>` | — | Truncate to first ~N lines (smart truncation preserves signatures). |
| `--tail-lines` | `<N>` | — | Keep only the last N lines. |
| `-n`, `--line-numbers` | flag | off | Prefix each line with its line number. |

When a file fails to read, the error is reported on stderr but remaining
files continue to be processed. The exit code is non-zero if any file failed.

