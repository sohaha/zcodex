# 2026-04-24 ztldr search 超时预算应与结构型 heavy action 分离

## 背景
- 用户反馈 `ztldr` 的部分全文检索会超时。
- 排查后确认 `native-tldr` daemon client 端把 `Search` 和 `Warm`、`Analyze`、`Semantic`、`Diagnostics` 等 action 统一放进 `DAEMON_HEAVY_IO_TIMEOUT = 180s`。
- 但 `Search` 的实现是 `search_project` 对仓库做逐文件同步全文扫描；它的延迟画像更接近“慢 I/O 扫描”，而不是结构索引或语义检索。

## 这轮有效做法
- 在 `codex-rs/native-tldr/src/daemon.rs` 为 `TldrDaemonCommand::Search` 单独拆出 `DAEMON_SEARCH_IO_TIMEOUT`，不要继续复用通用 heavy bucket。
- 保持改动落在 daemon query timeout 分配层，不去扩散到 tool schema、wire payload 或上层 handler。
- 用定向测试锁住 `io_timeout_for_command` 对 `Search` 的专用预算，避免后续重构时被并回 `DAEMON_HEAVY_IO_TIMEOUT`。

## 关键结论
- `ztldr search` 虽然也属于“重操作”，但它的超时预算不应与 `Analyze`/`Semantic` 这种索引或分析型命令完全等同。
- 当 user symptom 是“部分全文检索超时”而不是“结果错误”时，先检查 daemon client 端的 command-specific timeout bucket，再决定是否进入搜索实现层做性能优化。

## 后续建议
- 如果未来还有 search 超时反馈，再继续评估 `search_project` 本身是否需要流式读取、并行遍历或更强的文件级剪枝。
- 新增 daemon action 时，不要只按“轻/重”二分；要根据实际执行模型决定是否需要单独预算。
