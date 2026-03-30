---
type: tech-review
outputFor: [scrum-master, frontend, backend]
dependencies: [prd, architecture]
---

# 技术方案评审报告

## 1. 评审概述

- **项目名称**：tldr-agent-first-optimization
- **评审日期**：2026-03-30
- **评审人**：Tech Lead Agent
- **评审文档**：
  - PRD：`.agents/tldr-agent-first-optimization/prd.md`
  - 架构：`.agents/tldr-agent-first-optimization/architecture.md`
  - UI 规范：跳过（CLI / runtime routing 优化，无独立 UI）

## 摘要

> 下游 Agent 请优先阅读本节，需要细节时再查阅完整文档。

- **评审结论**：✅ 通过（按最终架构分阶段实施）
- **主要风险**：shell 级命令重写误判、`read_file` 前置策略体验不佳、分类模型与真实问题分布偏差。
- **必须解决**：统一问题分类模型；明确 raw/regex 优先级；保持结构分析与事实核对边界；所有阶段都朝同一终局架构收敛。
- **建议优化**：尽早把 shell 搜索识别与 planner 注入纳入设计，不要让 `grep_files` rewrite 成为长期孤岛。
- **技术债务**：如果只停在 MVP，会形成“文档说内嵌优先、实际只有局部改写”的认知裂缝。

---

## 2. 评审结论

| 维度 | 评分 | 说明 |
|------|------|------|
| 架构合理性 | ⭐⭐⭐⭐⭐ | 以内嵌重写层为核心，方向优于外接 hook 方案 |
| 技术选型 | ⭐⭐⭐⭐⭐ | 完全复用现有 core/native-tldr 能力，演进成本可控 |
| 可扩展性 | ⭐⭐⭐⭐⭐ | 从 `grep_files` 可自然扩展到 `read_file`、shell、planning |
| 可维护性 | ⭐⭐⭐⭐☆ | 需尽快抽统一分类模型，避免各入口各写一套 heuristic |
| 安全性 | ⭐⭐⭐⭐☆ | raw/regex 逃生路径必须先于一切自动重写 |

**总体评价**：最终版方案正确，且符合“我们是内嵌 `tldr`”这一事实。重点不在于再模仿外接 hook，而在于把 `tldr` 升级为系统级默认底座。允许阶段化落地，但禁止引入未来必须推倒重写的中间架构。

## 3. 技术风险评估

| 风险 | 等级 | 影响范围 | 缓解措施 |
|------|------|----------|----------|
| shell 搜索识别误把普通文本检索重写成 `tldr` | 中 | shell / router / UX | 先用统一分类模型判断问题类型，再决定 soft warning 或 direct rewrite |
| `read_file` 前置 `tldr` 影响逐字阅读体验 | 中 | read path / 用户预期 | 保留显式 raw read 与“按需展开原文”的二段式策略 |
| `grep_files`、`read_file`、planning 使用不同 heuristics | 高 | 可维护性 / 准确率 | 抽独立 classification 层供各入口共享 |
| 文档先行但运行时落地慢 | 中 | 产品认知 | 任务规划中明确 P1-P4 对应最终架构位置 |
| warm 机制引入额外开销 | 低 | 启动延迟 / 资源 | 先采用惰性 warm，再按配置升级 |

## 4. 技术可行性分析

### 4.1 核心功能可行性

| 功能 | 可行性 | 复杂度 | 说明 |
|------|--------|--------|------|
| 问题分类模型统一化 | 高 | 中 | 现有 directives 可作为起点，后续抽离为 classification 层 |
| `grep_files -> tldr` 强化 | 高 | 低 | 已有链路成熟 |
| `read_file` 前置 `tldr` | 高 | 中 | 可在 read path 前插入结构摘要层 |
| shell 搜索重写 | 中高 | 中高 | 需做命令形态识别，但内嵌 runtime 比外接 hook 更可控 |
| planner/subagent 注入 | 高 | 中 | 依赖现有 turn/session context 和 subagent 框架 |
| warm / preload | 高 | 中 | native-tldr 已有 daemon/warm 能力 |

### 4.2 技术难点

| 难点 | 解决方案 | 预估工时 |
|------|----------|----------|
| 抽象统一分类模型而不破坏现有行为 | 先兼容现有 directives，再逐步提升为共享 classification 层 | 1-2 天 |
| shell 命令形态识别的边界 | 先覆盖最常见 broad search 形态，保留 raw escape | 1-2 天 |
| `read_file` 如何既提效又不妨碍精确核对 | 采用“先结构摘要、再按需原文”的双阶段设计 | 1 天 |
| 降级与失败状态如何进入 agent 决策 | 统一利用 `degradedMode` / `structuredFailure` contract | 0.5-1 天 |

## 5. 架构改进建议

### 5.1 必须修改（阻塞项）

- [ ] 在文档和运行时统一三类问题模型：结构化 / 事实核对 / 混合。
- [ ] 明确 `raw grep/read/regex` 的优先级高于自动重写。
- [ ] 设计 `read_file` 的最终策略，而不是长期把它放在范围外。
- [ ] 设计 shell 搜索命令的原生拦截/重写层。
- [ ] 让所有分阶段任务都能映射回最终架构，而不是只优化一个局部函数。

### 5.2 建议优化（非阻塞）

- [ ] 增加 planner/subagent 的 `tldr-first` 上下文注入。
- [ ] 为 `AutoTldrContext` 增加 `last_action` / `last_problem_kind` / `last_degraded_mode`。
- [ ] 增加 session warm 策略与对应开关。
- [ ] 增加更明确的 router logging，解释为什么改写/没改写。

## 6. 实施建议

### 6.1 开发顺序建议

```mermaid
graph LR
    A[统一文档与 tool description] --> B[实现共享分类与 directives]
    B --> C[完成 grep_files 最终行为]
    C --> D[扩展 read_file 策略]
    D --> E[扩展 shell 搜索重写]
    E --> F[planner/subagent 注入与 warm]
```

### 6.2 里程碑建议

| 里程碑 | 内容 | 建议工时 | 风险等级 |
|--------|------|----------|----------|
| M1 | 最终版 PRD / 架构 / 评审 / 任务文档 | 0.5 天 | 低 |
| M2 | prompt + directives + `grep_files` 共享分类落地 | 1 天 | 中 |
| M3 | `read_file` 前置 `tldr` 与测试 | 1 天 | 中 |
| M4 | shell 搜索重写与可观测性 | 1-2 天 | 高 |
| M5 | planner/subagent 注入与 warm | 1 天 | 中 |

### 6.3 技术债务预警

| 潜在债务 | 产生原因 | 建议处理时机 |
|----------|----------|--------------|
| `grep_files` 成为唯一命中 `tldr` 的入口 | 只做局部 MVP | M3 前必须解决 |
| 文档强调内嵌优势但运行时未体现 | 实施滞后 | M2 前必须收敛 |
| 各入口各自维护 heuristics | 未抽分类层 | M2 阶段解决 |

## 7. 代码规范建议

### 7.1 目录结构规范

```
core/src/tools/spec.rs
core/src/tools/rewrite/directives.rs
core/src/tools/rewrite/auto_tldr.rs
core/src/tools/rewrite/classification.rs
core/src/tools/rewrite/read_gate.rs
core/src/tools/rewrite/shell_search_rewrite.rs
docs/tldr-agent-first-guidance/tool-description.md
```

### 7.2 命名规范

- **问题类型**：使用 `structural` / `factual` / `mixed` 这类明确枚举名
- **布尔意图**：保留 `force_tldr`、`force_raw_grep`、`force_raw_read`
- **测试命名**：直接描述行为差异与优先级

### 7.3 代码风格

- 统一用共享分类层，避免重复字符串 heuristics
- 重写逻辑尽量用 guard clause
- 不引入隐藏 fallback，所有回退必须可观测

## 8. 评审结论

- **是否通过**：通过（按最终架构实施）
- **阻塞问题数**：5 个
- **建议优化数**：4 个
- **下一步行动**：先完成最终版文档冻结，再按 M2 起实现共享分类与 `grep_files` 最终行为
