# 深化 Codex Rust TUI 的 Buddy 宠物系统

## 背景
- 当前状态：
  - 当前 Rust TUI 的 buddy 实现在 `codex-rs/tui`，而不是 `tui_app_server`。
  - 现有实现已具备稳定 seed、品种/名字/稀有度生成、`/buddy show|pet|hide|status` 命令，以及底部区域的一行文本渲染。
  - 现有渲染仍然是极简形态，核心表现接近 “`(=^.^=) Miso the rare cat` + 短暂 pet 文案”，与参考库的完整 sprite、气泡、状态表达和交互层级有明显差距。
- 触发原因：
  - 用户认为当前宠物功能“太简陋”，希望讨论是否需要深度分析参考库，并按更完整的方向重做/增强。
- 预期影响：
  - 需要把 buddy 从“最小可用功能”升级为“有更完整状态模型、渲染表达和可持续扩展能力”的子系统，同时尽量不破坏底部输入区、状态栏和 slash command 现有行为。

## 目标
- 目标结果：
  - 基于参考库重新设计 buddy 的状态模型、展示形态和交互反馈，使其从单行文本宠物升级为更完整、可持续演进的 TUI 宠物系统。
- 完成定义（DoD）：
  - 明确并实现新的 buddy 能力分层：稳定骨架（seed/bones）、运行态状态、渲染层、交互层、可选持久化边界。
  - 用户启动 TUI 后能看到默认出现的 buddy，并能通过 `/buddy` 获得明显优于当前版本的可见反馈。
  - 至少补齐对应的定向测试与 UI snapshot；涉及配置时同步 `config.schema.json`。
- 非目标：
  - 不直接照搬参考库的 React/Ink 组件结构。
  - 不把 buddy 扩展到 app-server v2 协议、MCP 协议或远程服务。
  - 不在本轮规划中承诺完整复刻参考库的所有 observer 文案和模型提示注入链路。

## 范围
- 范围内：
  - 重新分析参考库中的宠物骨架生成、sprite 渲染、窄屏/宽屏布局、pet 动效和气泡反馈。
  - 重新设计当前 Rust TUI 中 buddy 的状态模型与渲染抽象。
  - 将 buddy 的默认可见、关闭配置、交互反馈和布局自适应纳入统一设计。
  - 视情况引入最小必要持久化，但不允许通过配置伪造稀有度或外观骨架。
- 范围外：
  - 旧 TS CLI 或历史兼容实现对齐。
  - 把 companion intro/prompt attachment 直接接入现有模型消息链路，除非后续单独立项。
  - 把 buddy 做成聊天代理人格、独立发言者或工具调用参与者。

## 影响
- 受影响模块：
  - `codex-rs/tui/src/buddy.rs`
  - `codex-rs/tui/src/bottom_pane/mod.rs`
  - `codex-rs/tui/src/chatwidget.rs`
  - `codex-rs/tui/src/slash_command.rs`
  - `codex-rs/tui/src/chatwidget/tests/*`
  - `codex-rs/tui/src/snapshots/*`
  - `codex-rs/core/src/config/types.rs`
  - `codex-rs/core/src/config/mod.rs`
  - `codex-rs/core/src/config/config_tests.rs`
  - `codex-rs/core/config.schema.json`
- 受影响接口/命令：
  - `/buddy show|pet|hide|status`
  - `~/.codex/config.toml` 中 `[tui]` 的 buddy 相关开关
- 受影响数据/模式：
  - TUI buddy 运行态状态
  - `ConfigToml.tui` / `Config.tui_*` 派生字段
- 受影响用户界面/行为：
  - 底部区域的默认布局
  - 窄屏和宽屏下的 buddy 显示方式
  - pet 后的即时视觉反馈

## 约束与依赖
- 约束（时间/兼容性/安全/性能/发布窗口等）：
  - 必须遵守 `codex-rs/tui` 大模块控制规则，避免继续把复杂逻辑堆到 `chatwidget.rs` 或 `bottom_pane/mod.rs`。
  - 若修改 `ConfigToml` 或其嵌套类型，必须运行 `just write-config-schema`。
  - 若新增/修改用户可见 UI，必须补 snapshot。
  - 不允许把 rarity/species 等“骨架属性”直接作为用户可编辑配置暴露。
- 外部依赖（系统/人员/数据/权限等）：
  - 参考实现来自 `/tmp/claude-code-rev.7F9gvv/src/buddy/*`。
  - 本轮默认只在主 agent 内推进，不使用子 agent。

## 实施策略
- 总体方案：
  - 把 buddy 重构为两层模型：
    - `bones`：由稳定 seed 决定的稀有度、物种、外观特征、基础属性。
    - `soul/state`：运行态的显隐、最近 pet 时间、反应状态、可选持久化偏好。
  - 把渲染从“单行文字”升级为“窄屏简版 + 宽屏扩展版”的双形态。
  - 把交互从“仅命令回执”升级为“命令回执 + 可见动画/状态反馈”。
- 关键决策：
  - 参考库中“骨架由 seed 推导，存储只保留 soul，不允许伪造 rarity”这一原则应保留。
  - 参考库中的 sprite / bubble 设计值得吸收，但要用适合 `ratatui` 的方式实现，而不是复刻 React 结构。
  - buddy 默认显示应由配置控制；用户可在 `~/.codex/config.toml` 中关闭。
- 明确不采用的方案（如有）：
  - 不继续在现有 `BuddyWidget` 上做零碎 patch，把复杂能力全塞进一行 render。
  - 不在没有清晰状态边界前就引入模型 prompt 注入或“buddy 自己说话”的消息链路。

## 阶段拆分
### 阶段一：参考库深析与状态模型重设计
- 目标：
  - 提炼参考库的骨架生成、稀有度、属性、显示/反应状态边界，并映射到 Rust TUI。
- 交付物：
  - 参考库能力拆解结果。
  - Rust 侧新的 buddy 数据模型设计。
- 完成条件：
  - 明确区分哪些字段由 seed 决定，哪些字段属于运行态/持久化态。
- 依赖：
  - 参考库 `companion.ts`、`types.ts`、`prompt.ts`。

### 阶段二：渲染层升级
- 目标：
  - 为 buddy 增加比当前更丰富的终端可视表达。
- 交付物：
  - 窄屏简版渲染。
  - 宽屏扩展版渲染。
  - 必要的 snapshot。
- 完成条件：
  - 在常见宽度下都能稳定渲染，且不破坏输入区与状态栏布局。
- 依赖：
  - 阶段一的数据模型。

### 阶段三：交互与反馈升级
- 目标：
  - 让 `/buddy pet`、默认显示、状态查看等行为拥有明确的视觉反馈和可解释状态。
- 交付物：
  - 可见 pet 动效/反馈状态。
  - 更清晰的 `/buddy status` 输出。
  - 必要的交互测试。
- 完成条件：
  - 用户可以明显感知 pet 行为不仅改变文案，也改变界面表现。
- 依赖：
  - 阶段二的渲染能力。

### 阶段四：配置与持久化边界收口
- 目标：
  - 为默认显示、用户关闭、可选状态保留建立稳定配置边界。
- 交付物：
  - `[tui]` 下的 buddy 开关与相关 schema。
  - 配置测试与必要文档。
- 完成条件：
  - 用户能通过 `~/.codex/config.toml` 关闭默认 buddy；不会通过配置破坏骨架一致性。
- 依赖：
  - 阶段一的状态边界设计。

## 测试与验证
- 核心验证：
  - `cargo test -p codex-tui buddy -- --nocapture`
  - `cargo test -p codex-tui slash_buddy_show_then_pet_reports_state -- --nocapture`
  - `cargo test -p codex-core tui_config -- --nocapture`
- 必过检查：
  - `just write-config-schema`
  - `just fmt`
  - 对应 crate 的定向测试
- 回归验证：
  - slash command 列表与命令分发
  - bottom pane 布局
  - 有无 buddy 时的状态栏/输入区渲染
- 手动检查：
  - 启动 `codex`，观察默认 buddy 是否出现
  - 执行 `/buddy pet`、`/buddy hide`、`/buddy status`
  - 在窄终端和宽终端下检查 buddy 形态
- 未执行的验证（如有）：
  - 参考库同等功能的逐帧视觉对照尚未完成

## 风险与缓解
- 关键风险：
  - 底部区域布局耦合高，sprite / bubble 一旦设计不当，容易破坏 composer 和状态面板。
- 触发信号：
  - snapshot 大面积变化
  - 输入区高度错乱
  - slash command 或 pending preview 布局异常
- 缓解措施：
  - 先收敛状态模型，再做渲染分层。
  - 窄屏与宽屏分别设计，不强行用一个布局兼容所有场景。
  - 对 buddy 渲染引入独立 snapshot 和底部布局回归测试。
- 回滚/恢复方案（如需要）：
  - 保持 buddy 模块与渲染接线独立，必要时可回退到当前简版实现而不影响 slash command 主链路。

## 参考
- `codex-rs/tui/src/buddy.rs:17`
- `codex-rs/tui/src/chatwidget.rs:5586`
- `codex-rs/tui/src/bottom_pane/mod.rs:1127`
- `/tmp/claude-code-rev.7F9gvv/src/buddy/companion.ts:43`
- `/tmp/claude-code-rev.7F9gvv/src/buddy/companion.ts:124`
- `/tmp/claude-code-rev.7F9gvv/src/buddy/CompanionSprite.tsx:16`
- `/tmp/claude-code-rev.7F9gvv/src/buddy/CompanionSprite.tsx:225`
- `/tmp/claude-code-rev.7F9gvv/src/buddy/prompt.ts:7`
