# zmemory

`zmemory` 是 Codex 内置的可写长期记忆工具，使用独立的 `SQLite + FTS5`
数据库存储 durable memory。

它和原生 `memories` 是两套独立系统：

- `native_memories`：内置只读 memory pipeline
- `zmemory`：可读写、可搜索、可治理的本地长期记忆库

如果你只想确认两者的配置边界，先看 `docs/config.md`；如果你要实际使用
`zmemory` 的命令、系统视图和诊断方法，看这份文档。

## 默认存储

- 默认数据库路径：`$CODEX_HOME/zmemory/projects/<project-key>/zmemory.db`
- `project-key` 基于当前项目稳定解析得到；同一主仓库与其 worktree 默认共用同一库
- 如果显式设置 `[zmemory].path`：
  - 绝对路径直接使用
  - 相对路径在 git 仓库内相对主 repo root 解析
  - 非 git 目录相对当前 `cwd` 解析

如果你希望多个项目共享同一份库，请显式配置全局路径：

```toml
[zmemory]
path = "/absolute/path/to/.codex/zmemory/zmemory.db"
```

## 配置

最小配置示例：

```toml
[zmemory]
path = "./agents/memory.db"
```

完整 runtime profile 示例：

```toml
[zmemory]
path = "./agents/memory.db"
namespace = "team-alpha"
valid_domains = ["core", "project", "notes"]
core_memory_uris = [
  "core://agent/coding_operating_manual",
  "core://my_user/coding_preferences",
  "core://agent/my_user/collaboration_contract",
]
```

字段说明：

- `path`：可选数据库路径覆盖
- `namespace`：可选 runtime namespace 覆盖；默认仍为 `""`
- `valid_domains`：可选可写域列表覆盖
- `core_memory_uris`：可选 boot 锚点覆盖

优先级：

1. `[zmemory]` 配置
2. 环境变量 `VALID_DOMAINS` / `CORE_MEMORY_URIS`
3. 产品默认值

当前产品默认值：

- `VALID_DOMAINS=core,project,notes`
- `CORE_MEMORY_URIS=core://agent/coding_operating_manual,core://my_user/coding_preferences,core://agent/my_user/collaboration_contract`

`system` 是保留只读域，不需要写进 `VALID_DOMAINS`。

## 常用命令

```bash
codex zmemory stats --json
codex zmemory doctor --json
codex zmemory read system://workspace --json
codex zmemory read system://defaults --json
codex zmemory search profile --json
codex zmemory create core://agent-profile --content "Salem profile memory"
codex zmemory read core://agent-profile --json
codex zmemory rebuild-search --json
```

## 核心动作

当前 `zmemory` 支持：

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

## MCP 风格工具名

为对齐 MCP 生态，内置别名工具如下（映射到同一套动作层）：

- `read_memory` -> `read`
- `search_memory` -> `search`（主参数为 `query` + 可选 `uri` scope；`domain` 仅兼容旧调用）
- `create_memory` -> `create`
- `update_memory` -> `update`
- `delete_memory` -> `delete-path`（只删除路径；若同一节点仍有其他 path/alias，则底层内容仍可通过其他路径访问）
- `add_alias` -> `add-alias`
- `manage_triggers` -> `manage-triggers`

## 系统视图

常用系统视图：

- `system://boot`
- `system://defaults`
- `system://workspace`
- `system://index`
- `system://paths`
- `system://recent`
- `system://glossary`
- `system://alias`

其中最重要的是：

- `system://workspace`：当前会话实际生效的 runtime profile；包含当前使用的库、路径来源、`validDomains`、`coreMemoryUris`、`bootRoles`、`unassignedUris`、boot 健康度，以及是否显式覆盖默认路径
- `system://defaults`：产品默认事实；只报告内置默认 domains、boot anchors、`bootRoles` / `unassignedUris` 与默认路径策略，不混入当前项目或用户配置覆盖状态

建议排查顺序：

1. 先看 `system://workspace`
2. 再看 `system://defaults`
3. 再看 `stats` / `doctor`
4. 最后看 `system://paths`、`system://alias`、`recent`、`glossary`
   - `recent` 代表最近内容版本节点；alias/trigger/path 等治理动作请结合 `paths`、`alias`、`doctor` 判断

默认 coding boot profile 的 3 个角色槽位为：

- `agent_operating_manual`
- `user_preferences`
- `collaboration_contract`

如果当前 runtime profile 少于 3 个 boot anchors，`bootRoles` 会保留这些角色并返回 `configured=false`、`uri=null`；如果超过 3 个，多出来的锚点会出现在 `unassignedUris`。

## 诊断字段

`codex zmemory stats --json` 和 `codex zmemory doctor --json` 都会返回稳定诊断对象
`result.pathResolution`，同时把关键字段重复到 `result` 顶层：

```json
{
  "dbPath": "/home/me/.codex/zmemory/projects/my-repo-a1b2c3d4e5f6/zmemory.db",
  "workspaceKey": "my-repo-a1b2c3d4e5f6",
  "source": "projectScoped",
  "reason": "defaulted to project scope /home/me/.codex/zmemory/projects/my-repo-a1b2c3d4e5f6/zmemory.db from repo root /workspace/my-repo"
}
```

字段含义：

- `dbPath`：当前实际使用的 sqlite 文件
- `workspaceKey`：稳定项目 key；显式 `[zmemory].path` 时通常为 `null`
- `source`：`explicit` 或 `projectScoped`
- `reason`：人类可读的路径解析原因

## 如何区分“没有记忆”与“搜不到”

- `read <uri>` 返回 `memory not found`：只表示这条路径不存在
- `search <query>` 结果为空：不等于系统里完全没有相关记忆

当 `search` 为空时，先检查：

- `system://workspace.bootHealthy`
- `stats`
- `doctor`
- `system://alias`

很多时候问题是 recall coverage、alias 或 trigger 不足，而不是数据库里没有 durable
memory。

## 推荐的编码记忆配置

如果你想把 `zmemory` 用作“编码协作型长期记忆”，推荐把项目知识与临时结论从 `core` 中拆开：

```toml
[zmemory]
valid_domains = ["core", "project", "notes"]
core_memory_uris = [
  "core://agent/coding_operating_manual",
  "core://my_user/coding_preferences",
  "core://agent/my_user/collaboration_contract",
]
```

建议约定：

- `core://...`：长期稳定的协作规则
- `project://<repo>/...`：项目级架构、约束、常见坑
- `notes://...`：阶段性调试结论、迁移观察

建议让 boot 只保留少量高价值 `core://...` 节点；不要把整份项目知识都塞进 boot。默认项目库路径保持不变，只有需要跨项目共享数据库时才显式配置 `[zmemory].path` 为全局路径。

## 设计边界

- `zmemory` 使用独立 SQLite，不写入 `codex-state` 的 state DB
- `zmemory` 不替换原生 `core/memories` 启动摘要流程
- CLI 与 core tool 共用同一套 `tool_api / service / schema`
- `codex-zmemory` 内核仍聚焦嵌入式动作层；面向上游 web 的兼容 HTTP 接口由 `codex-cli` 的 `zmemory serve-compat` 在内核外提供
- 即使已支持上游 web 复用，路线仍是“内核外侧增加薄 adapter”，而不是把 crate 本身扩成独立 daemon / backend

## 上游 web 复用现状

当前仓库已经完成以下链路：

1. 先对齐 SQLite schema、`namespace`、runtime `dbPath` / boot contract
2. 再对齐 review / snapshot / maintenance 所需的数据语义
3. 在内核外增加 `codex zmemory serve-compat`，对接 `/api/browse`、`/api/review`、`/api/maintenance`
4. 用真实上游 frontend 复用 browse / review / maintenance 主链路

这意味着：

- 当前文档描述的 CLI / core / `system://...` 视图仍是事实来源
- 上游 web 必须连接与 CLI / core 相同的数据库与 runtime profile；当前 compat adapter 同时接受 `/api/*` 与代理剥前缀后的 `/*` 请求形态
- `system://workspace`、`system://defaults`、`system://alias`、`system://paths` 以及 `stats` / `doctor` 仍保留为本地诊断与治理增强，不会因为对齐上游而被静默删掉
- `/review` 已支持最小 approve / clear-all / rollback 写语义，且 review queue 会过滤无法构建 diff 的 orphan 节点，避免页面因为无效分组打挂
- 当前剩余缺口是 upstream memory browser 的 keyword manager 仍依赖 `/browse/glossary` POST / DELETE；compat adapter 只实现了 GET，因此不能宣称上游 web 已全量可写

## 稳定偏好主动写入

当 `Feature::Zmemory` 开启时，`codex-core` 会对高确定性的长期称呼偏好执行受控主动写入；这不是 `codex-zmemory` crate 自己的自治行为。

- 目标节点：
  - `core://my_user`：用户称呼偏好
  - `core://agent`：助手自称偏好
  - `core://agent/my_user`：双方协作称呼约定
- 写入前：
  - 先看 `system://workspace`，确认当前活动数据库
  - 再读目标 canonical URI，避免重复创建
- 写入规则：
  - 目标不存在时使用 `create`
  - 目标已存在且是同一主题修订时使用 `update`
- 写入后：
  - 再次 `read` 对应 canonical URI，确认内容已落到当前活动库
- 失败表现：
  - 上层会发出可观察 warning，不会静默伪成功

当前首批只覆盖“明确、稳定、低歧义”的命名/称呼偏好，不应把一次性临时指令或短期上下文误当长期记忆。

## 更多参考

- `docs/config.md`
- `codex-rs/zmemory/README.md`
- `.agents/embedded-zmemory-overhaul/architecture.md`
- `.agents/embedded-zmemory-overhaul/qa-report.md`
