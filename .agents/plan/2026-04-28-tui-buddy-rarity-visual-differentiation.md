# 强化 TUI Buddy 的稀有度视觉分层

> 不适用项写 `无`，不要留空。
> 只写已确认事实；若信息不清楚或需要假设，先回到对话澄清，不要把未确认内容写进计划。

## 背景
- 当前状态：
  - 当前 Buddy 实现在 `codex-rs/tui/src/buddy/`，已具备 deterministic bones、`/buddy show|full|pet|hide|status`、宽窄屏两档渲染、idle/pet/reaction 动画，以及基于稀有度的星级和颜色差异。
  - 当前不同稀有度的主要差异仍集中在 `status` 文案、星级文本和颜色；sprite 造型本身基本只由 species、eye、hat 决定，`Legendary` 只是使用更显眼的样式，并没有专属轮廓或额外装饰层。
  - 参考项目 `reference-projects/claude-code-rev/src/buddy/` 的价值主要在于：把 rarity 作为稳定骨架属性贯穿到颜色、布局、交互反馈与用户感知，而不是只有概率和标签。
- 触发原因：
  - 用户明确要求使用 `$using-cadence`，并参考 `reference-projects/claude-code-rev` 的宠物功能，继续加强我们现有 Buddy，尤其要求“不同等级需要有不同的造型或者其他的”。
- 预期影响：
  - 需要把 Buddy 从“稀有度只影响文案和颜色”升级为“稀有度会显著影响终端可见造型与识别信号”，并保持 deterministic seed、不破坏当前 slash command 与底栏布局。

## 目标
- 目标结果：
  - 让 `Common`、`Uncommon`、`Rare`、`Epic`、`Legendary` 在视觉上具有稳定、可辨认、逐级增强的差异，而不是只靠颜色和星星区分。
- 完成定义（DoD）：
  - 在宽屏 full sprite 和窄屏 compact 视图中，稀有度都能体现出明确的分层信号。
  - 至少一类“造型差异”直接由 rarity 驱动，而不是仅由 species/hat/shiny 决定。
  - `Legendary` 与 `Epic` 的视觉表现明显高于 `Rare` 以下，不需要进入 `/buddy status` 也能一眼识别。
  - `/buddy show|full|pet|hide|status` 现有语义保持不变，只增强展示与反馈。
  - Buddy 相关 snapshot 和定向测试更新完成，Rust 代码按仓库规范格式化。
- 非目标：
  - 不重做 Buddy 的 seed、概率、species 列表、AI Soul 或模型反应链路。
  - 不把稀有度改成可培养升级系统。
  - 不扩展 app-server、protocol 或 config schema，除非实施中发现现有边界无法承载本轮视觉增强。

## 范围
- 范围内：
  - `codex-rs/tui/src/buddy/model.rs` 中 rarity 相关稳定派生信息。
  - `codex-rs/tui/src/buddy/render.rs` 中 full/compact 渲染、identity line、可能的装饰层或特效字符。
  - `codex-rs/tui/src/buddy/mod.rs` 中与状态反馈、status 文案、测试和 snapshot 直接相关的最小接线。
  - `codex-rs/tui/src/chatwidget/tests` 或 Buddy 自身测试中与 `/buddy status`、显示反馈、snapshot 相关的定向回归。
- 范围外：
  - `codex-rs/core/src/buddy.rs` 的 AI 反应生成与 soul 持久化。
  - Buddy 命令面新增子命令。
  - 新增 species、大规模重画全部 ASCII 素材，或为每个 species × rarity 组合维护独立整套 sprite。

## 影响
- 受影响模块：
  - `codex-rs/tui/src/buddy/model.rs`
  - `codex-rs/tui/src/buddy/render.rs`
  - `codex-rs/tui/src/buddy/mod.rs`
  - `codex-rs/tui/src/buddy/snapshots/*`
  - `codex-rs/tui/src/chatwidget/tests/slash_commands.rs` 或其他现有 Buddy 相关测试文件（如确有必要）
- 受影响接口/命令：
  - `/buddy show`
  - `/buddy full`
  - `/buddy pet`
  - `/buddy hide`
  - `/buddy status`
- 受影响数据/模式：
  - Buddy rarity 到渲染装饰/轮廓的映射规则
  - Buddy status 文案中的可见特征摘要
- 受影响用户界面/行为：
  - Buddy 宽屏 full sprite 的轮廓、装饰、光效或标题行
  - Buddy 窄屏 compact 标签的视觉层级
  - `show/status` 反馈里对稀有度和造型特征的描述

## 约束与依赖
- 约束（时间/兼容性/安全/性能/发布窗口等）：
  - 必须保持 deterministic seed 原则，不让稀有度差异依赖运行时随机态。
  - 不应把更多复杂逻辑散落到 `chatwidget.rs` 或 `bottom_pane/mod.rs`；稀有度展示逻辑尽量收敛在 Buddy 模块内部。
  - TUI 用户可见变更必须补 snapshot，并严格限定 snapshot 接收范围，只处理 Buddy 相关文件。
  - Rust 改动完成后必须在 `codex-rs/` 下执行 `just fmt`；若改动达到非小修级别，结束前执行 `just fix -p codex-tui`。
  - 只跑受影响 crate 的定向测试；本轮不默认扩大到 workspace 全量。
- 外部依赖（系统/人员/数据/权限等）：
  - 参考实现位于 `reference-projects/claude-code-rev/src/buddy/`。
  - 当前 Cadence 轮次未获得子代理授权，默认由主 agent 本地完成 Planning。
  - 当前会话 `available capabilities` 适配检查结论：
    - 已使用 `llmdoc` 获取仓库路由与既有反思。
    - 已使用 `using-cadence` / `cadence-planning` 约束规划阶段。
    - `ztldr` 适合结构搜索，但本轮调用出现 `structuredFailure/tool_error`，因此本计划后续不将其作为阻断依赖。
    - 其余技能对当前 Planning 的事实提炼没有额外实质收益，记为 `none-applicable`。

## 实施策略
- 总体方案：
  - 以“稀有度是稳定视觉层”为核心，不为每个 species 重画五套完整 sprite，而是在现有 species 基础上新增 rarity-driven 的分层机制：
    - 低等级：保留当前基础轮廓，差异主要来自低强度装饰与标签。
    - 中等级：引入可见的轮廓升级、边框件、背景符号或更明确的配件优先级。
    - 高等级：增加专属 aura、冠饰/浮饰、标题行、特殊 compact face 或 status 特征描述，让 `Epic/Legendary` 不看文字也能识别。
  - 复用参考项目“rarity 贯穿整个可视层”的思路，但按当前 Rust TUI 的模块边界实现，不照搬 React/Ink 组件结构。
  - 优先把“显著差异”落在少量稳定、可测、可 snapshot 的渲染点，而不是引入大规模动态特效。
- 关键决策：
  - 采用“species 基础 sprite + rarity overlay/variant”而不是 `species × rarity` 全组合硬编码，控制维护成本。
  - `Legendary` 必须拥有专属视觉信号，不能只继续复用 `shiny_style()` 的颜色粗体。
  - `Rare` 以上至少在 full sprite 和 compact 两个视图都出现等级可见差异，避免只有宽屏用户能感知升级。
  - status 文案会补充“造型特征摘要”或“光效/冠饰说明”，作为视觉增强的文字兜底，但不是主要区分方式。
- 明确不采用的方案（如有）：
  - 不通过修改概率、seed 或 `/buddy pet` 行为来模拟“升级”。
  - 不引入新的配置开关让用户手动切换 rarity 样式。
  - 不为所有稀有度单独做复杂动画时序，以免本轮扩大为状态机重构。

## 阶段拆分
### 阶段一：稀有度视觉 contract 定义
- 目标：
  - 明确每个 rarity 的可见信号、叠加规则和共享渲染接缝。
- 交付物：
  - rarity 到装饰层/轮廓层/文案层的清晰映射。
  - 需要调整的测试与 snapshot 范围。
- 完成条件：
  - 执行阶段不再需要临时决定“传奇到底长什么样”。
- 依赖：
  - 当前 Buddy 渲染代码与参考项目 `RARITY_COLORS` / `CompanionSprite` 设计事实。

### 阶段二：full sprite 与 compact 视图增强
- 目标：
  - 在不破坏现有布局的前提下，把 rarity 差异落到 full 和 compact 两个主视图。
- 交付物：
  - full sprite 的 rarity-driven 差异。
  - compact face / 名称行 / 星级行的等级信号增强。
- 完成条件：
  - 宽屏、窄屏都能稳定识别至少 `Legendary`、`Epic`、`Rare+` 与低等级差异。
- 依赖：
  - 阶段一的 visual contract。

### 阶段三：status/反馈收口与验证
- 目标：
  - 让命令反馈、status 文案和 snapshot 与新视觉 contract 对齐。
- 交付物：
  - 更新后的 `show/status` 输出。
  - Buddy snapshots 与定向测试。
- 完成条件：
  - 自动化验证通过，用户通过 `/buddy status` 能读到与视觉一致的特征描述。
- 依赖：
  - 阶段二完成。

## 测试与验证
> 只写当前仓库中真正可执行、可复现的验证入口；如果没有自动化载体，就写具体的手动检查步骤，不要写含糊表述。

- 核心验证：
  - `cd codex-rs && cargo test -p codex-tui buddy::tests -- --nocapture`
  - `cd codex-rs && cargo test -p codex-tui slash_buddy_status_reports_traits -- --exact --nocapture`
- 必过检查：
  - `cd codex-rs && just fmt`
  - `cd codex-rs && cargo insta pending-snapshots -p codex-tui`
  - 若改动规模达到非小修级别：`cd codex-rs && just fix -p codex-tui`
- 回归验证：
  - `/buddy show|full|pet|hide|status` 仍可用
  - Buddy 宽屏 full snapshot
  - Buddy 窄屏 compact snapshot
  - status 文案与显示特征一致
- 手动检查：
  - 启动 TUI 后执行 `/buddy show` 和 `/buddy full`，观察不同 seed 下高低 rarity 的可见差异。
  - 执行 `/buddy status`，确认文字描述能解释当前造型特征。
- 未执行的验证（如有）：
  - 不要求本轮对全部 species 做逐种手动验收；以定向 snapshot 和代表性 rarity 样本为主。

## 风险与缓解
- 关键风险：
  - 稀有度差异做得太重，导致 ASCII 可读性下降或占位超出当前 layout 预期。
  - 稀有度差异做得太轻，最终仍然只能靠颜色/星级区分，达不到用户目标。
  - snapshot 范围失控，把 Buddy 以外的 pending snapshots 一并接收。
- 触发信号：
  - 窄屏渲染截断明显增加
  - full sprite 对齐错位
  - `Legendary` 与 `Rare` 在无颜色环境下仍难以区分
  - `cargo insta pending-snapshots -p codex-tui` 出现 Buddy 目录外的待接收项
- 缓解措施：
  - 优先使用局部 overlay、标题行、装饰符和可控配件，而不是大改 species 主体骨架。
  - 为代表性 rarity 增加或更新 snapshot，确保“差异可见”被测试锁住。
  - 严格按 Buddy 子目录审查 snapshot，不在 dirty workspace 根目录直接全量 accept。
- 回滚/恢复方案（如需要）：
  - 若新造型破坏布局，可先保留 rarity 文案增强和局部装饰层，回退过重的 full sprite 差异实现。

## 参考
- `/workspace/codex-rs/tui/src/buddy/model.rs:415`
- `/workspace/codex-rs/tui/src/buddy/model.rs:487`
- `/workspace/codex-rs/tui/src/buddy/render.rs:69`
- `/workspace/codex-rs/tui/src/buddy/render.rs:192`
- `/workspace/codex-rs/tui/src/buddy/render.rs:582`
- `/workspace/codex-rs/tui/src/buddy/mod.rs:60`
- `/workspace/codex-rs/tui/src/buddy/mod.rs:159`
- `/workspace/codex-rs/tui/src/buddy/mod.rs:181`
- `/workspace/reference-projects/claude-code-rev/src/buddy/types.ts:124`
- `/workspace/reference-projects/claude-code-rev/src/buddy/companion.ts:79`
- `/workspace/reference-projects/claude-code-rev/src/buddy/CompanionSprite.tsx:218`
- `/workspace/.agents/llmdoc/memory/reflections/2026-04-10-buddy-snapshot-accept-scope.md:1`
