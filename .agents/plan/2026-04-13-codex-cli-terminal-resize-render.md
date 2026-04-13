# codex-cli 终端尺寸变更后渲染异常复查

## 背景
- 当前状态：用户报告 `codex-cli` 在终端尺寸变化后出现渲染异常；截图显示同一段界面内容在新旧布局中同时可见，存在旧 frame 残留或 viewport/history 错位迹象。仓库内已有同类 `tmux` resize 渲染问题的进行中 Cadence 产物，说明该链路此前已被确认存在不稳定行为。
- 触发原因：终端尺寸变化会通过 `Event::Resize(_, _)` 触发一次 `TuiEvent::Draw`，随后走 `Tui::draw()` 中的 `pending_viewport_area()`、`update_inline_viewport()` 与 `Terminal::autoresize()` / `Terminal::clear()` 协同路径；当前链路里 viewport 重定位、缓冲区失效和清屏边界都与本次现象直接相关。
- 预期影响：修复后，`codex-cli` 在终端尺寸变化后不应出现重复卡片、旧边框残留、header/history 错位或局部未重绘。

## 目标
- 目标结果：定位并修复终端尺寸变化后的渲染异常根因，收敛到一个最小且可验证的 `codex-tui` 修复。
- 完成定义（DoD）：确认具体根因；完成最小代码修复；新增覆盖 resize 后残留/错位的自动化回归测试；受影响 crate 验证通过；若用户可见布局输出发生变化，则同步更新对应快照。
- 非目标：无意重做整个 TUI 布局系统；无意顺带处理与 resize 无关的渲染问题；无意引入仅针对单一终端的长期兼容分支。

## 范围
- 范围内：`codex-rs/tui` 中 resize 事件后的 viewport 重定位、缓冲区失效、清屏/重绘逻辑；必要的测试 backend 或 TUI 回归测试。
- 范围外：`core`、`cli`、`app-server`、模型/会话逻辑、非 resize 触发的 UI 缺陷。

## 影响
- 受影响模块：`codex-rs/tui/src/tui.rs`、`codex-rs/tui/src/custom_terminal.rs`、`codex-rs/tui/src/test_backend.rs`、`codex-rs/tui/tests` 或相邻 TUI 测试模块。
- 受影响接口/命令：`codex` 的终端交互界面；无新增外部命令或协议接口。
- 受影响数据/模式：无持久化数据、schema 或配置结构变更。
- 受影响用户界面/行为：inline 终端界面在 resize 后的 header/history/消息区重绘与定位行为。

## 约束与依赖
- 约束（时间/兼容性/安全/性能/发布窗口等）：保持最小必要改动；不通过静默 fallback 或无限制全量重绘掩盖问题；修复后不能破坏普通终端与已有 `tmux` 路径；遵守仓库既有 TUI、snapshot 与验证规范。
- 外部依赖（系统/人员/数据/权限等）：无。

## 实施策略
- 总体方案：先把当前用户截图与现有 resize 渲染链路、已有 `tmux` 计划/issue 对齐，确认是否为同一根因或同一路径的新症状；随后在 viewport 重定位、缓冲区失效与清屏边界中修复真正造成残留的点；最后补一个可重复的 resize 回归测试来锁住行为。
- 关键决策：
  - 以 `codex-rs/tui` 的通用 resize 路径为主战场，不把问题收窄成单一终端私有 bug。
  - 优先补可确定复现的自动化回归测试，而不是只依赖手工观察。
  - 先延续并复核现有 `tmux` resize 线索，再决定是否需要扩大到更通用的 terminal backend 行为修复。
- 明确不采用的方案（如有）：
  - 不仅靠终端类型判断或用户配置绕过问题。
  - 不以“每次 resize 后无条件全屏重置”为默认修法，除非根因证明这是唯一正确且副作用可接受的方案。
  - 不在本轮顺带重构无关的 TUI 布局或 history 插入体系。

## 阶段拆分
### 现象对齐与根因确认
- 目标：把当前截图症状与既有 resize 路径、既有 `tmux` issue 现状对齐，确认真正的失效点。
- 交付物：明确的代码切入点、复现/观察依据、根因判断。
- 完成条件：能够说明残留/错位是如何沿着 resize → viewport → clear/invalidate 路径发生的，以及与既有 issue 的关系。
- 依赖：用户截图、现有源码、已有 Cadence 产物。

### 修复实现与回归测试
- 目标：完成最小修复，并把 resize 后残留/错位写成稳定回归测试。
- 交付物：代码改动、测试改动、必要的 snapshot 更新。
- 完成条件：自动化测试能覆盖本次 resize 渲染异常；实现只修改任务相关链路。
- 依赖：现象对齐与根因确认。

### 验证与收尾
- 目标：完成格式化、测试、自审和结果整理。
- 交付物：验证结果、风险说明、必要时的提交。
- 完成条件：满足仓库要求的最小验证闭环，并明确剩余风险或未执行验证。
- 依赖：修复实现与回归测试。

## 测试与验证
- 核心验证：在 `codex-rs` 目录运行 `cargo nextest run -p codex-tui`；若本地缺少 `cargo-nextest`，则运行 `cargo test -p codex-tui`。
- 必过检查：在 `codex-rs` 目录运行 `just fmt`；若改动规模达到 lint 修复阈值，则运行 `just fix -p codex-tui`。
- 回归验证：新增或更新一个能模拟 resize 后残留/错位的 TUI 测试；若输出快照变化，审阅并接受仅与本任务相关的 snapshot。
- 手动检查：在 `codex-cli` 会话中调整终端宽高，确认消息卡片、header 和底部区域不会重复显示或残留旧边框。
- 未执行的验证（如有）：无。

## 风险与缓解
- 关键风险：修复可能过度清屏，造成普通终端闪烁、scrollback 丢失或已有 inline viewport 行为回退。
- 触发信号：相关测试或 snapshot 大范围变化；resize 后出现明显跳屏、history 丢失或新的布局错位。
- 缓解措施：把改动限制在 resize 相关路径；优先用回归测试锁定症状；仅在确有证据时扩大清理范围。
- 回滚/恢复方案（如需要）：若修复引入更广泛回归，可回退本轮 resize 相关改动并重新拆分为更小的 viewport / clear 修补策略。

## 参考
- `.agents/plan/2026-04-06-tmux-resize-render.md`
- `.agents/issues/2026-04-06-tmux-resize-render.toml`
- `codex-rs/tui/src/tui/event_stream.rs:247`
- `codex-rs/tui/src/tui.rs:505`
- `codex-rs/tui/src/tui.rs:595`
- `codex-rs/tui/src/tui.rs:641`
- `codex-rs/tui/src/custom_terminal.rs:258`
- `codex-rs/tui/src/custom_terminal.rs:415`
- `codex-rs/tui/src/test_backend.rs:19`
- `codex-rs/tui/tests/suite/vt100_history.rs:25`
