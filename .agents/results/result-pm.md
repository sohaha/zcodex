status: completed
summary:
  - 基于现有 ztok 实现、既有计划和 sqz selective-reference 边界，整理了下一阶段适合推进的功能分层。
  - 结论重点是：下一步应继续扩展“通用内容压缩底座的覆盖面、会话缓存治理与可观察性”，而不是引入 sqz 的 hook/proxy/gain/resume/dashboard/插件产品面。
files_changed:
  - .agents/results/result-pm.md
acceptance_criteria_checklist:
  - [x] 基于仓库现状与已有文档分析
  - [x] 输出 P0/P1/P2 分层
  - [x] 每项包含作用、预期效果、依赖关系、适配当前边界的理由
  - [x] 明确非目标边界
  - [x] 给出推荐阶段顺序

## ztok 下一阶段功能分层

### 当前基线

- 已完成能力：
  - 共享内容压缩入口已覆盖 `read/json/log/summary`
  - 会话级 exact dedup 已落地，并由 CLI 注入 `CODEX_THREAD_ID -> CODEX_ZTOK_SESSION_ID`
  - SimHash + LCS near-diff 已落地
  - `basic/enhanced` 行为模式已落地，`basic` 会整条链路绕开 dedup / near-diff / sqlite
  - `tracking.rs` 仍是显式 no-op 适配层，不承接 analytics / telemetry / persistence 产品面
  - `.version/sqz.toml` 与 `upgrade-rtk` 已把 `sqz` 固定为 selective reference，而不是 parity 目标

- 现阶段真正缺的不是“再做一次 dedup”，而是：
  - 让更多高价值输出进入现有压缩底座
  - 给已有 sqlite session cache 补治理与可观察性
  - 让 generic compression 从 `read/json/log/summary` 扩到更通用的 shell / wrapper 输出

### P0

- 内容感知的通用 shell 输出压缩
  - 作用：把现有 `compression` 底座接到 `ztok shell` 与高频 wrapper 的 stdout/stderr 后处理，而不只局限于 `read/json/log/summary`
  - 预期效果：`gh api`、`curl`、`kubectl`、`docker`、`npm/pnpm`、`cargo test/build` 等命令输出能自动落到 JSON/日志/文本压缩路径，减少“必须先手工选对 ztok 子命令”的成本
  - 依赖关系：复用现有 `compression.rs`、`compression_json.rs`、`compression_log.rs`，需要在 `runner.rs` / 各 wrapper 命令返回后增加统一出口
  - 适合当前边界的原因：仍然是“Codex 内嵌命令过滤层”；不需要引入 hook/proxy/resume，只是把已存在的底座复用到更多入口
  - 不适合拖后的原因：当前 generic compression 的价值还只覆盖少数命令，用户实际最常见的是 arbitrary shell 和 wrapper 输出

- session cache 生命周期治理
  - 作用：为 `CODEX_HOME/.ztok-cache/<session-id>.sqlite` 增加过期、清理、损坏恢复与容量上限策略
  - 预期效果：避免长期使用后缓存无限膨胀、历史 session 残留、损坏文件导致 dedup 反复 fallback
  - 依赖关系：基于现有 `session_dedup.rs` 的 schema / path 逻辑扩展即可
  - 适合当前边界的原因：这是当前已落地 sqlite cache 的必要收尾，不是扩张产品面
  - 为什么是 P0：当前缓存已经在真实链路中生效，但还缺明确治理；这属于“已有功能可持续运行”的基础能力

- dedup / near-diff 最小可观察性
  - 作用：让用户或调试者能看见本次输出是 full、short reference、diff，命中原因是什么，fallback 原因是什么
  - 预期效果：排查误命中、误回退、basic/enhanced 行为差异时，不需要读源码或猜 sqlite 状态
  - 依赖关系：现有 `CompressionResult.output_kind`、`ExplicitFallbackReason` 已具备数据模型，只差统一暴露方式
  - 适合当前边界的原因：这是对现有显式行为合同的补强，不是 `sqz stats/gain/dashboard`
  - 边界控制：应做成轻量 debug / inspect / verbose 级别能力，而不是独立分析产品

### P1

- 新内容类型压缩器：diff / patch / tabular / config 文本
  - 作用：在现有 `Code/Json/Log/Text` 之外，补上高频但结构特殊的输出类型
  - 预期效果：
    - unified diff / patch 输出更适合“只看变更块”
    - 表格/列表输出更适合裁剪列与汇总
    - `toml/yaml/env/ini` 这类配置文本不必退回普通 text
  - 依赖关系：扩展 `ContentKind`、内容探测与对应 renderer
  - 适合当前边界的原因：仍属于 compression seam 的自然扩展，比引入 sqz 外围产品面更贴近仓库价值
  - 为什么不是 P0：现有用户痛点首先是“更多命令接到底座”和“cache 可治理”，再往后才是内容类型细分

- shell 重写器与压缩底座联动增强
  - 作用：让 `cat/head/tail/find/rg` 之外的更多 shell 形态更稳定地落到 `ztok` 专用子命令或统一压缩出口
  - 预期效果：减少 `rewrite.rs` 因 unsupported arguments / command shape 直接回退 raw shell 的比例
  - 依赖关系：依赖现有 `rewrite.rs` allowlist 与 command-shape 判定；也依赖 P0 的“通用 shell 输出压缩”作为兜底
  - 适合当前边界的原因：这是 embedded command surface 的增强，符合 `upgrade-rtk` 对 curated surface 的定义
  - 限制：不应为了覆盖率引入复杂 shell 解释器或 hook 注入

- binary / 超大输入的元数据压缩路径
  - 作用：避免 `read_to_string` 失败或大文件把当前压缩链路直接拖入异常/无意义全文输出
  - 预期效果：对二进制、超长单行、大型日志/JSON 文件输出“文件类型 + 尺寸 + 结构摘要/采样窗口”，而不是失败或全量
  - 依赖关系：需要在 `read.rs` 和内容探测层加入非 UTF-8 / oversized 输入分支
  - 适合当前边界的原因：仍是本地 CLI 过滤层能力，不涉及外部平台
  - 为什么是 P1：价值高，但不如 P0 直接补当前已上线 session cache 与 generic shell 覆盖的缺口

- session cache inspect / clear 操作
  - 作用：提供最小运维入口，查看某 session 是否命中过 dedup、清理指定 session cache
  - 预期效果：调试和 CI 更容易复现实验，不必手工删 `.ztok-cache`
  - 依赖关系：基于当前 cache 文件布局增加只读/删除命令即可
  - 适合当前边界的原因：是对已有 session cache 的运维补充，不等于 `sqz resume/stats`
  - 为什么不是 P0：优先级低于“自动治理”和“输出可观察性”；更像运维辅助面

### P2

- 阈值与策略配置化
  - 作用：把 near-diff 的 `max_hamming_distance`、`min_similarity_ratio`、`max_diff_lines` 等从硬编码变为可配置策略
  - 预期效果：不同仓库或不同命令可调优 dedup/diff 灵敏度
  - 依赖关系：依赖现有 behavior/config 桥接模式；应由 CLI 配置系统桥接，不让 ztok 自己解析全局配置
  - 适合当前边界的原因：属于现有算法可调优，不是新产品面
  - 为什么放 P2：在当前缺少更广覆盖和 cache 治理前，开放参数只会放大调试面

- 更强的 source-aware lineage 策略
  - 作用：把 dedup / near-diff 候选从“同 output_signature 池内比较”升级为更理解来源语义的比较，例如区分同一路径、同命令、同资源类型
  - 预期效果：降低跨来源误命中，提高 diff 可读性
  - 依赖关系：需要扩展 `output_signature` 或缓存索引结构
  - 适合当前边界的原因：仍然完全在本地 sqlite + compression seam 内
  - 为什么是 P2：当前实现已有可用版本，先补覆盖与治理更值当

- 针对特定 wrapper 的垂直压缩器
  - 作用：为 `git`、`cargo`、`pytest`、`kubectl`、`docker` 等提供更专用的摘要模板
  - 预期效果：输出比 generic log/json/text 更准，更贴近用户任务
  - 依赖关系：建立在 P0/P1 通用出口和内容探测基础上
  - 适合当前边界的原因：属于 embedded wrapper 的精修
  - 为什么不是更早：当前还没把所有 wrapper 稳定送入通用压缩出口，过早做垂直模板会导致维护面分散

## 非目标

- 不适合当前仓库边界，建议明确排除：
  - `sqz init`、hook 安装、shell/profile 注入
  - proxy、dashboard、browser/IDE 插件、MCP 打包
  - gain/stats/telemetry 分析产品面
  - resume / cross-session narrative 历史恢复产品
  - 完整 `sqz_engine` parity 或 wholesale import
  - 将 `tracking.rs` 从 no-op 扩成遥测/持久化系统

## 推荐阶段顺序

1. P0.1 内容感知的通用 shell 输出压缩
   原因：这是把“已完成的通用压缩底座”变成真实高频能力覆盖面的关键一步。

2. P0.2 session cache 生命周期治理
   原因：sqlite cache 已经上线，继续扩功能前先补运行面稳定性。

3. P0.3 dedup / near-diff 最小可观察性
   原因：后续任何调参与覆盖扩展都需要可观测证据，不然排障成本会快速上升。

4. P1.1 diff / table / config 等新内容类型
   原因：在更广入口都进入底座后，再做内容细分类，收益最大。

5. P1.2 binary / 超大输入元数据路径
   原因：这会补足 read/generic shell 的边界鲁棒性。

6. P1.3 inspect / clear 运维命令
   原因：适合在 cache 规则基本稳定后补。

7. P2 阈值配置、source lineage、wrapper 专用模板
   原因：这些都属于“在底座覆盖和治理到位后再做精调”的工作。

## 一句话结论

ztok 下一阶段最值得做的不是继续追 `sqz` 产品面，而是把现有“共享压缩 + session dedup + near-diff”底座接到更多真实命令出口，并补齐 sqlite cache 的治理与可观察性；这三件事完成后，再做内容类型扩展和策略调优才划算。
