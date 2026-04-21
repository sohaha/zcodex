# ztok runtime settings 应先统一成单个桥接载荷，再把会话缓存 IO 从 dedup 编排层拆开

## 背景

2026-04-21 在推进 `ztok` 下一阶段路线图的 `a1` 时，需要把已有的 `behavior` 开关、`CODEX_THREAD_ID -> CODEX_ZTOK_SESSION_ID` 会话桥接、以及 `session_dedup.rs` 里混在一起的 sqlite schema / row IO / near-diff 调度拆出稳定边界。

## 这次确认的做法

- `ztok` 的运行时设置不要继续靠多个散落 env 分别读取；应由 `cli` 把配置和线程上下文收敛成单个 runtime payload，再由 `ztok::settings` 统一消费。
- 为了兼容已有调用面，可以保留 legacy env fallback，但生产路径应优先写统一 payload，避免后续再往 `main.rs` 和 `ztok` 各处追加单字段 env 判断。
- `session_dedup.rs` 更适合只保留编排职责：判定 basic bypass、exact/near-diff 分支、fallback 合同；sqlite schema、candidate 查询、snapshot 落库应放进独立 `session_cache.rs`。
- `near_dedup` 即便暂时只有 text 实现，也应尽早变成目录模块，为后续 `json/log` 类型感知策略预留稳定挂点。

## 为什么值得记

- 如果只是在 `ztok` 内局部读取新的 env，行为很容易重新碎成“CLI 一部分、命令模块一部分、dedup 一部分”，后续 a2/a3/a6/a7 会再次重复接线。
- 让 `session_dedup.rs` 同时管理 cache path、schema 和 near-diff，会把后续 cache 生命周期治理和调试视图绑死在同一个高耦合文件里。
- 这轮 `validate_by` 指定的 `cargo test -p codex-core --lib` 暴露了另一条经验：当共享 crate 测试因为仓库既有漂移失败时，要把“本 issue 的定向验证已通过”和“全链 validate_by 被外部失败阻塞”拆开记录，不能把两者混成“实现未完成”。

## 下次复用

- 再给 `ztok` 扩运行时开关时，先问“是否应该进统一 runtime payload”，而不是先加新的 env 常量。
- 涉及 `session cache` 的改动时，优先改 `session_cache.rs`；涉及 dedup 策略切换时，优先改 `settings.rs` 或 `session_dedup.rs` 的编排层。
- `Cadence` issue 的 `validate_by` 如果包含共享大套件，回写 notes 时应明确区分：
  - 哪些定向验证已经通过
  - 哪些共享验证被仓库既有失败阻塞
  - 阻塞是否与当前 issue 直接耦合
