# `/zmission` 的 bare slash 入口应像 `/buddy` 一样在 dispatch 层打开菜单

## 现象

- 用户直接输入 `/zmission` 时，当前实现会走 `Command::parse("") -> Status`，只输出状态，而不是像 `/buddy` 一样弹出可选菜单。
- 这会让 `zmission` 的 discoverability 明显低于同类 TUI 本地工作流入口，也让用户必须先记住 `start/status/continue/view/reset` 这些子命令。

## 根因

- `/zmission` 的“无参数默认行为”被编码在 `zmission_command::Command::parse()` 里，空参数直接映射为 `Status`。
- 但 TUI 里 bare slash command 的产品化入口其实在 `ChatWidget::dispatch_command()` 这一层；`/buddy` 已经在那里把无参行为收口成 `open_buddy_menu()`。
- 如果去改 parser，把空参数改成某个新枚举，只会把菜单语义扩散到 app 层；更小、更稳的 seam 是保留带参解析不变，只让 bare `/zmission` 在 dispatch 层直接打开菜单。

## 修复策略

- 在 `ChatWidget` 本地新增 `open_zmission_menu()`，菜单项直接发送现有 `AppEvent::ZmissionCommand(...)`：
  - `Start { goal: None }`
  - `Status`
  - `Continue { note: None }`
  - `View`
  - `Reset`
- 只把 `SlashCommand::Zmission` 的 bare dispatch 从“直接发 `Status`”改为“打开 `zmission-menu`”。
- 保持 `/zmission status`、`/zmission start ...` 等 inline args 继续走原有 parser 和 app handler，不改 Mission 状态机。

## 经验

- TUI 中“命令可发现性”的默认行为，优先检查 `dispatch_command()` 是否已有同类入口模式，不要先改 parser。
- 对于带子命令的本地工作流，如果 bare slash 的目标是引导用户而不是执行单一动作，菜单通常比默认落到某个子命令更合适。
- 用户可见 popup 改动应补 snapshot 测试，但 dirty 分支上的既有编译错误可能先阻断 snapshot 生成；这类阻断要在结果里明确拆分为“本次逻辑已完成”和“仓库基线未通过”。
