# ZTeam Mission V2 产品契约

## 结论

- `ZTeam` 采用 `mission-first, dual-runtime-second` 契约。
- 固定 `frontend/backend` 双 worker 继续保留，但只作为 `tui` 内部 runtime 底座、恢复锚点和默认协作者集合，不再是主要用户心智。

## 默认入口

- 推荐主路径：`/zteam start <goal>`
- 兼容入口：`/zteam start`
- 高级手动干预：`/zteam frontend ...`、`/zteam backend ...`、`/zteam relay ...`

其中：

- `/zteam start <goal>` 负责建立本轮 mission brief，并启动 `Mission Autopilot` 驱动 Mission Board。
- `/zteam start` 只保留为不带目标的兼容启动入口。
- 旧手动命令不会绕开 mission 状态；它们必须被收口为 manual override 语义。

## 状态面约束

- 用户主视图是 Mission Board，至少稳定呈现：
  - `Mission`
  - `Acceptance`
  - `Worker Assignments`
  - `Validation`
  - `Activity`
- `goal`、`mode`、`phase`、assignment、acceptance、validation、blocker、`next_action` 是 mission 层真相源。
- `Mission Autopilot` 也是稳定产品面的一部分，至少要让用户看见：
  - 当前 `cycle`
  - `pending_auto_action`
  - `waiting_on`
  - 最近一次自动动作结果
  - `repair` / `manual_override` 状态

## 恢复与降级约束

- 若只恢复到历史上下文而未 live attach，状态必须保持恢复态 / blocked；不能误报成 live attach 成功。
- 若没有历史 mission 但恢复到了 worker 状态，应自动合成 recovery mission。
- 若没有 mission 却使用旧手动命令，应自动合成 manual override mission。
- 若已有 mission 再触发旧手动命令，应在现有 mission 中显式记录 manual override，而不是静默覆盖。

## Autopilot 约束

- 自动推进只负责决定“下一步什么时候触发”，不在 TUI 里硬编码业务拆分；具体 cycle 规划、分派、归纳和验证仍通过 root agent follow-up turn 完成。
- root 级 autopilot 动作必须锚定 primary thread；不能因为当前用户停在某个 worker thread，就把 root follow-up 误投到该 worker 线程。
- `manual override` 只能中断当前自动动作，不能压住 `repair`；worker 缺失或掉线时，系统仍应优先进入 attach-first repair。
- attach-first repair 成功后，后续动作必须按当前 cycle 真实状态恢复：需要重新规划时回到 `plan_cycle`，仍可继续当前轮时再进入 `dispatch_cycle`。
- repair 默认面向当前 mission 明确缺失的 required workers，不应在缺少 `waiting_on` 细节时无差别重建 `ALL`。

## 明确不做

- 不把 `ZTeam` 扩展成动态 `N` worker 平台。
- 不引入第三个 verifier worker。
- 不为 Mission V2 新增公共协议或独立 mission store。
- 不把 federation 重构成主路径产品心智；federation 仍是本地 adapter seam。
