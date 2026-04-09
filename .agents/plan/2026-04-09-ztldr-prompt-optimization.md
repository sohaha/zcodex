# 收敛 ztldr 工具描述与提示词优化

## 背景
- 当前状态：`ztldr` 已完成从 `tldr` 到 `ztldr` 的命名统一，但当前对模型可见的工具描述、参数说明、shell 搜索拦截提示与 agent-first 文档仍存在“能力口径不一致、路由指令偏弱、失败/降级语义未前置到决策提示层”的问题。已完成本地审查，确认关键触点集中在 `codex-rs/mcp-server/src/tldr_tool.rs`、`codex-rs/tools/src/tool_spec.rs`、`codex-rs/core/src/tools/rewrite/shell_search_rewrite.rs`、`docs/tldr-agent-first-guidance/tool-description.md` 与 `codex-rs/docs/codex_mcp_interface.md`。
- 触发原因：用户要求深度审查并优化当前 `ztldr` 的工具描述或提示词，并要求参考本仓库 `ztok` 的路由/提示风格以及 `Continuous-Claude-v3` 的 agent/workflow 注入思路。
- 预期影响：提升模型在结构化代码问题上优先调用 `ztldr` 的概率，减少无效 broad grep/read 起手；同时让降级与失败结果在模型侧更容易被正确表述，而不是被误当成正常成功。

## 目标
- 目标结果：形成并落地一组一致的 `ztldr` prompt/description 优化，使运行时、文档与 MCP/工具 schema 对“何时先用 `ztldr`、何时不要优先用、失败/降级如何解释”给出统一口径。
- 完成定义（DoD）：
  - `ztldr` 对模型暴露的主 description 从“能力介绍”升级为“决策引导型描述”。
  - 参数说明与动作列表和真实 `TldrToolAction` 能力保持一致，不再出现明显缺项或别名混杂。
  - shell 搜索拦截提示能明确说明拦截原因、推荐 action、raw escape hatch 与降级/失败解释边界。
  - agent-first 文档与 MCP 接口文档完成同步，分别覆盖“路由决策”与“接口 contract”。
  - 改动附带针对相关文案/行为的测试或现有测试更新，并完成最小必要验证。
- 非目标：
  - 无意在本轮扩展 `native-tldr` 新能力或新增 `TldrToolAction`。
  - 无意重构整套 auto-tldr classification 架构。
  - 无意引入子代理/并行执行机制。

## 范围
- 范围内：
  - `ztldr` MCP/tool spec 描述文案。
  - `shell_search_rewrite` 的拦截提示文案与必要测试。
  - `docs/tldr-agent-first-guidance/tool-description.md` 与 `codex-rs/docs/codex_mcp_interface.md` 的同步更新。
  - 视需要补充 `read_gate` / `auto_tldr` 相邻说明，前提是不扩大为行为重构。
- 范围外：
  - `native-tldr` 引擎实现、索引、daemon 生命周期语义本身。
  - `ztok` 行为或 allowlist 逻辑修改。
  - 非 `ztldr` 相关工具的大范围提示词重写。

## 影响
- 受影响模块：
  - `codex-rs/mcp-server`
  - `codex-rs/tools`
  - `codex-rs/core/src/tools/rewrite`
  - `docs/`
  - `codex-rs/docs/`
- 受影响接口/命令：
  - MCP `ztldr` tool 描述
  - Responses API / tool spec 中的 `ztldr`
  - broad shell search 被拦截时返回给模型的提示消息
- 受影响数据/模式：
  - 无持久化数据结构变更；仅涉及 prompt/description/文档与相邻测试预期
- 受影响用户界面/行为：
  - 模型工具选择路径可能变化
  - 结构化代码问题下更早命中 `ztldr`
  - 遇到 `degradedMode` / `structuredFailure` 时的模型表述更一致

## 约束与依赖
- 约束（时间/兼容性/安全/性能/发布窗口等）：
  - 必须保持真实能力口径，不得在 description 中虚构未实现 action 或语义。
  - 不做未被请求的架构扩张；优先最小闭环优化。
  - 文案更新需区分“agent-first 路由提示”和“接口 contract 文档”，避免把两者混为一谈。
  - 当前会话未授权子代理，Cadence 本轮默认由主代理本地完成。
- 外部依赖（系统/人员/数据/权限等）：
  - 外部参考依赖 `Continuous-Claude-v3` 公开仓库说明；若后续需要更细粒度网页内容且本地无缓存，可能受网络权限影响。

## 实施策略
- 总体方案：
  - 先统一事实源：核对真实 `TldrToolAction`、当前 tool description、拦截提示与文档现状。
  - 再按“最强路由触点优先”原则收敛三层文案：`tool description`、`rewrite interception prompt`、`agent/docs guidance`。
  - 最后用现有测试或新增最小测试锁定文案与行为边界，避免回归到“静态说明书式”描述。
- 关键决策：
  - 优先把 `ztldr` 描述写成“什么时候先用/什么时候不用/失败如何解释”的决策型文案，而不是继续堆砌 action 列表。
  - 借鉴 `ztok` 的边界表达方式，用可诊断、可回退的提示替代抽象口号。
  - 借鉴 `Continuous-Claude-v3` 的 workflow 注入思路，但仅落在现有触点的提示增强，不在本轮引入新的 hook/framework。
- 明确不采用的方案（如有）：
  - 不在本轮通过新增复杂分类器或 planner/subagent 注入来解决问题。
  - 不把 `codex_mcp_interface` 写成 agent prompt 说明书；它仍主要承担接口参考角色。

## 阶段拆分
### 阶段一：收敛事实与目标文案
- 目标：确认真实 action/语言/降级 contract，并形成统一的文案优化原则。
- 交付物：修改清单、统一口径、目标文案草案。
- 完成条件：明确哪些文件需要更新、每处更新承担什么职责。
- 依赖：已完成的本地审查结果与外部参考结论。

### 阶段二：落地运行时提示与描述
- 目标：更新最直接影响模型路由决策的描述与拦截提示。
- 交付物：`mcp-server`、`tools`、`core/src/tools/rewrite` 相关改动与测试更新。
- 完成条件：代码侧描述口径一致，测试覆盖关键文案或路由边界。
- 依赖：阶段一统一口径。

### 阶段三：同步文档与自审
- 目标：让 agent-first 文档和 MCP 接口文档与代码侧口径一致。
- 交付物：文档更新、自审结论、Cadence issue 生成输入。
- 完成条件：文档完成同步且无明显口径冲突。
- 依赖：阶段二已稳定。

## 测试与验证
- 核心验证：
  - 运行受影响 crate 或模块的定向测试，至少覆盖 `shell_search_rewrite` / `tldr_tool` / `tool_spec` 相邻测试。
- 必过检查：
  - 相关 Rust 测试通过。
  - 文档与代码中的 action/能力口径人工核对一致。
- 回归验证：
  - 确认 broad shell search 仍只在符合条件时拦截，regex/raw grep 不被误导改写。
  - 确认 `degradedMode` / `structuredFailure` 相关说明未从接口文档中丢失。
- 手动检查：
  - 人工复读更新后的描述，确认能直接回答“何时先用 ztldr / 何时不要 / 失败怎么说”。
- 未执行的验证（如有）：
  - 若需要更大范围全量测试，再根据改动落点决定是否升级验证范围。

## 风险与缓解
- 关键风险：
  - 文案优化后仍与真实 action/支持语言不一致。
  - 为了增强路由而把 raw grep/read 的适用边界写得过窄，导致误导模型。
  - 文档层和运行时层继续出现双重口径。
- 触发信号：
  - `tool_spec` / MCP 文案只列出部分 action，而真实 `TldrToolAction` 另有未覆盖项。
  - 拦截提示没有提到 raw escape hatch 或降级/失败语义。
  - 文档中出现和测试/实现不一致的能力表述。
- 缓解措施：
  - 所有文案修改前先以 `native-tldr` 的 `TldrToolAction` 为事实源。
  - 在提示中显式保留 regex、逐字核对、用户明确 raw 请求的直通边界。
  - 将代码、文档、测试作为一个变更单元同步更新。
- 回滚/恢复方案（如需要）：
  - 若新文案引发测试或行为争议，先回退到保守但一致的 action 描述，再迭代增强决策性提示。

## 参考
- `codex-rs/mcp-server/src/tldr_tool.rs:47`
- `codex-rs/tools/src/tool_spec.rs:77`
- `codex-rs/core/src/tools/rewrite/shell_search_rewrite.rs:14`
- `codex-rs/core/src/tools/rewrite/read_gate.rs:26`
- `codex-rs/core/src/tools/rewrite/auto_tldr.rs:26`
- `codex-rs/native-tldr/src/tool_api.rs:36`
- `codex-rs/ztok/src/rewrite.rs:58`
- `docs/tldr-agent-first-guidance/tool-description.md:1`
- `codex-rs/docs/codex_mcp_interface.md:151`
- `https://github.com/parcadei/Continuous-Claude-v3`
- `https://skills.sh/parcadei/continuous-claude-v3/repoprompt`
