# zmemory 底层内容治理与冲突收敛

## 背景
- 当前状态：`codex-zmemory` 的 `update`/`batch-update`/`create`/`import` 只负责写入或替换内容版本，并通过 `deprecated` 机制保证单个节点只保留一个 active memory row；但底层没有“同一节点内容内互斥事实”的规范化或冲突检测能力。`codex-rs/zmemory/src/service/update.rs:25` `codex-rs/zmemory/src/service/batch.rs:32`
- 触发原因：当前实际出现了 `zmemory` 长期记忆节点内容前后冲突、上层 agent 未能自动消解的问题；用户明确要求从底层彻底解决，且不能仅对身份记忆做特判补丁，也不能与上游官方原版 `memories` 主流程冲突。
- 预期影响：需要在不改写官方原版 `memories` 主流程职责的前提下，为 `codex-zmemory` 增加通用内容治理能力，使底层能识别、收敛或显式暴露节点内容冲突，上层 `core` 只消费治理结果而不承担修脏库责任。`/workspace/docs/zmemory.md:7`

## 目标
- 目标结果：为 `codex-zmemory` 建立底层内容治理框架，覆盖写入、诊断与审查链路，防止“单节点单 active row 但内容内部存在互斥事实”长期留存。
- 完成定义（DoD）：
  - `codex-zmemory` 新增通用内容治理入口，而不是把冲突处理散落在 `codex-core` 的偏好捕获逻辑中。
  - `create`、`update`、`batch-update`、`import` 等底层写入路径对受治理节点执行统一的规范化/冲突判定。
  - `doctor` 与 `review` 能显式报告内容治理异常，而不再只检查结构层问题。`codex-rs/zmemory/src/doctor.rs:7` `codex-rs/zmemory/src/service/review.rs:15`
  - `codex-core` 的 `zmemory` recall / proactive capture 链路继续可用，且不再依赖上层自定义补救逻辑来修复底层脏内容。`codex-rs/core/src/memories/zmemory_preferences.rs:54` `codex-rs/core/src/memories/zmemory_preferences.rs:132`
  - 相关 crate 级测试覆盖新增治理合同与回归场景。
- 非目标：
  - 改写官方原版 `core/memories` 启动摘要或 consolidation 主流程。
  - 把 `codex-zmemory` 扩展成自治后台、daemon 或额外会话管理系统。
  - 在本阶段一次性为所有 URI 设计复杂语义规则；首轮仅要求通用框架落地，并用当前已知高风险 canonical URI 验证框架有效。

## 范围
- 范围内：
  - `codex-rs/zmemory/src/service/` 中与写入、读取、doctor、review、contracts 相关的底层治理接线。
  - 必要时新增 `codex-rs/zmemory/src/service/governance.rs` 或等价小模块，承载内容规范化与冲突检测逻辑。
  - `codex-rs/core/src/memories/zmemory_preferences.rs` 的最小接线调整，用于删除未来多余的上层补救逻辑并对齐底层新合同。
  - 与上述改动直接相关的 `codex-zmemory` / `codex-core` 测试与必要文档更新。
- 范围外：
  - 非 `zmemory` 节点的通用自然语言事实推理系统。
  - 与当前问题无关的 alias/trigger/namespace/path 设计重构。
  - 无关运行面（TUI、app-server、ztldr、ztok）的顺手改动。

## 影响
- 受影响模块：
  - `codex-rs/zmemory/src/service/update.rs`
  - `codex-rs/zmemory/src/service/create.rs`
  - `codex-rs/zmemory/src/service/import.rs`
  - `codex-rs/zmemory/src/service/batch.rs`
  - `codex-rs/zmemory/src/doctor.rs`
  - `codex-rs/zmemory/src/service/review.rs`
  - `codex-rs/zmemory/src/service/contracts.rs`
  - 可能新增 `codex-rs/zmemory/src/service/governance.rs`
  - `codex-rs/core/src/memories/zmemory_preferences.rs`
- 受影响接口/命令：
  - `codex zmemory create`
  - `codex zmemory update`
  - `codex zmemory batch-update`
  - `codex zmemory import-memory`
  - `codex zmemory doctor`
  - `codex zmemory read`
  - 依赖同一动作层的 MCP 风格工具：`create_memory`、`update_memory`、`read_memory`
- 受影响数据/模式：
  - 现有 `memories` 版本链与 `deprecated` 行为继续保留；新增的是“内容治理”规则与诊断合同，而不是替换现有 schema 的版本语义。`codex-rs/zmemory/src/schema.rs:410`
  - 可能需要扩展 doctor/review 的返回 contract，以表达“内容需规范化”或“内容冲突待审查”。
- 受影响用户界面/行为：
  - `doctor` / `review` / `read` / 写入相关命令的返回内容与错误语义可能变得更明确。
  - 上层 `## Zmemory Recall` 注入内容会间接受到底层治理结果影响，但不应改变官方原版 `memories` 主流程。`codex-rs/core/src/memories/zmemory_preferences.rs:156`

## 约束与依赖
- 约束（时间/兼容性/安全/性能/发布窗口等）：
  - 必须保持 `zmemory` 与原生 `memories` 的系统边界清晰，不与官方原版 `memories` 主流程冲突。`/workspace/docs/zmemory.md:7`
  - 不能只做 `core://agent` / `core://my_user` / `core://agent/my_user` 的上层特判补丁；需要把规则放在 `codex-zmemory` 底层可复用框架内。
  - 不允许通过静默 fallback、静默降级或吞错掩盖冲突；冲突若无法安全归一化，必须显式暴露。
  - 保持现有版本历史、audit、search reindex 与 namespace 行为不被破坏。`codex-rs/zmemory/src/service/update.rs:77`
- 外部依赖（系统/人员/数据/权限等）：
  - 无额外外部系统依赖。
  - 依赖现有仓库测试基建、`codex-zmemory` 与 `codex-core` 的现有 e2e/单测入口。

## 实施策略
- 总体方案：在 `codex-zmemory` 增加通用“内容治理”框架，放在 service 层、紧邻写入与诊断链路；写入时执行规范化或冲突判定，doctor/review 时暴露治理异常；`codex-core` 只保留最小接线，不再承担底层数据修复责任。
- 关键决策：
  - 把问题定义为“单节点内容治理缺失”，而不是“多 active memory row”问题；因为当前 schema 已能防多 active row，但不能防同一 active row 内互斥事实。`codex-rs/zmemory/src/doctor.rs:23`
  - 优先在 `codex-zmemory` service 层接线，而不是改 schema 约束或把逻辑塞回 `codex-core`。`codex-rs/zmemory/src/service/update.rs:25` `codex-rs/core/src/memories/zmemory_preferences.rs:54`
  - 首轮以通用框架 + 已知高风险 canonical URI 规则作为验证路径，但规则承载形式必须可扩展到其他 URI，而不是写死在 `core` prompt 或上层偏好逻辑里。
  - 读取路径默认不应偷偷重写数据库；诊断和写入路径负责落库治理，读取与 review 负责可观察性。`codex-rs/zmemory/src/service/read.rs:11`
- 明确不采用的方案（如有）：
  - 不采用“只在 `core/src/memories/zmemory_preferences.rs` 里做 merge/replace”的上层补丁方案。
  - 不采用“直接改官方 `memories` 主流程去收敛 `zmemory` 冲突”的方案。
  - 不采用“遇到冲突一律静默覆盖旧值且不留下诊断痕迹”的方案。

## 阶段拆分
### 阶段一：抽象底层内容治理框架
- 目标：在 `codex-zmemory` service 层引入统一的治理接口与结果模型。
- 交付物：新的治理模块或等价抽象、URI 匹配与内容规范化/冲突检测接口、相应 contract 草案。
- 完成条件：写入、doctor、review 至少有一个共享入口可复用该治理能力，且不再需要在各处各写一套特判。
- 依赖：现有 `service/update.rs`、`service/read.rs`、`doctor.rs`、`service/review.rs` 的职责边界。`codex-rs/zmemory/src/service/update.rs:25` `codex-rs/zmemory/src/doctor.rs:7` `codex-rs/zmemory/src/service/review.rs:15`

### 阶段二：接入写入路径并收紧错误语义
- 目标：让 `create` / `update` / `batch-update` / `import` 在落库前统一走治理逻辑。
- 交付物：写入前规范化/冲突判定、必要的 audit/返回字段调整、相关测试。
- 完成条件：受治理节点的写入不会再把已知互斥事实直接写成新的 active 内容；无法安全判定时有明确错误或显式可审查结果。
- 依赖：阶段一的治理抽象；现有写入路径和批量路径复用点。`codex-rs/zmemory/src/service/update.rs:42` `codex-rs/zmemory/src/service/batch.rs:32` `codex-rs/zmemory/src/service/import.rs:12`

### 阶段三：扩展诊断与审查能力
- 目标：让 `doctor` 与 `review` 能发现并展示内容治理异常。
- 交付物：新增 doctor issue code、review 结果中的内容治理信息、必要的 contract 与测试。
- 完成条件：维护者可以通过 `doctor` / `review` 明确知道哪些节点存在内容治理冲突或需规范化，而不是只能从上层 recall 异常倒推。
- 依赖：阶段一的治理检测能力；现有 doctor/review contract。`codex-rs/zmemory/src/doctor.rs:15` `codex-rs/zmemory/src/service/review.rs:53`

### 阶段四：最小对齐 core 接线与文档
- 目标：让 `codex-core` 的 recall / proactive capture 依赖底层治理结果，而不是继续叠加上层修复。
- 交付物：必要的 `codex-core` 最小接线调整、core e2e 回归、必要文档更新。
- 完成条件：现有 `Feature::Zmemory` 行为与新底层合同一致，且没有引入对官方 `memories` 主流程的职责侵入。
- 依赖：阶段二、阶段三完成。

## 测试与验证
- 核心验证：
  - `cd /workspace/codex-rs && cargo nextest run -p codex-zmemory`
  - 若本地无 `cargo-nextest`，则使用 `cd /workspace/codex-rs && cargo test -p codex-zmemory`
- 必过检查：
  - `cd /workspace/codex-rs && just fmt`
  - 若改动范围主要在 `codex-zmemory`，结束前运行 `cd /workspace/codex-rs && just fix -p codex-zmemory`
- 回归验证：
  - `cd /workspace/codex-rs && cargo nextest run -p codex-core --test all suite::zmemory_e2e`
  - 若本地无 `cargo-nextest`，则使用 `cd /workspace/codex-rs && cargo test -p codex-core --test all suite::zmemory_e2e`
- 手动检查：
  - 使用当前工作区 `zmemory` 数据库，人工对比同一受治理 URI 在写入前后、`read`、`doctor`、`review` 中的表现是否一致。
  - 人工确认 `docs/zmemory.md` 与实际新合同一致，且仍明确写出 `zmemory` 与原生 `memories` 是两套独立系统。`/workspace/docs/zmemory.md:5`
- 未执行的验证（如有）：
  - 无

## 风险与缓解
- 关键风险：
  - 内容治理规则如果设计成当前问题特判，后续仍会退化成补丁堆积。
  - 写入路径改动可能误伤现有版本历史、audit 或 import/batch 行为。
  - doctor/review contract 扩展后，现有 CLI 或测试断言可能同步回归。
  - 若把治理职责错误地下沉到官方 `memories` 主流程，会破坏系统边界。
- 触发信号：
  - 新实现仍需要在 `codex-core` 中保留大量手工 merge/replace 逻辑。
  - `codex-zmemory` 测试出现历史链、audit、search 文档数或 review 输出异常。
  - 计划中的“通用治理框架”最终只剩 1 个 URI 的硬编码分支。
- 缓解措施：
  - 先抽象治理接口，再接入写入与诊断，避免边做边散落特判。
  - 用 `codex-zmemory` crate 级测试先锁住底层合同，再做 `codex-core` 最小接线回归。
  - 所有新规则先通过 review/doctor 可观察化，而不是只在 prompt 侧观察结果。
- 回滚/恢复方案（如需要）：
  - 若底层治理框架引发广泛回归，优先回滚新增治理接线，恢复当前 `zmemory` service 原始行为；保留调查结论和失败用例，避免继续在 `core` 层叠加临时补丁。

## 参考
- `codex-rs/zmemory/src/service/update.rs:25`
- `codex-rs/zmemory/src/service/update.rs:108`
- `codex-rs/zmemory/src/service/batch.rs:32`
- `codex-rs/zmemory/src/service/read.rs:11`
- `codex-rs/zmemory/src/doctor.rs:7`
- `codex-rs/zmemory/src/service/review.rs:15`
- `codex-rs/zmemory/src/service/review.rs:53`
- `codex-rs/zmemory/src/schema.rs:410`
- `codex-rs/core/src/memories/zmemory_preferences.rs:54`
- `codex-rs/core/src/memories/zmemory_preferences.rs:132`
- `docs/zmemory.md:5`
