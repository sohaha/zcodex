# ZTeam worker roster 必须收敛到单一共享规格，避免工作台、恢复和提示各自写死

## 背景

- ZTeam 工作台最初只把 `frontend/backend` 当成固定双 worker。
- 真实协作流已经出现第三个长期 worker：`/root/ios`，并且用户会继续从工作台查看、恢复和分派它。
- 结果是渲染层、slash 用法、恢复逻辑、adapter summary 和启动 prompt 出现事实分裂：有的地方显示两个 worker，有的地方实际已经有三个。

## 观察

- 这类问题的根因不是“少补一个字符串”，而是 **worker roster 没有单一真相源**。
- 如果 `task_name`、展示名、角色名和默认 worker 集合分散在多个文件里，任何新增 worker 都会漏改至少一处：
  - `start_prompt` 会继续创建旧集合；
  - `usage` / hint 仍然提示旧命令；
  - `view` 还按旧槽位渲染；
  - `recovery` 只会恢复旧集合；
  - `federation adapter` summary 也会漏掉新 worker。
- ZTeam 仍应维持“固定 canonical task name 识别本地 worker”的约束，不要为了去掉双槽位写死而退化成对任意 subagent 宽匹配。

## 结论

- ZTeam 需要一个集中式 worker 规格层，至少统一：
  - `task_name`
  - 展示名
  - 角色名
  - 默认启动集合
- 后续涉及 ZTeam worker 扩容时，应优先检查并联动以下触点：
  - slash parser / usage
  - start prompt
  - workbench view
  - recovery / attach
  - worker source / adapter summary
  - 对应 snapshot tests

## 后续默认做法

- 新增或调整 ZTeam worker 时，先改共享 worker 规格，再让视图、恢复和提示全部按这份规格迭代。
- 除非产品目标真的改成“任意动态 worker 编排”，否则不要放弃固定 canonical task name 这一身份边界。
