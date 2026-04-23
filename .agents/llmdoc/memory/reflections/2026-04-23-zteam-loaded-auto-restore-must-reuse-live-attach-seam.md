# ZTeam 的 loaded 自动恢复若要标记 Live，必须复用真实的 live attach seam

## 背景

- `ZTeam` 在新线程、恢复或分叉后，会根据 loaded thread 集合自动恢复当前 primary 的 worker。
- 工作台里 `Live` / “已附着” 文案的含义，不只是“知道这个线程存在”，而是“当前 TUI 已经重新接上事件回流”。

## 观察

- 仅凭 `thread_loaded_list + thread_read`，只能证明线程当前处于 loaded 集合，不能证明本地已经挂回 listener。
- 如果自动恢复路径直接把 loaded worker 标成 `Live`，但没有走 `attach_live_thread_for_selection(...)` 之类的真实重挂路径，就会出现 UI 说“已附着”，实际上收不到后续通知的假阳性。
- 显式 `/zteam attach` 已经有正确的合同：
  - 先确认候选；
  - 对可 live 的候选走真实 attach seam；
  - attach 失败时回退成 `ReattachRequired`。

## 结论

- loaded 自动恢复和显式 `/zteam attach` 不应维护两套 “Live” 判定逻辑。
- 只要某条恢复链路最终要把 worker 标成 `WorkerConnection::Live`，它就必须真的复用 live attach seam，而不是自己推断。
- attach seam 失败时，正确行为不是丢弃恢复结果，而是恢复最近状态并落到 `ReattachRequired`。

## 后续默认做法

- 以后看到“自动恢复”“后台预热”“loaded backfill”这类路径时，只要 UI 文案里出现“已附着”“已连接”“可继续回流”，都先检查它是否真的重建了 listener/channel。
- 对这类修复，至少留一条测试锁住：attach seam 失败时不能继续把 worker 暴露成 `Live`。
