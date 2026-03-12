# RTK (Codex Curated) - Token-Optimized Commands

## Golden Rule

Prefer `rtk` for noisy shell output. When Codex embeds a dedicated wrapper, use it. Otherwise `rtk` may fall back to the underlying command, so avoid claiming special filtering unless the command below is explicitly listed.

## Build & Compile

- `rtk cargo build`
- `rtk cargo check`
- `rtk cargo clippy`
- `rtk tsc`
- `rtk lint`
- `rtk prettier --check`
- `rtk next build`
- `rtk go build`
- `rtk go vet`
- `rtk golangci-lint`

## Test

- `rtk cargo test`
- `rtk vitest run`
- `rtk playwright test`
- `rtk pytest`
- `rtk go test`
- `rtk test <cmd>`

## Git & Review

- `rtk git status`
- `rtk git log`
- `rtk git diff`
- `rtk git show`
- `rtk git add`
- `rtk git commit`
- `rtk git push`
- `rtk git pull`
- `rtk git branch`
- `rtk git fetch`
- `rtk git stash`
- `rtk git worktree`
- `rtk gh ...`
- `rtk gt ...`

## Files & Search

- `rtk read <file>`
- `rtk ls <path>`
- `rtk tree <path>`
- `rtk find ...`
- `rtk grep <pattern> <path>`
- `rtk json <file>`
- `rtk deps [path]`
- `rtk env`
- `rtk wc ...`

## Packages & App Tooling

- `rtk pnpm ...`
- `rtk npm run <script>`
- `rtk npx <cmd>`
- `rtk prisma ...`
- `rtk pip ...`
- `rtk format ...`
- `rtk ruff ...`
- `rtk mypy ...`

## Infra & Network

- `rtk docker ...`
- `rtk kubectl ...`
- `rtk aws ...`
- `rtk psql ...`
- `rtk curl ...`
- `rtk wget <url>`

## Generic Noise Reduction

- `rtk err <cmd>` for errors and warnings only
- `rtk log [file]` for deduplicated interesting log lines
- `rtk diff <file1> [file2]` for condensed diffs
- `rtk summary <cmd>` for heuristic summaries

Do not reference upstream RTK bootstrap, analytics, or hook-management commands such as `rtk init`, `rtk gain`, `rtk discover`, `rtk learn`, `rtk rewrite`, `rtk hook-audit`, or `rtk verify`; Codex does not embed them.
