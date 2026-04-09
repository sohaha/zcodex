# 一次性收敛 ztldr 内嵌路由 contract

## 背景
- 当前状态：上一轮已完成 `ztldr` 的代码侧描述、shell 搜索拦截提示和文档同步，解决了“文案过弱与动作口径不一致”的第一层问题；但 `read_gate`、`auto_tldr`、`shell_search_rewrite` 仍是分散决策与分散提示，存在后续继续漂移风险。
- 触发原因：用户明确要求“避免内嵌大量提示词，最好一次性解决；我们是内嵌 tools，可以做很多事情”。这意味着目标从“补文案”升级为“统一内嵌路由机制”。
- 预期影响：把 `ztldr` 路由从多处硬编码提示词收敛为可复用的内核 contract，减少重复文案、降低维护成本，并提升结构化问题的稳定命中率与可测试性。

## 目标
- 目标结果：在 `codex-rs/core/src/tools/rewrite` 内引入统一的 `ztldr` 路由决策模型（分类 + 决策 + 短解释），并让 `read_gate`、`auto_tldr`、`shell_search_rewrite` 接入同一 contract。
- 完成定义（DoD）：
  - 新增共享路由模块，明确问题分类、路由意图、passthrough 原因与失败语义表达位点。
  - `read_gate`、`auto_tldr`、`shell_search_rewrite` 至少在“是否改写 + 改写到哪个 action + 为什么保留 raw”上共享同一套决策结构。
  - 移除或显著收敛分散在各入口的长文本提示，改为统一短解释模板输出。
  - 保持现有 raw escape hatch 行为不回退：`explicit raw`、`factual`、`regex/exact text`、`non-code` 等路径仍能稳定 passthrough。
  - 关键行为有定向测试覆盖，且现有相关测试不过度碎裂。
- 非目标：
  - 不扩展 `native-tldr` 的 action 或引擎能力。
  - 不在本轮引入 planner/subagent 注入体系重构。
  - 不做跨仓或上游参考代码搬运。

## 范围
- 范围内：
  - `codex-rs/core/src/tools/rewrite/` 下 `read_gate.rs`、`auto_tldr.rs`、`shell_search_rewrite.rs` 以及新增共享路由模块。
  - 与上述改动直接相关的测试更新。
  - 必要时对 `docs/tldr-agent-first-guidance/tool-description.md` 做最小同步（仅在行为边界有用户可见变化时）。
- 范围外：
  - `codex-rs/native-tldr` 的能力扩展。
  - MCP 接口字段变更或 schema 结构改造。
  - 无关工具的路由策略调整。

## 影响
- 受影响模块：
  - `codex-rs/core/src/tools/rewrite`
  - `codex-rs/core` 相关测试
  - （可选）`docs/tldr-agent-first-guidance`
- 受影响接口/命令：
  - `read_file`、`grep_files`、shell 搜索拦截的内部改写决策
  - `ztldr` 路由解释输出（模型可见文本）
- 受影响数据/模式：
  - 无持久化 schema 变更；新增内部路由 decision 数据结构
- 受影响用户界面/行为：
  - 结构化问题下工具选择更稳定
  - raw 场景边界更清晰
  - 提示文案更短、更一致

## 约束与依赖
- 约束（时间/兼容性/安全/性能/发布窗口等）：
  - 兼容当前 `ToolRewriteDecision` 主流程，不破坏现有调用链。
  - 不用长提示词堆叠替代路由机制设计；文案只保留最短必要解释。
  - 维持既有回退边界与用户显式 raw 请求优先级。
  - 默认不派发子代理（当前 Cadence 未新增并行授权）。
- 外部依赖（系统/人员/数据/权限等）：
  - `functions.ztldr` 可用于结构化检索和触点核对。
  - 不依赖外网即可完成主体改造；外部参考仅用于对照，不作为实现前置。

## 实施策略
- 总体方案：
  - 先抽象共享路由 contract（分类、意图、理由、passthrough 原因）。
  - 再逐个接入三入口：`auto_tldr` → `shell_search_rewrite` → `read_gate`。
  - 最后统一短解释模板与测试断言，避免每处硬编码长文案。
- 关键决策：
  - 使用结构化 decision 而不是继续在各入口扩写自然语言提示。
  - 保持行为优先、文案从属：先确保同一输入得到同一决策，再收敛提示文本。
  - 入口测试优先断言决策结构（reason/intent/passthrough），文案只做必要 contains 验证。
- 明确不采用的方案（如有）：
  - 不继续“入口各写一段大 prompt”的 patch 模式。
  - 不把 MCP 接口文档当运行时路由逻辑承载层。

## 阶段拆分
### 阶段一：抽象统一路由 contract
- 目标：在 `rewrite` 目录新增共享路由 decision 模块。
- 交付物：新模块与单元测试，覆盖分类与决策规则。
- 完成条件：三入口可调用统一 decision API，且保留原有可观测 reason 语义。
- 依赖：现有三入口行为审查结论。

### 阶段二：接入三入口并收敛提示输出
- 目标：让 `auto_tldr`、`shell_search_rewrite`、`read_gate` 使用统一 contract 与短解释模板。
- 交付物：三入口改动与相邻测试更新。
- 完成条件：行为一致、raw escape hatch 稳定、解释模板不再分散长文案。
- 依赖：阶段一 contract 稳定。

### 阶段三：验证与最小文档同步
- 目标：完成定向测试与必要文档同步，确保行为和指南一致。
- 交付物：测试通过记录、最小文档修订（如需）。
- 完成条件：核心测试通过，且无新增口径冲突。
- 依赖：阶段二完成。

## 测试与验证
- 核心验证：
  - `cd /workspace/codex-rs && cargo test -p codex-core shell_search_rewrite`
  - `cd /workspace/codex-rs && cargo test -p codex-core read_gate`
  - `cd /workspace/codex-rs && cargo test -p codex-core auto_tldr`
- 必过检查：
  - `just fmt`
  - 上述定向测试全部通过
- 回归验证：
  - 确认 `factual`、`force_raw_read`、`disable_auto_tldr_once`、regex/exact-text 场景仍 passthrough
  - 确认结构化场景仍正确改写到 `context`/`semantic`/`extract`
- 手动检查：
  - 抽样 3 类问题回放（结构化、混合、逐字核对）验证路由与解释是否一致
- 未执行的验证（如有）：
  - 若需更大范围验证，再根据触及范围决定是否补跑更广的 `codex-core` 相关测试集。

## 风险与缓解
- 关键风险：
  - 统一 contract 过程中误改现有 raw passthrough 边界。
  - 文案收敛时破坏已有测试稳定性或引入过度脆弱断言。
  - 三入口接入顺序不当，导致中间态行为不一致。
- 触发信号：
  - `factual` 或 `explicit raw` 场景被错误改写。
  - 同一类输入在不同入口得到不同 action/理由。
  - 测试大量依赖整句文案导致小改动大面积失败。
- 缓解措施：
  - 先收敛决策结构，再收敛文案。
  - 先改一入口并稳定测试，再逐步接入其它入口。
  - 测试优先断言结构化 reason/intent，文案断言改为关键片段。
- 回滚/恢复方案（如需要）：
  - 保留每阶段最小可回退提交粒度；若某入口接入导致回归，回滚该入口接入而保留共享 contract 基础设施。

## 参考
- `codex-rs/core/src/tools/rewrite/read_gate.rs:26`
- `codex-rs/core/src/tools/rewrite/auto_tldr.rs:26`
- `codex-rs/core/src/tools/rewrite/shell_search_rewrite.rs:14`
- `codex-rs/tools/src/tool_spec.rs:77`
- `codex-rs/mcp-server/src/tldr_tool.rs:47`
- `docs/tldr-agent-first-guidance/tool-description.md:12`
- `.agents/llmdoc/guides/ztldr-prompt-optimization.md`
