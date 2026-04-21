# ztok fetcher 输出一旦复用共享 trace，就要同时锁 URL source redaction 和新入口覆盖

## 背景

2026-04-21 在推进 `ztok` 下一阶段路线图的 `a4` 时，我把 `curl` 与 `wget -O -` 的内容输出面接进了统一的 fetcher 压缩入口：同一层里做 JSON/text 压缩选择、session dedup 和 `stderr` compression decision trace。

第一轮实现和测试先锁住了共享压缩与 dedup 复用，但自审时发现一个完成度空白：既然这两条 fetcher 路径也已经接进 `track_compression_decision`，验证就不能只看 stdout 和 dedup，还要把新的 side channel 一起锁住，尤其是 fetcher 的 `source` 现在来自 URL，天然有 query/token 泄露风险。

## 这次确认的做法

- fetcher 家族进入共享压缩后，不应直接把原始 URL 当作 `source_name`。应先去掉 userinfo、query、fragment 和协议头，再做统一截断，避免 dedup 短引用文案或 trace side channel 把用户名、密码、token、签名或调试参数带出去。
- 只要某个新入口接进了统一 decision trace，就要补至少一条该入口自己的 trace 集成测试，而不是沿用旧入口的 trace 覆盖去“代证”。
- 对 `curl` / `wget -O -` 这类内容型 fetcher，最小验证集合应同时覆盖：
  - enhanced 模式下进入共享压缩与 dedup
  - basic 模式下继续绕开 session dedup
  - 内部 URL 的 JSON 继续保留原始正文，而不是被 schema 化
  - 显式开启 `--trace-decisions` 时，`stderr` 只出现结构化事件，不泄露 query 参数和原始正文
- `aws` 这类命令族若仍混有多条专用 parser 路径，不要只把 generic path 半接到共享 fetcher 层。宁可暂时明确保持边界，也不要制造“同一命令族部分共享、部分旁路”的半成品。

## 为什么值得记

- fetcher 的 `source` 和普通 `read/json/log` 不同，它往往直接来自 URL；如果不显式做 redaction，新增 trace 后很容易在 stderr 泄露敏感 query。
- 共享底座扩面时，只测 dedup 不测 trace，会让“生产已接线”和“验证已覆盖”再次脱节，等到 Cadence 收口或后续重构时才暴露缺口。
- `aws` 这类已有专用摘要器的命令族，最大的风险不是“没接入”，而是“半接入”后行为边界变得不可解释。

## 下次复用

- 给 `ztok` 的新 wrapper 接共享压缩时，先枚举该 wrapper 会不会把用户可见 `source` 建在 URL、路径或命令片段上；若会，就先定义 redaction 合同，再接 trace。
- 新入口只要复用了 `track_compression_decision`，就把“该入口至少一条 trace 集成测试”当作默认完成线。
- 遇到命令族内部存在多条专用 parser / passthrough 路径时，先判断能否整族保持同一合同；做不到就明确延后，不做半接线。
