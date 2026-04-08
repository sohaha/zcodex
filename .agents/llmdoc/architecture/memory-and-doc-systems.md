# 记忆与文档系统

## 用途
- 说明当前仓库里几套“知识存储”系统分别解决什么问题，避免混用。

## 系统分工
- `.ai/`：项目级工作记忆。保存 handoff、已知风险、业务规则、项目概览等文本知识。
- `.agents/llmdoc/`：稳定知识地图。按 `must/`、`overview/`、`architecture/`、`guides/`、`reference/` 组织项目理解。
- `/tmp/llmdoc/workspace-8af22c44f404/investigations/`：llmdoc 初始化/更新期间的临时调查草稿。
- `zmemory`：Codex 内置的 SQLite 持久长期记忆工具，独立于 `.ai/` 文本记忆。
- `.agents/plan`、`.agents/issues`、专题目录：任务规划、issue 追踪和专项产物，不等价于稳定文档索引。

## `.ai` 与 `zmemory` 的区别
- `.ai` 偏向仓库内可审阅文本记忆，围绕当前项目协作。
- `zmemory` 偏向工具可读写的 durable memory，支持 `system://workspace`、`search`、`create`、`update`、`doctor` 等动作。
- `zmemory` 默认把数据库放在 `$CODEX_HOME/zmemory/projects/<project-key>/zmemory.db`，与当前项目稳定绑定；显式配置 `[zmemory].path` 时才改为指定路径。

## 当前高风险边界
- project-scoped config 与 turn cwd override 的耦合仍是已知风险，尤其影响 `zmemory`、子线程和直接读取 `turn.config.*` 的链路。
- 处理配置/路径问题时，优先检查 `system://workspace` 和 `system://defaults`，不要只看静态默认值。
- 当前仓库还保留大量 `.agents/*` 专题文档；它们可作为证据，但不能替代 `llmdoc` 的稳定路由职责。

## 写回规则
- 项目事实、稳定架构和工作流放进 `.agents/llmdoc/`。
- 项目协作记忆和交接摘要继续落到 `.ai/`。
- 工具侧 durable preference / searchable memory 放进 `zmemory`。
- 调查过程和不稳定草稿只放临时目录，不污染稳定文档。

## 相关文档
- `.agents/llmdoc/must/doc-routing.md`
- `.agents/llmdoc/reference/build-and-test-commands.md`
- `docs/zmemory.md`
