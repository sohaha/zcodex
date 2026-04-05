# Architecture

## 目标
- 继续保持 `codex-zmemory` 作为嵌入式 Rust crate，仅由 `codex-cli/core` 工具调用。
- 把所有上游 `memory skill` 要求的 review/admin/alias 命令，映射到本地 `system://…` 视图和 `stats/doctor` 报表，不扩 REST 或 daemon 服务。
- 明确 `system://workspace`/`system://defaults` 分析当前 runtime 数据库、`core_memory_uris` 启动信息以及 path resolution。
- 确保 `limit` 参数对 `boot/index/paths/recent/glossary/alias` 这些分页视图保持一致；`defaults/workspace` 继续作为导出入口，但不承诺分页，文档/skill/QA 描述需同源。
- `system://paths` 为显式“全部路径”浏览入口，`system://workspace`/`system://defaults`/`system://alias` 继续作为本地分叉，帮助 agent 了解运行时 path/alias 状态。

## 接入点
1. **动作层**：`tool_api` → `service` → `create/update/search` / `stats/doctor/alias/doctor/doctor` → SQLite schema。
2. **系统视图**：`system://boot/defaults/workspace/index/paths/recent/glossary/alias`，其中 `alias`、`index` 与 `paths` 都与 review/trigger governance 直接关联。
3. **CLI/MCP**：`codex zmemory`子命令（`create`/`export`/`stats`/`doctor`）和 tool handler（`zmemory` + MCP 别名）。
4. **Skill/agent**：`.codex/skills/memory/SKILL.md` 与 Guardian/feature instructions 约束何时 `read/update`.

## 本地分叉
- `system://index` 保留本地既有索引语义；`system://paths` 负责显式列出当前活跃 path 全量。文档/skill 里要明确两者分工，避免继续混用。
- `system://alias`、`stats`/`doctor` 拓展出 alias coverage、review priority、suggested keywords，是本地治理强化。
- `system://workspace`/`system://defaults` 提供 runtime path resolution 与默认 path policy，在上游部署场景中不一定有等价物。
- `system://workspace`/`system://defaults`/`system://alias` 的能力在本地侧更强，agent 通过这些视图观察 alias coverage、trigger status 及 path-level diagnostics。

## 紧急收口
- 如果要再扩展，优先收紧 `system://` 视图的错误合同（`Unknown` vs 空结果）并补 `doctor` 的 path-level 诊断，而不是去追 daemon/REST.
