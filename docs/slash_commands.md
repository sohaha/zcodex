# Slash commands

Codex CLI 的斜杠命令总览见官方文档：

- https://developers.openai.com/codex/cli/slash-commands

这个页面只补充当前仓库里新增或本地化的命令行为说明。

## ZTeam

`/zteam` 是 TUI 内的本地双 worker 协作入口。它不是动态 roster，也不是独立的公共多实例协议面；当前固定只管理两个 worker：

- `frontend`
- `backend`

### 用法

```text
/zteam
/zteam start
/zteam status
/zteam attach
/zteam <frontend|backend> <任务>
/zteam relay <frontend|backend> <frontend|backend> <消息>
```

### 命令说明

- `/zteam`
  打开 ZTeam 工作台，不会修改运行中的线程状态。
- `/zteam start`
  向主线程提交一条启动指令，要求主线程继续通过 `spawn_agent` 创建两个长期 worker。
  固定约束是：
  `task_name = "frontend"`，`agent_type = "frontend-engineer"`
  `task_name = "backend"`，`agent_type = "backend-engineer"`
- `/zteam status`
  打开工作台并查看当前状态摘要、阻塞原因、最近任务和最近结果。
- `/zteam attach`
  扫描仍归属当前主线程的最近 worker 线程，恢复状态，并在可能时重新附着 live 会话。
- `/zteam frontend <任务>`
  把一条任务直接分派给前端 worker。
- `/zteam backend <任务>`
  把一条任务直接分派给后端 worker。
- `/zteam relay frontend backend <消息>`
  让前端 worker 向后端 worker 转发消息。
- `/zteam relay backend frontend <消息>`
  让后端 worker 向前端 worker 转发消息。

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

工作台会显示四类核心信息：

- 概况：整体状态、阻塞提示、是否需要重新附着
- Worker 面板：每个 worker 的连接状态、线程 id、最近任务、最近结果
- 任务板：当前最近一次分派到各 worker 的任务
- 消息流：主线程分派、worker 之间 relay、注册事件、恢复事件

常见状态含义：

- `等待注册`
  已经提交 `/zteam start`，但主线程还没有回流 worker 创建结果。
- `部分注册`
  只注册了一个 worker，另一个 worker 仍未出现。
- `待再附着`
  找到了最近线程，但当前没有 live 连接；通常先用 `/zteam attach`。
- `已附着`
  当前 worker 已连接，可直接分派任务或继续 relay。

### 实际使用案例

#### 案例 1：前后端拆分并行推进

适用场景：你已经知道要把 UI 和接口实现拆开，并希望两个 worker 长时间协作。

```text
/zteam start
/zteam frontend 重做设置页布局，优先处理移动端断点和表单校验提示
/zteam backend 新增 settings 保存接口，返回统一错误结构，并补一条集成测试
/zteam relay frontend backend 前端需要最终字段名、错误码和必填约束，整理后回我
```

推荐观察点：

- 用 `/zteam` 或 `/zteam status` 看两个 worker 是否都已注册
- 如果只注册了一个 worker，先等 workbench 从“部分注册”转成“已就绪”
- 如果你重开了 TUI，先尝试 `/zteam attach`

#### 案例 2：只让一个 worker 深挖单侧问题

适用场景：问题明显只在某一侧，不值得同时调两个人。

```text
/zteam start
/zteam backend 排查登录接口为什么在 token 过期时返回 500，不要改前端，只给根因和修复
```

这种情况下另一个 worker 会继续留在工作台中待命，不需要额外关闭。

#### 案例 3：恢复一次被中断的协作会话

适用场景：你重开了 TUI，或者 worker 最近线程还在，但当前没有连接。

```text
/zteam attach
/zteam status
```

如果工作台显示“已恢复最近状态，但未重新附着 live 会话”，说明历史状态已经找回，但当前线程连接还没有恢复成功；这时可以继续观察工作台提示，必要时再执行：

```text
/zteam start
```

`/zteam start` 的语义是重新发起一轮 worker 创建，不是单纯刷新状态。

### 实用建议

- 先看工作台，再分派任务。`/zteam start` 提交后，最常见的问题不是命令失败，而是主线程还没真正完成 worker 创建。
- 让 `relay` 只传“需要同步的事实”，不要让 worker 互相发完整长需求。
- 如果工作台长时间停在“等待注册”或“部分注册”，优先检查主线程是否真的调用了 `spawn_agent`；不要马上重复塞更多分派命令。
