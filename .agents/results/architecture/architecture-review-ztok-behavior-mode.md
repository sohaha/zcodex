# ztok 行为模式与 RTK 兼容开关架构评审

status: recommend
method: design_twice + recommendation_mode
decision_scope: 为本地 ztok 增加可配置的“更接近 RTK”行为模式，并规划最小 rollout

## Architecture Problem

当前 `ztok` 的 `read/json/log/summary` 四条用户可见链路都统一进入共享 `compression` + `session_dedup`，而 `session_dedup` 又继续连接 SQLite 会话缓存与 `near_dedup` 差分逻辑。这个结构的收益是跨命令复用，但代价是：

- 产品语义被埋在共享实现细节里，用户无法明确选择“Codex 优化行为”还是“更接近 RTK 的行为”
- `summary` 的 exact dedup 身份过粗：`output_signature` 只区分 `success=true|false`，而 fingerprint 仅基于摘要输出，不包含命令身份
- `session_dedup` 持久化 `snapshot = raw_content` 与 `output = summarized output`，导致 `summary` 的原始 stdout/stderr 会写入会话缓存

这不是 UI 设计、PM 排期或 Terraform 问题，而是一个跨 CLI 配置边界、运行时行为边界与持久化边界的架构问题。

## Context Readout

只读检查得到的关键事实：

- `read.rs`、`json_cmd.rs`、`log_cmd.rs`、`summary.rs` 都先生成 `CompressionResult`，再调用 `session_dedup::{dedup_read_output|dedup_output}`。
- `session_dedup.rs` 在有 session id 时会打开 `$CODEX_HOME/.ztok-cache/<session>.sqlite`，并写入 `snapshot`（原始内容）、`output`（压缩后内容）、`output_signature` 与 `simhash`。
- `near_dedup.rs` 会在相似度满足阈值时返回 `[ztok diff ...]` 差分输出；这是典型的 sqz-derived 行为，不是 RTK 风格的基础能力。
- `cli/src/main.rs` 的 `run_ztok_subcommand()` 目前只桥接 `CODEX_THREAD_ID -> CODEX_ZTOK_SESSION_ID`，没有任何 ztok 独立配置桥接。
- 仓库现有配置体系把“能力开关”放在 `[features]`，把“产品级行为参数”放在独立配置块，例如 `[zmemory]`；这与 explorer 建议新增 `[ztok]` 块一致。

## Options

### Option A: 独立 `[ztok]` 行为档枚举

建议形态：

```toml
[ztok]
behavior = "codex" # or "rtk_compatible"
```

推荐语义：

- `codex`：保留当前 sqz-derived 行为，包含共享 compression 路由、session dedup、near-diff、会话缓存。
- `rtk_compatible`：保留命令本身的基础过滤/摘要职责，但关闭 sqz-derived 的会话复用和差分行为，使输出更接近 RTK。

成本评估：

- 实现成本：低到中。主要是配置类型、schema、文档、CLI 桥接、ztok 运行时 gating。
- 运维成本：低。单一枚举，排障和用户沟通都简单。
- 团队复杂度：低。只有两个受支持档位，没有组合爆炸。
- 未来变更成本：低。后续若要新增第三档行为，可以继续扩枚举。

### Option B: `[features]` 或多个底层布尔开关

示例：

```toml
[features]
ztok_rtk_compatible = true

[ztok]
disable_session_dedup = true
disable_near_diff = true
disable_shared_compression = true
```

问题：

- 把产品行为拆成实现细节，用户与开发者都要理解内部模块边界
- 会产生无意义组合，例如 dedup 关掉但 near-diff 开着，或 shared compression 关掉但其它逻辑还依赖它
- `[features]` 在仓库里已经承担“能力启停”语义，不适合承载这种长期产品行为档

成本评估：

- 实现成本：中。表面简单，但测试矩阵和文档复杂度更高。
- 运维成本：中到高。组合态多，难以稳定支持。
- 团队复杂度：高。排查问题时必须先还原一组实现细节组合。
- 未来变更成本：高。越改越像内部 debug knobs，不像产品配置。

## Recommendation

推荐 **Option A**：新增独立 `[ztok]` 配置块，使用 `behavior = "codex" | "rtk_compatible"`。

推荐理由：

1. 它把用户可理解的语义放在配置边界，而不是把内部 seam 暴露成一串布尔开关。
2. 它与仓库现有 `[zmemory]` 这类“独立子系统配置块”的模式一致，也符合 explorer 提议。
3. 它允许 rollout 先以“行为 gating”消除 `summary` 的主要风险，而不要求第一期就重写所有内部复用层。

## A. 推荐产品语义与命名

推荐命名：

- 配置块：`[ztok]`
- 键：`behavior`
- 枚举值：`"codex"`、`"rtk_compatible"`

不推荐：

- `mode`
  原因：仓库里已大量使用 `mode` 表达运行模式、沙箱模式、传输模式，语义过载。
- 放进 `[features]`
  原因：这是稳定的产品行为选择，不是实验能力启停。

推荐产品语义：

- `codex` 是“Codex 优化档”，不是“默认打开所有实验项”的含糊说法。
- `rtk_compatible` 是“更接近 RTK 的行为档”，不是“与 RTK 字节级完全一致”的承诺。

## B. 关闭时应切断的实现层

第一期必须切断：

1. CLI 桥接层
   `cli/src/main.rs` 不能只桥接 session id；还要把 `[ztok].behavior` 作为单一运行时真相传给 `codex-ztok`。

2. 用户可见命令的 dedup 接缝
   `read.rs`、`json_cmd.rs`、`log_cmd.rs`、`summary.rs` 在 `rtk_compatible` 下都应绕过 `session_dedup`。

3. 会话缓存持久化层
   `session_dedup.rs` 在 `rtk_compatible` 下不应打开 SQLite、不应加载候选、不应 `store_snapshot()`。

4. near-diff 层
   `near_dedup.rs` 在 `rtk_compatible` 下应整体不可达；不能只是“命中后不展示”，否则仍有不必要计算与潜在语义漂移。

第一期不建议强拆：

5. `compression.rs` 分发层
   只要 `rtk_compatible` 下已经不再进入 session dedup / near-diff / snapshot persistence，`compression.rs` 仍可作为内部路由保留。它是结构冗余问题，不是当前最高风险点。

## C. 最小可执行 rollout 分期

### Phase 1: 配置与桥接落地，不改默认行为

- 新增 `[ztok].behavior`
- 更新 config schema 与文档
- CLI alias / `codex ztok` 统一桥接该配置到 `codex-ztok`
- 默认仍为 `codex`

目标：
- 不改变现有用户行为
- 建立单一行为开关的正式入口

### Phase 2: `rtk_compatible` 只关闭 session 复用与差分

- `read/json/log/summary` 全部绕过 `session_dedup`
- 禁止创建/写入 `.ztok-cache/*.sqlite`
- 禁止返回 `[ztok dedup ...]` 与 `[ztok diff ...]`

目标：
- 先解决 `summary` 原始输出落库风险
- 让兼容档马上得到清晰、可见的行为差异

### Phase 3: 视 parity 差距再做内部解耦

- 只有在 Phase 2 后仍发现与 RTK 的核心体验偏差，再评估是否把 `read/json/log/summary` 从共享 `compression.rs` facade 中进一步拆开

目标：
- 避免把“更接近 RTK”误做成一次过度重构

## D. 最值得写进计划的风险与非目标

### 风险

1. `summary` 风险不能靠“只关闭短引用输出”解决，必须连 SQLite snapshot 落库一起关闭。
2. 如果只在 `summary` 单点上做兼容，`read/json/log` 仍保留 session 行为，最终会出现“一个模式多个语义”。
3. 如果配置读取只发生在 `codex ztok` 主命令，alias 与嵌入式桥接可能分叉。
4. 如果过早拆 `compression.rs`，会把本来可控的配置切换做成中型重构。

### 非目标

1. 不承诺所有 ztok 子命令都变成 RTK 字节级复刻。
2. 不把该行为档放进 `[features]`。
3. 不在第一期顺手重构 `shell` / `err` / `test` / rewrite 分析链路。
4. 不在本轮同时修改 native-tldr / `ztldr` / MCP `ztldr` tool 行为。

## Validation Steps

1. 配置验证：`ConfigToml`、schema、文档同时出现 `[ztok].behavior`，且没有新增等价 feature flag。
2. 桥接验证：`run_ztok_subcommand()` 除 session id 外还会统一传入行为档。
3. 兼容验证：`rtk_compatible` 下 `read/json/log/summary` 不再出现 `[ztok dedup ...]` / `[ztok diff ...]`，且无 `.ztok-cache/*.sqlite` 写入。
4. 回归验证：默认 `codex` 模式维持现有行为，现有 dedup/near-diff 相关测试仍成立。

## Artifacts Created

- `.agents/results/architecture/architecture-review-ztok-behavior-mode.md`
- `.agents/results/result-architecture.md`
