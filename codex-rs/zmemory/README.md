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

底层仍复用 `read system://...`：

- `export boot` -> `system://boot`
- `export index --domain core` -> `system://index/core`
- `export recent --limit 5` -> `system://recent/5`
- `export glossary` -> `system://glossary`

## 创建语义

`zmemory` 支持两种路径创建模式：

- **完整 URI 写入（现行模式）**：直接传 `core://team/alice` 等完整路径，`create` 会在此位置插入新记忆，仅需 `--content`、可选 `--priority`/`--disclosure`。这条路径等价于早期版本，保持向后兼容。
- **父路径 + 标题（上游兼容模式）**：也可以传 `--parent-uri core://team` 和 `--title alice-profile`，工具会在父路径下构建新节点，`title` 会用于生成那一段路径（空 title 将自动编号，非法字符会报错）。此模式对齐 upstream `nocturne_memory` 的 `parent_uri + title` 合同，也允许显式调整 `priority` 与 `disclosure`。

两个模式可以并存：仍可传 `URI` 路径以避免改动，也可以借用 `parent-uri/title` 接口来复用上游的更新逻辑。CLI 中的 `create` 命令接受 `--parent-uri` 和 `--title` 可选参数，使用时推荐先阅读 `.agents/zmemory/tasks.md` 中的具体任务拆解。
