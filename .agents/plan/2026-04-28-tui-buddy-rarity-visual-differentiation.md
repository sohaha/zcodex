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
 
## 约束确认（constraints 阶段产物）
 
> 以下不变量由代码审查确认，后续实现阶段必须满足。
 
### 确定性约束（INV-DET）
 
1. **seed→rarity 不可变**：`roll_rarity()` 的概率分布（Common 60% / Uncommon 25% / Rare 10% / Epic 4% / Legendary 1%）和 `BuddyBones::from_seed()` 的确定性逻辑不能被本轮改动破坏。
2. **视觉差异只依赖 bones**：所有新增渲染差异只能读取 `bones.rarity` / `bones.species` / `bones.shiny` / `bones.hat` 等 seed 派生字段，不能引入运行时随机态。
3. **snapshot 固定 seed**：测试使用 `"codex-home::project"` / `"codex-goose::project"` / `"codex-snail::project"` 等固定 seed；同一 seed 在改动前后必须产生同一 bones（species/rarity/hat/eye/shiny 不变）。
 
### 布局约束（INV-LAYOUT）
 
4. **sprite 行宽不变**：species sprite 固定 3 行 × 10 字符宽（由 `apply_offset` 保持）。新增装饰（prefix/suffix/边框）不能改变 sprite 主体宽度。
5. **full layout 总高受控**：当前 full layout = 上边框(可选 1行) + sprite(3~4行含 hat) + 下边框(可选 1行) + identity(1行) = 最多 6 行。新增装饰层最多再增加 2 行（冠饰行上移或 aura 行下移），full layout 总高不超过 8 行。
6. **compact 视图保持单行**：`render_narrow_line()` 输出必须仍是 1 行，不能破坏窄屏布局。
7. **宽度阈值不变**：`FULL_LAYOUT_WIDTH = 58` 和 `MIN_RENDER_WIDTH = 12` 不在本轮调整范围内。
8. **pet burst 兼容**：pet 动画的 `<3` hearts 行必须在稀有度装饰之前或之上渲染，不能被新装饰覆盖或打乱时序。
 
### 模块边界约束（INV-MOD）
 
9. **改动收敛于 buddy/ 模块**：只改 `model.rs`、`render.rs`、`mod.rs`（含其测试），不触碰 `chatwidget.rs`、`bottom_pane/` 或 `buddy/` 外的任何文件。
10. **render.rs 行数警戒**：`render.rs` 当前 663 行，接近 800 行警戒线。新增 rarity overlay 逻辑如果超过 ~80 行，必须提取为 render.rs 内的独立函数组（如 `rarity_overlay` 子模块），而不是继续内联膨胀。
11. **model.rs 已超警戒**：`model.rs` 当前 811 行，已超过 500 行目标线和 800 行警戒线。不在本轮继续向 model.rs 增加渲染相关逻辑；rarity 到视觉信号的映射在 model.rs 只保留纯数据访问方法（如 `frame_symbol` / `aura_line` / `crown_symbol`），复杂渲染逻辑放 render.rs。
 
### 视觉分层不变量（INV-VIS）
 
12. **逐级增强原则**：每一级 rarity 的视觉信号强度严格递增（Common ≤ Uncommon ≤ Rare ≤ Epic ≤ Legendary），不能出现低等级视觉信号意外强于高等级的情况。
13. **无颜色环境下仍可区分**：Legendary vs Epic vs Rare 必须在单色终端（无 ANSI 颜色）下仍能通过字符差异识别，不能仅靠颜色区分。
14. **Common 保持朴素**：Common 不增加任何装饰层、边框或专属符号，保留"空白就是朴素"的视觉语义。
15. **Uncommon 必须获得至少一个可见造型元素**：当前 Uncommon 在 full 视图中与 Common 完全相同（仅颜色不同）；本轮必须让 Uncommon 获得至少一个非颜色类的可见差异（如专属前缀符号或轮廓标记）。
 
### 兼容性约束（INV-COMPAT）
 
16. **slash 命令语义不变**：`/buddy show|full|pet|hide|status` 的行为语义和返回结构不变，只增强展示。
17. **status 文案增强而非重写**：`status()` 方法返回的 `message` 字段可以补充造型特征描述，但必须保留现有字段（峰值属性、可见性等），不能破坏依赖该字段的调用方。
18. **BuddyRarity 枚举不变**：不新增或删除 rarity 变体，不改概率分布。
 
### 验证约束（INV-TEST）
 
19. **snapshot 范围严格限定**：只接收 `buddy/snapshots/` 目录下的 pending snapshots，不执行 `cargo insta accept` 全量操作。
20. **新增 snapshot 覆盖**：为新增的 rarity-driven 视觉差异补对应的定向测试和 snapshot（至少覆盖 Legendary full 和 Epic full 场景）。
21. **格式化与 lint**：改动完成后执行 `just fmt`；若 render.rs 或 model.rs 改动超过 ~30 行，额外执行 `just fix -p codex-tui`。
22. **定向测试范围**：只跑 `cargo nextest run -p codex-tui`（或 `cargo test -p codex-tui`），不扩大到 workspace 全量。
 
### 风险约束（INV-RISK）
 
23. **不引入 species × rarity 全组合**：维护成本 O(N×M) 的硬编码 sprite 被明确排除。只能用 overlay/variant 机制。
24. **不引入动画时序重构**：本轮不为 rarity 增加专属帧动画或状态机扩展，fidget/excited/pet burst 时序逻辑不变。
25. **不扩展 config schema**：不为 rarity 样式引入用户配置开关或 `config.toml` 字段。

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

## 目标澄清 (Intent)

> 执行阶段：目标澄清（Intent）。

### 核心目标

**让不同稀有度在终端中一眼可辨**，不只是颜色/星星的差异，而是造型层面有稳定、逐级增强的视觉信号。

### 当前状态（已确认事实）

- `BuddyRarity` 在 `model.rs:416-548` 定义，包含 `Common/Uncommon/Rare/Epic/Legendary` 五个等级。
- 已有的 rarity 相关渲染接口：
  - `rarity_style()` — 为 Legendary 启用 `shiny_style()`（品红+粗体），其他等级使用不同颜色（dim/green/cyan/magenta）。
  - `sprite_prefix()` — 仅 `Legendary` 返回 `Some("✦ ")`。
  - `sprite_suffix()` — `Epic` 返回 `Some(" ✨")`，`Legendary` 返回 `Some(" ✦")`。
  - `frame_symbol()` — `Rare` 返回 `Some("·")`，`Epic` 返回 `Some("✦")`，`Legendary` 返回 `Some("★")`。
  - `compact_symbol()` — Uncommon→Rare→Epic→Legendary 分别返回 `◆/✦/★/✧`，Common 返回空。
  - `visual_trait()` — 每个等级返回一句中文描述（"普通外观"/"微光轮廓"等）。
- 渲染入口在 `render.rs:48-68`：full sprite 路径已使用 `frame_symbol` 画边框，`prefix`/`suffix` 生效；narrow/compact 路径仅展示 `stars` + `compact_symbol`，无边框。
- `model.rs` 的 `BuddyBones::from_seed` 已实现确定性生成，不依赖运行时随机态。
- 参考项目 `reference-projects/claude-code-rev/src/buddy/companion.ts` 的设计：rarity 影响 hat 是否为 none（common 必定无帽）、stat floor、颜色 key，但不直接改变 sprite 骨架。关键价值：rarity 作为"贯穿所有视觉层"的稳定属性。
- `render.rs` 当前只有 3 行 sprite（无 body 层/装饰层分离），所有 species 共用同一 3 行 ASCII 格式，扩展空间有限但可叠加装饰。

### 未完全确认/需要澄清的假设

| 假设 | 状态 | 需要的澄清 |
|------|------|-----------|
| Rare/Epic 不画边框时是否仍需要至少一个专属视觉信号 | 需确认 | 计划需明确 Rare/Epic 的 fallback 信号 |
| compact 视图是否需要在单行内展示比 stars+symbol 更多信息 | 需确认 | 影响 compact 行的改法幅度 |
| status 文案中的 visual_trait 是否作为主要区分手段 | 需确认 | 目前已存在，但可能需要更具体 |
| "造型差异"的具体定义：边框装饰/prefix/suffix/颜色/compact 符号，如何分配 | 待决策 | 这正是阶段一要解决的 visual contract |

### 明确成功标准

1. **全视图可辨**：full sprite（宽屏）和 compact（窄屏）两个视图，高 rarity（Epic/Legendary）都具备不同于低 rarity 的即时可辨信号。
2. **造型层差异**：至少一种造型层（边框/装饰/前缀后缀/mini-face 变体之一）由 rarity 直接驱动，而不是只有颜色。
3. **无颜色环境可区分**：Legendary 在无颜色环境下仍能通过符号/边框与 Rare 区分。
4. **现有语义不变**：`/buddy show|full|pet|hide|status` 命令行为不变。
5. **snapshot 通过**：所有 Buddy 相关 snapshot 更新并通过。
6. **确定性保持**：稀有度差异由 seed 决定，无运行时随机态污染。

### 明确非目标

- 不修改 `roll_rarity` 概率。
- 不引入培养/升级系统。
- 不重写全部 species sprite 骨架。
- 不修改 app-server、protocol 或 config schema。
- 不在 `chatwidget.rs` 或 `bottom_pane/mod.rs` 散落 rarity 逻辑。

### 阶段一目标（Visual Contract）

需要回答以下问题：
1. Rare/Epic/Common/Uncommon 的 full sprite 视觉信号是什么（边框？装饰行？前缀后缀？）？
2. compact 视图的视觉层次是否需要强化，如何强化？
3. status 文案的 `visual_trait` 是否需要升级为更具体的描述？
4. 哪些信号只在 full 视图出现，哪些在两个视图共享？

### 下一步

完成本阶段后，计划进入**阶段一**：定义 visual contract——即每种 rarity 的具体视觉信号清单，作为阶段二编码的蓝图。


## 上下文收集 (Context) — 已完成

### 代码结构确认

- **`codex-rs/tui/src/buddy/model.rs`** (811 行)
  - `BuddyRarity` 枚举 (L416-422)：`Common/Uncommon/Rare/Epic/Legendary`，已 derive `Ord`，可做大小比较。
  - 现有 rarity 方法（全部已实现）：
    - `label()` — 中文标签
    - `stars()` — 星级文本（★ 到 ★★★★★）
    - `styled_span()` / `stars_span()` — 带颜色 Span
    - `stat_floor()` — 属性下限
    - `sprite_prefix()` — 仅 Legendary 返回 `"✦ "`
    - `sprite_suffix()` — Epic 返回 `" ✨"`，Legendary 返回 `" ✦"`
    - `frame_symbol()` — Rare/`·`，Epic/`✦`，Legendary/`★`
    - `compact_symbol()` — 空字符串/◆/✦/★/✧
    - `visual_trait()` — 中文描述（"普通外观"到"传奇光效"）
  - `BuddyBones::from_seed()` (L555+)：确定性生成，rarity 由 `roll_rarity()` 决定（Common 60%/Uncommon 25%/Rare 10%/Epic 4%/Legendary 1%）。Common 必定无 hat；Rare+ 可选 rare_species（Dragon/Ghost/Robot）。
  - `BuddyBones` 结构体 (L540)：name, species, rarity, eye, hat, shiny, stats。
  - `BuddySpecies` 有 20 种，每种有独立 `_lines(eye, frame) -> [String; 3]` 渲染函数。

- **`codex-rs/tui/src/buddy/render.rs`** (663 行)
  - `render_lines()` (L48-68)：根据 width 和 mode 分流到 `render_wide_lines()` 或 `render_narrow_line()`。
  - `render_wide_lines()` (L69-130)：
    - 如果 rarity 有 `frame_symbol()`（Rare+），画上下边框行 + sprite 行两侧带 prefix/suffix。
    - 否则只画 sprite 行 + prefix/suffix。
    - 最后 `render_identity_line()` 显示 name + stars + rarity label + species + mood。
  - `render_narrow_line()` (L132-200)：
    - 显示 `mini_face + name + stars + compact_symbol + species` 或 reaction 文本。
    - 无边框、无额外装饰行。
  - `sprite_lines()` (L250-300)：分派到各 species 的 3 行 ASCII 渲染，加上可选 hat_line。
  - 所有 species 的 sprite 都是固定 3 行、10 字符宽的 ASCII art，通过 `apply_offset()` 实现 fidget 动画。
  - `rarity_style()` (L630-640)：Common=默认, Uncommon=green, Rare=cyan, Epic=magenta, Legendary=shiny_style(magenta+bold)；shiny 叠加 bold。
  - `mini_face()` (L543-620)：每个 species × frame 组合返回一个迷你表情字符串，用于 compact 视图。

- **`codex-rs/tui/src/buddy/mod.rs`** (560 行)
  - `BuddyWidget`：管理 bones/state/soul，实现 `Renderable`。
  - `/buddy show|full|pet|hide|status` 命令入口。
  - `status()` (L170-230)：输出含 stars、visual_trait、hat、eye、mood、pet_count、primary_stat 的完整文案。
  - 测试模块 (L400-560)：9 个 insta snapshot 测试 + 3 个行为测试。Snapshot 文件在 `buddy/snapshots/`。

### 当前视觉分层现状（gap 分析）

| Rarity | Full Sprite 信号 | Compact 信号 | Gap |
|--------|------------------|-------------|-----|
| Common | 无 prefix/suffix，无边框，默认颜色 | 空白 compact_symbol | ✅ 足够朴素 |
| Uncommon | 无 prefix/suffix，无边框，绿色 | `◆` + 绿色 | ⚠️ 仅靠颜色和单个符号，无造型层差异 |
| Rare | 边框 `·` + cyan | `✦` + cyan | ⚠️ 边框符号很弱，容易忽略 |
| Epic | 边框 `✦` + suffix `✨` + magenta | `★` + magenta | ⚠️ 有后缀和边框但 compact 仍靠单符号 |
| Legendary | prefix `✦` + suffix `✦` + 边框 `★` + shiny_style | `✧` + magenta.bold | ⚠️ Full 视图最丰富但 compact 与 Epic 符号差异小 |

**核心 gap**：
1. Common/Uncommon 在 full 视图中无任何造型差异，只有颜色不同。
2. Rare 的边框符号 `·` 极其微弱，几乎不可见。
3. Compact 视图所有 rarity 的差异仅靠颜色 + 单个符号，无造型层信号。
4. Legendary 与 Epic 在 compact 视图中仅 `✧` vs `★`，区分度不足。
5. Uncommon 无任何专属造型元素（无 prefix/suffix/frame_symbol）。

### 约束确认

- **Deterministic**：所有视觉差异必须由 seed 驱动，无运行时随机态。
- **布局安全**：sprite 固定 3 行 10 字符宽，边框/装饰行在 sprite 行上下各加 1 行。扩展不能破坏现有宽度对齐。
- **snapshot 优先**：TUI 用户可见变更必须补 snapshot，`cargo insta accept -p codex-tui` 只在 Buddy 相关文件范围内执行。
- **格式化**：Rust 改动完成后执行 `just fmt`；非小修则 `just fix -p codex-tui`。
- **测试**：定向跑 `cargo nextest run -p codex-tui` 验证 Buddy 相关测试。
- **模块边界**：rarity 展示逻辑收敛在 `buddy/` 模块内部，不散落到 chatwidget 或 bottom_pane。
- **文件尺度**：render.rs 663 行接近 800 行警戒线；新增装饰层逻辑优先提取为 render.rs 内的独立函数或考虑拆子模块。

### 参考项目关键设计

- `reference-projects/claude-code-rev/src/buddy/types.ts`：Rarity 作为稳定骨架属性，影响 stat floor、颜色 key、hat 是否为 none。
- `reference-projects/claude-code-rev/src/buddy/companion.ts`：rarity 影响生成逻辑（Common 无帽）。
- `reference-projects/claude-code-rev/src/buddy/CompanionSprite.tsx`：React 组件，sprite 渲染使用 CSS 样式和表情组合。
- **对我们有价值的思路**：让 rarity 贯穿所有视觉层（颜色+符号+边框+装饰+identity line），而不仅是概率和标签。我们当前已部分实现但还不够深入。

### 已有测试和 Snapshot

- `buddy/snapshots/` 下 8 个 `.snap` 文件，覆盖 wide/compact/bubble/teaser/petted/goose/snail 场景。
- 测试使用固定 seed `"codex-home::project"` 等，确保确定性。
- 任何 visual contract 变更都会影响现有 snapshot，需逐一 review 并接受。

## 方案设计（Architecture）

### 设计原则

1. **逐级增强**：每个稀有度等级在前一级基础上叠加新视觉层，形成清晰的视觉阶梯。
2. **Deterministic**：所有差异由 seed → rarity 驱动，不引入运行时随机态。
3. **模块收敛**：视觉逻辑收敛在 `buddy/` 模块内，不散落到 chatwidget/bottom_pane。
4. **最小改动**：不改 species sprite 核心 ASCII art，通过装饰层（frame/prefix/suffix/aura）叠加稀有度信号。
5. **文件尺度安全**：render.rs 已 663 行，接近 800 警戒线；新逻辑提取为独立函数，新增约 30-50 行，总量约 700-710，不拆子模块。

### 视觉分层方案

#### Full（宽屏）视图分层

| Rarity | 背景光晕 (aura) | 边框 (frame) | 前缀/后缀 (prefix/suffix) | Identity line 信号 |
|--------|-----------------|-------------|--------------------------|-------------------|
| Common | 无 | 无 | 无 | ★ + "常见" dim |
| Uncommon | 无 | 上下 `~` 浅绿波浪线 | 无 | ★★ + "少见" green |
| Rare | 无 | 上下 `·` cyan 点线 | 无 | ★★★ + "稀有" cyan |
| Epic | 精灵上方 `✧  ✧  ✧` 光点行 | 上下 `✦` magenta | 后缀 `✨` | ★★★★ + "史诗" magenta |
| Legendary | 精灵上下各 `✦ ✦ ✦` 光晕行 | 上下 `★` shiny magenta | 前缀 `✦` + 后缀 `✦` | ★★★★★ + "传奇" magenta.bold + "闪亮" |

#### Compact（窄屏）视图分层

| Rarity | 符号 | 样式 |
|--------|------|------|
| Common | 无 | 默认颜色 |
| Uncommon | `◆` | 绿色 |
| Rare | `✦` | cyan |
| Epic | `★` | magenta bold |
| Legendary | `✧✧` | shiny magenta + name 前后 `✧` 包裹 |

**Compact 增强**：Legendary 在 name 前后加 `✧` 包裹符（如 `✧ Mochi ✧`），并在 species 后追加 `✦` 尾缀，使其与 Epic 在紧凑视图中也能一眼区分。

### 具体改动清单

#### 1. `model.rs` — BuddyRarity 扩展

**新增方法：**

- `aura_lines(self) -> Option<(&'static str, &'static str)>` — 返回 (上光晕行, 下光晕行)。`Epic` 返回 `Some(("  ✧   ✧   ✧  ", ""))`，`Legendary` 返回 `Some((" ✦  ✦  ✦ ", " ✦  ✦  ✦ "))`，其余 `None`。
- `compact_prefix(self) -> &'static str` — compact 视图中 name 前的包裹符。仅 `Legendary` 返回 `"✧ "`。
- `compact_suffix(self) -> &'static str` — compact 视图中 species 后的尾缀。仅 `Legendary` 返回 `" ✦"`。
- `identity_badge(self) -> Option<&'static str>` — identity line 尾部额外标签。`Epic` 返回 `Some("✨")`，`Legendary` 返回 `Some("✦✦✦")`。

**修改方法：**

- `frame_symbol(self)` — 扩展到 `Uncommon` 也返回 `Some("~")`，使 Uncommon 获得波浪边框。
- `visual_trait(self)` — 更新描述文案以匹配新视觉层（如 Uncommon → "柔和波纹"，Epic → "星光点缀"）。

#### 2. `render.rs` — 渲染管线增强

**新增函数：**

- `fn render_aura_line(bones: &BuddyBones, is_top: bool) -> Option<Line<'static>>` — 根据 `bones.rarity.aura_lines()` 渲染光晕行。
- `fn rarity_frame_style(bones: &BuddyBones) -> Style` — 边框行的样式，使边框颜色匹配稀有度而非统一 dim。

**修改 `render_wide_lines()`：**

渲染顺序：
```
1. pet_burst_frame (如有)
2. frame 上边框 (Uncommon+ 的 ~ / Rare+ 的 · / Epic+ 的 ✦ / Legendary 的 ★)
3. aura 上行 (Epic+ 或 Legendary)
4. sprite 行 (带 prefix/suffix)
5. aura 下行 (仅 Legendary)
6. frame 下边框
7. identity_line (带 badge)
```

边框样式从统一 `dim()` 改为 `rarity_frame_style(bones)`。

**修改 `render_narrow_line()`：**

- 对于 Legendary，name 前插入 `compact_prefix`，species 后追加 `compact_suffix`。
- compact_symbol 样式对 Legendary 使用 `shiny_style()`。

**修改 `render_identity_line()`：**

- 在 shiny 标记之后，追加 `identity_badge`（如有）。

#### 3. `mod.rs` — 测试和 snapshot 更新

- 新增 4 个 snapshot 测试用例，覆盖 Uncommon/Epic/Legendary full + Legendary compact 场景。
- 现有 8 个 snapshot 逐一 review 后 accept。

### 数据流

```
seed → BuddyBones::from_seed()
  → roll_rarity() → BuddyRarity
  → rarity 决定 frame_symbol / aura_lines / prefix / suffix / compact_* / badge
  → render_wide_lines() / render_narrow_line() 组合渲染
  → 传给 BuddyWidget::render() → 底栏显示
```

### 状态所有权

- **BuddyRarity** — 在 `BuddyBones` 创建时确定，之后不可变。所有视觉分支从 `bones.rarity` 派生。
- **BuddyState** — 管理 animation frame / pet / reaction 状态，不参与 rarity 视觉决策。
- **render 函数** — 纯函数，接收 `(&BuddyBones, &str, &BuddyState, u16)` 返回 `Vec<Line>` 或 `Line`，无副作用。

### 集成点

- `BuddyWidget::render()` → 调用 `render::render_lines()`，现有调用点不变。
- `/buddy status` → 调用 `bones.rarity.visual_trait()`，文案更新自然反映。
- Snapshot 测试 → 新增 rarity-specific 用例，现有用例更新。

### 风险与缓解

| 风险 | 缓解 |
|------|------|
| 宽度对齐被 aura/badge 打破 | aura 行宽度与 sprite 行一致（≤ 10 char），badge 在 identity line 末尾追加，不影响对齐 |
| Uncommon 波浪边框 `~` 在某些终端渲染不清 | 使用 `fg(Color::Green)` 而非纯 dim，确保可见 |
| Legendary compact 包裹符在极窄屏被截断 | `truncate_with_ellipsis` 已兜底，包裹符会被自然截断 |
| Snapshot 全部失效 | 逐个 review，只 accept Buddy 相关 snapshot，不盲目全量 accept |

### 可执行任务拆分（供下一阶段参考）

## 执行计划详案

### 任务依赖关系图

```
Task 1: model.rs BuddyRarity 扩展
  │
  ├─ 1.1 新增 aura_lines() 方法
  ├─ 1.2 新增 compact_prefix() 方法
  ├─ 1.3 新增 compact_suffix() 方法
  ├─ 1.4 新增 identity_badge() 方法
  ├─ 1.5 扩展 frame_symbol() 支持 Uncommon → "~"
  └─ 1.6 更新 visual_trait() 文案
        │
        ▼
Task 2: render.rs 渲染管线增强
  │
  ├─ 2.1 新增 rarity_frame_style() 函数
  │     依赖: Task 1
  ├─ 2.2 修改 render_wide_lines() 渲染顺序
  │     依赖: Task 1, Task 2.1
  ├─ 2.3 修改 render_narrow_line() 支持 Legendary 包裹
  │     依赖: Task 1
  └─ 2.4 修改 render_identity_line() 追加 badge
        依赖: Task 1
              │
              ▼
Task 3: 测试与 snapshot 验证
  │
  ├─ 3.1 新增 4 个 rarity-specific snapshot 测试
  ├─ 3.2 运行 cargo nextest run -p codex-tui
  ├─ 3.3 review *.snap.new 文件
  └─ 3.4 cargo insta accept -p codex-tui
              │
              ▼
Task 4: 格式化与 lint
  │
  ├─ 4.1 just fmt
  └─ 4.2 just fix -p codex-tui (如需)
```

### 详细实施步骤

#### Task 1: model.rs — BuddyRarity 扩展

**文件**: `codex-rs/tui/src/buddy/model.rs` (L416-530 BuddyRarity impl 块)

**1.1 新增 `aura_lines()` 方法**

在 `frame_symbol()` 方法之后添加：

```rust
/// 稀有度专属光晕行（用于 full sprite 上下）
pub(crate) fn aura_lines(self) -> Option<(&'static str, &'static str)> {
    match self {
        Self::Epic => Some(("  ✧   ✧   ✧  ", "")),
        Self::Legendary => Some((" ✦  ✦  ✦ ", " ✦  ✦  ✦ ")),
        _ => None,
    }
}
```

**1.2 新增 `compact_prefix()` 方法**

```rust
/// 窄屏视图中 name 前的包裹符
pub(crate) fn compact_prefix(self) -> &'static str {
    match self {
        Self::Legendary => "✧ ",
        _ => "",
    }
}
```

**1.3 新增 `compact_suffix()` 方法**

```rust
/// 窄屏视图中 species 后的尾缀
pub(crate) fn compact_suffix(self) -> &'static str {
    match self {
        Self::Legendary => " ✦",
        _ => "",
    }
}
```

**1.4 新增 `identity_badge()` 方法**

```rust
/// identity line 尾部额外标签
pub(crate) fn identity_badge(self) -> Option<&'static str> {
    match self {
        Self::Epic => Some("✨"),
        Self::Legendary => Some("✦✦✦"),
        _ => None,
    }
}
```

**1.5 扩展 `frame_symbol()` 支持 Uncommon**

当前（L493-499）Uncommon 返回 `None`，修改为返回 `Some("~")`：

```rust
pub(crate) fn frame_symbol(self) -> Option<&'static str> {
    match self {
        Self::Uncommon => Some("~"),
        Self::Rare => Some("·"),
        Self::Epic => Some("✦"),
        Self::Legendary => Some("★"),
    }
}
```

**1.6 更新 `visual_trait()` 文案**

```rust
pub(crate) fn visual_trait(self) -> &'static str {
    match self {
        Self::Common => "朴素本色",
        Self::Uncommon => "柔和波纹",
        Self::Rare => "星点边框",
        Self::Epic => "星光点缀",
        Self::Legendary => "炫目光辉",
    }
}
```

**Task 1 验收**: `cargo check -p codex-tui` 编译通过。

---

#### Task 2: render.rs — 渲染管线增强

**文件**: `codex-rs/tui/src/buddy/render.rs`

**2.1 新增 `rarity_frame_style()` 函数**

在 `rarity_style()` 函数附近（约 L648）添加：

```rust
/// 边框行的样式，颜色匹配稀有度
fn rarity_frame_style(bones: &BuddyBones) -> Style {
    let base = match bones.rarity {
        BuddyRarity::Common => Style::default(),
        BuddyRarity::Uncommon => Style::default().green(),
        BuddyRarity::Rare => Style::default().cyan(),
        BuddyRarity::Epic => Style::default().magenta(),
        BuddyRarity::Legendary => shiny_style(),
    };
    if bones.shiny { base.bold() } else { base }
}
```

**2.2 修改 `render_wide_lines()` 渲染顺序**

当前代码结构（L59-110），按以下顺序重组：

```
1. pet_burst_frame (如有)
2. frame 上边框 (frame_symbol)
3. aura 上行 (Epic+ 或 Legendary)
4. sprite 行 (带 prefix/suffix)
5. aura 下行 (仅 Legendary)
6. frame 下边框
7. identity_line (带 badge)
```

核心变化点：
- 在边框分支内，`frame_symbol` 存在时先插入 `aura_top` 行（如有），再插入 sprite 行，再插入 `aura_bottom` 行（如有且非空）
- 边框样式从 `rarity_style(bones).dim()` 改为 `rarity_frame_style(bones).dim()`（更少冲突但更一致）
- 无边框分支保持不变（Common 仍然无装饰）

**2.3 修改 `render_narrow_line()` 支持 Legendary 包裹**

当前 `label` 构建（L135-148），修改为：

```rust
let compact_prefix = bones.rarity.compact_prefix();
let compact_suffix = bones.rarity.compact_suffix();
let compact_symbol = bones.rarity.compact_symbol();
let shiny = if bones.shiny { " *" } else { "" };
let symbol_sep = if compact_symbol.is_empty() { "" } else { " " };
format!(
    "{}{}{}{}{}{}{}{} {}",
    compact_prefix,
    name,
    bones.rarity.stars(),
    symbol_sep,
    compact_symbol,
    shiny,
    compact_suffix,
    bones.species.label()
)
```

同时，compact_symbol 渲染逻辑（L163-171）需跳过 Legendary（已用 prefix/suffix 替代）：

```rust
let compact_symbol = bones.rarity.compact_symbol();
if !compact_symbol.is_empty() && bones.rarity.compact_prefix().is_empty() {
    spans.push(" ".into());
    spans.push(Span::styled(compact_symbol.to_string(), rarity_style(bones)));
}
```

**2.4 修改 `render_identity_line()` 追加 badge**

当前代码（L273-276），在 shiny 判断之后追加：

```rust
if let Some(badge) = bones.rarity.identity_badge() {
    spans.push(" ".into());
    spans.push(Span::styled(badge, shiny_style()));
}
```

**Task 2 验收**: `cargo check -p codex-tui` 编译通过。

---

#### Task 3: 测试与 snapshot 验证

**文件**: `codex-rs/tui/src/buddy/mod.rs` (tests 模块)

**3.1 新增 4 个 snapshot 测试**

参考现有 `goose_buddy_full_snapshot` 模式（L486-502），新增：

- `uncommon_buddy_full_snapshot` — 强制 rarity=Uncommon，宽屏
- `epic_buddy_full_snapshot` — 强制 rarity=Epic，宽屏
- `legendary_buddy_full_snapshot` — 强制 rarity=Legendary，宽屏
- `legendary_buddy_narrow_snapshot` — 强制 rarity=Legendary，窄屏

每个测试用 `BuddyBones::from_seed("codex-home::project")` 构造后手动设置 `bones.rarity`。

**3.2 运行测试**

```bash
cd codex-rs && cargo nextest run -p codex-tui
```

**3.3 review snapshot**

```bash
cargo insta pending-snapshots -p codex-tui
```

逐个检查 `.snap.new` 内容是否符合预期。

**3.4 接受 snapshot**

确认无误后：

```bash
cargo insta accept -p codex-tui
```

**Task 3 验收**: 所有测试绿，snapshot 已 accept。

---

#### Task 4: 格式化与 lint

```bash
cd codex-rs && just fmt
cd codex-rs && just fix -p codex-tui
```

**Task 4 验收**: 无 clippy 警告。

---

### 回滚边界

- **回滚粒度**: 每个 task 可独立回滚
- **安全回滚点**: Task 1 完成后、Task 3 完成后
- **注意**: Task 2-3 存在耦合（snapshot 变化是预期行为），若 Task 2 导致现有 snapshot 以外的渲染异常，需整体回滚 Task 1-2
- **不回滚条件**: 新增 4 个 snapshot 是预期新增；现有 8 个 snapshot 的变化是预期行为（因为 rarity 方法扩展会影响已有 seed 对应的 buddy 渲染）

---

### 验收标准汇总

| 步骤 | 验收条件 | 命令 |
|------|----------|------|
| Task 1 | 编译通过 | `cargo check -p codex-tui` |
| Task 2 | 编译通过 | `cargo check -p codex-tui` |
| Task 3 | 测试全绿，snapshot 已 accept | `cargo nextest run -p codex-tui` |
| Task 4 | 无 clippy 警告 | `just fix -p codex-tui` |

### 出口条件

- 所有 4 个验收条件满足
- 8 个现有 snapshot 更新 + 4 个新增 snapshot 已 accept
- 无遗留 clippy 警告
- 代码已格式化

---

## Worker 定义

### Worker 概览

基于已有实施计划中的 4 个 Task，定义 3 种 Worker 类型。每种 Worker 可被独立派发、独立验收。

| Worker 类型 | 职责 | 对应 Task | 输入文件 | 输出文件 |
|-------------|------|-----------|----------|----------|
| `model-worker` | 扩展 `BuddyRarity` 的数据方法 | Task 1 | `model.rs` | `model.rs` |
| `render-worker` | 增强渲染管线的视觉分层 | Task 2 | `model.rs`, `render.rs` | `render.rs` |
| `test-worker` | 补充 snapshot 测试并验证 | Task 3 + Task 4 | `model.rs`, `render.rs`, `mod.rs` | `mod.rs`, `snapshots/*` |

### Worker 1: `model-worker`

**职责**：扩展 `BuddyRarity` 枚举的数据方法，新增 aura 渲染、compact 前后缀、identity badge 等稳定派生信息。

**输入**：
- `codex-rs/tui/src/buddy/model.rs` — 当前 `BuddyRarity` 定义（L416-512）
- 计划中的 Task 1 规格（新增 7 个方法 + 1 个 visual_trait 改写）

**输出**：
- 修改后的 `model.rs`，包含以下新增方法：
  - `aura_top() -> Option<&'static str>` — Rare 以下 None，Epic 返回 `Some("·  ✦  ·")`，Legendary 返回 `Some("✧ ✦ ✧ ✦ ✧")`
  - `aura_bottom() -> Option<&'static str>` — 仅 Legendary 返回 `Some("✦ · ★ · ✦")`
  - `compact_prefix() -> &'static str` — 仅 Legendary 返回 `"✧"`，其他空串
  - `compact_suffix() -> &'static str` — 仅 Legendary 返回 `"✧"`，其他空串
  - `identity_badge() -> Option<&'static str>` — Epic 返回 `Some("★")`，Legendary 返回 `Some("✦")`
  - `frame_style() -> Style` — 返回对应稀有度的 Style（无 bones 依赖，仅基于 self）
  - `visual_trait()` 改写 — 更新描述文案

**验收条件**：
- `cargo check -p codex-tui` 编译通过
- 所有新方法为 `pub(crate)` 且在 `impl BuddyRarity` 块内
- 不修改 `BuddyBones` 结构体字段或 `from_seed` 逻辑
- 不引入新依赖

**交接格式**：修改后的 `model.rs` 文件，供 `render-worker` 和 `test-worker` 调用新增方法。

**预估复杂度**：低（纯数据方法，无渲染逻辑）

---

### Worker 2: `render-worker`

**职责**：修改 `render.rs` 中的 `render_wide_lines()`、`render_narrow_line()`、`render_identity_line()`，使稀有度差异在终端视觉上逐级增强。

**输入**：
- `codex-rs/tui/src/buddy/render.rs` — 当前渲染管线
- `codex-rs/tui/src/buddy/model.rs` — 由 `model-worker` 扩展后的 `BuddyRarity` 新方法
- 计划中的 Task 2 规格（4 个子步骤）

**输出**：
- 修改后的 `render.rs`，包含以下渲染增强：
  - **2.1** 新增 `rarity_frame_style(bones: &BuddyBones) -> Style` 函数（在 `rarity_style()` 附近）
  - **2.2** `render_wide_lines()` 重构渲染顺序：pet_burst → frame 上边框 → aura_top → sprite 行 → aura_bottom → frame 下边框 → identity_line
  - **2.3** `render_narrow_line()` 支持 Legendary compact_prefix/suffix 包裹，非 Legendary 保持现有行为
  - **2.4** `render_identity_line()` 追加 rarity badge（Epic/Legendary）

**验收条件**：
- `cargo check -p codex-tui` 编译通过
- Common 级别无任何新增视觉元素（无边框、无 aura、无 badge）
- Rare 及以上有边框（由现有 `frame_symbol()` 驱动）
- Epic 有 aura_top 行 + identity badge
- Legendary 有 aura_top + aura_bottom + identity badge + compact 前后缀
- 不修改 `sprite_lines()`、`mini_face()`、`render_bubble()` 等无关函数
- 不修改 `chatwidget.rs` 或 `bottom_pane/mod.rs`

**交接格式**：修改后的 `render.rs` 文件。`test-worker` 需要此文件来生成正确的 snapshot。

**依赖**：必须在 `model-worker` 完成后执行（需要新增的 `BuddyRarity` 方法）。

**预估复杂度**：中（涉及 3 个渲染函数的结构性修改）

---

### Worker 3: `test-worker`

**职责**：新增稀有度分层 snapshot 测试，更新受影响的现有 snapshot，运行格式化和 lint。

**输入**：
- `codex-rs/tui/src/buddy/mod.rs` — 现有测试模块（L383 起）
- `codex-rs/tui/src/buddy/snapshots/` — 现有 8 个 snapshot 文件
- `model.rs` 和 `render.rs` — 由前两个 worker 修改后的版本
- 计划中的 Task 3 + Task 4 规格

**输出**：
- `mod.rs` 中新增 4 个 snapshot 测试函数：
  - `uncommon_buddy_full_snapshot` — 强制 rarity=Uncommon，宽屏
  - `epic_buddy_full_snapshot` — 强制 rarity=Epic，宽屏
  - `legendary_buddy_full_snapshot` — 强制 rarity=Legendary，宽屏
  - `legendary_buddy_narrow_snapshot` — 强制 rarity=Legendary，窄屏
- 更新现有 8 个 snapshot（预期行为变化）
- 新增 4 个 `.snap` 文件
- 格式化和 lint 输出

**验收条件**：
- `cargo nextest run -p codex-tui` 全绿（或 `cargo test -p codex-tui`）
- `cargo insta pending-snapshots -p codex-tui` 无待处理
- `just fmt` 无变更
- `just fix -p codex-tui` 无 clippy 警告
- 现有测试（`bones_generation_is_stable`、`hidden_buddy_has_no_height`、`buddy_status_reports_peak_stat_and_visibility` 等）不受影响

**交接格式**：最终可交付的代码状态。无需进一步交接。

**依赖**：必须在 `model-worker` 和 `render-worker` 都完成后执行。

**预估复杂度**：中（新增测试 + snapshot 审查 + 格式化/lint）

---

### 执行顺序与并行性

```
model-worker ──→ render-worker ──→ test-worker
```

- 严格串行：每个 worker 依赖前一个的输出
- 不支持并行：render-worker 需要 model-worker 的新方法，test-worker 需要两者的完整渲染输出
- 可选的快速验证点：model-worker 完成后先 `cargo check` 确认编译，再派发 render-worker

### Worker 间数据契约

**model → render**：
- `BuddyRarity::aura_top() -> Option<&'static str>`
- `BuddyRarity::aura_bottom() -> Option<&'static str>`
- `BuddyRarity::compact_prefix() -> &'static str`
- `BuddyRarity::compact_suffix() -> &'static str`
- `BuddyRarity::identity_badge() -> Option<&'static str>`
- `BuddyRarity::visual_trait() -> &'static str`（改写版）

**render → test**：
- 完整的渲染管线输出（通过 `render_wide_lines()` 和 `render_narrow_line()` 的函数签名不变）
- snapshot 测试通过 `BuddyWidget` 公共 API 间接调用渲染，不直接依赖 render 函数签名

### 回滚策略

- `model-worker` 回滚：`git checkout -- codex-rs/tui/src/buddy/model.rs`
- `render-worker` 回滚：`git checkout -- codex-rs/tui/src/buddy/render.rs`
- `test-worker` 回滚：`git checkout -- codex-rs/tui/src/buddy/mod.rs codex-rs/tui/src/buddy/snapshots/`
- 整体回滚：`git checkout -- codex-rs/tui/src/buddy/`

### 当前代码基线摘要

供 worker 上下文参考：
- `BuddyRarity` 枚举：model.rs L416，5 个变体，已有 `label/stars/styled_span/stars_span/stat_floor/sprite_prefix/sprite_suffix/frame_symbol/compact_symbol/visual_trait` 共 10 个方法
- `render_wide_lines()`：render.rs L59，当前逻辑为 pet_burst → frame 边框分支 → identity_line
- `render_narrow_line()`：render.rs L122，当前逻辑为 label 构建 → face → spans 拼接
- `render_identity_line()`：render.rs L260，当前逻辑为 name + stars + rarity label + species + mood
- 现有 snapshot：8 个，位于 `codex-rs/tui/src/buddy/snapshots/`
- 测试模块起始：mod.rs L383

> Worker 定义完成。可通过 `codex mission continue` 推进到下一阶段。
