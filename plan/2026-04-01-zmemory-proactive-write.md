# zmemory 主动写入长期偏好

> 不适用项写 `无`，不要留空。
> 只写已确认事实；若信息不清楚或需要假设，先回到对话澄清，不要把未确认内容写进计划。

## 背景
- 当前状态：`zmemory` 已作为可写长期记忆工具接入，但当前 `codex-core` 仅在 `Feature::Zmemory` 开启时注入开发者提示，并通过 tool handler 执行显式 `zmemory` 调用；未确认到独立的自动 capture 或 post-turn 主动写入链路。
- 触发原因：当前会话中，用户明确给出稳定的长期偏好（称呼“指挥官”、助手自称“小白”），系统先在回复层遵从，但没有立即主动写入当前活动 `zmemory` 库，暴露出“明显应长期记忆的信息没有自动持久化”的产品缺口。
- 预期影响：需要在不破坏 native memory 与 `zmemory` 既有边界的前提下，为“稳定且低歧义的用户长期偏好”补齐主动写入策略、canonical URI 约定、验证覆盖与必要文档。

## 目标
- 目标结果：当用户明确声明稳定、可复用、低歧义的长期偏好时，系统能够优先查重并主动写入 `zmemory`，而不是只依赖模型临场决定是否调用工具。
- 完成定义（DoD）：至少覆盖用户称呼偏好、助手自称偏好、双方协作称呼约定这类明确偏好；写入目标 URI、查重策略、更新策略、失败表现、测试与文档均明确；不会把短期上下文或一次性指令误写为长期记忆。
- 非目标：替换 native memory pipeline；把所有会话内容自动落库；引入 daemon/REST 服务；在本轮顺带重构整个 `zmemory` 治理体系。

## 范围
- 范围内：`codex-rs/core` 中 `zmemory` 接入与运行时编排、偏好识别后的主动写入触发策略、canonical memory 路径约定、相关 e2e/单测与文档更新。
- 范围外：native memory 阶段化总结逻辑；非偏好类 durable memory 的全量自动 capture；项目级 `.ai` 记忆协议改造；与本问题无关的 UI 或 CLI 大改。

## 影响
- 受影响模块：`codex-rs/core/src/codex.rs`、`codex-rs/core/src/tools/handlers/zmemory.rs` 周边运行时接线、可能新增的 memory orchestration 模块、`codex-rs/core/tests/suite/zmemory_e2e.rs`、必要时 `codex-rs/zmemory` 的 canonical URI/辅助逻辑与相关文档。
- 受影响接口/命令：模型侧 `zmemory` tool 使用时机、可能新增的内部主动写入策略；外部 CLI `codex zmemory` 命令接口预计无 breaking change。
- 受影响数据/模式：`core://agent`、`core://my_user`、`core://agent/my_user` 这三类核心 durable memory 节点的内容与更新策略；同一偏好的去重/更新规则。
- 受影响用户界面/行为：用户明确声明稳定偏好后，后续会话应更可靠地记住；若主动写入失败，系统应保持可观察而非静默伪成功。

## 约束与依赖
- 约束（时间/兼容性/安全/性能/发布窗口等）：必须保持 `zmemory` 与 native memory 的边界，不把 `codex-zmemory` 从动作层改成独立服务；不得把短期临时指令误当长期偏好；失败处理必须显式、可观察；改动优先最小化。
- 外部依赖（系统/人员/数据/权限等）：无额外外部系统依赖；依赖当前仓库已有 `zmemory` tool、core e2e 测试能力和文档更新流程。

## 实施策略
- 总体方案：在 `codex-core` 增加一个窄范围的主动写入编排层，只针对“明确、稳定、低歧义”的用户长期偏好触发；优先读取当前活动 `zmemory` 库与目标 canonical URI 查重，再决定 `create` 或 `update`；保持 `codex-zmemory` 继续作为动作层，不把“何时写”下沉到 `zmemory` crate 自己。
- 关键决策：优先把主动写入策略放在 `codex-core` 而不是 `codex-zmemory`；首批 canonical URI 以 `core://my_user`、`core://agent`、`core://agent/my_user` 为主；先实现 write gate，再考虑更泛化的 capture policy；失败时宁可显式暴露未写入，也不做静默补偿。
- 明确不采用的方案（如有）：仅继续依赖开发者提示词软约束模型自行决定；让 `system://boot` 或 `zmemory` system view 自动补建节点；一次性扩展为对所有 durable memory 类型的全自动 capture。

## 阶段拆分
> 可按需增减阶段；简单任务可只保留一个阶段。

### 阶段一：固化写入边界与 canonical 约定
- 目标：明确哪些偏好信号允许主动写入，写到哪些 URI，如何区分 `create`、`update` 与“不应写入”。
- 交付物：偏好分类规则、canonical URI 约定、查重/更新策略、失败可观察性约定。
- 完成条件：后续实现不再依赖模糊判断；工程师可据此直接进入 issue generation 而不需要再次猜测边界。
- 依赖：当前已确认的 `zmemory` 动作层能力、boot anchor 与 `core` 接入点事实。

### 阶段二：在 `codex-core` 增加主动写入编排
- 目标：在不破坏现有 tool contract 的前提下，增加长期偏好 write gate 与实际落库流程。
- 交付物：主动写入触发点、对当前活动 `zmemory` 库的读取/查重/写入代码、必要日志或可观察反馈。
- 完成条件：用户明确声明稳定偏好时，系统能对当前活动库执行可验证的 `create`/`update`；没有匹配条件时不误写。
- 依赖：阶段一的边界与 canonical 规则。

### 阶段三：补齐测试与文档
- 目标：让行为变化可回归、可审查、可解释。
- 交付物：针对主动写入的 core e2e/单测、必要的 `zmemory`/配置文档更新、若涉及用户可见文本则同步快照或断言。
- 完成条件：最小可靠验证通过，文档明确说明 `zmemory` 仍是动作层，而主动写入策略位于上层编排。
- 依赖：阶段二实现完成。

## 测试与验证
> 只写当前仓库中真正可执行、可复现的验证入口；如果没有自动化载体，就写具体的手动检查步骤，不要写含糊表述。

- 核心验证：为 `codex-core` 增加围绕长期偏好主动写入的定向测试；运行 `cargo nextest run -p codex-core --test all suite::zmemory_e2e::`，若本地无 `cargo-nextest`，则运行 `cargo test -p codex-core --test all suite::zmemory_e2e::`。
- 必过检查：`just fmt`；若改动 `codex-core`，在大改完成前执行聚焦测试，必要时再执行 `just fix -p codex-core`。
- 回归验证：确认普通 `zmemory` 显式工具调用仍按原 contract 工作；确认未命中长期偏好条件的普通对话不会触发误写；确认 `system://workspace` 指向的当前活动库被实际写入。
- 手动检查：复现“用户要求被称呼为某称呼、助手自称某称呼”的场景，检查 `read core://my_user`、`read core://agent` 与 `read system://workspace` 的输出是否符合预期。
- 未执行的验证（如有）：无。

## 风险与缓解
- 关键风险：把一次性指令误判为长期偏好，导致 durable memory 污染；或写入策略放错层级，重新把 `zmemory` 动作层和编排层耦合。
- 触发信号：测试中出现普通会话也触发写入；同类偏好被重复创建为多个节点；写入的数据库不是 `system://workspace` 所示当前活动库。
- 缓解措施：首批仅支持高确定性的显式偏好语句；先读 `system://workspace` 与 canonical URI 查重；优先更新现有 canonical 节点而不是扩散新路径；用定向 e2e 验证当前活动库路径。
- 回滚/恢复方案（如需要）：若主动写入策略造成误写或路径错误，先回退 `codex-core` 新增 write gate，只保留现有显式工具调用路径，再重新收敛规则。

## 参考
- `codex-rs/core/src/codex.rs:3620`
- `codex-rs/core/src/codex.rs:3627`
- `codex-rs/core/templates/zmemory/write_path.md:3`
- `codex-rs/core/templates/zmemory/write_path.md:8`
- `codex-rs/core/src/tools/handlers/zmemory.rs:27`
- `codex-rs/zmemory/README.md:219`
- `codex-rs/zmemory/README.md:226`
- `codex-rs/zmemory/src/config.rs:12`
- `codex-rs/zmemory/src/system_views.rs:127`
- `.agents/embedded-zmemory-overhaul/architecture.md:19`
- `.agents/embedded-zmemory-overhaul/architecture.md:33`
