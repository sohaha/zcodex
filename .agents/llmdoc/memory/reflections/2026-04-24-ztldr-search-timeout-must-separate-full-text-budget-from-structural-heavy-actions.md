# 2026-04-24 ztldr search 不应靠放宽 timeout 掩盖同步全文扫描

## 背景
- 用户反馈 `ztldr` 的部分全文检索会超时。
- 初看像是 daemon client 端把 `Search` 和其他 heavy action 共用 `DAEMON_HEAVY_IO_TIMEOUT = 180s` 导致，但继续下钻后确认真正瓶颈在 `native-tldr/src/search.rs`。
- 旧版 `search_project` 采用单线程 walker + `std::fs::read_to_string` 逐文件整块读取，再逐行跑 regex；这是实现层选错了执行模型，而不是 timeout 桶分配本身。

## 这轮有效做法
- 把 `search_project` 改成以 `rg` 为主执行面：用 `rg --json` 流式读取 match 事件，拿到 `max_results` 后立刻停止并标记 `truncated`。
- 用单独的 `rg --files --null` + 同一套 language globs 统计 `indexed_files`，保持现有 `SearchResponse` 契约不变。
- 仅当运行环境缺少 `rg` 时，才回退到原先的 walker 实现作为兼容兜底，而不是把它继续当主路径。
- 同时清理“给 Search 单独放宽 daemon timeout”的临时思路，避免把症状修复误留成长期设计。

## 关键结论
- `ztldr search` 的主路径应该使用成熟的底层全文检索器，而不是在 Rust 里手写逐文件整读扫描。
- 当用户症状是“大仓库/高命中全文检索超时”时，不要先加大 timeout；先检查是否存在整仓同步扫描、整文件读取、或“先收全量结果再截断”这类错误执行模型。
- `truncated` 语义必须在搜索进程仍在运行时就能触发停止，而不是等全量输出完成后再裁掉前 100 条。

## 后续建议
- 以后若继续优化 `ztldr search`，优先围绕 `rg` 调用参数、language globs、以及结果解析/终止策略迭代，不要把主路径退回到 in-process walker。
- 若新增新的全文检索型 daemon action，默认先考虑外部成熟搜索器 + 流式消费，而不是复用分析类 action 的执行模板。
