---
name: memory
description: 在当前仓库中编排 zmemory 的 recall、capture、refine、linking、review 与 handoff。适用于需要决定何时调用 `read`、`search`、`create`、`update`、`add-alias`、`manage-triggers`、`stats`、`doctor`、`export` 的场景。
---

# Memory Skill

这个 skill 只做当前仓库所需的最小编排，不整包复制 upstream memory skill。

## 触发场景

- 新会话恢复项目上下文
- 回答前怀疑已有长期记忆
- 对话中出现稳定的新结论
- 需要修订旧记忆而不是新增重复节点
- 需要整理 trigger / alias / recent / glossary / doctor 信号
- 会话收尾需要 handoff

## 动作映射

### bootstrap

1. 先读 `system://boot`
2. 若已知项目主节点，再读该节点
3. 只简短总结加载了什么，不复读全文

### recall

- 已知 URI：优先 `read`
- 不知 URI：优先 `search`
- 如果 query 含多个别名/分隔符变体，直接用原始 query 即可；当前 search 会做 separator-normalization 与 alias 去重。

### capture

- 新稳定信息：优先 `create`
- 创建后尽快补 `manage-triggers`

### refine

- 已有节点修订：优先 `update`
- 能 patch / append 就不要重复 create

### linking

- 跨语境入口：用 `add-alias`
- 提高召回：用 `manage-triggers`

### review

按这个顺序：

1. `stats` 看 `orphanedMemoryCount` / `deprecatedMemoryCount`
2. `doctor` 看结构与 review 告警
3. `export recent` 看最近变化
4. `export glossary` 看 trigger 覆盖
5. 同时检查 disclosure 治理：`pathsMissingDisclosure` / `disclosuresNeedingReview`
6. 若要确认“当前到底有哪些活跃路径”，用 `export paths [--domain <domain>]`
7. `export alias [--limit N]` 看 alias coverage、trigger count、缺 trigger 列表、治理优先级、优先级原因、建议关键词和推荐命令
8. 若 search 行为异常，再检查 query 是否命中了 domain 校验、token boundary 或 snippet 回退语义
9. 再决定是否 `update`、`delete-path`、`add-alias`、`manage-triggers`

### handoff

- `zmemory` 不新增 handoff API
- 项目级 handoff 继续走 `.ai/memory/handoff.md` 与现有 vbm 流程

## 边界

- 不扩到 daemon / REST / 独立 admin 服务
- 不替换项目现有 handoff 机制
- 不把 upstream `.codex/skills/memory/` 整包复制进来

## 引用

- `[references/cli-recipes.md](references/cli-recipes.md)`：与当前 `codex zmemory` CLI 完全一致的命令示例，按 recall/capture/refine/linking/review 组织。
- `[references/review-playbook.md](references/review-playbook.md)`：当前 review 顺序与清单，直接调用 `stats`/`doctor`/`export` 等动作。
