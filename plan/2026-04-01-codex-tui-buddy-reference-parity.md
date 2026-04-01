# 参考项目对齐：重做 Codex Rust TUI 的动态 Buddy

> 不适用项写 `无`，不要留空。
> 只写已确认事实；若信息不清楚或需要假设，先回到对话澄清，不要把未确认内容写进计划。

## 背景
- 当前状态：
  - 当前 Buddy 实现在 `codex-rs/tui`，已具备 deterministic bones、`/buddy show|pet|hide|status`、默认显示开关，以及窄屏/宽屏两档静态渲染。
  - 当前实现仍以静态文本为主，宽屏只是在 footer 中堆叠 bubble + sprite + traits，缺少持续 tick 驱动的动态帧、动态 hearts、bubble 生命周期和参考项目那种更完整的布局策略。
  - 参考项目 `claude-code-rev` 的 Buddy 实现已经具备 500ms tick、idle sequence、pet hearts、speech bubble、宽窄屏差异布局、startup teaser，以及 companion prompt intro 边界。
- 触发原因：
  - 用户明确要求 `$using-cadence`，并要求彻底参考“参考项目”，把现有宠物功能做得更智能、更可爱、并且是动态的，而不是停留在简单表情文本。
- 预期影响：
  - 需要重新定义 Rust TUI Buddy 的行为 contract，把“动态 sprite + bubble 生命周期 + 宽窄屏策略 + 更丰富的交互反馈”升级为一等能力，并为后续继续贴近参考项目预留边界。

## 目标
- 目标结果：
  - 让 `codex-rs/tui` 中的 Buddy 达到与参考项目同层级的核心体验：默认静默存在、持续动态、可 pet、会说话、宽窄屏表现不同，且仍符合 ratatui/TUI 约束。
- 完成定义（DoD）：
  - Buddy 具备稳定 bones 与独立运行态，运行态至少覆盖 tick、reaction、pet burst、visible/muted 以及与布局相关的派生状态。
  - 宽屏下显示真正的多行动态 sprite 与 speech bubble；窄屏下退化为动态 face + quip，而不是简单静态标签。
  - `/buddy pet` 触发动态 hearts / burst 效果，`/buddy show|hide|status` 保持可用且语义与新状态模型一致。
  - Buddy 默认启用；用户可通过 `~/.codex/config.toml` 关闭。
  - 为用户可见变更补齐定向测试与 snapshot。
- 非目标：
  - 不照搬参考项目的 React/Ink 组件结构。
  - 不把 Buddy 扩展到 app-server、MCP 或远程协议层。
  - 不在本轮把 Buddy 接成独立聊天代理或工具调用参与者。

## 范围
- 范围内：
  - 对齐参考项目 `companion.ts` / `types.ts` 的 deterministic bones 与 rarity/stat 生成原则。
  - 对齐参考项目 `CompanionSprite.tsx` 的 tick 驱动、idle sequence、pet burst、bubble 生命周期、宽窄屏差异布局。
  - 对齐参考项目 `useBuddyNotification.tsx` 的 startup teaser 思路，但实现方式按 TUI 现有通知/消息体系收敛。
  - 重构 Rust TUI Buddy 的模块边界、渲染路径、slash command 接线与相关测试。
  - 保持默认显示，允许通过配置显式关闭。
- 范围外：
  - `prompt.ts` 中 companion intro attachment / watcher 提示词链路的完整接入。
  - 参考项目 teaser 的时间窗口营销策略原样复刻。
  - 任何需要服务端模型配合的“Buddy 自主发言链路”。

## 影响
- 受影响模块：
  - `codex-rs/tui/src/buddy/mod.rs`
  - `codex-rs/tui/src/buddy/model.rs`
  - `codex-rs/tui/src/buddy/render.rs`
  - `codex-rs/tui/src/bottom_pane/mod.rs`
  - `codex-rs/tui/src/chatwidget.rs`
  - `codex-rs/tui/src/chatwidget/tests/slash_commands.rs`
  - `codex-rs/tui/src/**/snapshots/*`
  - `codex-rs/core/src/config/types.rs`
  - `codex-rs/core/src/config/mod.rs`
  - `codex-rs/core/src/config/config_tests.rs`
  - `codex-rs/core/config.schema.json`
- 受影响接口/命令：
  - `/buddy`
  - `/buddy show`
  - `/buddy pet`
  - `/buddy hide`
  - `/buddy status`
  - `~/.codex/config.toml` 中 `[tui].show_buddy`
- 受影响数据/模式：
  - Buddy bones 生成规则
  - Buddy 运行态与 redraw/tick 时序
  - TUI 配置 schema
- 受影响用户界面/行为：
  - footer / bottom pane 中 Buddy 的展示形式
  - 窄屏与宽屏下的 Buddy 布局与文本占位
  - pet / show / hide / status 的即时反馈

## 约束与依赖
- 约束（时间/兼容性/安全/性能/发布窗口等）：
  - 必须继续以 `codex-rs/tui` 为主线实现，不按 `tui_app_server` 做迁移规划。
  - 必须避免把更多复杂逻辑继续堆进 `chatwidget.rs` 或 `bottom_pane/mod.rs`；Buddy 复杂度应尽量收敛在独立模块。
  - 用户可见 UI 改动必须补 snapshot。
  - 改 `ConfigToml` 或嵌套配置时必须同步 `just write-config-schema`。
  - Rust 改动完成后必须 `just fmt`，并跑对应 crate 的定向测试。
  - 工作区已有无关脏改动；执行阶段必须只改和 Buddy 任务相关的文件，避免误提交。
- 外部依赖（系统/人员/数据/权限等）：
  - 参考实现位于 `/tmp/claude-code-rev.7F9gvv/src/buddy/*`。
  - 当前 Cadence 轮次未获得子代理授权，默认仅主 agent 执行。
  - 当前可用能力筛查结论：`tldr` 可用于快速定位 Rust 影响面；其余可用技能对本轮 Planning 没有实质增益，记为 `none-applicable`。

## 实施策略
- 总体方案：
  - 以参考项目为蓝本，把 Buddy 拆成三层：
    - `bones`：完全由 deterministic seed 推导的 rarity/species/eye/hat/shiny/stats。
    - `runtime state`：tick、reaction、pet burst、visibility、last action、layout 派生状态。
    - `render/layout`：宽屏 full sprite + bubble，窄屏 compact face + quip。
  - 让 bottom pane 只负责接线与重绘调度，具体动画帧、bubble 文本、layout 宽度与生命周期都下沉到 buddy 模块。
  - 对用户可见行为维持 slash command 入口不变，以降低外部使用面的破坏。
- 关键决策：
  - 明确以参考项目的 `TICK_MS = 500`、`PET_BURST_MS = 2500`、idle sequence、bubble fade window 作为 Rust 侧设计基准。
  - 明确采用“宽屏完整 sprite / 窄屏退化 quip”的双布局，而不是把所有场景塞进单一静态行。
  - 明确保留 deterministic bones 原则，不让用户通过配置伪造 rarity 或外观骨架。
  - 明确把“更智能”收敛为本地可观察的反应状态、动态反馈与 teaser/quip 表达，不在本轮引入模型消息链路。
- 明确不采用的方案（如有）：
  - 不继续在现有静态 ASCII 基础上做零碎补丁。
  - 不先接 prompt attachment，再回头补动画与布局。
  - 不为了“动态”而引入不可观察的隐藏 fallback 或 silent degradation。

## 阶段拆分
### 阶段一：参考能力对齐与状态 contract 重写
- 目标：
  - 把参考项目中的 bones、tick、reaction、pet burst、layout reservation、teaser 边界整理成 Rust 可执行 contract。
- 交付物：
  - 更新后的 Buddy 状态模型与模块边界。
  - 明确的 execution issue 合同。
- 完成条件：
  - 能清楚区分 deterministic bones、runtime state、render/layout state，以及哪些能力本轮明确不做。
- 依赖：
  - `claude-code-rev` buddy 参考代码。

### 阶段二：动态 sprite 与生命周期渲染
- 目标：
  - 为 Buddy 建立持续 tick 驱动的动态视觉表现。
- 交付物：
  - idle/blink/fidget/excited 等帧状态。
  - pet hearts / burst 效果。
  - bubble 显示、衰减、消失的生命周期。
- 完成条件：
  - Buddy 在不执行命令时也具备可见动态；pet 后能持续数个 tick 显示动态反馈。
- 依赖：
  - 阶段一的状态 contract。

### 阶段三：宽窄屏布局与接线重构
- 目标：
  - 让宽屏与窄屏采用不同布局策略，同时不破坏现有 bottom pane 主流程。
- 交付物：
  - 宽屏完整 sprite + bubble 布局。
  - 窄屏 compact buddy 表现。
  - bottom pane / chatwidget 接线与 redraw 调度更新。
- 完成条件：
  - 布局变化可控，slash command 与输入区行为不回归。
- 依赖：
  - 阶段二的动态渲染能力。

### 阶段四：配置、teaser 与验证收口
- 目标：
  - 收口默认启用、用户关闭、可选 teaser 与回归验证。
- 交付物：
  - `show_buddy` 默认开启的配置链路与 schema 保持一致。
  - startup teaser 的实现或明确不实现的 contract。
  - snapshot、定向测试、review 与提交。
- 完成条件：
  - 用户默认能看到更完整的 Buddy；关闭配置仍可生效；验证入口完整可复现。
- 依赖：
  - 前三阶段完成。

## 测试与验证
> 只写当前仓库中真正可执行、可复现的验证入口；如果没有自动化载体，就写具体的手动检查步骤，不要写含糊表述。

- 核心验证：
  - `cd codex-rs && cargo test -p codex-tui buddy -- --nocapture`
  - `cd codex-rs && cargo test -p codex-tui slash_buddy_show_then_pet_reports_state -- --nocapture`
  - `cd codex-rs && cargo test -p codex-core show_buddy -- --nocapture`
- 必过检查：
  - `cd codex-rs && just fmt`
  - `cd codex-rs && cargo insta pending-snapshots -p codex-tui`
  - 如改配置：`cd codex-rs && just write-config-schema`
- 回归验证：
  - Buddy 默认显示 / 配置关闭
  - `/buddy show|pet|hide|status`
  - bottom pane 高度与 footer 布局
  - 窄屏与宽屏 snapshot
- 手动检查：
  - `mise run build` 后启动新二进制，确认 Buddy 默认显示
  - 在 TUI 中执行 `/buddy pet`，观察 hearts / bubble / sprite 动态是否出现
  - 调整终端宽度，确认宽窄屏展示策略不同
- 未执行的验证（如有）：
  - 与参考项目逐帧视觉完全一致不作为本轮必验项。

## 风险与缓解
- 关键风险：
  - 动态 tick 与 bottom pane 重绘节奏耦合，容易引入 redraw 抖动或布局回归。
  - 宽屏 bubble 与 sprite 占位若处理不当，可能挤压 composer 或干扰现有 footer 信息。
  - 当前工作区有大量无关改动，执行阶段容易误碰或误提交。
- 触发信号：
  - snapshot 大面积漂移
  - 输入区高度或换行异常
  - `/buddy` 命令反馈与实际 UI 状态不一致
  - `mise run build` 产物与运行二进制不一致
- 缓解措施：
  - 先重写状态 contract，再实现 tick 与渲染，最后接线。
  - 用窄屏/宽屏分离 snapshot 与 slash command 测试覆盖回归点。
  - 执行阶段使用隔离 `CARGO_TARGET_DIR`，并严格限制提交文件集合。
- 回滚/恢复方案（如需要）：
  - 保持 Buddy 模块化改动，必要时可单独回退 buddy 模块与接线，不影响其他 TUI 主链路。

## 参考
- `codex-rs/tui/src/buddy/mod.rs:21`
- `codex-rs/tui/src/buddy/render.rs:13`
- `codex-rs/tui/src/bottom_pane/mod.rs:1192`
- `codex-rs/tui/src/chatwidget.rs:4832`
- `codex-rs/tui/src/chatwidget.rs:5586`
- `codex-rs/core/src/config/mod.rs:394`
- `codex-rs/core/src/config/mod.rs:2796`
- `codex-rs/core/src/config/types.rs:840`
- `codex-rs/core/src/config/config_tests.rs:1030`
- `/tmp/claude-code-rev.7F9gvv/src/buddy/companion.ts:107`
- `/tmp/claude-code-rev.7F9gvv/src/buddy/companion.ts:127`
- `/tmp/claude-code-rev.7F9gvv/src/buddy/CompanionSprite.tsx:16`
- `/tmp/claude-code-rev.7F9gvv/src/buddy/CompanionSprite.tsx:23`
- `/tmp/claude-code-rev.7F9gvv/src/buddy/CompanionSprite.tsx:152`
- `/tmp/claude-code-rev.7F9gvv/src/buddy/CompanionSprite.tsx:167`
- `/tmp/claude-code-rev.7F9gvv/src/buddy/prompt.ts:7`
- `/tmp/claude-code-rev.7F9gvv/src/buddy/prompt.ts:15`
- `/tmp/claude-code-rev.7F9gvv/src/buddy/useBuddyNotification.tsx:43`
