CHARTER_CHECK:
- Clarification level: MEDIUM
- Task domain: architecture
- Must NOT do: 不把 ztok 运行时逻辑塞进 codex-core；不破坏现有 `codex ztok`/`ztok` stdout 语义；不把 domain-specific 解析器粗暴替换成通用压缩导致信息丢失
- Success criteria: 明确 5 个候选方向的模块落点、CLI 桥接边界、配置/数据模型演进、docs/tests/schema 影响，并给出至少两种方案对比与推荐
- Assumptions: 本轮只做架构分析不改代码；继续沿用现有 CLI->env->ztok 桥接原则；`gh api` 等显式 passthrough 契约默认保持不变

# ztok 下一阶段架构评审

## Status

- 已完成

## Architecture Problem

当前 `ztok` 的增强路径已经形成一条真实运行链路，但它的“可演进边界”还不稳定：

1. 配置面只有 `[ztok].behavior`，而 `session_dedup.rs` / `near_dedup.rs` 里的策略和阈值基本硬编码。
2. session cache 直接以内嵌 SQLite + `.ztok-cache/<thread>.sqlite` 实现，缺少生命周期治理、容量边界、schema 版本化和调试可见性。
3. JSON / 日志虽然已经接入共享压缩，但 near-diff 仍按文本 simhash + 行 diff 处理，结构语义没有进 dedup 决策。
4. 压缩决策没有 side channel，排查“为什么走 full / dedup / diff / fallback”只能读源码或猜。
5. 共享压缩目前主要覆盖 `read/json/log/summary`，而一些高价值入口仍停留在模块内 ad hoc 压缩，复用边界不清楚。

这不是 UI 设计问题，也不是 PM 排期问题，更不是 Terraform 交付问题；它是 `ztok` 运行时配置、缓存、策略抽象和模块边界的问题。

## Recommendation Summary

推荐沿用“Codex 全局配置由 `cli` 解析，`ztok` 只消费运行时桥接设置”的大方向，但把单一行为开关升级成一个显式的 `ztok runtime settings` 边界。

核心建议：

1. 保持配置解析在 `cli`/`config` 侧，新增 `codex-rs/ztok/src/settings.rs` 作为 `ztok` 运行时唯一设置入口。
2. 不继续把缓存治理和 near-diff 演进堆进 [`/workspace/codex-rs/ztok/src/session_dedup.rs`]，应拆出 `session_cache.rs` 与 `near_dedup/*` 子模块。
3. session cache 继续保持“每线程一个 SQLite 文件”的隔离模型，但补齐元数据、清理策略、容量边界和 schema 迁移。
4. JSON / 日志 near-diff 采用“共享框架 + 内容类型策略”的方式落在 `ztok` crate 内，不做命令级重复实现。
5. 压缩决策调试视图采用“结构化 side channel”，默认关闭，开启后走 `stderr` 或 `CODEX_HOME/log/ztok/*.jsonl`，不污染正常 stdout。
6. 扩展共享压缩优先覆盖已经自然落到 JSON / log / text 抽象上的入口，例如 `container.rs` 的 log 路径、`curl_cmd.rs` 的 JSON/text 路径；不要去破坏 `gh api` 这种显式 passthrough 契约。

## Significant Decisions

### 1. 策略配置化：继续多环境变量，还是引入统一运行时设置

**Option A：继续追加单个环境变量**

- 做法：在 `cli/src/main.rs` 里继续 `set_var` 多个 `CODEX_ZTOK_*`，`ztok` 侧各模块各自读。
- 实现成本：低
- 运行成本：低
- 团队复杂度：中，字段一多很快分散
- 未来变更成本：高，近 diff / cache / debug 每加一项都要扩散

**Option B：新增统一 `ZtokRuntimeSettings`，由 CLI 一次性桥接**

- 做法：`cli` 仍解析全局配置，但把 `config.ztok` 序列化为一个内部 env（例如 `CODEX_ZTOK_SETTINGS_JSON`），`ztok/src/settings.rs` 统一反序列化；`CODEX_ZTOK_SESSION_ID` 继续单独保留，因为它是 per-turn 动态值。
- 实现成本：中
- 运行成本：低
- 团队复杂度：低
- 未来变更成本：低

**推荐：Option B**

原因：

- 现有 `behavior` 桥接已经证明“配置归 CLI、运行归 ztok”是对的，下一步不该退回让 `ztok` 直接解析 `config.toml`。
- 候选功能包含 cache policy、near-diff policy、debug trace、shared compression scope，继续拆成散乱 env 会迅速失控。
- 统一 settings 可以把 schema、默认值、测试、调试语义集中。

**建议落点**

- `codex-rs/config/src/types.rs`
  - 扩展 `ZtokToml`
- `codex-rs/core/src/config/types.rs`
  - 扩展 `ZtokConfig`
- `codex-rs/core/src/config/mod.rs`
  - 继续作为 effective config 汇总点
- `codex-rs/cli/src/main.rs`
  - 在 `run_ztok_subcommand` 内桥接 settings
- `codex-rs/ztok/src/settings.rs`
  - 新增 `ZtokRuntimeSettings`

**应避免**

- 不要把 settings 解析、cache 策略实现或 near-diff 逻辑放进 `codex-core`
- 不要让 `codex-rs/ztok` 直接依赖 `Config::load_*`

### 2. dedup / near-diff 是暴露底层阈值，还是暴露策略级配置

**Option A：直接把 `NearDuplicateConfig` 全量外露**

- 做法：把 `max_hamming_distance`、`min_similarity_ratio`、`max_lcs_cells` 等全部进 `[ztok]`
- 实现成本：低
- 运行成本：低
- 团队复杂度：高，调用者需要理解算法细节
- 未来变更成本：高，算法一变就背兼容包袱

**Option B：先暴露策略枚举 / 模式，再保留内部默认阈值**

- 做法：配置面先定义诸如 `dedup.mode = off|exact|exact_or_near_diff`、`near_diff.mode = off|text|content_aware`、`session_cache.mode = off|session`，具体阈值暂留内部默认值；只有验证出必要性时才补 expert knobs。
- 实现成本：中
- 运行成本：低
- 团队复杂度：低
- 未来变更成本：低

**推荐：Option B**

原因：

- 当前真正需要的是“行为边界稳定”，不是把算法常量外包给配置层。
- `basic/enhanced` 本身就是策略层语义；下一阶段应该细化策略层，而不是直接下沉到底层数学阈值。

**建议配置形状**

```toml
[ztok]
behavior = "enhanced"

[ztok.session_cache]
mode = "session"
max_age_days = 7
max_db_bytes = 10485760
cleanup = "lazy-open"

[ztok.dedup]
mode = "exact_or_near_diff"

[ztok.near_diff]
mode = "content_aware"
```

### 3. session cache 治理：每线程独立 SQLite，还是切换到共享全局 SQLite

**Option A：继续每线程一个 SQLite 文件，并补治理能力**

- 做法：保留 `.ztok-cache/<thread>.sqlite`，新增 metadata/prune/version 机制
- 实现成本：中
- 运行成本：低
- 团队复杂度：低
- 未来变更成本：低

**Option B：改成共享全局 SQLite，按 `session_id` 分区**

- 做法：所有线程落一个 DB，表里显式带 `session_id`
- 实现成本：高
- 运行成本：中，锁竞争和清理复杂度更高
- 团队复杂度：中高
- 未来变更成本：中

**推荐：Option A**

原因：

- 当前 dedup 语义天然是“同一线程/会话内复用”，单文件隔离与语义一致。
- 全局库只有在需要跨线程复用时才值得，而目前需求焦点是治理，不是跨线程共享。
- SQLite 单文件隔离能避免多个 `ztok` 进程争用同一个热点库。

**建议模块拆分**

- 从 [`/workspace/codex-rs/ztok/src/session_dedup.rs`] 提取：
  - `codex-rs/ztok/src/session_cache.rs`
    - 打开/迁移/清理 DB
    - schema version / metadata
    - load/store candidate rows
  - `codex-rs/ztok/src/session_dedup.rs`
    - 只保留 orchestrator / policy

**建议数据模型演进**

- `session_cache` 表新增：
  - `content_kind TEXT NOT NULL DEFAULT 'text'`
  - `last_accessed_at INTEGER`
  - `strategy_version TEXT NOT NULL DEFAULT 'v1'`
- 新增 `cache_meta` 表：
  - `schema_version`
  - `created_at`
  - `last_pruned_at`

**治理策略**

- `lazy-open` 时顺手清理过期 `.sqlite`
- 按 DB 文件大小与最大年龄做 prune
- 对 snapshot 设字节上限，避免把大块原文永久留在 cache

### 4. JSON / 日志类型感知 near-diff：命令内特判，还是统一策略框架

**Option A：在 `json_cmd.rs` / `log_cmd.rs` 里各做一套 near-diff**

- 实现成本：低
- 运行成本：低
- 团队复杂度：高，逻辑分叉
- 未来变更成本：高

**Option B：保留统一 near-diff 框架，按 `ContentKind` 插策略**

- 实现成本：中
- 运行成本：低到中
- 团队复杂度：中
- 未来变更成本：低

**推荐：Option B**

原因：

- exact dedup / candidate 选择 / fallback reason / cache 读写是共享问题，不应命令级重复。
- 当前 `CompressionResult` 已经携带 `content_kind`，足够作为策略分派入口。

**建议落点**

- `codex-rs/ztok/src/near_dedup/mod.rs`
  - 统一 orchestrator
- `codex-rs/ztok/src/near_dedup/text.rs`
  - 当前 simhash + 行 diff 迁入这里
- `codex-rs/ztok/src/near_dedup/json.rs`
  - JSON canonicalization / path-level diff
- `codex-rs/ztok/src/near_dedup/log.rs`
  - 复用日志归一化后的事件桶 diff

**配套演进**

- `compression_log.rs` 里的 `normalize_log_line` 提升为可复用内部 helper
- JSON canonicalization 不要放进 `codex-core`；只服务 ztok 的 token 优化链路
- `output_signature` 需要带上 `strategy_version`，避免旧 cache 被新算法误命中

### 5. 压缩决策调试视图：嵌进 stdout，还是走 side channel

**Option A：把 debug footer 直接拼进正常输出**

- 实现成本：低
- 运行成本：低
- 团队复杂度：低
- 未来变更成本：高，会破坏脚本/管道和快照稳定性

**Option B：结构化 side channel（stderr 或文件）**

- 实现成本：中
- 运行成本：低
- 团队复杂度：中
- 未来变更成本：低

**推荐：Option B**

原因：

- `ztok` 是命令包装器，stdout 契约很重要；debug 不应该污染模型可见正文。
- 结构化 trace 还能直接喂测试和未来诊断工具。

**建议落点**

- `codex-rs/ztok/src/decision_trace.rs`
  - `CompressionDecisionTrace`
  - `TraceSink`
- `codex-rs/ztok/src/settings.rs`
  - debug sink 配置
- `codex-rs/cli/src/main.rs`
  - 可选 CLI override 到 env

**建议内容**

- behavior / dedup mode / near-diff mode
- `CompressionHint` / `CompressionIntent`
- `content_kind`
- `output_kind`
- fallback reason
- cache enabled/path
- candidate 数、命中类型、similarity / hamming distance

### 6. 扩展共享压缩到更多高价值入口：扩 `compression`，还是继续命令内各自处理

**Option A：把更多命令统一收敛到 `compression::{CompressionRequest, CompressionIntent}`**

- 实现成本：中
- 运行成本：低
- 团队复杂度：低到中
- 未来变更成本：低

**Option B：维持命令内 ad hoc 解析**

- 实现成本：低
- 运行成本：低
- 团队复杂度：中高
- 未来变更成本：高

**推荐：优先用 Option A，但只覆盖自然映射到 text/json/log 的入口**

**第一批推荐入口**

- `codex-rs/ztok/src/container.rs`
  - `docker logs`
  - `kubectl logs`
  - `docker compose logs`
  - 理由：已经复用 `log_cmd::run_stdin_str`，很容易升级到共享 dedup + trace
- `codex-rs/ztok/src/curl_cmd.rs`
  - 现有 `filter_curl_output` 已能识别 JSON，适合收敛到统一 JSON/text 压缩与 dedup
- `codex-rs/ztok/src/wget_cmd.rs`
  - 仅限 `run_stdout` 这种正文输出路径

**明确不建议纳入第一批**

- `codex-rs/ztok/src/gh_cmd.rs::run_api`
  - 当前显式选择 passthrough，是产品契约，不应因“共享压缩”而改变
- `cargo_cmd.rs` / `pnpm_cmd.rs`
  - 它们已有更强的 domain parser；应继续走 parser 基础设施，而不是退回通用压缩

## File / Module Impact

### 应该新增

- `codex-rs/ztok/src/settings.rs`
- `codex-rs/ztok/src/session_cache.rs`
- `codex-rs/ztok/src/decision_trace.rs`
- `codex-rs/ztok/src/near_dedup/mod.rs`
- `codex-rs/ztok/src/near_dedup/text.rs`
- `codex-rs/ztok/src/near_dedup/json.rs`
- `codex-rs/ztok/src/near_dedup/log.rs`

### 应该修改

- `codex-rs/cli/src/main.rs`
  - 继续做唯一桥接点
- `codex-rs/config/src/types.rs`
  - TOML 配置面
- `codex-rs/core/src/config/types.rs`
  - effective config
- `codex-rs/core/src/config/mod.rs`
  - effective config 装配
- `codex-rs/core/src/config/config_tests.rs`
  - 配置反序列化/装配测试
- `codex-rs/ztok/src/session_dedup.rs`
  - 缩成 orchestrator
- `codex-rs/ztok/src/compression.rs`
  - 把策略版本/trace hook 接进来
- `codex-rs/ztok/src/compression_json.rs`
  - 复用 canonicalization helper
- `codex-rs/ztok/src/compression_log.rs`
  - 暴露内部归一化 helper 给 log near-diff
- `codex-rs/ztok/src/container.rs`
- `codex-rs/ztok/src/curl_cmd.rs`
- `codex-rs/ztok/src/wget_cmd.rs`

### 不应放进 codex-core

- near-diff 算法
- session cache SQLite schema / prune 逻辑
- compression decision trace
- JSON/log canonicalization for ztok-specific dedup

`codex-core` 只承载配置类型和 effective config 装配即可。

## Docs / Tests / Schema Changes

### docs

- `docs/config.md`
  - 扩展 `[ztok]` 配置说明
- `docs/example-config.md`
  - 给出非默认样例
- 如果 debug trace 对用户可见，再补一份专门的 `docs/ztok.md` 或相关章节

### tests

- `codex-rs/cli/tests/ztok.rs`
  - 双入口一致性：`codex ztok` / alias `ztok`
  - settings 桥接
  - debug trace sink
  - container/curl/wget 新接线的 CLI 回归
- `codex-rs/ztok/src/session_dedup.rs` / 新 `session_cache.rs`
  - prune / migration / version
- `codex-rs/ztok/src/near_dedup/*.rs`
  - text/json/log 各自策略测试

### schema

- 由于 `[ztok]` 配置面会扩展，需要更新：
  - `codex-rs/core/config.schema.json`
  - 运行命令：`just write-config-schema`

app-server / MCP schema 本轮不需要改，因为这些设置只影响本地 `ztok` 命令运行时。

## Risks

1. 统一 settings env 若做成多处分散读取，会把桥接收益打掉。
2. 若让 debug trace 混入 stdout，会破坏现有 CLI 测试和脚本管道。
3. 若 JSON/log near-diff 不带 `strategy_version`，老 cache 会产生错配。
4. 若共享压缩扩展到 `gh api` 一类显式 passthrough 命令，会造成行为回归。
5. `session_dedup.rs` 和 `near_dedup.rs` 已偏大，若不先拆模块，继续迭代会提高冲突率和评审成本。

## Validation Steps

建议实现时按这个顺序验证：

1. 配置层：
   - `core/src/config/config_tests.rs`
   - `just write-config-schema`
2. CLI 桥接：
   - `cli/tests/ztok.rs` 新增 settings/env 回归
3. ztok 单元测试：
   - settings parse
   - session cache migration/prune
   - text/json/log near-diff
4. 入口级回归：
   - `read/json/log/summary`
   - `container` log 路径
   - `curl` / `wget -O -`
5. 负向验证：
   - `gh api` 仍 passthrough
   - `basic` 模式全链路不触达 dedup/sqlite/near-diff

## Artifacts Created

- `.agents/results/result-architecture.md`
- `.agents/results/architecture/ztok-next-stage-architecture.md`
