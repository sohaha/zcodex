# Review Playbook

## 最小 review 检查清单

1. `codex zmemory export workspace --json`：先确认当前实际 DB、路径来源、默认路径差异与 boot 健康度。
2. `codex zmemory export defaults --json`：确认当前现象是不是产品默认，而不是当前 workspace 特例。
3. `codex zmemory stats --json`：读取 `orphanedMemoryCount`、`deprecatedMemoryCount`、`aliasNodeCount`、`triggerNodeCount`、`pathsMissingDisclosure`、`disclosuresNeedingReview`，判断是否有 review 压力和 alias/trigger/disclosure 覆盖盲点。
4. `codex zmemory doctor --json`：查看 `issues` 里是否有 `orphaned_memories`、`deprecated_memories_awaiting_review`、`alias_nodes_missing_triggers`、`paths_missing_disclosure`、`disclosures_need_review`，确认 FTS / keyword / alias / disclosure 兼容状态。
5. `codex zmemory export recent --json`：确认最近内容版本节点已经被系统视图收录；若问题涉及 alias/trigger/path 等治理动作，不要只看 recent。
6. `codex zmemory export glossary --json`：确认 trigger / keyword 覆盖，判断是否需要 `manage-triggers` 或 `add-alias`。
7. `codex zmemory export alias --json`：确认 alias scope，查明哪些 alias node 还没 trigger；优先处理 `reviewPriority=high` 的节点，结合 `priorityReason` / `suggestedKeywords` 判断是否直接执行 `recommendations[].command`。
8. `codex zmemory export paths --json`：当问题涉及“当前有哪些活跃 path / alias”时，用它核对实际 path 集合。
9. 若 search 结果异常，先确认是否属于预期合同：未知 domain 会直接报 valid domains；alias 查询会做 separator-normalization；`search_memory` 主参数是 `uri` scope，`domain` 仅兼容旧调用；CJK 只按 token boundary / trigger 命中，不按任意裸子串误召回。
10. 若 disclosure 有缺口，优先补齐单一 disclosure：
   - `codex zmemory update core://legacy --disclosure "review" --json`
   - 避免一个 disclosure 同时塞多个触发意图（如 `review or handoff`）
11. 若 alias coverage 低于 trigger，优先补 trigger：
   - `codex zmemory manage-triggers core://legacy --add review --json`
   - `codex zmemory add-alias alias://legacy core://legacy --json`

## 交接指引

- review 完成后，把新的关键结论写入项目主节点，记录 `stats`/`doctor` 的变化。
- 若有长周期治理安排，更新 `.ai/memory/handoff.md` 并让 vbm 跟进。

## project init checklist

1. `codex zmemory create core://project-alpha --content "Project constraints" --priority 2 --json`
2. `codex zmemory create core://project-alpha/architecture --content "Architecture notes" --json`
3. `codex zmemory add-alias alias://project-alpha core://project-alpha --json`
4. `codex zmemory manage-triggers core://project-alpha --add launch --json`
5. 之后执行上面的 review 检查清单，保证 trigger/alias 覆盖已到位。
