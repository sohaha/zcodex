# ZTeam attach 在尝试 live reattach 前，必须先收敛为当前 primary 的最新 descendant worker

## 背景

- `ZTeam` 的显式 `/zteam attach` 需要做两件事：
  - 从 `thread/list` / `thread/read` 恢复最近 worker 状态；
  - 对仍在 loaded 集合里的线程走真正的 live reattach seam。
- 第二轮实现为了把 `attach` 接到 `attach_live_thread_for_selection(...)`，把候选线程改成逐条恢复。

## 观察

- `thread/list` 会返回当前会话里所有符合 source kind 的 subagent thread，不仅是当前 primary 的 worker。
- `ZTeam` 自己已经有正确的不变量：只认当前 `primary_thread_id` 的 descendant thread，并且每个 slot 只保留最近更新的一条。
  这个规则已经封装在 `latest_local_threads_for_primary(...)`。
- 如果在 `attach` 路径绕过这层过滤，后果会同时出现两类：
  - 旧线程覆盖新线程：同一 slot 的历史 worker 会在后续循环中覆盖当前最新 worker；
  - 错线程混入：别的 primary 下、角色/昵称恰好匹配的 subagent 也可能被当成 ZTeam worker 恢复。
- live attach 本身不是候选筛选规则的替代品。
  先尝试 `attach_live_thread_for_selection(...)`，不代表候选集合已经正确。

## 结论

- `/zteam attach` 的正确顺序应当是：
  1. 扫 `thread/list` / `thread/read` 拿候选线程；
  2. 先用 `latest_local_threads_for_primary(...)` 收敛到“当前 primary 的最新 frontend/ios/backend descendant”；
  3. 只对这批已收敛候选，根据 loaded-thread 证据决定是否尝试 live reattach；
  4. 再把结果写回工作台状态。
- 也就是说，`attach` 的“恢复候选选择”与“live reattach 执行”是两个独立不变量，不能在接 live seam 时把前者绕开。

## 后续默认做法

- 以后给恢复逻辑接入 live listener、resume seam 或 replay seam 时，先确认是否仍保留了原来的候选选择约束。
- 只要某条恢复路径原本依赖“当前 primary descendant + 每 slot 最新”这类归并规则，就不要把它展开成“逐条 thread/list 记录直接恢复”。
- 对这类修复，至少保留两类证据：
  - 候选选择规则仍复用既有 helper；
  - 运行中 gate 或 attach seam 的输入路径有端到端测试，避免只修 dispatch 层、漏掉 composer 层。
