# 2026-04-14 cli lifecycle daemon-lock 探测竞态反思

## 背景
- `cargo test -p codex-cli` 在 `tldr_cmd::lifecycle_tests::ensure_running_records_launcher_wait_in_two_process_race` 上偶发失败。
- 表象在不同轮次会漂移成 `fake_daemon_direct.spawned`、`launcher_wait_counter_direct.log` 或 `direct_child*.done` 超时，看起来像测试同步点不稳。
- 继续把两个 contender 的子进程输出落到独立日志后，可以观察到更深一层的事实：有时同一个 contender 会先记录一次 launcher wait，再恢复成真正的 spawn owner；另一个 contender 会非常快地拿到 `ready = false` 并退出。

## 根因
- CLI 版 `spawn_native_tldr_daemon()` 比 `core` / `mcp-server` 多做了一次 `daemon_lock_is_held(project_root)?` 预检查。
- 这个检查本身是通过“短暂抢占 daemon lock 再释放”的方式探测锁状态；在双 contender 场景里，另一个进程恰好做同样探测时，会让 launcher owner 把同伴的瞬时探测误判成“外部 daemon lock owner 已存在”，从而直接返回 `Ok(false)`。
- 一旦 launcher owner 被这个瞬时探测误伤，测试表象就会退化成随机的 spawned/wait/done 超时，容易误判成 fake daemon 或父测试同步脚本的问题。

## 本轮有效做法
- 先把 flaky 现象压缩到单个生命周期测试，再通过 100 次二进制级循环确认它不是一次性失败。
- 用独立 shell 脚本复刻父测试，把两个 contender 的 stdout/stderr 按子进程落盘，确认“同一个 PID 同时写 launch_counter 与 launcher_wait_counter”这一异常模式。
- 对齐 `core` / `mcp-server` 的实现，去掉 CLI `spawn_native_tldr_daemon()` 里这层多余的 daemon-lock 预检查，把 daemon lock 判定留在上层 lifecycle manager 的 launcher-lock 流程里处理。
- 为 race 测试补两个显式同步点：
  - contender 在真正调用 `ensure_daemon_running_detailed()` 前先等父进程 release，避免“entered 文件已经落盘，但另一个进程还没真正开始争锁”。
  - spawn owner 在测试模式下等父进程 release 后再从 `launch()` 返回，避免 5 秒 ready timeout 把本应观察 race 的 contender 先耗死。

## 结果
- 定向 `cargo test -p codex-cli --bin codex tldr_cmd::lifecycle_tests::ensure_running_records_launcher_wait_in_two_process_race -- --exact` 通过。
- 基于新生成的 test binary 连跑 100 次该单测通过，确认 flaky 被压掉。
- 隔离 `CARGO_HOME` / `CARGO_TARGET_DIR` 后，完整 `cargo test -p codex-cli` 通过。

## 后续建议
- 以后碰到“锁探测 helper 本身通过抢锁实现”的代码，不要在 launcher owner 的最终 spawn 入口再重复探测一次；这类重复探测很容易把同伴的健康检查误判成真实 owner。
- 排查 cross-process flaky test 时，优先判断失败信号是不是上层同步点漂移；如果日志里出现“同一 PID 同时扮演 waiter 和 spawner”，通常说明 owner 被更深层的锁探测或状态探测误伤了。
