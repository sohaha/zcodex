# CLI Recipes

以下命令均使用当前仓库的 `codex zmemory` 子命令，确保与最新实现一致：

## bootstrap / recall
- `codex zmemory read system://boot --json`
- `codex zmemory read core://agent --json`
- `codex zmemory search "GraphService" --json`

## project-init / contextual bootstrap
- `codex zmemory create core://project-alpha --content "Project constraints" --priority 2 --json`
- `codex zmemory create core://project-alpha/architecture --content "Architecture notes" --json`
- `codex zmemory add-alias alias://project-alpha core://project-alpha --json`
- `codex zmemory manage-triggers core://project-alpha --add launch --json`

## capture / create
- `codex zmemory create core://project-alpha --content "Project constraints" --priority 2 --json`
- `codex zmemory create --parent-uri core://project-alpha --title notes --content "Nested note" --json`

## refine / update
- `codex zmemory update core://project-alpha --append "\nNew insight" --json`
- `codex zmemory update core://project-alpha --old-string "constraints" --new-string "guidelines" --json`
- `codex zmemory update core://project-alpha --priority 5 --disclosure "review" --json`

## linking / alias / triggers
- `codex zmemory add-alias alias://latest-guidance core://project-alpha --json`
- `codex zmemory manage-triggers core://project-alpha --add strategy --add review --json`

## recall helpers
- `codex zmemory search "review pressure" --uri core://project-alpha --limit 10 --json`

## review / admin
- `codex zmemory stats --json`
- `codex zmemory doctor --json`
- `codex zmemory update core://project-alpha --disclosure "review" --json`
- `codex zmemory export recent --limit 5 --json`
- `codex zmemory export glossary --json`
- `codex zmemory rebuild-search --json`
- `codex zmemory export alias --json`（alias coverage + missing triggers）
- `codex zmemory export alias --limit 5 --json`（查看 alias 覆盖排名前 5 的节点）

## recall helpers
- `codex zmemory search "review pressure" --uri core://project-alpha --limit 10 --json`

每个命令都直接映射到 `codex-rs/zmemory` 的 `run_zmemory_tool`；缺省的 `--json` 能保持结构化输出，便于 skill 评估与自动化。
