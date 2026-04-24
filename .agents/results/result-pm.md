status: completed

summary:
- 结论：`ZTeam` 已经具备一个可演示、可恢复、可路由的 TUI-first 本地双 worker MVP，但还不适合作为默认主协作工作流推广。
- 原因不在底层链路缺失，而在产品闭环仍偏弱：`/zteam start` 依赖模型按提示自行调用 `spawn_agent`，成功判定不确定；命令发现面只暴露“打开入口”，没有把 `start/attach/dispatch/relay` 的操作心智产品化；失败与卡住时用户只能看到 pending/workbench，而缺少显式的超时、健康检查和下一步建议。
- 双 worker `frontend/backend` 作为 MVP 是合理的，足以覆盖“界面+接口”类简单全栈任务；但以固定槽位写死在共享模型中，当前并不足以支撑更广泛的真实协作场景。
- federation adapter 当前对真实日常使用帮助很小，主要价值仍是架构接缝和状态展示；它已经形成底座，但还没有形成 ZTeam 的高频工作流收益。

files_changed:
- `.agents/results/result-pm.md`

practicality_assessment:
- 用户价值：中等偏上。对已经熟悉 TUI、多 agent、并且确实在做“前端一条线 + 后端一条线”拆分的用户，`/zteam` 能提供一个比纯手工切线程更顺手的工作台入口。
- 可发现性：中等偏低。命令面和文档面都能找到它，但解释层仍然过薄，用户很难从“打开 ZTeam 协作入口”自然推导出完整工作流。
- 协作闭环：中等。状态回流、恢复和 worker 间 relay 都已经接通，但启动成功、异常反馈、重试建议和完成态判断还不够产品化。
- 推广建议：不应作为默认主路径强推；更适合标成实验性或高阶协作入口，先补工作流反馈再扩大曝光。

priority_findings:
- P0: `/zteam start` 不是确定性“创建两个 worker”的产品动作，而是把一段提示词提交给当前主线程，要求模型自行调用 `spawn_agent`。代码在提交后立即 `mark_zteam_start_requested()` 并打开工作台，但没有校验两个 worker 是否真的创建成功，也没有为“模型没调工具/只调了一个/调错 task_name”建立显式失败路径。对用户来说，这会把“命令成功执行”和“模型可能稍后照做”混成一件事，是当前最大实用性缺口。
  evidence:
  - `codex-rs/tui/src/app.rs:2057`
  - `codex-rs/tui/src/app.rs:2079`
  - `codex-rs/tui/src/zteam.rs:604`
- P1: 命令可发现性不足。slash 列表里 `Zteam` 的描述只有“打开 ZTeam 协作入口”，工作台底部 hint 只显示 `start`、`attach`、`/zteam <worker> <任务>`，没有把 `relay` 暴露出来，也没有说明“这是固定 frontend/backend 双 worker 模式”。用户必须读 `usage()` 或试错才能形成完整心智。
  evidence:
  - `codex-rs/tui/src/slash_command.rs:85`
  - `codex-rs/tui/src/zteam/view.rs:201`
  - `codex-rs/tui/src/zteam.rs:600`
- P1: ZTeam 默认开启，但文档支撑明显不足。配置层把 `zteam_enabled` 默认设为 `true`，而仓库外显文档基本只有 `docs/config.md` 的一个小节；相比之下 federation 已进入 app-server README 的 API 概述。默认暴露一个尚未完全产品化的入口，会放大误用和预期落差。
  evidence:
  - `codex-rs/config/src/types.rs:579`
  - `docs/config.md:63`
  - `codex-rs/app-server/README.md:139`
- P1: federation adapter 对当前真实工作流的帮助接近“可见但不可用”。ZTeam state 会保存 adapter 摘要并在工作台展示，但实际 worker 路由仍全部绑定本地 `/root/frontend|backend` 线程；`WorkerSource::FederationBridge` 甚至还未被真正接入。用户会看到 federation 文案，却无法把它当成 ZTeam 的实际 worker 来源。
  evidence:
  - `codex-rs/tui/src/zteam/worker_source.rs:17`
  - `codex-rs/tui/src/zteam/worker_source.rs:38`
  - `codex-rs/tui/src/zteam.rs:319`
  - `codex-rs/tui/src/zteam/view.rs:53`
- P2: 双 worker 设定适合作为 MVP，但不足以覆盖更真实的协作分工。当前共享模型里只有 `Frontend` 和 `Backend` 两个槽位，展示名甚至写成 `Android 前端` 与 `后端`。这对“登录页 UI + 登录接口”类任务足够，但对 Web 前端、测试、评审、数据迁移、DevOps 等常见子任务没有扩展位，也没有动态 roster。
  evidence:
  - `codex-rs/tui/src/zteam.rs:37`
  - `codex-rs/tui/src/zteam.rs:65`
  - `codex-rs/tui/src/zteam.rs:97`
- P2: 恢复设计本身是对的，但“恢复最近状态”和“恢复 live 会话”仍然太隐性。当前实现已经正确区分 loaded-list 自动恢复与 thread-list attach，并只在 loaded-thread 证据存在时尝试 live attach；这让状态语义比较真实。但用户面只有几条 info message，没有更显式的恢复来源、失败原因、或“为什么现在只能看历史不能继续分派”的解释。
  evidence:
  - `codex-rs/tui/src/app.rs:3765`
  - `codex-rs/tui/src/app.rs:3881`
  - `codex-rs/tui/src/app.rs:3987`
  - `codex-rs/tui/src/app.rs:4043`

worker_roster_assessment:
- 对 MVP 来说，固定 `frontend/backend` 是够的。它让 task name、路由、恢复、工作台展示和错误消息都能围绕同一份共享规格收敛，复杂度可控。
- 对真实高频使用来说，不够。它默认把主线程变成唯一 PM/集成者，缺少测试/评审/运维等并行位，也没有“按项目类型切换 roster”的能力。
- 判断：保留双 worker 作为默认最小配置是对的，但不应把它包装成“通用协作模式已完成”。

federation_assessment:
- 价值：作为本地 adapter seam 是合理的，说明架构边界克制，未来可扩外部 worker。
- 当前收益：低。对 ZTeam 用户来说，它目前只增加一个“外部 adapter”摘要，不能直接改善启动、调度、恢复或结果回流。
- 当前成本：中。虽然实现侵入不大，但它会把用户注意力带向 federation 心智，而当前真正可用的还是本地 subagent 模式。
- 判断：保留，但不应在 ZTeam 价值叙事里占主位。

recommended_priority_order:
1. 先把 `/zteam start` 做成可验证的工作流动作，而不是纯提示词触发。
2. 再补发现面，把 slash 描述、工作台 hint、文档入口统一成“查看状态 / 创建 worker / 再附着 / 分派 / relay”的完整心智。
3. 然后补失败反馈与恢复解释，让 pending、未注册、待再附着、live 四类状态更可操作。
4. 最后再决定是否扩 roster，或把 federation 从“摘要”推进到“可选 worker 来源”。

acceptance_criteria_checklist:
- [x] 基于仓库代码与文档给出 ZTeam 实用性结论
- [x] 覆盖命令可发现性、协作闭环、失败反馈、恢复体验
- [x] 评估双 worker 设定是否足够
- [x] 评估 federation adapter 的真实帮助与成本
- [x] 给出按优先级排序的问题清单
- [x] 提供可追溯的证据路径与行号
