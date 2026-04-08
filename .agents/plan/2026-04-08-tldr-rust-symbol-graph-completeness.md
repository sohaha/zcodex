# tldr 多语言符号图完整性完善

> 不适用项写 `无`，不要留空。
> 只写已确认事实；若信息不清楚或需要假设，先回到对话澄清，不要把未确认内容写进计划。

## 背景
- 当前状态：`tldr context` 查询 `TldrHandler` 时，只能命中 `pub struct TldrHandler`，没有把 `impl ToolHandler for TldrHandler`、`handle()` 等关联方法纳入结果；查询 `run_tldr_handler_with_hooks` 时，函数级上下游又能正常返回。进一步核查确认：当前只有 Rust 走 `rust_analysis.rs` 的专用提取器，其他已支持语言仍主要走 `semantic.rs` 中的 `extract_units_fallback()` 启发式提取，尚未确认是否具备与 Rust 同等级的类型/实现关系表达能力。
- 触发原因：用户要求用 Cadence 彻底完善该问题，不接受只对单点现象打补丁。
- 预期影响：需要从 `codex-native-tldr` 的符号提取、图关系建模、查询聚合与对外展示链路整体修正，先解决 Rust 已确认缺陷，同时对其他已支持语言做一致性审计，并对可确认缺陷一起修复或显式标注能力边界。

## 目标
- 目标结果：让 `tldr` 在已支持语言的代码分析中，对“类型/实现/关联方法”这类关系具备一致、可解释的能力边界；其中 Rust 需完成真实关系建模与查询修复，其他语言需完成一致性审计，并对已确认缺陷实施修复或明确标注暂不支持的边界。
- 完成定义（DoD）：
  - 按类型名查询时，能看到该类型关联的方法或 impl 信息，而不是只返回孤立的 struct 节点。
  - `context` / 结构化分析结果包含可追踪的类型归属关系，而不仅有 `calls` / `imports` / `references`。
  - 现有函数级调用图能力保持可用，不因本次重构退化。
  - Rust 相关新增或调整的行为有自动化测试覆盖；其他已支持语言至少有一致性审计与边界测试/说明，不再保持“看似支持但语义不明”的状态。
  - 若对外输出结构、支持说明或文档受影响，同步更新文档。
- 非目标：
  - 无根据地把所有语言都提升到与 Rust 完全同深度的 AST/语义建模。
  - 改变 `tldr` 的 CLI/MCP 基本入口语义。
  - 为掩盖问题增加隐式 fallback 或仅改 summary 文案而不修正底层模型。

## 范围
- 范围内：
  - `codex-rs/native-tldr/src/rust_analysis.rs`
  - `codex-rs/native-tldr/src/analysis.rs`
  - `codex-rs/native-tldr/src/semantic.rs`
  - `codex-rs/native-tldr/src/lang_support/`
  - 与多语言符号图输出直接相关的 API/测试/文档
- 范围外：
  - 与本问题无关的 daemon 生命周期、semantic cache、CLI auto-start 机制
  - 与“类型/实现/关联方法”一致性无关的 daemon 生命周期、semantic cache 细节优化

## 影响
- 受影响模块：`codex-native-tldr` 为主；如结构化输出字段、支持说明或展示文本变化，可能波及 `codex-mcp-server`、`codex-core`、相关文档与测试。
- 受影响接口/命令：`tldr context` 以及依赖相同结构化分析明细的调用链路；必要时包含 `structure` 等共享明细构建路径。
- 受影响数据/模式：`EmbeddingUnit` 及其派生的 analysis detail / graph edges / symbol query 行为，尤其是多语言下的符号关系表达与支持边界。
- 受影响用户界面/行为：CLI / MCP / Core 中看到的符号分析结果会更完整，且不同语言的能力边界更明确，不再默认把 fallback 启发式结果包装成与 Rust 同语义等级。

## 约束与依赖
- 约束（时间/兼容性/安全/性能/发布窗口等）：
  - 需要遵守仓库 Rust 规范、格式化与测试要求。
  - 不能以硬编码特例修复 `TldrHandler` 个案；必须抽象成通用 Rust 方案，并对其他语言的支持边界给出一致结论。
  - 当前已确认只有 Rust 有专用提取器，其他语言主要依赖 fallback；本轮不得把未经验证的语言能力写成“已支持完整关系建模”。
  - 若对外结构化输出或支持等级说明发生变化，需要兼顾现有测试契约并明确更新文档/快照。
- 外部依赖（系统/人员/数据/权限等）：无。

## 实施策略
- 总体方案：先审计已支持语言当前走专用提取器还是 fallback 启发式，并定义统一的能力边界；在此基础上重构 Rust 符号抽取模型，必要时抽出可复用的跨语言关系表达，再更新 analysis 层的查询聚合与图边构建，最后补齐 CLI/MCP/Core 侧的回归验证与必要文档。
- 关键决策：
  - 不以补充更多 `symbol_aliases` 作为唯一修复手段。
  - 优先在 `native-tldr` 内建立稳定的语义关系源，再由上层消费，而不是在 Core/MCP 侧做二次猜测。
  - 对类型名查询采用“聚合关联单元”的策略，但保持精确查询能力不退化。
  - 对其他语言若暂无法可靠表达同类关系，要显式输出支持边界或降低承诺，而不是继续隐式复用模糊 fallback 结果。
- 明确不采用的方案（如有）：
  - 仅在 `symbol_matches()` 中追加字符串前缀/后缀匹配来碰运气命中方法。
  - 仅修改 `summary` 文本或 UI 展示而不修正底层索引与图结构。

## 阶段拆分

### 阶段一：完成多语言能力审计与边界定义
- 目标：确认各已支持语言当前是专用提取器还是 fallback 启发式，以及它们对类型/实现/关联方法关系的真实支持边界。
- 交付物：多语言能力审计结论、统一边界说明与需要修复/显式降级的清单。
- 完成条件：后续实现不再建立在“其他语言可能也支持同等关系语义”的假设上。
- 依赖：当前 `lang_support`、`semantic.rs`、`rust_analysis.rs` 的现状分析结果。

### 阶段二：重建 Rust 关系模型并抽象共享表达
- 目标：明确 Rust 提取阶段应产出的拥有者、impl 归属、trait impl、关联方法等基础关系，并在需要时抽出多语言可复用的关系表达层。
- 交付物：Rust 符号抽取设计落点、关系字段/结构实现与对应测试。
- 完成条件：`rust_analysis` 能为后续 graph/query 层提供稳定的关系信息，而不是只输出扁平 symbol + alias。
- 依赖：阶段一能力审计结论。

### 阶段三：重构查询、图构建与多语言输出边界
- 目标：让按类型名查询时可聚合关联方法/impl，并在 analysis detail 中体现关系边；对其他语言统一输出已确认的支持边界。
- 交付物：更新后的 filtering / symbol lookup / graph edge 构建逻辑、边界提示/契约测试。
- 完成条件：`context` 等分析结果能同时体现 Rust 类型关系；其他语言的能力边界明确且不再误导；函数级 `calls` 行为不回退。
- 依赖：阶段二输出的关系模型。

### 阶段四：对外契约、验证与收尾
- 目标：确保 Core/MCP/CLI 消费链路在新模型下稳定工作，并完成文档与回归验证。
- 交付物：相关测试、必要的文档更新、验证记录。
- 完成条件：受影响 crate 的针对性测试通过；若输出结构、支持等级说明或文档有变化，仓库文档已同步。
- 依赖：阶段三完成。

## 测试与验证
- 核心验证：
  - 为 `codex-native-tldr` 增加/更新 Rust 符号关系与 analysis 聚合测试，覆盖 struct → impl → method / trait impl 的查询与图输出。
  - 增加多语言一致性审计测试或最小契约测试，确认哪些语言仍走 fallback，以及这些语言在类型关系查询上的边界是可解释且稳定的。
  - 针对 `TldrHandler` 这一已复现样例增加回归验证，确保按类型名查询不再只返回孤立 struct。
- 必过检查：
  - `just fmt`（在 `codex-rs` 目录）
  - `cargo nextest run -p codex-native-tldr` 或仓库推荐的 `just native-tldr-test-fast`
- 回归验证：
  - 若 `codex-core` 或 `codex-mcp-server` 的 `tldr` 展示/契约受影响，补跑对应目标测试。
  - 若对 MCP tool 输出结构或支持说明有调整，补充/更新 `codex-mcp-server` 相关测试。
- 手动检查：
  - 使用 `tldr context` 对代表性 Rust 类型与方法做抽查，确认类型查询结果能带出关联方法/impl。
  - 对至少 1~2 种非 Rust 已支持语言做抽查，确认输出与文档声明的能力边界一致。
- 未执行的验证（如有）：无。

## 风险与缓解
- 关键风险：
  - 关系模型改造过浅，仍停留在字符串匹配层，后续继续漏掉复杂 impl 场景。
  - 对其他语言的能力边界判断不准，导致过度承诺或无意回退。
  - 图边与查询聚合改造过猛，导致现有函数级查询、summary 文本或上层契约回归。
  - 输出结构或支持说明变化波及 `core` / `mcp-server` 测试与文档。
- 触发信号：
  - 类型名查询仍只能看到 struct 本体。
  - 现有 `run_tldr_handler_with_hooks`、MCP `tldr` 契约测试出现退化。
  - `analysis detail` 中出现重复节点、错误归属或不稳定 edge。
- 缓解措施：
  - 先完成多语言能力审计并定义边界，再推进实现。
  - 先定义明确的关系模型与测试样例，再推进实现。
  - 以 `codex-native-tldr` 单元/契约测试锁定行为，再补跑受影响上层测试。
  - 若输出字段或支持说明发生变化，同步更新文档与消费侧断言。
- 回滚/恢复方案（如需要）：若重构导致上层契约大面积回归，回退到最近可通过的 `native-tldr` 行为基线，再按阶段缩小改动重新推进。

## 参考
- `codex-rs/native-tldr/src/lang_support/mod.rs:1`
- `codex-rs/native-tldr/src/rust_analysis.rs:136`
- `codex-rs/native-tldr/src/rust_analysis.rs:204`
- `codex-rs/native-tldr/src/analysis.rs:109`
- `codex-rs/native-tldr/src/analysis.rs:378`
- `codex-rs/native-tldr/src/analysis.rs:874`
- `codex-rs/native-tldr/src/semantic.rs:680`
- `codex-rs/native-tldr/src/semantic.rs:772`
- `codex-rs/native-tldr/src/semantic.rs:895`
- `codex-rs/native-tldr/src/semantic.rs:972`
- `codex-rs/core/src/tools/handlers/tldr.rs:38`
- `codex-rs/core/src/tools/handlers/tldr.rs:99`
- `codex-rs/core/src/tools/handlers/tldr.rs:219`
- `.agents/codex-cli-native-tldr/architecture.md:9`
