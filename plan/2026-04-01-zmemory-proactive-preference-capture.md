# 让 zmemory 对稳定用户偏好主动写入

> 不适用项写 `无`，不要留空。
> 只写已确认事实；若信息不清楚或需要假设，先回到对话澄清，不要把未确认内容写进计划。

## 背景
- 当前状态：`codex-zmemory` 已提供 `read/search/create/update/delete-path/add-alias/manage-triggers/stats/doctor/rebuild-search` 等动作，但当前实现把“什么时候读、什么时候写、什么时候整理”留给上层 skill 或工作流编排；`codex-core` 当前只在启用 `Feature::Zmemory` 时注入一段开发者提示，不存在已确认的自动 capture / post-turn write hook。`codex-rs/zmemory/README.md:219` `codex-rs/zmemory/README.md:228` `codex-rs/core/src/codex.rs:3627` `codex-rs/core/templates/zmemory/write_path.md:3`
- 触发原因：当前产品行为无法保证在用户明确给出长期稳定偏好（如称呼偏好、自称偏好）时主动写入 `zmemory`；本轮已实际出现“对话层遵从了，但未立即持久化到当前活动 `zmemory`”的现象。
- 预期影响：需要在不破坏 `zmemory` 作为动作层定位的前提下，为 `codex-core` 增加稳定用户偏好的主动写入编排，并明确 canonical URI、查重与验证路径。

## 目标
- 目标结果：当用户明确声明稳定且可复用的长期偏好时，系统能在当前活动 `zmemory` 中主动、可观察、可验证地写入对应 durable memory，而不是仅依赖模型临场决定是否调用工具。
- 完成定义（DoD）：至少覆盖“用户称呼偏好 / 助手自称偏好 / 双方协作称呼约定”这一类高确定性长期偏好；写入前会查重或读现有节点；写入后能通过 `read system://workspace` 与对应 `core://...` 节点验证结果；相关测试覆盖新编排行为；默认行为不引入静默降级或隐式伪成功。
- 非目标：无条件把所有会话信息都写入 `zmemory`；在 `codex-zmemory` crate 内引入 daemon、REST、后台自治写入；扩展到所有复杂人格画像或模糊偏好推断。

## 范围
- 范围内：`codex-core` 中 `zmemory` 相关 developer instructions、运行时编排入口、稳定偏好识别与写入策略、针对 `core://agent` / `core://my_user` / `core://agent/my_user` 的 canonical 落点、相关 core/zmemory 测试与必要文档。
- 范围外：原生只读 memories pipeline 的阶段算法重写；`zmemory` CRUD 能力扩展；通用人格建模；与当前任务无关的 memory 治理改造。

## 影响
- 受影响模块：`codex-rs/core/src/codex.rs`、`codex-rs/core/src/memories/prompts.rs`、`codex-rs/core/templates/zmemory/write_path.md`、可能新增的 `codex-rs/core` memory orchestration 模块、`codex-rs/core/tests/suite/zmemory_e2e.rs`，以及必要时的 `codex-rs/zmemory/README.md` / `docs/zmemory.md`。
- 受影响接口/命令：模型对 `zmemory` tool 的使用策略；`read system://workspace`、`read system://boot` 与 `read core://agent|my_user|agent/my_user` 的验证路径；必要时新增或调整内部编排调用点，但不改变现有 `codex zmemory` CLI 基础动作接口。
- 受影响数据/模式：`core://agent`、`core://my_user`、`core://agent/my_user` 三类核心 durable memory 节点内容与 boot 覆盖率；可能补充 alias/trigger 以提升后续 recall。
- 受影响用户界面/行为：当用户明确声明稳定偏好时，后续行为会更稳定地记住该偏好；若写入发生，应在代码和测试层可观察，不依赖“看起来记住了”的表面现象。

## 约束与依赖
- 约束（时间/兼容性/安全/性能/发布窗口等）：必须保持 `codex-zmemory` 的动作层定位，不把“自动决定何时写入”下沉到 `codex-zmemory`；不能引入静默 fallback；写入规则需尽量保守，优先高确定性偏好，避免把短期上下文噪声错误持久化。
- 外部依赖（系统/人员/数据/权限等）：无额外外部系统依赖；依赖当前仓库已有 `zmemory` tool、core runtime、现有测试基建与可执行的 crate 级验证命令。

## 实施策略
- 总体方案：在 `codex-core` 增加一个“稳定用户偏好写入 gate”，位于上层编排而非 `codex-zmemory` 动作层；对高确定性偏好先做分类与 canonical URI 选择，再执行查重/读现有节点，最后触发 `create` 或 `update`，并通过现有 `zmemory` 读取路径验证写入结果。
- 关键决策：
  - 把“是否需要主动写入”定义为 `codex-core` 的 policy / orchestration 责任，而不是 `codex-zmemory` crate 的自治行为。`codex-rs/zmemory/README.md:226`
  - 首批仅覆盖高确定性、低歧义、可直接映射的长期偏好：`core://my_user`（用户称呼偏好）、`core://agent`（助手自称偏好）、`core://agent/my_user`（双方协作称呼约定）。`codex-rs/zmemory/src/config.rs:13`
  - 写入前优先查重，避免重复节点或错误覆盖；写入后使用现有 system view / read 路径验真，而不是仅看对话表象。`codex-rs/core/templates/zmemory/write_path.md:11` `codex-rs/zmemory/src/system_views.rs:189`
- 明确不采用的方案（如有）：
  - 不把自动写入逻辑直接塞进 `codex-rs/zmemory/src/service.rs`，因为该模块当前明确是动作层而非编排层。`codex-rs/zmemory/README.md:228`
  - 不仅靠强化 prompt 文字来解决问题；提示词可以辅助，但不能替代 runtime write gate。
  - 不对所有用户消息做模糊偏好抽取和自动持久化，避免误记。

## 阶段拆分
> 可按需增减阶段；简单任务可只保留一个阶段。

### 阶段一：确定主动写入 contract 与 canonical memory 结构
- 目标：把“哪些偏好必须主动写入、分别写到哪条 URI、写前写后如何验证”收口成清晰 contract。
- 交付物：稳定偏好类型清单、URI 映射规则、去重/更新规则、最小 disclosure/priority 约定、验证路径说明。
- 完成条件：工程师可以不靠猜测就知道何时写 `core://my_user`、何时写 `core://agent`、何时写 `core://agent/my_user`，以及何时应 `create` / `update`。
- 依赖：当前 `zmemory` 默认 core URI、system view 行为与现有工具契约。`codex-rs/zmemory/src/config.rs:13` `codex-rs/zmemory/src/system_views.rs:127`

### 阶段二：在 codex-core 落地稳定偏好 write gate
- 目标：把高确定性偏好识别与主动写入编排接入 `codex-core`。
- 交付物：新的 runtime orchestration 逻辑、必要的 prompt 辅助更新、与现有 `zmemory` tool/handler 的接线。
- 完成条件：系统在识别到明确稳定偏好时，不再只“建议模型可以写”，而是具备受控、可验证的主动写入路径。
- 依赖：阶段一的 contract 已明确；当前 `Feature::Zmemory` 接线与 tool handler 可复用。`codex-rs/core/src/codex.rs:3627` `codex-rs/core/src/tools/handlers/zmemory.rs:27`

### 阶段三：补齐测试、文档与治理验证
- 目标：为新编排补齐最小可靠验证闭环，并更新文档说明边界。
- 交付物：core e2e / crate 测试、必要文档更新、对 `system://workspace` 与 core 节点读回的验证用例。
- 完成条件：新增行为有自动化覆盖；文档明确区分“动作层 `zmemory`”与“上层主动写入编排”的职责边界；至少有一组测试覆盖“明确称呼偏好会被落库”。
- 依赖：阶段二完成并具备稳定调用点。

## 测试与验证
> 只写当前仓库中真正可执行、可复现的验证入口；如果没有自动化载体，就写具体的手动检查步骤，不要写含糊表述。

- 核心验证：`cargo nextest run -p codex-core --test all suite::zmemory_e2e::`（若本地无 `cargo-nextest`，则使用 `cargo test -p codex-core --test all suite::zmemory_e2e::`）；必要时补充 `cargo nextest run -p codex-zmemory` 或 `cargo test -p codex-zmemory` 作为动作层回归。
- 必过检查：`just fmt`；若改动较大且集中在 `codex-core`，收尾时执行 `just fix -p codex-core`；新增/修改测试需全部通过。
- 回归验证：确认未启用 `Feature::Zmemory` 时不会错误触发该写入编排；确认已有 `zmemory` CLI/tool 基础动作接口不被破坏；确认 `system://workspace` 仍正确指向当前活动数据库。
- 手动检查：在受控测试场景中发送明确称呼偏好，随后检查 `read system://workspace`、`read core://my_user`、`read core://agent`、`read core://agent/my_user` 的结果是否与预期一致，并确认未写入错误数据库。
- 未执行的验证（如有）：完整工作区测试套件 `just test` 是否需要执行，取决于实际改动是否波及 shared crates；若波及 common/core/protocol，再按仓库规则与用户确认。

## 风险与缓解
- 关键风险：把短期指令误判为长期偏好，导致 durable memory 被污染；或只更新了 prompt 但没有真正的 runtime 保证。
- 触发信号：出现重复/冲突的 `core://my_user` 内容；`system://boot` 仍长期显示核心 URI 缺失；测试只能证明“模型可能会写”，不能证明“系统会在规则满足时写”。
- 缓解措施：首版只纳入高确定性偏好类型；采用 canonical URI + 查重/更新规则；写入后强制通过读取路径验证；在测试中显式断言当前活动 DB 与节点内容。
- 回滚/恢复方案（如需要）：若主动写入策略不稳定，可先回退 `codex-core` 的 write gate，只保留提示词与手动 `zmemory` 调用能力，不回滚 `codex-zmemory` 动作层本身。

## 参考
- `codex-rs/core/src/codex.rs:3627`
- `codex-rs/core/templates/zmemory/write_path.md:3`
- `codex-rs/core/src/tools/handlers/zmemory.rs:27`
- `codex-rs/zmemory/README.md:219`
- `codex-rs/zmemory/README.md:226`
- `codex-rs/zmemory/README.md:228`
- `codex-rs/zmemory/src/config.rs:13`
- `codex-rs/zmemory/src/system_views.rs:127`
- `codex-rs/zmemory/src/system_views.rs:189`
- `.agents/embedded-zmemory-overhaul/architecture.md:19`
- `.agents/embedded-zmemory-overhaul/architecture.md:33`
