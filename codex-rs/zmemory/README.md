# codex-zmemory

`codex-zmemory` 是 `codex-cli` 内置的核心长期记忆模块。

## 范围

当前只做 M0 核心能力：

- `read`
- `history`
- `search`
- `export`
- `import`
- `create`
- `batch-create`
- `update`
- `batch-update`
- `delete-path`
- `add-alias`
- `manage-triggers`
- `stats`
- `audit`
- `doctor`
- `rebuild-search`

明确不包含：

- 前端
- REST API
- daemon
- 替换现有 `core/memories`

## 存储

- 默认数据库位置：`$CODEX_HOME/zmemory/projects/<project-key>/zmemory.db`
- 当 `[zmemory].path` 显式配置时：
  - 绝对路径直接使用
  - 相对路径在 git 仓库内相对主 repo root 解析，非 git 目录相对当前 `cwd` 解析
- 配置入口：

```toml
[zmemory]
path = "./agents/memory.db"
valid_domains = ["core", "project", "notes"]
core_memory_uris = [
  "core://agent/coding_operating_manual",
  "core://my_user/coding_preferences",
  "core://agent/my_user/collaboration_contract",
]
```

- 如果你想在多个项目间共享同一份库，请显式配置全局路径：

```toml
[zmemory]
path = "/absolute/path/to/.codex/zmemory/zmemory.db"
```

- 存储引擎：`SQLite + FTS5`
- 为了降低环境依赖，当前使用 `rusqlite` 的 `bundled` sqlite

## 域与 boot 基线

- `[zmemory].valid_domains` / `[zmemory].core_memory_uris` 可直接声明 runtime profile
- `VALID_DOMAINS`：逗号分隔的可写域列表；默认 `core`
- `CORE_MEMORY_URIS`：逗号分隔的 boot 锚点 URI；默认 `core://agent,core://my_user,core://agent/my_user`
- 优先级：`[zmemory]` 配置 > 环境变量 > 产品默认值
- `system` 是保留只读域，不需要写进 `VALID_DOMAINS`
- 当读写到未知 domain 时，会返回 `unknown domain 'X'. valid domains: ...` 这种显式错误，便于 CLI / tool 调用方直接修正输入。

## 路径与视图

普通记忆路径：

- `core://agent-profile`
- `core://team/salem`

搜索行为补充：

- 查询会做 separator-normalization，像 `foo/bar`、`foo and bar`、`foo，bar` 这类 alias/trigger 变体可以互相召回。
- alias 命中会按 `node_uuid` 去重，避免同一节点因为多个 alias 重复出现在结果里。
- 排序优先级为：`priority` 升序，其次更短路径，再其次 URI 字典序。
- snippet 优先展示 literal 命中，其次 token 命中，再退回内容前缀；对 disclosure / URI 命中则回退到正文片段。
- CJK 搜索遵循 token boundary 规则：精确 trigger 可命中，任意内部裸子串不会误命中。

系统视图：

- `system://boot`
- `system://defaults`
- `system://workspace`
- `system://index`
- `system://paths`
- `system://recent`
- `system://glossary`
- `system://alias`

## 设计边界

- `zmemory` 使用独立 SQLite，不写入 `codex-state` 的 state DB
- `zmemory` 不替换 `core/memories` 的启动期摘要流程
- CLI 与 core tool 共用同一套 `tool_api / service / schema`

## MCP 风格工具名

为对齐 MCP 生态，内置别名工具如下（映射到同一套动作层）：

- `read_memory` -> `read`
- `search_memory` -> `search`（主参数为 `query` + 可选 `uri` scope；`domain` 仅兼容旧调用）
- `create_memory` -> `create`
- `update_memory` -> `update`
- `delete_memory` -> `delete-path`（只删除路径；若同一节点仍有其他 path/alias，则底层内容仍可通过其他路径访问）
- `add_alias` -> `add-alias`
- `manage_triggers` -> `manage-triggers`

## 常用命令

```bash
codex zmemory stats --json
codex zmemory create core://agent-profile --content "Salem profile memory"
codex zmemory read core://agent-profile --json
codex zmemory batch-create --items-json '[{"uri":"core://agent-batch","content":"A"}]' --json
codex zmemory batch-update --items-json '[{"uri":"core://agent-batch","append":" more"}]' --json
codex zmemory history core://agent-profile --json
codex zmemory search profile --json
codex zmemory export-memory --uri core://agent-profile --json
codex zmemory import-memory --items-json '[{"uri":"core://imported-profile","content":"Imported profile","keywords":["imported-profile"],"aliases":[{"uri":"core://imported-profile-alias"}]}]' --json
codex zmemory export glossary --json
codex zmemory audit --limit 10 --json
codex zmemory rebuild-search --json
codex zmemory doctor --json
```

## 导出语义

`export-memory` / `import-memory` 是真实记忆导入导出入口，直接桥接到底层动作层：

- `codex zmemory export-memory --uri core://agent-profile`
- `codex zmemory export-memory --domain core`
- `codex zmemory import-memory --items-json '<json array>'`

其中：

- `export-memory` 对应 tool action `export`
  - 必须且只能提供一个范围：`--uri <uri>` 或 `--domain <domain>`
  - `--uri` 保留请求 URI 为主项，并把同节点的其他 live path 放到 `aliases`
  - `--domain` 导出该 domain 下全部活跃节点
- `import-memory` 对应 tool action `import`
  - `--items-json` 传入导入数组
  - 数组项支持 `uri`、`content`、`priority`、`disclosure`、`keywords`、`aliases`
  - `aliases` 数组项支持 `uri`、`priority`、`disclosure`

示例：

```bash
codex zmemory export-memory --uri core://agent-profile --json
codex zmemory export-memory --domain core --json
codex zmemory import-memory --items-json '[
  {
    "uri": "core://imported-profile",
    "content": "Imported profile",
    "keywords": ["imported-profile"],
    "aliases": [
      {
        "uri": "core://imported-profile-alias"
      }
    ]
  }
]' --json
```

返回语义：

- `export-memory` 返回 `scope`、`count`、`items`
- `import-memory` 返回 `count`、`results`、`documentCount`

## 系统视图导出语义

`export` 是本地 CLI 的薄封装，用来导出内置系统视图，不会扩成 REST API、daemon 或独立服务。

- `codex zmemory export boot [--limit N]`
- `codex zmemory export defaults`
- `codex zmemory export workspace`
- `codex zmemory export index [--domain core] [--limit N]`
- `codex zmemory export paths [--domain core] [--limit N]`
- `codex zmemory export recent [--limit N]`
- `codex zmemory export glossary [--limit N]`
- `codex zmemory export alias [--limit N]`

这些 `export` 入口只是为了 discoverability；底层 contract 仍以 `read system://...` 为准。

底层仍复用 `read system://...`：

- `export boot` -> `system://boot`
- `export defaults` -> `system://defaults`
- `export workspace` -> `system://workspace`
- `export index --domain core` -> `system://index/core`
- `export paths --domain core` -> `system://paths/core`
- `export recent --limit 5` -> `system://recent/5`
- `export glossary` -> `system://glossary`
- `export alias --limit 5` -> `system://alias/5`

`system://index` 与 `system://paths` 现在刻意区分：

- `system://index`：面向浏览/召回的稳定索引视图，保留当前本地的索引语义。
- `system://paths`：显式列出全部活跃记忆路径，适合治理、排查“到底有哪些 path”这类问题。

`system://boot` 现在优先返回 `CORE_MEMORY_URIS` 中已存在的锚点节点，并显式给出缺失锚点列表；不再按全库 priority 直接截取前 N 条。

`system://defaults` / `system://workspace` 用来显式区分“产品默认事实”和“当前工作区实际事实”：

- `system://defaults`：返回产品默认 `validDomains` / `coreMemoryUris`、默认 DB path policy 与 boot contract 固定事实对象；不会被用户配置改写。
- `system://workspace`：返回当前实际 `dbPath/source/reason/workspaceKey/workspaceBase`、`hasExplicitZmemoryPath`、`defaultWorkspaceKey/defaultDbPath/dbPathDiffers`、runtime `validDomains/coreMemoryUris`，并内嵌 `boot` / `bootHealthy`。
- 当问题是在问“现在到底用的是哪个记忆库”“这是产品默认还是当前仓库覆盖”时，应先读 `system://workspace`，再用 `system://defaults` 校对默认值。

## review 治理入口

当前本地 review 不额外引入独立服务，而是复用现有动作层：

- `codex zmemory read system://workspace --json`：确认当前工作区实际 DB、默认路径差异、boot 健康度
- `codex zmemory read system://defaults --json`：确认产品默认 domains / boot anchors / 默认路径策略
- `codex zmemory stats --json`：查看 `orphanedMemoryCount`、`deprecatedMemoryCount`、`pathsMissingDisclosure`、`disclosuresNeedingReview`
- `codex zmemory doctor --json`：查看 FTS/关键词一致性，以及 alias/disclosure 等 review 相关告警
- `codex zmemory audit --json`：查看最近元数据治理动作时间线；支持 `--action <create|update|add-alias|manage-triggers|delete-path>` 与 `--uri <core://...>` 精确过滤
- `codex zmemory history <uri> --json`：查看单个节点的内容版本链，适合排查 update 产生的 deprecated 历史
- `codex zmemory batch-create --items-json '<json array>' --json`：一次性创建多个节点，数组项支持 `uri`/`parentUri`/`content`/`title`/`priority`/`disclosure`，按 URI 顺序返回 `nodeUuid`
- `codex zmemory batch-update --items-json '<json array>' --json`：一次性更新多个 URI，数组项支持 `uri`、`content`、`oldString`/`newString`、`append`、`priority`、`disclosure`，返回每条 `contentChanged`/`nodeUuid`
- `codex zmemory stats --json` / `doctor --json`：同时查看稳定诊断对象 `pathResolution`，并在顶层重复输出 `dbPath` / `workspaceKey` / `source` / `reason`
- `codex zmemory export recent --json`：查看最近内容版本节点（按节点内容时间聚合，不反映 alias/trigger/path 元数据治理动作）
- `codex zmemory export glossary --json`：查看当前 trigger 网络

当前 `pathResolution` 的稳定字段为：

```json
{
  "dbPath": "/home/me/.codex/zmemory/projects/my-repo-a1b2c3d4e5f6/zmemory.db",
  "workspaceKey": "my-repo-a1b2c3d4e5f6",
  "source": "projectScoped",
  "reason": "defaulted to project scope /home/me/.codex/zmemory/projects/my-repo-a1b2c3d4e5f6/zmemory.db from repo root /workspace/my-repo"
}
```

- `dbPath`：当前实际使用的 sqlite 文件
- `workspaceKey`：默认项目库的稳定项目 key；显式 `[zmemory].path` 场景通常为 `null`
- `source`：`explicit` / `projectScoped`
- `reason`：人类可读的解析原因

建议的最小 review 顺序：

1. 先看 `system://workspace` 判断当前实际 DB、boot 是否健康、是否显式覆盖默认路径。
2. 再看 `system://defaults`，确认当前现象是产品默认还是 workspace 特例。
3. 再看 `stats` / `doctor` 判断 orphan / deprecated / alias / disclosure 压力。
4. 再用 `audit` / `export paths` / `export recent` / `export glossary` 判断最近治理动作、活跃路径、最近内容版本与 trigger wiring 是否进入召回网络。
5. 若 `stats` / `doctor` 提示 alias/trigger 缺口，再用 `export alias` 或 `read system://alias` 观察 alias coverage 百分比与缺 trigger 列表。

### 区分“没有记忆”与“搜不到”

- `read <uri>` 返回 `memory not found`，只代表这条具体路径不存在。
- `search <query>` 结果为空，不能直接当作“系统没有相关记忆”；先检查 `system://workspace.bootHealthy`、`stats` / `doctor`、以及 `system://alias` 的 trigger 覆盖。
- 若 `doctor/stats` 已显示 alias/trigger/disclosure 缺口，当前更接近“可检索性不足”，而不是“没有 durable memory”。

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

### 旧节点桥接策略

- `update` 在内容变化时会写入新的 memory 版本，并把旧版本标记为 `deprecated`，同时记录 `migrated_to`。
- `delete-path` 在删除最后一个 path 引用时，会把对应 memory 标记为 deprecated；此时 `orphanedMemoryCount` 会升高，表示节点仍在库里但已经没有活跃 path。
- `stats` / `doctor` 暴露 `deprecatedMemoryCount` 和 `orphanedMemoryCount`，用于区分“迁移中的旧版本”和“需要手工处理的孤儿旧节点”。
- 当前 bridge 策略是显式治理而不是自动迁移：先保留一个 canonical live node，再用 `add-alias` 保留旧叫法、用 `manage-triggers` 补自然问法，最后复跑 `stats` / `doctor` 观察压力是否下降。

## 项目内参考

- `docs/config.md`：查看 `native_memories` / `zmemory` feature、`[zmemory]` 配置块、默认路径策略与 `system://workspace` / `system://defaults` 的用途。
- `.agents/embedded-zmemory-overhaul/architecture.md`：查看 recall/orchestration、治理闭环和 defaults-vs-workspace 设计背景。
- `.agents/embedded-zmemory-overhaul/qa-report.md`：查看当前验证命令、通过项和剩余风险。

## 创建语义

`zmemory` 支持两种路径创建模式：

- **完整 URI 写入（现行模式）**：直接传 `core://team/alice` 等完整路径，`create` 会在此位置插入新记忆，仅需 `--content`、可选 `--priority`/`--disclosure`。这条路径等价于早期版本，保持向后兼容。
- **父路径 + 标题（上游兼容模式）**：也可以传 `--parent-uri core://team` 和 `--title alice-profile`，工具会在父路径下构建新节点，`title` 会用于生成那一段路径（空 title 将自动编号，非法字符会报错）。此模式对齐 upstream `nocturne_memory` 的 `parent_uri + title` 合同，也允许显式调整 `priority` 与 `disclosure`。

两个模式可以并存：仍可传 `URI` 路径以避免改动，也可以借用 `parent-uri/title` 接口来复用上游的更新逻辑。CLI 中的 `create` 命令接受 `--parent-uri` 和 `--title` 可选参数，使用时推荐先阅读 `.agents/zmemory/tasks.md` 中的具体任务拆解。

## 批量语义

- `batch-create` / `batch-update` 在单个事务里顺序执行全部子项。
- 任意一项失败会整体回滚，不会留下部分成功结果。
- tool 与 function-call 入口使用 `items` 数组；CLI 为了便于传参，使用 `--items-json '<json array>'`。

## 与 upstream memory skill 的最小衔接

当前不直接把 upstream `.codex/skills/memory/` 整包搬进 `codex-rs`，只保持一层可审计的动作映射：

- `bootstrap` / `recall`：优先 `read system://boot`、`read <uri>`、`search <query>`
- `capture`：新增稳定信息时用 `create`，已有节点修订时优先 `update`
- `refine`：复用已对齐的 patch / append / metadata-only `update`
- `linking`：复用 `add-alias` 与 `manage-triggers`
- `review`：复用 `export paths|recent|glossary|alias`、`stats`、`doctor`、`rebuild-search`
- `handoff`：当前仍由上层项目记忆或 agent 工作流负责，不在 `codex-zmemory` crate 内扩成新的会话管理接口

这样做的目的，是让 `codex-zmemory` 继续只提供稳定的本地动作层，而把“什么时候读、什么时候写、什么时候整理”留给上层 skill 或项目流程编排。

## 稳定偏好主动写入

当 `codex-core` 启用 `Feature::Zmemory` 时，上层运行时会只针对“明确、稳定、低歧义”的称呼/自称偏好触发主动写入；`codex-zmemory` 本身仍只负责动作执行，不负责决定何时落库。

- 主动写入前：上层会先读 `system://workspace` 确认当前活动库，再读目标 canonical URI 查重
- canonical URI：`core://my_user`（用户称呼偏好）、`core://agent`（助手自称偏好）、`core://agent/my_user`（双方协作称呼约定）
- 写入规则：目标节点不存在时用 `create`，已存在且是同一主题修订时用 `update`
- 写入后验证：再次 `read` 对应 canonical URI，确认内容已落到当前活动库
- 失败表现：上层会发出可观察 warning；不会把“已记住”伪装成成功

如果问题是在排查“为什么这次没有自动记住称呼偏好”，先检查：

1. 当前会话是否启用了 `Feature::Zmemory`
2. `system://workspace` 是否指向预期数据库
3. 当前输入是否属于明确、稳定、低歧义的偏好表达，而不是一次性临时指令

## 推荐的编码记忆配置

推荐把 `zmemory` 作为项目级知识库使用：

```toml
[zmemory]
valid_domains = ["core", "project", "notes"]
core_memory_uris = [
  "core://agent/coding_operating_manual",
  "core://my_user/coding_preferences",
  "core://agent/my_user/collaboration_contract",
]
```

建议分层：

- `core://...`：长期稳定的协作规则
- `project://<repo>/...`：项目架构、模块图、测试入口、常见坑
- `notes://...`：阶段性 debug 结论、迁移观察、待沉淀经验

建议让 boot 只保留少量高价值 `core://...` 锚点，不要把整份项目知识全量塞进 `system://boot`。默认数据库仍是项目库；只有需要跨项目共享时，才显式把 `[zmemory].path` 指向全局 DB。
