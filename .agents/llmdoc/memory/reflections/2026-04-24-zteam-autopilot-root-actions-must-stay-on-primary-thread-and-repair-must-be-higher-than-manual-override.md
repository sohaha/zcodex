# ZTeam Autopilot 的 root 动作必须锚定 primary thread，repair 优先级必须高于 manual override

## 背景

- 这轮 `Mission Autopilot` 已经把 `/zteam start <goal>` 后的 bootstrapping、首轮派工、结果归纳、自动验证和 attach-first repair 接进 `codex-rs/tui/src/zteam.rs` 与 `app.rs`。
- 第一版实现跑通主路径后，又暴露出两个容易被漏掉的收口洞：
  - root follow-up prompt 如果沿用当前 active thread，用户在 worker thread 里观察时会把 autopilot 的 root 动作误投到 worker 线程；
  - `manual override_active` 如果直接压住后续自动动作，会把真正更高优先级的 repair 一起压没，worker 掉线后系统就卡在“已人工接管”的假稳定态。

## 本轮有效做法

- 在 `App` 层显式区分 `zteam_root_thread_id()` 与当前 active thread，并为 root autopilot 动作走单独的 `submit_zteam_root_text_turn(...)` 路径，确保 root prompt 永远回到 primary thread。
- `zteam.rs` 里与 root marker、root turn completed 相关的判断统一收窄到 primary thread，不再把“任意非 worker 线程”误当成 root 线程。
- `manual override` 只中断当前 cycle 的自动派工/归纳，不拦截 repair 分支；一旦 worker 缺失，仍优先进入 attach-first repair。
- attach-first repair 成功后，不再无脑继续 `DispatchCycle`，而是按当前 cycle 状态决定回 `PlanCycle` 还是继续 `DispatchCycle`。
- `apply_summary_result(... status = \"repair\")` 在缺少 `waiting_on` 细节时，默认 repair 当前 mission 的 required workers，而不是扩大成 `ALL`。

## 关键结论

- `ZTeam` 这类 mission 编排面里，root/autopilot 与 worker/user 线程不是“当前在哪个线程看 UI 就投给哪个线程”的关系；root 动作必须有单独的 primary-thread 锚点。
- `manual override` 的语义是“打断当前自动推进并显式接管”，不是“冻结所有系统级恢复动作”；repair 是运行时健康恢复，优先级高于人工派工覆盖。
- repair 完成后的恢复动作不能写死为“继续派工”；它取决于当前 cycle 是否已经失效。否则 repair 会把旧计划强行续上，造成 cycle 语义漂移。

## 后续默认做法

- 以后继续给 `ZTeam` 加自动动作时，先问“这是 root 语义还是 worker 语义”，并把 root 语义显式锚定到 primary thread。
- 以后再引入新的 interrupt / pause / override 状态时，先明确它是否应影响 repair；默认不要让它挡住 attach-first recovery。
- 评审 `ZTeam` 自动编排改动时，若看到“当前 thread 直接决定 root prompt 投递目标”或“manual override 全局压住 repair”，默认视为高风险回归信号。
