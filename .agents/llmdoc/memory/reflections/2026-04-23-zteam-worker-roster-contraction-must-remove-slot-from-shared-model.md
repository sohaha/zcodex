# ZTeam worker roster 收缩时，必须从共享模型整体删槽位，不能只在 UI 层隐藏

## 背景

- ZTeam 先前存在 `frontend / ios / backend` 三槽位模型。
- 新需求把 `/zteam start` 收敛回只创建 `frontend` 和 `backend` 两个长期 worker。
- 实现曾停在“prompt 只创建两个 worker，但内部状态机、恢复、adapter、usage、测试和快照仍保留 `ios`”的半收敛状态。

## 观察

- 这类回归和“新增 worker 忘同步”本质相同，都是 **worker roster 没有被当作单一共享规格维护**。
- 如果只是让 `view` 或 workbench 默认不展示旧槽位：
  - slash parser 仍会接受旧 worker；
  - `missing_worker_message` 仍会指导用户等待永远不会再被创建的 canonical task；
  - recovery / attach 仍会尝试恢复旧槽位；
  - federation adapter summary 仍会对外暴露旧 worker；
  - 测试和快照继续把旧槽位当成正式合同。
- 结果就是产品表面看似已经收敛，内部却保留一条默认流程永远无法满足的死入口。

## 结论

- 当 ZTeam worker roster **收缩** 时，不能做“UI 隐藏 + 状态机保留”的半步改造。
- 必须把旧槽位从共享模型整体移除，再让以下触点统一跟随：
  - `WorkerSlot` / `task_name` / `role_name` / `display_name`
  - `usage` / parser / 用户提示
  - `start_prompt`
  - recovery / attach / worker source / adapter summary
  - slash tests / snapshot tests

## 后续默认做法

- 任何 ZTeam roster 调整，无论扩容还是缩容，都先修改共享 worker 规格，再逐层同步调用点。
- 如果产品目标已经明确移除某个 worker，就直接删掉对应槽位，而不是继续保留“也许以后还会用”的 dormant state。
