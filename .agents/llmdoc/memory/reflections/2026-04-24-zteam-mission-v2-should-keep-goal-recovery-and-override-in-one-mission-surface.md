# ZTeam Mission V2 应把目标、恢复态和手动 override 收敛到同一个 mission surface

## 背景

- 这轮 `ZTeam Mission V2` 重构前，产品心智还停在“固定 frontend/backend 双 worker 控制台”。
- 现有 runtime 底座其实已经够用：固定 task name、线程通知回流、`attach` 恢复、worker 间 relay 都在；真正缺的是把这些能力收口成面向目标的工作流。

## 观察

- 如果只把 `/zteam start <goal>` 做成新命令，而不把本地状态、工作台和恢复语义一起切到 mission-first，用户仍会掉回旧的 slot/status 心智。
- 恢复态和旧 `frontend/backend/relay` 命令不能成为 mission 旁路：
  - 部分恢复时，如果没有历史 mission，也要合成 recovery mission，把 blocker、validation summary、next action 放进同一个 Mission Board。
  - 旧手动命令如果发生在没有 mission 的上下文里，也要合成 manual override mission；如果已有 mission，则显式把这次行为标记为 manual override，而不是静默覆盖当前状态。
- 只在 live attach 成功时标记 `Live` 仍然是硬边界；历史恢复只能展示恢复态 / blocked，上层产品语义不能为了“看起来已恢复”而稀释这个约束。
- 文案也要跟着 contract 走：在 mission 已存在的阻塞/重建提示里，默认应推荐 `/zteam start <goal>`，兼容入口 `/zteam start` 只在无 mission 上下文时再出现。

## 结论

- `ZTeam` 的稳定产品契约应是：
  1. 默认入口是 `/zteam start <goal>`；
  2. Workbench 主视图是 Mission Board，而不是双 worker 状态板；
  3. recovery 和 manual override 都必须进入 mission state，而不是绕开它。
- 以后继续演进 `ZTeam` 时，优先判断“这次状态是否仍能在 Mission Board 中解释清楚”，而不是优先判断“slot 状态是否还能补一个分支”。

## 后续默认做法

- 新增恢复、降级或手动干预语义时，先决定要合成哪类 mission 状态，再决定底层连接状态如何映射。
- 新增用户提示时，先判断当前是否已有 mission；有 mission 时默认推荐 `/zteam start <goal>`，无 mission 时才推荐兼容入口 `/zteam start`。
- 评审 `ZTeam` 相关改动时，若发现旧命令或恢复逻辑能绕开 Mission Board，默认视为架构回退信号。
