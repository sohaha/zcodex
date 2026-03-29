# codex-zmemory

`codex-zmemory` 是 `codex-cli` 内置的核心长期记忆模块。

## 范围

当前只做 M0 核心能力：

- `read`
- `search`
- `create`
- `update`
- `delete-path`
- `add-alias`
- `manage-triggers`
- `stats`
- `doctor`
- `rebuild-search`

明确不包含：

- 前端
- REST API
- daemon
- 替换现有 `core/memories`

## 存储

- 数据库位置：`$CODEX_HOME/zmemory/zmemory.db`
- 存储引擎：`SQLite + FTS5`
- 为了降低环境依赖，当前使用 `rusqlite` 的 `bundled` sqlite

## 路径与视图

普通记忆路径：

- `core://agent-profile`
- `core://team/salem`

系统视图：

- `system://boot`
- `system://index`
- `system://recent`
- `system://glossary`
- `system://alias`

## 设计边界

- `zmemory` 使用独立 SQLite，不写入 `codex-state` 的 state DB
- `zmemory` 不替换 `core/memories` 的启动期摘要流程
- CLI 与 core tool 共用同一套 `tool_api / service / schema`

## 常用命令

```bash
codex zmemory stats --json
codex zmemory create core://agent-profile --content "Salem profile memory"
codex zmemory read core://agent-profile --json
codex zmemory search profile --json
codex zmemory export glossary --json
codex zmemory rebuild-search --json
codex zmemory doctor --json
```

## 导出语义

`export` 是本地 CLI 的薄封装，用来导出内置系统视图，不会扩成 REST API、daemon 或独立服务。

- `codex zmemory export boot [--limit N]`
- `codex zmemory export index [--domain core] [--limit N]`
- `codex zmemory export recent [--limit N]`
- `codex zmemory export glossary [--limit N]`
- `codex zmemory export alias [--limit N]`

底层仍复用 `read system://...`：

- `export boot` -> `system://boot`
- `export index --domain core` -> `system://index/core`
- `export recent --limit 5` -> `system://recent/5`
- `export glossary` -> `system://glossary`
- `export alias --limit 5` -> `system://alias/5`

## review 治理入口

当前本地 review 不额外引入独立服务，而是复用现有动作层：

- `codex zmemory stats --json`：查看 `orphanedMemoryCount` 与 `deprecatedMemoryCount`
- `codex zmemory doctor --json`：查看 FTS/关键词一致性，以及 review 相关告警
- `codex zmemory export recent --json`：查看最近变更节点
- `codex zmemory export glossary --json`：查看当前 trigger 网络

建议的最小 review 顺序：

1. 先看 `stats` 判断 orphan / deprecated 压力。
2. 再看 `doctor` 判断是否存在需要优先修复的告警。
3. 再用 `export recent` / `export glossary` 判断新节点是否进入召回网络。
4. 视 `stats` 中 alias/trigger 覆盖后，再用 `export alias` 或 `read system://alias` 观察 alias coverage 百分比与缺 trigger 列表。

`system://alias` 视图返回结构：

- `aliasNodeCount` / `triggerNodeCount` / `aliasNodesMissingTriggers`
- `coveragePercent`：已填 trigger 的 alias 节点比例
- `recommendations`：最多 3 条缺 trigger alias，直接给出 `nodeUri`、`reviewPriority`、`priorityScore`、`priorityReason`、`suggestedKeywords`、建议动作和可复制的命令
- `entries`：按治理优先级排序，包含 `aliasCount`、`triggerCount`、`missingTriggers`、`reviewPriority`、`priorityScore`、`priorityReason` 与 `suggestedKeywords`

推荐在 review 流程中：先看 `stats`/`doctor` 找出是否有 alias 覆盖不足，再用 `system://alias` 看到具体有哪些 alias 节点缺 trigger，最后执行 `manage-triggers` 或 `add-alias` 补强。

其中 `suggestedKeywords` 会根据该 node 的现有 path / alias path 提取一组可直接尝试的关键词，`priorityReason` 则解释为什么该节点被排在当前治理优先级。

### alias/trigger 治理输出

为了进一步支持 alias/trigger 审核，可直接 `read system://alias [limit]`，该视图汇总 alias nodes、trigger 覆盖与 alias-without-trigger 的列表：

- `codex zmemory export alias --json`：查看 alias/trigger 总量与缺失概况。
- `codex zmemory export alias --limit 5 --json`：按治理优先级排序，优先列出缺 trigger 且 alias 扇出更高的节点。
- `codex zmemory read system://alias --json` / `read system://alias/5 --json`：仍保留为底层等价入口。

这些信息配合 `stats`/`doctor` 能形成“alias coverage + trigger wiring”评估，为 alias review 清单提供输入。

## memory skill 参考

- 根级 skill `memory` 里详细记录了 recall/capture/refine/linking/review/handoff 的最小编排（见 `.codex/skills/memory/SKILL.md`）。
- 任何时候想直接复用 CLI 示例，可参考 `.codex/skills/memory/references/cli-recipes.md`，按已实现的 `codex zmemory` 命令顺序排列。
- 需要一个 review 复核清单时，可参阅 `.codex/skills/memory/references/review-playbook.md` 中列出的 `stats`/`doctor`/`export` 流程。
- 想在 review 中具体处理 alias/trigger 覆盖，可直接查 `.codex/skills/memory/references/review-playbook.md` 里新增的 alias check，例如 `read system://alias` + `manage-triggers` 建议。
- 需要启动一个新项目时，可以参考 review-playbook 里的 project init checklist，它直接用 `create`/`add-alias`/`manage-triggers` 等命令搭建骨架。

## 创建语义

`zmemory` 支持两种路径创建模式：

- **完整 URI 写入（现行模式）**：直接传 `core://team/alice` 等完整路径，`create` 会在此位置插入新记忆，仅需 `--content`、可选 `--priority`/`--disclosure`。这条路径等价于早期版本，保持向后兼容。
- **父路径 + 标题（上游兼容模式）**：也可以传 `--parent-uri core://team` 和 `--title alice-profile`，工具会在父路径下构建新节点，`title` 会用于生成那一段路径（空 title 将自动编号，非法字符会报错）。此模式对齐 upstream `nocturne_memory` 的 `parent_uri + title` 合同，也允许显式调整 `priority` 与 `disclosure`。

两个模式可以并存：仍可传 `URI` 路径以避免改动，也可以借用 `parent-uri/title` 接口来复用上游的更新逻辑。CLI 中的 `create` 命令接受 `--parent-uri` 和 `--title` 可选参数，使用时推荐先阅读 `.agents/zmemory/tasks.md` 中的具体任务拆解。

## 与 upstream memory skill 的最小衔接

当前不直接把 upstream `.codex/skills/memory/` 整包搬进 `codex-rs`，只保持一层可审计的动作映射：

- `bootstrap` / `recall`：优先 `read system://boot`、`read <uri>`、`search <query>`
- `capture`：新增稳定信息时用 `create`，已有节点修订时优先 `update`
- `refine`：复用已对齐的 patch / append / metadata-only `update`
- `linking`：复用 `add-alias` 与 `manage-triggers`
- `review`：复用 `export recent|glossary`、`stats`、`doctor`、`rebuild-search`
- `handoff`：当前仍由上层项目记忆或 agent 工作流负责，不在 `codex-zmemory` crate 内扩成新的会话管理接口

这样做的目的，是让 `codex-zmemory` 继续只提供稳定的本地动作层，而把“什么时候读、什么时候写、什么时候整理”留给上层 skill 或项目流程编排。
