# ZTeam 的运行中查看应按子命令放通，attach 的 Live 只能建立在 loaded thread 证据上

## 背景

- `ZTeam` 已经在 `tui` 中具备 workbench、start、attach、dispatch 和 relay 入口。
- 首轮实现把 `/zteam` 整个命令族都挂在通用 `available_during_task` 布尔门禁上，同时让 `attach` 通过 `thread/list` / `thread/read` 恢复最近状态。

## 观察

- `/zteam` 不是纯变更命令，它同时承担“打开工作台查看状态”的只读入口。
  如果把整个命令族按 `SlashCommand::Zteam` 粗粒度禁用，运行中连 bare `/zteam` 和 `/zteam status` 也会一起被挡掉，直接破坏 workbench 的核心用途。
- 这类入口不能只在 `ChatWidget` dispatch 层做特判，因为 composer 在 slash 解析阶段就会先做一次 reject。
  运行中可用性必须收敛成共享判断，让 composer 预检和 dispatch 共用。
- `attach` 的“已附着/Live”也不能仅凭 `thread/list` 行或一次 `thread/read` 成功就判定。
  对 ZTeam 这种依赖 live thread 通知持续回流状态的工作台，`Live` 至少要建立在“线程当前仍位于 loaded thread 集合”这类运行态证据上；否则 UI 只是在展示最近状态，不是在表示真正已附着。
- `start` 若要表达“重建 worker”，就不能保留旧 worker 的 live 绑定继续接任务。
  发起新的 start 时应先把当前 slot 置回 `Pending`，直到新的 spawn 事件注册。

## 结论

- 对“兼有只读查看与写操作”的 slash 命令族，运行中门禁不能只停留在命令级布尔值，必须支持子命令级判断。
  ZTeam 的最小正确做法是：
  - bare `/zteam` 允许；
  - `/zteam status` 允许；
  - `/zteam start|attach|frontend|backend|relay` 继续受限。
- 对依赖 live worker 的恢复 UI，`Live` 必须绑定明确的 loaded-thread 证据；否则一律保持 `ReattachRequired`，哪怕最近状态已经恢复出来。
- 对“重新创建 worker”型入口，开始新一轮创建前要先清当前 slot 的连接态，避免任务误发给上一轮 worker。

## 后续默认做法

- 以后在 TUI 里为某个本地模式加 slash 入口时，先区分：
  - 只读查看态；
  - 变更/路由/重建态。
- 如果入口既承担状态查看又承担操作，不要再用单个 `available_during_task` 布尔值覆盖全部子命令。
- 只要 UI 文案含义是“已附着”“已连接”“可继续回流”，就必须先确认背后存在 live runtime 证据，而不是仅有历史恢复结果。
