# ztok 下一阶段压缩能力路线图

## 背景
- 当前状态：
  - `ztok` 已完成共享内容压缩入口、会话级 exact dedup、SimHash + LCS near-diff，以及 `read/json/log/summary` 四条主路径的共享 dedup 判定层。[`/workspace/.agents/plan/2026-04-20-ztok-general-content-compression.md`]( /workspace/.agents/plan/2026-04-20-ztok-general-content-compression.md )
  - `codex-rs/cli` 已在进入 `ztok` 前桥接 `CODEX_THREAD_ID -> CODEX_ZTOK_SESSION_ID`，并桥接 `CODEX_ZTOK_BEHAVIOR`；`ztok` 运行时仍只消费环境变量，不直接解析全局配置。[`/workspace/codex-rs/cli/src/main.rs:1291`]( /workspace/codex-rs/cli/src/main.rs:1291 )
  - `session_dedup.rs` 当前同时承载会话缓存路径、SQLite schema、exact dedup、near-diff 候选读取与 fallback 判定，`NearDuplicateConfig` 仍以默认值硬编码注入生产路径。[`/workspace/codex-rs/ztok/src/session_dedup.rs:32`]( /workspace/codex-rs/ztok/src/session_dedup.rs:32 ) [`/workspace/codex-rs/ztok/src/session_dedup.rs:120`]( /workspace/codex-rs/ztok/src/session_dedup.rs:120 ) [`/workspace/codex-rs/ztok/src/near_dedup.rs:8`]( /workspace/codex-rs/ztok/src/near_dedup.rs:8 )
  - `tracking.rs` 明确仍是 no-op 运行期适配层，仓库边界仍排除 analytics / telemetry / proxy / hook / resume / dashboard / 插件等 `sqz` 产品面。[`/workspace/codex-rs/ztok/src/tracking.rs:4`]( /workspace/codex-rs/ztok/src/tracking.rs:4 ) [`/workspace/.codex/skills/upgrade-rtk/SKILL.md:114`]( /workspace/.codex/skills/upgrade-rtk/SKILL.md#L114 )
  - 当前已有 `basic/enhanced` 行为模式，且 CLI 测试已覆盖 exact dedup 与 basic 模式绕过会话缓存，但尚无 CLI 级 near-diff 命中、cache 治理和决策调试视图覆盖。[`/workspace/codex-rs/cli/tests/ztok.rs:186`]( /workspace/codex-rs/cli/tests/ztok.rs:186 )
- 触发原因：
  - 用户要求在 Cadence 中，用多个子代理汇总 `ztok` 在当前压缩内核基础上“下一阶段全部适合实现的功能”以及完整实施步骤。
  - 当前仓库已经完成第一轮 selective `sqz` 参考能力移植；下一步重点从“把能力做出来”转向“扩大覆盖面、收紧架构边界、补治理与验证闭环”。
- 预期影响：
  - 为 `ztok` 形成一份可直接进入 Issue Generation 的完整路线图，覆盖功能分层、实现顺序、模块落点、测试闭环、风险与回滚策略。
  - 避免把 `ztok` 继续推向 `sqz` 全量 parity，而是稳定沿着当前仓库已确认的嵌入式命令过滤层边界推进。

## 目标
- 目标结果：
  - 为 `ztok` 下一阶段建立一份完整的压缩能力路线图，覆盖当前适合实现的全部功能，并明确 P0 / P1 / P2 优先级、模块归属、实施顺序与验证闭环。
  - 保持 `ztok` 的既有边界：继续围绕共享压缩、会话缓存、near-diff 与 wrapper 输出压缩扩展，不引入 `sqz` 的外部产品面。
- 完成定义（DoD）：
  - 计划明确列出每个候选功能的作用、预期效果、依赖关系、模块落点、验证方式、风险与回滚策略。
  - 阶段拆分足以支持后续 `.agents/issues/*.toml` 稳定生成，不会因范围过粗、边界含混或缺少验证入口而卡在 Issue Generation 或 Execution。
  - 计划对“做什么”和“不做什么”都有明确约束，避免下一轮执行误扩到 hook / proxy / telemetry / resume / dashboard / 插件。
- 非目标：
  - 不在本阶段直接改 Rust 代码、配置 schema、文档或测试。
  - 不宣称要把 `ztok` 做成 `sqz` 完整上游对齐版本。
  - 不在本路线图里混入与 `ztok` 压缩内核无关的 TUI / core / app-server 功能。

## 范围
- 范围内：
  - dedup / near-diff 运行时策略配置化
  - 会话缓存生命周期治理与运维面
  - JSON / 日志类型感知 near-diff
  - 压缩决策调试视图
  - 共享压缩扩展到更多高价值入口
  - 新内容类型压缩器
  - binary / 超大输入的元数据压缩路径
  - 与上述能力直接相关的配置、测试、文档、schema 与验证规划
- 范围外：
  - `sqz init`、shell/profile hook、proxy、gain/stats、resume、dashboard、浏览器/IDE 插件、MCP 打包
  - `tracking.rs` 遥测化或持久化
  - 全局共享 SQLite 会话系统
  - `sqz_engine` 全量 parity 或 wholesale import

## 影响
- 受影响模块：
  - `codex-rs/ztok`
  - `codex-rs/cli`
  - `codex-rs/config`
  - `codex-rs/core` 中与 `ConfigToml` / effective config / schema 生成直接相关的配置汇总层
  - `docs/`
- 受影响接口/命令：
  - `codex ztok`
  - `ztok`
  - `ztok read`
  - `ztok json`
  - `ztok log`
  - `ztok summary`
  - 候选扩展入口：`ztok curl`、`ztok wget`、`ztok docker logs`、`ztok kubectl logs` 及其他适合复用共享压缩的 wrapper 输出
- 受影响数据/模式：
  - `[ztok]` 及其子配置块
  - `CODEX_ZTOK_*` 运行时桥接约定
  - `CODEX_HOME/.ztok-cache/<session-id>.sqlite`
  - `core/config.schema.json`
- 受影响用户界面/行为：
  - 默认 `enhanced` 模式下，更多入口会自动进入共享压缩 / dedup / near-diff 路径
  - 启用调试视图时，会新增结构化 side channel，但不应污染正常 stdout
  - 会话缓存的保留、清理、损坏恢复将变得显式可控

## 约束与依赖
- 约束（时间/兼容性/安全/性能/发布窗口等）：
  - 不把运行时逻辑塞进 `codex-core`；`ztok` 继续作为嵌入式命令过滤层实现。
  - `ztok` 不直接解析全局 `config.toml`；配置继续由 CLI 解析并桥接给运行时。
  - 默认 stdout 契约不可被调试信息污染；调试视图只能走 side channel。
  - `basic` 必须继续保持“整条链路绕开 session dedup / near-diff / sqlite”的完整模式语义，不能被新功能重新打穿。
  - 现有 domain-specific parser 不应被粗暴退化成通用压缩；例如显式 passthrough 或已有专用摘要的命令要按入口族审慎接入。
  - 当前工作区存在大量与本任务无关的脏改动；后续进入实现时必须继续收紧变更边界，仅触碰 `ztok` 相关文件和必要配置链路。
- 外部依赖（系统/人员/数据/权限等）：
  - 需要依赖仓库现有配置加载、schema 生成、Rust 测试与文档更新工作流。
  - 若未来继续调整 `sqz` 参考面，仍通过 `.version/sqz.toml` 与 `upgrade-rtk` 统一记录，但这不是本路线图的执行前置。

## 实施策略
- 总体方案：
  - 沿用“CLI 解析配置，ztok 只消费运行时设置”的现有原则，把目前 coarse 的 `behavior` 开关升级为统一 `ZtokRuntimeSettings` 边界。
  - 把当前混在 `session_dedup.rs` 的 cache、candidate、policy 和 fallback 逻辑逐步拆成更稳定的模块边界，再在此基础上扩共享压缩覆盖面与内容类型策略。
  - 功能推进顺序遵循：先补运行时边界与可观察性，再补 cache 治理，再扩共享压缩入口与类型感知 near-diff，最后再做更细的策略调优与垂直模板。
- 关键决策：
  - 配置层优先暴露策略级配置而不是直接暴露全部 near-diff 数学阈值。
  - 会话缓存继续保持“每线程一个 SQLite 文件”，不切到全局共享 DB。
  - JSON / 日志 near-diff 采用共享 orchestrator + `ContentKind` 策略分派，而不是在 `json_cmd.rs` / `log_cmd.rs` 各做一套。
  - 决策调试视图默认关闭，并走 `stderr` 或 `CODEX_HOME/log/ztok/*.jsonl` 之类的 side channel。
  - 共享压缩扩面优先选择已自然落在 JSON / log / text 抽象上的入口，例如 `container.rs`、`curl_cmd.rs`、`wget_cmd.rs`，明确不破坏 `gh api` 这类显式 passthrough 契约。
- 明确不采用的方案（如有）：
  - 不继续追加散落的单个环境变量作为长期策略桥接方案。
  - 不把 cache 演进为跨线程共享 SQLite。
  - 不先暴露全部 near-diff 数学阈值。
  - 不把 debug footer 拼进正常 stdout。

## 阶段拆分
### 阶段 1：运行时设置边界与模块拆分
- 目标：
  - 把当前 `behavior + session_dedup + near_dedup` 的粗边界收敛为可演进的运行时设置与模块结构。
- 交付物：
  - `ZtokRuntimeSettings` 方案
  - `settings.rs` / `session_cache.rs` / `near_dedup/*` / `decision_trace.rs` 的模块设计
  - 配置层建议形状：`[ztok]`、`[ztok.session_cache]`、`[ztok.dedup]`、`[ztok.near_diff]`
- 完成条件：
  - 后续 issue 能在不继续膨胀 `session_dedup.rs` 的前提下落地。
  - `basic` / `enhanced` 边界和新策略边界不冲突。
- 依赖：
  - 当前 CLI 桥接链路与已有 `[ztok].behavior` 配置事实

### 阶段 2：会话缓存治理与最小可观察性
- 目标：
  - 给已存在的 session cache 补治理、恢复与调试能力。
- 交付物：
  - TTL / 容量上限 / lazy prune / schema version / metadata 方案
  - cache inspect / clear 的最小运维面
  - 结构化决策调试视图
- 完成条件：
  - cache 不再只有“命中/不命中”，而具备可解释、可恢复、可治理的运行时语义。
  - 调试时不再需要通过手查 sqlite 猜测 fallback 原因。
- 依赖：
  - 阶段 1 的模块边界

### 阶段 3：共享压缩扩面到更多高价值入口
- 目标：
  - 把现有共享压缩底座从 `read/json/log/summary` 扩到更多真实高频输出面。
- 交付物：
  - `structured fetchers` 家族接入方案：优先 `curl`、`wget`，再评估 `aws`
  - `container/log surfaces` 家族接入方案：`docker logs`、`kubectl logs`、`docker compose logs`
  - 明确保留 passthrough 或既有专用 parser 的入口名单
- 完成条件：
  - 同类 JSON / 日志 / 文本在不同命令上的压缩合同趋于一致。
  - 新接入入口不破坏 `basic` 模式和 stdout 契约。
- 依赖：
  - 阶段 1 和阶段 2

### 阶段 4：类型感知 near-diff
- 目标：
  - 在共享框架内补齐 JSON / 日志的结构语义 near-diff，而不是只做文本 simhash + 行 diff。
- 交付物：
  - `near_dedup/text.rs`
  - `near_dedup/json.rs`
  - `near_dedup/log.rs`
  - 相关 snapshot / diff / fallback 合同
- 完成条件：
  - JSON 的字段新增/删除/重排和日志的归一化事件桶变化能被更清晰地表达。
  - 结构化 near-diff 不会吞掉关键值变化或把不同错误误归为近重复。
- 依赖：
  - 阶段 1
  - 与阶段 3 不完全互斥，但实现上建议先完成扩面再做内容特化

### 阶段 5：新内容类型与边界输入路径
- 目标：
  - 为共享压缩底座补足更多内容种类和边界输入行为。
- 交付物：
  - `diff/patch`、表格/列表、`toml/yaml/env/ini` 等配置文本的压缩器
  - binary / 非 UTF-8 / 超大输入的元数据摘要路径
- 完成条件：
  - 高价值输出不再被过度归入普通文本。
  - 大输入与二进制不再只能失败或原样灌入上下文。
- 依赖：
  - 阶段 3
  - 阶段 4 部分能力可复用，但不是强前置

### 阶段 6：策略细化与垂直模板
- 目标：
  - 在前述共享底座稳定后，再做更细的策略控制和 wrapper 专用模板。
- 交付物：
  - dedup / near-diff 策略级配置化
  - source-aware lineage
  - `git/cargo/pytest/kubectl/docker` 等 wrapper 的垂直摘要模板
- 完成条件：
  - 行为边界稳定且可解释，不需要靠新增参数掩盖前序设计问题。
  - 专用模板建立在统一底座之上，而不是形成另一套平行实现。
- 依赖：
  - 前五阶段稳定收口

## 测试与验证
- 核心验证：
  - `cd /workspace/codex-rs && env -u RUSTC_WRAPPER cargo test -p codex-ztok`
  - `cd /workspace/codex-rs && env -u RUSTC_WRAPPER cargo test -p codex-cli --test ztok`
- 必过检查：
  - `cd /workspace/codex-rs && just fmt`
  - 如修改 `ConfigToml` 或嵌套配置类型：`cd /workspace/codex-rs && just write-config-schema`
- 回归验证：
  - 现有 exact dedup 与 basic 模式绕过会话缓存的 CLI 测试继续成立
  - 新增入口在 `basic` 模式下仍绕开 session dedup / near-diff / sqlite
  - 新增策略不导致 `codex ztok` 与 alias `ztok` 行为分叉
  - JSON / 日志 near-diff 的结构化语义不吞掉关键字段或关键错误变化
  - 决策调试视图默认关闭，开启后不污染 stdout
- 手动检查：
  - 重复运行 `read/json/log/summary` 和未来新接入入口，确认 dedup / diff / fallback 行为可解释
  - 人工破坏 `.ztok-cache/<session>.sqlite` 或制造只读目录，确认显式回退
  - 以 JSON、日志、diff、配置文本、二进制/超大输入样本验证内容路由是否符合预期
  - 验证 `gh api` 等保留 passthrough 契约的命令不被误改为共享压缩输出
- 未执行的验证（如有）：
  - 当前 Planning 阶段未新增实现，因此未运行新的 schema / docs / issue 级验证；仅引用现有已通过的 `codex-ztok` 与 `codex-cli --test ztok` 基线结果。

## 风险与缓解
- 关键风险：
  - 过早暴露算法常量，导致配置面先于行为边界稳定而膨胀。
  - cache 治理与共享压缩扩面耦合推进，造成回归面过大。
  - JSON / 日志特化 near-diff 与现有文本 near-diff 混在一个 issue 中，导致验证困难。
  - 决策调试视图若默认可见，会污染正常输出合同。
  - 新入口接入若忽略 passthrough / domain parser 边界，会让用户可见输出大幅漂移。
- 触发信号：
  - `session_dedup.rs` 持续膨胀、承担越来越多不相干职责
  - `.ztok-cache` 持续增长或频繁出现 `DedupCacheUnavailable`
  - CLI 级测试仍只有 exact dedup，缺少 `[ztok diff ...]` 断言和 cache 治理用例
  - 同类 JSON / 日志在不同 wrapper 上输出风格不一致
  - debug 信息出现在正常 stdout
- 缓解措施：
  - 优先按“运行时边界 -> cache 治理 -> 扩面 -> 类型特化 -> 策略细化”的顺序推进
  - 把 `structured fetchers`、`container/log surfaces`、`json near-diff`、`log near-diff`、`decision trace`、`cache governance` 拆成独立 issue
  - 继续用 CLI 作为唯一配置桥接点，避免全局配置解析逻辑渗进 `ztok`
  - 用 side channel 承载调试视图，并补 redaction 测试
- 回滚/恢复方案（如需要）：
  - 若某个新入口接入造成明显输出回归，按入口家族单独回退，不影响 `read/json/log/summary` 主路径
  - 若 cache 治理或策略配置化引发系统性不稳定，优先回退到当前默认值路径和现有 per-session DB 语义
  - 若结构化 near-diff 质量不稳定，可只对对应内容类型关闭 near-diff，保留 exact dedup 与 full output

## 参考
- `/workspace/.agents/plan/2026-04-20-ztok-general-content-compression.md`
- `/workspace/.agents/plan/2026-04-20-ztok-vs-sqz-analysis.md`
- `/workspace/.agents/plan/2026-04-21-ztok-compression-behavior-switch.md`
- `/workspace/.agents/results/result-pm.md`
- `/workspace/.agents/results/result-architecture.md`
- `/workspace/.agents/results/result-qa.md`
- `/workspace/codex-rs/cli/src/main.rs:1291`
- `/workspace/codex-rs/ztok/src/session_dedup.rs:32`
- `/workspace/codex-rs/ztok/src/session_dedup.rs:120`
- `/workspace/codex-rs/ztok/src/near_dedup.rs:8`
- `/workspace/codex-rs/ztok/src/near_dedup.rs:87`
- `/workspace/codex-rs/ztok/src/container.rs:171`
- `/workspace/codex-rs/ztok/src/container.rs:375`
- `/workspace/codex-rs/ztok/src/curl_cmd.rs:8`
- `/workspace/codex-rs/ztok/src/summary.rs:20`
- `/workspace/codex-rs/ztok/src/tracking.rs:4`
- `/workspace/codex-rs/cli/tests/ztok.rs:186`
- `/workspace/.codex/skills/upgrade-rtk/SKILL.md:114`
