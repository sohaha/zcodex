# ztok 下一阶段架构笔记

## 结论

下一阶段不应该把 `ztok` 的新能力继续堆成“一个 behavior 开关 + 若干硬编码 if/else”。推荐把它收敛成三层：

1. `config` / `cli`
   - 负责解析 `[ztok]` 并桥接运行时设置
2. `ztok::settings`
   - 负责统一读取运行时设置
3. `ztok` 内部策略模块
   - `compression`
   - `session_cache`
   - `near_dedup::{text,json,log}`
   - `decision_trace`

## 最小可行拆分

- `session_dedup.rs`
  - 保留 orchestrator
- `session_cache.rs`
  - SQLite schema / migration / prune / row IO
- `near_dedup/text.rs`
  - 当前 simhash + LCS 行 diff
- `near_dedup/json.rs`
  - canonical JSON + path diff
- `near_dedup/log.rs`
  - 归一化日志事件桶 diff
- `settings.rs`
  - behavior / cache policy / dedup policy / trace sink
- `decision_trace.rs`
  - 结构化调试输出

## 不变量

- `ztok` 不直接解析全局 `config.toml`
- `CODEX_THREAD_ID -> CODEX_ZTOK_SESSION_ID` 仍由 `cli` 决定
- `basic` 必须继续是“整条链路绕开 session dedup / near-diff / sqlite”
- `gh api` passthrough 契约保持不变

## 第一批扩展入口

- `container.rs` 的日志路径
- `curl_cmd.rs`
- `wget_cmd.rs` 的 stdout 路径

## 暂缓项

- 不先暴露全部 near-diff 数学阈值
- 不先改成全局共享 SQLite
- 不把 domain parser 强行并入通用 compression
