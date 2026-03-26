# RTK (Codex Curated) - Token-Optimized Commands

## Golden Rule

Prefer `codex rtk` for noisy shell output. When Codex embeds a dedicated wrapper, use it. Otherwise `codex rtk` may fall back to the underlying command, so avoid claiming special filtering unless the command below is explicitly listed.

When a dedicated built-in RTK function tool exists, prefer it over shelling out to `codex rtk ...`.
- Use `rtk_read`, `rtk_grep`, `rtk_find`, `rtk_diff`, `rtk_json`, `rtk_deps`, `rtk_log`, `rtk_ls`, `rtk_tree`, `rtk_wc`, `rtk_git_status`, `rtk_git_diff`, `rtk_git_show`, and `rtk_git_log` for token-optimized inspection work.
- Use `rtk_summary` and `rtk_err` for noisy command summarization or error filtering.
- Fall back to `shell_command` + `codex rtk ...` only for RTK capabilities that are not exposed as built-ins.

## Build & Compile

- `codex rtk cargo build`
- `codex rtk cargo check`
- `codex rtk cargo clippy`
- `codex rtk tsc`
- `codex rtk lint`
- `codex rtk prettier --check`
- `codex rtk next build`
- `codex rtk go build`
- `codex rtk go vet`
- `codex rtk golangci-lint`

## Test

- `codex rtk cargo test`
- `codex rtk vitest run`
- `codex rtk playwright test`
- `codex rtk pytest`
- `codex rtk go test`
- `codex rtk test <cmd>`

## Git & Review

- `codex rtk git status`
- `codex rtk git log`
- `codex rtk git diff`
- `codex rtk git show`
- `codex rtk git add`
- `codex rtk git commit`
- `codex rtk git push`
- `codex rtk git pull`
- `codex rtk git branch`
- `codex rtk git fetch`
- `codex rtk git stash`
- `codex rtk git worktree`
- `codex rtk gh ...`
- `codex rtk gt ...`

## Files & Search

- `codex rtk read <file>`
- `codex rtk ls <path>`
- `codex rtk tree <path>`
- `codex rtk find ...`
- `codex rtk grep <pattern> <path>`
- `codex rtk json <file>`
- `codex rtk deps [path]`
- `codex rtk env`
- `codex rtk wc ...`

## Packages & App Tooling

- `codex rtk pnpm ...`
- `codex rtk npm run <script>`
- `codex rtk npx <cmd>`
- `codex rtk prisma ...`
- `codex rtk pip ...`
- `codex rtk format ...`
- `codex rtk ruff ...`
- `codex rtk mypy ...`

## Infra & Network

- `codex rtk docker ...`
- `codex rtk kubectl ...`
- `codex rtk aws ...`
- `codex rtk psql ...`
- `codex rtk curl ...`
- `codex rtk wget <url>`

## Generic Noise Reduction

- `codex rtk err <cmd>` for errors and warnings only
- `codex rtk log [file]` for deduplicated interesting log lines
- `codex rtk diff <file1> [file2]` for condensed diffs
- `codex rtk summary <cmd>` for heuristic summaries

## `mise run` Guidance

- If `mise run <task>` ultimately maps to a supported RTK command, prefer the direct command.
  - e.g. use `codex rtk cargo test` instead of `codex rtk test mise run test` when the underlying command is known.
- If you need to preserve the `mise` task wrapper, use the generic wrappers:
  - `codex rtk test mise run <task>` for test tasks
  - `codex rtk err mise run <task>` for errors/warnings only
  - `codex rtk summary mise run <task>` for multi-step task summaries

Do not reference upstream RTK bootstrap, analytics, or hook-management commands such as `codex rtk init`, `codex rtk gain`, `codex rtk discover`, `codex rtk learn`, `codex rtk rewrite`, `codex rtk hook-audit`, or `codex rtk verify`; Codex does not embed them.
