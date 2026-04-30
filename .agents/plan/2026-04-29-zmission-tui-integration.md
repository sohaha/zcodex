# zmission TUI 集成：消除 stdin 阻塞确认，复用 TUI 渲染

> 不适用项写 `无`，不要留空。
> 只写已确认事实；若信息不清楚或需要假设，先回到对话澄清，不要把未确认内容写进计划。

## 背景
- 当前状态：`codex zmission start/continue` 使用纯文本 `println!` 输出阶段信息，用 `stdin().read_line()` 阻塞式 `[Y/n]` 确认，用户体验割裂且无法与 TUI 生态复用
- 触发原因：用户反馈每次推进都需要手动运行 `codex zmission continue`，体验不好；希望复用 codex-cli TUI，在阶段完成后展示选项让用户回车确认，输出也应通过 TUI 渲染
- 预期影响：`zmission` 命令从独立 CLI 流程升级为 TUI 内嵌交互模式，用户体验从"终端打印 + stdin 阻塞"变为"TUI 渲染 + 事件驱动确认"

## 目标
- 目标结果：zmission 阶段完成后在 TUI 中展示确认选项（继续/暂停），用户回车即推进；阶段输出通过 TUI 渲染而非 println
- 完成定义（DoD）：
  1. `codex zmission start <goal>` 启动后进入 TUI 界面（或在已有 TUI 会话内触发）
  2. 每阶段完成时，TUI 底部弹出选项面板：「继续下一阶段」/「暂停」
  3. 选择「继续」自动推进到下一阶段，选择「暂停」保留状态并退出循环
  4. 阶段信息（状态、提示、出口条件）通过 TUI card/paragraph 渲染
  5. 现有 `codex zmission continue` CLI 路径保持向后兼容
- 非目标：
  - 不改造 `launch_exec_for_phase` 内部的 agent session 逻辑
  - 不改变 mission 状态存储格式
  - 不为 zmission 新建独立 TUI 应用（复用现有 codex TUI）

## 范围
- 范围内：
  - `codex-rs/cli/src/zmission_cmd.rs`：`run_phases_loop` 和 `confirm_continue` 改造
  - `codex-rs/tui/src/app_event.rs`：新增 zmission 相关事件
  - `codex-rs/tui/src/zteam.rs` 或新模块：zmission 阶段状态展示与确认交互
  - `codex-rs/tui/src/bottom_pane/mod.rs`：复用 `show_selection_view` 展示确认选项
- 范围外：
  - `codex_core::mission` 模块（MissionPlanner、MissionPlanningStep 等）
  - `codex exec` 子命令本身
  - zteam autopilot 的自动编排逻辑

## 影响
- 受影响模块：`codex-cli`（zmission_cmd）、`codex-tui`（app_event、zteam、bottom_pane）
- 受影响接口/命令：`codex zmission start`、`codex zmission continue` 的运行时行为
- 受影响数据/模式：无数据格式变更，MissionStateStore 保持不变
- 受影响用户界面/行为：阶段完成后的确认交互从 stdin 文本变为 TUI 弹出选项

## 约束与依赖
- 约束：
  - `codex zmission continue` CLI 路径必须保持可用（非 TUI 环境降级）
  - 不破坏现有 zteam autopilot 的 /zteam 命令体系
  - 遵循 codex-rs 代码规范：无 inline format args、collapsible if、模块 < 500 LoC
- 外部依赖：
  - `codex_core::mission::MissionPlanner` API 保持稳定
  - `ratatui` 和 `crossterm` 已是 codex-tui 的依赖

## 实施策略
- 总体方案：将 `run_phases_loop` 从同步 stdin 确认改为通过 channel 接收 TUI 确认事件；新增 zmission 阶段视图复用 `SelectionView` 组件；CLI 入口增加 `--tui` 标志（默认开启）或自动检测 TUI 可用性
- 关键决策：
  1. **改造点在 `zmission_cmd.rs` 而非 core**：`run_phases_loop` 是 CLI 层逻辑，改它不影响 core API
  2. **复用 `SelectionViewParams` + `show_selection_view`**：TUI 已有成熟的列表选择组件，直接用于「继续/暂停」选项
  3. **通过 `AppEvent` 通道传递确认结果**：新增 `AppEvent::ZmissionPhaseConfirm` 事件，TUI 处理后回写结果到 oneshot channel
  4. **降级路径**：检测到非 TUI 环境时回退到原有 stdin 确认
- 明确不采用的方案：
  - 不在 core 层引入 TUI 依赖
  - 不新建独立的 zmission TUI 二进制
  - 不改造 zteam autopilot 的自动编排逻辑来"包裹" zmission

## 阶段拆分

### 阶段一：事件通道与 confirm_continue 改造
- 目标：将 `confirm_continue` 从 stdin 阻塞改为 channel 事件驱动，支持 TUI 和 CLI 两种模式
- 交付物：
  - 新增 `ConfirmReceiver` trait 或 enum，抽象 TUI/CLI 两种确认方式
  - `run_phases_loop` 接收确认方式参数
  - `app_event.rs` 新增 `ZmissionPhaseComplete { phase_label, next_label, goal }` 事件
- 完成条件：`cargo nextest run -p codex-cli` 通过；CLI 降级路径保持 `[Y/n]` 确认
- 依赖：无

### 阶段二：TUI 阶段确认视图
- 目标：在 TUI 中展示阶段完成信息并提供「继续/暂停」选择
- 交付物：
  - 新模块 `tui/src/zteam/mission_phase_view.rs`（或 `tui/src/zmission_phase_view.rs`）
  - 复用 `SelectionViewParams` 展示两个选项：「继续下一阶段」/「暂停」
  - `app.rs` 中处理 `ZmissionPhaseComplete` 事件，调用 `show_selection_view`
- 完成条件：TUI 中可看到阶段确认弹出面板，选择后正确触发继续或暂停
- 依赖：阶段一

### 阶段三：阶段输出 TUI 渲染
- 目标：将阶段状态、提示、出口条件从 println 改为 TUI 内容渲染
- 交付物：
  - 阶段信息通过 `AppEvent` 发送到 TUI，以 HistoryCell 或 card 形式展示
  - `print_planning_step` 改为发送事件而非直接 println
- 完成条件：TUI 中阶段信息以结构化格式展示（不再是原始文本打印）
- 依赖：阶段一

### 阶段四：集成测试与收尾
- 目标：完整流程验证
- 交付物：
  - 端到端测试：`codex zmission start` → TUI 弹出 → 选择继续 → 下一阶段 → 选择暂停 → 退出
  - `just fmt -p codex-cli -p codex-tui`
  - `just fix -p codex-cli -p codex-tui`
- 完成条件：所有测试通过；CLI 和 TUI 两种路径均可正常工作
- 依赖：阶段一、二、三

## 测试与验证
- 核心验证：`cargo nextest run -p codex-cli` + `cargo nextest run -p codex-tui`
- 必过检查：`just fmt` + `just fix -p codex-cli -p codex-tui`
- 回归验证：现有 `codex zmission continue` CLI 路径在非 TUI 环境下仍可用
- 手动检查：
  1. 运行 `codex zmission start "test goal"` 进入 TUI
  2. 阶段完成后确认弹出面板出现
  3. 选择「继续」推进到下一阶段
  4. 选择「暂停」退出循环并保留状态
  5. 运行 `codex zmission continue` 恢复（CLI 降级路径）
- 未执行的验证：完整 7 阶段 mission 端到端（依赖 agent session 配置）

## 风险与缓解
- 关键风险：
  1. `run_phases_loop` 是 async 函数，与 TUI 事件循环的集成需要 careful async/channel 设计
  2. TUI 的 `show_selection_view` 是同步 UI 组件，需要确认其与 async 后端的交互模式
- 触发信号：编译错误、TUI 无响应、确认事件丢失
- 缓解措施：
  1. 参照现有 `AppEvent::ZteamCommand` 的通道模式设计新事件
  2. 保留 stdin 降级路径作为安全网
  3. 分阶段实现，每阶段独立可验证
- 回滚/恢复方案：每个阶段完成后可通过 git revert 回退；CLI 降级路径确保功能不丢失

## 参考
- `codex-rs/cli/src/zmission_cmd.rs:170` — `run_phases_loop` 函数
- `codex-rs/cli/src/zmission_cmd.rs:241` — `confirm_continue` 函数（stdin 阻塞点）
- `codex-rs/tui/src/zteam.rs:88` — `Command` 枚举（zteam 命令解析模式）
- `codex-rs/tui/src/zteam/autopilot.rs` — `AutopilotState` 状态机（可参考事件驱动模式）
- `codex-rs/tui/src/app_event.rs:115` — `AppEvent::ZteamCommand`（事件总线接入点）
- `codex-rs/tui/src/bottom_pane/mod.rs:871` — `show_selection_view`（可复用的选择视图组件）
 - `codex-rs/tui/src/bottom_pane/list_selection_view.rs` — `SelectionViewParams` + `ListSelectionView`
- `codex-rs/tui/src/zteam/view.rs` — `WorkbenchView`（ratatui 渲染模式参考）
