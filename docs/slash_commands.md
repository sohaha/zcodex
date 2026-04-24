# Slash commands

Codex CLI 的斜杠命令总览见官方文档：

- https://developers.openai.com/codex/cli/slash-commands

这个页面只补充当前仓库里新增或本地化的命令行为说明。

## ZTeam

`/zteam` 是 TUI 内的本地协作入口。当前底层仍固定复用两个本地 worker，但推荐心智已经从“手动管理 frontend/backend 双 worker”切到“先给目标，再进入 ZTeam mission 协作”。

当前内部固定的两个 worker 仍然是：

- `frontend`
- `backend`

### 用法

```text
/zteam
/zteam start <目标>
/zteam start
/zteam status
/zteam attach
/zteam <frontend|backend> <任务>
/zteam relay <frontend|backend> <frontend|backend> <消息>
```

### 命令说明

- `/zteam`
  打开 ZTeam 工作台，不会修改运行中的线程状态。
- `/zteam start <目标>`
  推荐主路径。向主线程提交一条带目标的启动指令，要求后续通过 `spawn_agent` 创建两个长期 worker，并围绕这个目标组织第一轮协作。
  当前版本仍复用固定双 worker runtime，但用户应该优先把它理解成“以目标启动 ZTeam”，而不是“先开两个槽位再自己调度”。
- `/zteam start`
  兼容入口。只提交 worker 启动指令，不带 mission 目标。
  固定约束是：
  `task_name = "frontend"`，`agent_type = "frontend-engineer"`
  `task_name = "backend"`，`agent_type = "backend-engineer"`
- `/zteam status`
  打开工作台并查看当前状态摘要、阻塞原因、最近任务和最近结果。
- `/zteam attach`
  扫描仍归属当前主线程的最近 worker 线程，恢复状态，并在可能时重新附着 live 会话。
  如果只恢复到历史线程或部分上下文，Mission Board 会显式进入恢复态 / degraded 语义，提示你继续 `/zteam attach`，或直接用新的 `/zteam start <goal>` 重新建立本轮 mission brief。
- `/zteam frontend <任务>`
  把一条任务直接分派给前端 worker。属于高级手动干预路径，不再是推荐主流程；Mission Board 会把这次操作标记为 manual override，而不是默默绕开 mission 状态。
- `/zteam backend <任务>`
  把一条任务直接分派给后端 worker。属于高级手动干预路径，不再是推荐主流程；Mission Board 会把这次操作标记为 manual override，而不是默默绕开 mission 状态。
- `/zteam relay frontend backend <消息>`
  让前端 worker 向后端 worker 转发消息。属于高级手动干预路径；Mission Board 会把这次 relay 标记为 manual override。
- `/zteam relay backend frontend <消息>`
  让后端 worker 向前端 worker 转发消息。属于高级手动干预路径；Mission Board 会把这次 relay 标记为 manual override。

### 运行中限制

如果主线程当前已经有任务在运行：

- 允许：裸 `/zteam`
- 允许：`/zteam status`
- 禁止：`/zteam start`
- 禁止：`/zteam attach`
- 禁止：`/zteam <worker> <任务>`
- 禁止：`/zteam relay ...`

这是为了保证 workbench 可以随时查看，但不会在一个进行中的主线程 turn 里插入新的协作命令。

### 工作台怎么看

工作台会稳定显示五类核心信息：

- `Mission`：目标、模式、阶段、cycle、整体状态
- `Acceptance`：当前验收项是否已满足、待验证或受阻
- `Worker Assignments`：每个 worker 的连接状态、当前职责、当前分派、最近结果
- `Validation`：当前验证结论、阻塞原因、下一步建议、最近阶段结果
- `Activity`：主线程分派、worker 之间 relay、注册事件、恢复事件

常见状态含义：

- `等待注册`
  已经提交 `/zteam start`，但主线程还没有回流 worker 创建结果。
- `部分注册`
  只注册了一个 worker，另一个 worker 仍未出现。
- `待再附着`
  找到了最近线程，但当前没有 live 连接；通常先用 `/zteam attach`。Mission Board 会把这种状态明确显示为恢复态 / blocked，而不是误报成 live。
- `已附着`
  当前 worker 已连接，可直接分派任务或继续 relay。

### 实际使用案例

#### 案例 1：以目标启动一次协作

适用场景：你已经知道目标，希望先让 ZTeam 进入这次任务的协作上下文。

```text
/zteam start 重做设置页体验，覆盖移动端布局、保存接口和错误提示
```

这条命令当前会：

- 要求主线程创建两个长期 worker
- 把这次目标作为当前 ZTeam 启动上下文带给主线程
- 让后续协作从“围绕目标展开”而不是“先手动记住两个 slot”

#### 案例 2：前后端拆分并行推进

适用场景：你已经知道要把 UI 和接口实现拆开，并希望两个 worker 长时间协作。

```text
/zteam start 重做设置页布局并补齐保存链路
/zteam frontend 重做设置页布局，优先处理移动端断点和表单校验提示
/zteam backend 新增 settings 保存接口，返回统一错误结构，并补一条集成测试
/zteam relay frontend backend 前端需要最终字段名、错误码和必填约束，整理后回我
```

推荐观察点：

- 用 `/zteam` 或 `/zteam status` 看两个 worker 是否都已注册
- 如果只注册了一个 worker，先等 workbench 从“部分注册”转成“已就绪”
- 如果你重开了 TUI，先尝试 `/zteam attach`

#### 案例 3：只让一个 worker 深挖单侧问题

适用场景：问题明显只在某一侧，不值得同时调两个人。

```text
/zteam start 排查登录接口在 token 过期时返回 500
/zteam backend 排查登录接口为什么在 token 过期时返回 500，不要改前端，只给根因和修复
```

这种情况下另一个 worker 会继续留在工作台中待命，不需要额外关闭。

#### 案例 4：恢复一次被中断的协作会话

适用场景：你重开了 TUI，或者 worker 最近线程还在，但当前没有连接。

```text
/zteam attach
/zteam status
```

如果工作台显示“已恢复最近状态，但未重新附着 live 会话”，说明历史状态已经找回，但当前线程连接还没有恢复成功；这时可以继续观察工作台提示，必要时再执行：

```text
/zteam start <目标>
```

不带 `<目标>` 的 `/zteam start` 仍可用，但它现在只是兼容入口；推荐优先重新用带目标的形式启动新一轮协作。

### 实用建议

- 先给目标，再看工作台。`/zteam start <目标>` 提交后，最常见的问题不是命令失败，而是主线程还没真正完成 worker 创建。
- 如果必须手动干预，再使用 `frontend/backend/relay`。这些子命令仍然可用，但不再是推荐主路径。
- 让 `relay` 只传“需要同步的事实”，不要让 worker 互相发完整长需求。
- 如果工作台长时间停在“等待注册”或“部分注册”，优先检查主线程是否真的调用了 `spawn_agent`；不要马上重复塞更多分派命令。
