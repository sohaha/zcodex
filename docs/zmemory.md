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

`zmemory` 还会读取这些环境变量：

- `VALID_DOMAINS`：可写域列表，默认 `core`
- `CORE_MEMORY_URIS`：boot 锚点 URI，默认 `core://agent,core://my_user,core://agent/my_user`

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

## 系统视图

常用系统视图：

- `system://boot`
- `system://defaults`
- `system://workspace`
- `system://index`
- `system://recent`
- `system://glossary`
- `system://alias`

其中最重要的是：

- `system://workspace`：当前会话实际使用的库、路径来源、boot 健康度、是否显式覆盖默认路径
- `system://defaults`：产品默认值，不混入当前项目的实际覆盖状态

建议排查顺序：

1. 先看 `system://workspace`
2. 再看 `system://defaults`
3. 再看 `stats` / `doctor`
4. 最后看 `system://alias`、`recent`、`glossary`

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

## 设计边界

- `zmemory` 使用独立 SQLite，不写入 `codex-state` 的 state DB
- `zmemory` 不替换原生 `core/memories` 启动摘要流程
- CLI 与 core tool 共用同一套 `tool_api / service / schema`

## 更多参考

- `docs/config.md`
- `codex-rs/zmemory/README.md`
- `.agents/embedded-zmemory-overhaul/architecture.md`
- `.agents/embedded-zmemory-overhaul/qa-report.md`
