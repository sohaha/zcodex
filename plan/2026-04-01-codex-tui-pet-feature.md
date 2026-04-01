# 为 Codex Rust TUI 增加宠物功能

## 背景
- 当前状态：`claude-code-rev` 已有一套 `buddy/companion` 体系，包含基于稳定种子的宠物生成、底部区域精灵渲染、`pet` 动画状态，以及与 `/buddy` 命令关联的交互入口；本仓库当前活跃终端实现是 `codex-rs/tui_app_server`，现有 TUI 仅有斜杠命令、状态栏与底部输入区，没有宠物功能。
- 触发原因：用户要求先分析 `claude-code-rev` 里的宠物逻辑，再把类似能力添加到我们当前的 Codex CLI。
- 预期影响：为 Rust TUI 增加可见宠物、可执行的宠物命令和必要的持久化配置，同时保持现有命令分发、底部渲染和配置写回链路稳定。

## 目标
- 目标结果：在 `codex-rs/tui_app_server` 中新增一版宠物功能，用户可通过 `/buddy` 命令孵化/查看/抚摸/显隐宠物，并在底部区域看到稳定、可复现的宠物展示与短暂交互反馈。
- 完成定义（DoD）：Rust TUI 有新的 `/buddy` 命令入口；宠物状态可在当前会话展示并支持最小必要持久化；底部区域会渲染宠物或其紧凑形态；至少补齐针对命令分发、状态渲染或配置持久化的自动化测试/快照；按仓库规范完成 `fmt` 和定向测试。
- 非目标：不复刻 `claude-code-rev` 的完整 observer 气泡评论链路；不修改旧版 TypeScript `codex-cli`；不引入新的远程服务、MCP 接口或跨进程协议扩展。

## 范围
- 范围内：
  - 分析 `claude-code-rev` 中宠物生成、渲染和 `/buddy` 相关逻辑，提炼可迁移子集。
  - 在 Rust TUI 中增加宠物相关命令、运行态状态和底部 UI 展示。
  - 如需跨会话保留宠物信息，在现有 `[tui]` 配置下增加受控字段，并同步 schema 与写回逻辑。
  - 为新增 UI/命令补充对应测试与快照。
- 范围外：
  - 旧 TS CLI 目录的功能对齐。
  - 将宠物引入 app-server v2 API、core 协议消息或工具调用协议。
  - 让主模型感知宠物人格并输出伴随对话内容。

## 影响
- 受影响模块：
  - `codex-rs/tui_app_server` 的斜杠命令、`ChatWidget` 命令分发、底部渲染与相关测试。
  - `codex-rs/core` 的 TUI 配置类型、配置编辑辅助和 schema（仅在需要持久化时）。
- 受影响接口/命令：
  - 新增 `/buddy` 命令及其参数分支。
  - 可能新增或扩展 `[tui]` 下的宠物配置键。
- 受影响数据/模式：
  - TUI 本地配置结构。
  - TUI 运行时宠物状态与动画时间戳。
- 受影响用户界面/行为：
  - 底部输入区/状态区增加宠物展示。
  - 输入 `/buddy` 后新增宠物交互反馈与可见状态变化。

## 约束与依赖
- 约束（时间/兼容性/安全/性能/发布窗口等）：
  - 需要遵守 `tui_app_server/src/bottom_pane/AGENTS.md` 的 bottom pane 文档同步要求；若修改 `chat_composer` 状态机或其文档假设，必须同步 `docs/tui-chat-composer.md`。
  - 若修改 `ConfigToml` 或其嵌套类型，需要运行 `just write-config-schema`。
  - 需要保持改动局部化，优先复用现有斜杠命令和底部渲染管线，不扩大到旧 TS 实现或 app-server 协议面。
- 外部依赖（系统/人员/数据/权限等）：
  - 无额外外部服务依赖。
  - 依赖仓库现有 Rust 工具链、`just fmt` 与 crate 定向测试可执行。

## 实施策略
- 总体方案：
  - 以 `claude-code-rev` 的设计为参考，只迁移“稳定宠物身份 + TUI 可视化 + `/buddy` 交互”这条最短闭环。
  - 在 Rust TUI 内新增独立宠物模块，封装宠物生成、展示文案、交互冷却/动画状态，避免把逻辑散落到 `chatwidget.rs` 和 `footer.rs`。
  - 通过 `SlashCommand` 和 `ChatWidget::dispatch_command` 接入 `/buddy`；通过 bottom pane/footer 渲染链接入紧凑宠物视图；仅当跨会话保留确有必要时再扩展 `[tui]` 配置。
- 关键决策：
  - 功能实现以 Rust TUI 为主，不在旧 `codex-cli` 目录做重复开发。
  - 首版不实现基于转录内容的 observer 发言与模型提示注入，避免把功能扩散到消息协议和采样链路。
  - 若加入持久化，只保存宠物“灵魂”或显隐偏好等最小字段，外观仍由稳定种子推导，避免配置篡改影响稀有度/外观一致性。
- 明确不采用的方案（如有）：
  - 不直接照搬 `claude-code-rev` 的 React/Ink 组件结构。
  - 不把宠物功能做成独立网络服务、插件或 MCP 工具。
  - 不在首版引入完整多行 ASCII 精灵加气泡评论的全量复刻方案，避免对现有底部布局造成过大扰动。

## 阶段拆分
### 阶段一：落定接入面与数据模型
- 目标：确定 Rust TUI 中宠物状态、命令入口和可选持久化字段的最小闭环。
- 交付物：宠物模块骨架、命令枚举与分发接入、必要的配置类型设计。
- 完成条件：`/buddy` 命令可被识别并路由到明确处理逻辑；宠物状态来源与存储边界明确。
- 依赖：现有 `SlashCommand`、`ChatWidget` 命令分发、`[tui]` 配置结构。

### 阶段二：实现底部宠物展示与交互反馈
- 目标：在不破坏现有底部布局的前提下显示宠物，并对 `/buddy` 交互给出可见反馈。
- 交付物：宠物渲染组件/辅助函数、交互后状态更新、必要的紧凑布局处理。
- 完成条件：宠物在常见终端宽度下稳定显示；执行 `/buddy` 相关操作后可观察到展示或状态变化。
- 依赖：阶段一的数据模型与命令入口。

### 阶段三：验证、文档与收尾
- 目标：补齐测试、快照、格式化及必要文档/配置产物。
- 交付物：定向测试、更新后的快照、必要文档与 schema 产物。
- 完成条件：相关测试通过；如改动了配置类型或 bottom pane 状态机，配套文档与 schema 已同步。
- 依赖：前两阶段实现完成。

## 测试与验证
- 核心验证：
  - `cargo nextest run -p codex-tui` 或在无 `cargo-nextest` 时运行 `cargo test -p codex-tui`。
  - 若修改 `codex-rs/core` 配置类型，再补充相关 crate 的定向测试。
- 必过检查：
  - 在 `codex-rs` 目录执行 `just fmt`。
  - 若修改 `ConfigToml` 或其嵌套类型，执行 `just write-config-schema`。
- 回归验证：
  - 斜杠命令弹窗与命令分发未破坏现有命令。
  - 底部状态栏/输入区在有无宠物、宽窄终端下均可正常渲染。
- 手动检查：
  - 启动 TUI，输入 `/buddy` 相关命令，确认宠物出现、状态变化和错误提示符合预期。
  - 切换终端宽度，确认底部区域没有明显错位或截断。
- 未执行的验证（如有）：
  - 无。

## 风险与缓解
- 关键风险：`chatwidget.rs` 与 bottom pane 是高接触区域，直接堆逻辑容易引起布局回归和状态耦合。
- 触发信号：快照大面积变化、底部状态栏消失、现有斜杠命令行为异常。
- 缓解措施：新增独立宠物模块；将命令分发、状态更新、渲染拆开；优先增加定向快照覆盖。
- 回滚/恢复方案（如需要）：宠物状态与渲染保持独立接线，必要时可只回退 `/buddy` 接入与渲染模块，不影响现有命令和状态栏主链路。

## 参考
- `/tmp/claude-code-rev.7F9gvv/src/buddy/companion.ts:15`
- `/tmp/claude-code-rev.7F9gvv/src/buddy/CompanionSprite.tsx:152`
- `/workspace/codex-rs/tui_app_server/src/slash_command.rs:7`
- `/workspace/codex-rs/tui_app_server/src/chatwidget.rs:4741`
- `/workspace/codex-rs/core/src/config/types.rs:761`
- `/workspace/codex-rs/tui_app_server/src/bottom_pane/AGENTS.md:1`
