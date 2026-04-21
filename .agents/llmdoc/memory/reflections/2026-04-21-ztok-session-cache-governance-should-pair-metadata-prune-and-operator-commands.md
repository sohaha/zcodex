# ztok session cache 治理要把 metadata、容量裁剪、损坏回退和运维命令一起落下

## 背景

2026-04-21 在推进 `ztok` 下一阶段路线图的 `a2` 时，`session cache` 已经能作为 per-session SQLite 载体参与 dedup，但还缺少可治理语义：没有 schema version、没有容量边界，也没有 inspect / clear 之类的最小运维入口。

## 这次确认的做法

- `session cache` 的治理不要只加一个 schema version；至少要同时补三类能力：
  - metadata：例如 `schema_version`、`max_entries`
  - 生命周期：固定容量裁剪或等价的保留策略
  - 运维面：最小 `inspect` / `clear`
- `inspect` 与 `clear` 这类最小运维面直接放在 `ztok` 命令层即可，不需要为了这类单文件 SQLite 管理再引入共享服务或 dashboard。
- `session_dedup` 的合同要保持稳定：即使 cache 损坏、路径不可写、或 schema 不兼容，也应显式回退到完整输出，而不是吞掉错误或静默地产生空结果。
- `clear` 不应只删除正常 sqlite 文件；如果 cache 路径已经被错误地占成目录，也应允许直接清理，减少手工恢复成本。

## 为什么值得记

- 只补 metadata 而不补容量边界，会让单会话 sqlite 无限膨胀，后续 inspect 看到的状态也没有操作意义。
- 只补容量边界而不补运维命令，用户仍然得手工找 `.ztok-cache/<session-id>.sqlite`，不符合“可治理、可受控”的完成定义。
- 只在 `session_cache.rs` 层修损坏恢复而不锁 `session_dedup` fallback 测试，很容易在后续重构时把“显式回退到 full output”的行为弄丢。

## 下次复用

- 再给 `ztok` 扩 session cache 治理时，默认成组思考：
  1. metadata 怎么存
  2. 保留策略怎么收口
  3. 用户如何 inspect / clear
  4. 损坏或 schema 演进失败时如何显式回退
- 若需要新增治理字段，优先继续放在 `session_cache_metadata`，不要把版本/上限硬编码散落到多个 SQL 查询里。
- 做完 Rust 改动后，优先先拿定向测试证据，再跑 `just fix -p ...` 收尾；若仓库规则要求 fix 后不重跑测试，就在 Cadence notes 里明确写出“测试通过发生在 fix 之前”。
