# 文档路由

## 启动顺序
- 先读本目录 `startup.md`。
- 并行工作前，先读 `.ai/project/overview.md`、`.ai/project/config-map.md`、`.ai/memory/handoff.md`、`.ai/memory/known-risks.md`。
- 修改实现前，优先召回 `.ai/memory/bugs/`、`.ai/memory/decisions/` 和 `.ai/project/business-rules.md` 里的相关记录。

## 何时读哪里
- 需要仓库定位或职责边界：读 `overview/` 与 `architecture/`。
- 需要执行步骤和验证顺序：读 `guides/`。
- 需要稳定命令、schema、配置事实：读 `reference/`。
- 同一子系统反复返工或此前失败过：补读 `memory/reflections/`。

## 调查草稿边界
- llmdoc 的临时调查草稿放在 `/tmp/llmdoc/workspace-8af22c44f404/investigations/`。
- 草稿只服务当前初始化/更新，不直接当稳定知识引用。
- 稳定结论应回写到 `.agents/llmdoc/`；项目级工作记忆继续写入 `.ai/`。
