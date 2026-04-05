# Technical Review

## 已验证假设
- `codex-zmemory` 继续只提供动作执行，决策何时写入交由 `.codex/skills/memory` / 上层 agent orchestrator。
- 所有 `read/search/create/update/add-alias/manage-triggers/stats/doctor/export` 都通过 `codex_zmemory` tool 命令暴露，无需新增 MCP/REST surface。
- 本地保持了 `system://workspace/defaults/alias` 的诊断视图，并新增 `system://paths` 作为显式全活跃路径浏览入口，因此 agent 并不缺“查看全部路径”的能力；文档里需明确 `paths` 与 `index` 的分工。
- `limit` 参数的欢迎特性已在 architecture/QA/skill 里同等说明，`boot/index/paths/recent/glossary/alias` 视图都共享这一截断合同。

## 分叉决策
- **上游差异**：不追 `system://index` 的节点聚合语义；本地通过单独的 `system://paths` 满足“查看全部路径”的治理需求，同时保留 `index` 的既有索引语义。
- **保留增强**：`system://alias` 视图提供 coveragePercent、recommendations；`stats/doctor` 扩展 orphaned/deprecated/trigger 报警；README/skill 里明确这些 review 顺序。
- **路径治理**：`doctor` 与 `stats` 在 path/alias 维度输出 coverage、priorityScore、aliasNodesMissingTriggers 等指标，QA 与 tech-review 里都记录如何从 CLI/skill 查看这些信号，避免引入新的 review service。
- **不扩的方向**：不启用 daemon、HTTP、SSE 或 remote admin service，不把 `codex-zmemory` 抽出独立 memory service。

## 欠缺与下一步
- `system://paths` 与 `system://index` 的语义差异仍需在 README/skill/QA 中持续强调。
- `limit` 参数的文档/CLI/handler 合同要一致，尤其要说明 `limit` 同样适用于 `boot/index/paths/recent/glossary/alias`。
- `.agents/zmemory/tasks.md`/`qa-report.md` 中现有任务状态必须和实际验证一致。
