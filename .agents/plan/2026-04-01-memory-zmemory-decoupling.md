# 原生 memory 与 zmemory 彻底解耦

> 不适用项写 `无`，不要留空。
> 只写已确认事实；若信息不清楚或需要假设，先回到对话澄清，不要把未确认内容写进计划。

## 背景
- 改造前状态（规划时基线）：仓库当时用同一个 `Feature::MemoryTool` 同时控制原生 startup memory pipeline 与 `zmemory` tool 暴露；原生 memory 的 developer prompt 仍会注入只读 memory 指引；`zmemory` 当时仍通过顶层 `zmemory_path` 和默认 workspace 隔离路径策略工作。
- 触发原因：用户明确要求原生 openai/codex 内置 memory 与 `zmemory` 在语义、feature、prompt、配置、启动流程和运行时行为上彻底解耦，并接受一次性 breaking change。
- 预期影响：需要调整 feature 命名与接线、拆分 prompt 与配置归属、把 `zmemory` 从旧的顶层 `zmemory_path` 入口迁移到独立配置块、重写 `zmemory` 默认路径策略、更新测试与文档，确保两套系统默认同时开启但彼此独立。说明：本计划中的“默认路径改为全局根”属于当时阶段性结论，后续又由 `2026-04-01-zmemory-project-default-path` 改为默认项目库。

## 目标
- 目标结果：原生 memory 与 `zmemory` 各自拥有独立 feature、独立 prompt、独立配置入口和独立运行时控制逻辑；默认两者都开启，但开关和行为互不影响。
- 完成定义（DoD）：关闭任一系统时另一套仍可工作；`[memories]` 仅服务原生 memory；旧的顶层 `zmemory_path` 不再作为主入口，`zmemory` 改由独立 `[zmemory]` 配置块承载路径/开关；本计划阶段内曾把 `zmemory` 默认落到全局根路径；prompt 不再混淆两套 memory 的只读/可写语义；相关代码、测试、文档同步更新。当前最新默认路径以后续项目默认库合同为准。
- 非目标：无条件保留旧 feature/旧路径策略的兼容桥接；扩展新的外部 memory 后端；修改与本次解耦无关的业务功能。

## 范围
- 范围内：`codex-rs/features` 的 memory 相关 feature 定义与接线；`codex-rs/core` 中原生 memory startup/prompt/config 接线；`codex-rs/zmemory` 的默认路径解析与相关说明；`codex-rs/core` 中 `zmemory` tool 暴露与 prompt 注入逻辑；对应测试与文档。
- 范围外：重设计原生 memory 的阶段算法；新增 `zmemory` 以外的记忆产品能力；保留旧耦合行为的长期兼容层。

## 影响
- 受影响模块：`codex-rs/features`、`codex-rs/core/src/memories`、`codex-rs/core/src/codex.rs`、`codex-rs/core/src/tools/spec.rs`、`codex-rs/core/src/config/*`、`codex-rs/zmemory/*`、相关测试与文档。
- 受影响接口/命令：memory feature 开关、`codex` 启动时的原生 memory 流程、`zmemory` tool 暴露、`zmemory` 独立配置块及其路径/开关语义、`codex zmemory` 的默认落库路径说明。
- 受影响数据/模式：原生 memory 的线程/阶段状态 gating；`zmemory` 默认数据库路径与 workspace key 语义（后续又进一步收口为“字段名保持 workspaceKey，但默认库改为项目级”）；配置 schema 与文档中的 memory/zmemory 说明。
- 受影响用户界面/行为：默认同时开启两套 memory；关闭某一套不会再影响另一套；模型 prompt 中对 memory 与 `zmemory` 的指引会分离；该计划阶段内 `zmemory` 默认不再按 workspace 隔离路径落库，后续已继续调整为默认项目库。

## 约束与依赖
- 约束（时间/兼容性/安全/性能/发布窗口等）：用户明确接受一次性 breaking change；`[memories]` 语义保持只服务原生 memory；默认行为必须是两套都开启但互不影响；计划阶段只依据当前仓库可确认事实，不引入未确认兼容承诺。
- 外部依赖（系统/人员/数据/权限等）：无额外外部系统依赖；实现期依赖仓库现有 Rust 测试与文档更新流程。

## 实施策略
- 总体方案：先把“原生 memory”与“zmemory”从 feature gate 层拆开，再分别清理 prompt 注入与配置归属；随后把 `zmemory` 从旧的顶层 `zmemory_path` 迁移到独立 `[zmemory]` 配置块并将默认路径策略先改到全局根，最后补齐测试/文档并做回归验证，确认默认双开时两边互不影响。补充：当前仓库后续已继续把默认路径收口为项目默认库。
- 关键决策：拆分 `Feature::MemoryTool` 为两个独立 feature；保留 `[memories]` 给原生 memory；把旧的顶层 `zmemory_path` 迁移为独立 `[zmemory]` 配置块的一部分并作为 breaking change 收口；本阶段当时把 `zmemory` 默认路径改为全局根；原生 memory prompt 与 `zmemory` prompt 各自独立维护。当前最新默认路径以后续项目默认库合同为准。
- 明确不采用的方案（如有）：不继续沿用单一 feature 同控两套系统；不让 `[memories]` 同时承载 `zmemory` 配置；不保留“workspace 隔离默认路径 + 通过显式 `[zmemory].path` 才切全局根”的旧默认模型。

## 阶段拆分
> 可按需增减阶段；简单任务可只保留一个阶段。

### 阶段一：梳理边界并拆分 feature / 配置归属
- 目标：明确原生 memory 与 `zmemory` 的开关、配置与运行时入口归属，并完成代码接线拆分。
- 交付物：新的 feature 定义与引用修改、原生 memory 仅依赖自己的 feature/配置、`zmemory` tool 仅依赖自己的 feature/配置的实现与测试更新、旧顶层 `zmemory_path` 的迁移方案落到新的 `[zmemory]` 配置块。
- 完成条件：原生 memory startup/prompt 不再读取 `zmemory` 的开关；`zmemory` tool 不再依赖原生 memory feature；旧顶层 `zmemory_path` 主入口已删除或迁移到独立 `[zmemory]` 配置块；相关单测能表达两边独立开关行为。
- 依赖：现有 feature/config 引用点清单与仓库测试基线。

### 阶段二：分离 prompt 与默认路径策略
- 目标：把原生 memory prompt 与 `zmemory` prompt 独立出来，并将 `zmemory` 默认路径策略先改为全局根（该结论随后又被项目默认库策略取代）。
- 交付物：独立 prompt 模板/注入逻辑、更新后的 `zmemory` 路径解析实现、相关 core/zmemory 测试与快照更新。
- 完成条件：prompt 文案不再混淆只读/可写语义；`zmemory` 在该阶段内无显式配置时默认解析到全局根路径；相关系统视图、说明与测试和当时策略一致。当前最新默认路径以后续项目默认库合同为准。
- 依赖：阶段一完成后的 feature/config 归属。

### 阶段三：文档与验证收口
- 目标：同步 README / config 文档 / schema / 回归测试，确认默认双开且互不影响。
- 交付物：更新后的文档、配置 schema、定向测试结果与必要快照/断言调整。
- 完成条件：文档准确描述两套 system 的边界；相关 crate 测试通过；能证明单独关闭任一系统时另一套不受影响。
- 依赖：阶段一、阶段二完成。

## 测试与验证
> 只写当前仓库中真正可执行、可复现的验证入口；如果没有自动化载体，就写具体的手动检查步骤，不要写含糊表述。

- 核心验证：`cargo test -p codex-core --test all suite::memories:: --quiet`、`cargo test -p codex-core --test all suite::zmemory_e2e:: --quiet`、`cargo test -p codex-zmemory --quiet`、`cargo test -p codex-core config::tests:: --quiet`、`cargo test -p codex-core zmemory_tool_is_available_by_default --quiet`、`cargo test -p codex-core zmemory_tool_requires_feature_flag --quiet`。
- 必过检查：功能相关的 feature/config/tool spec 测试；`just fmt`；如变更了 `MemoriesConfig`、新增 `zmemory` 配置类型或变更了 config schema，则运行 `just write-config-schema`。
- 回归验证：验证原生 memory startup pipeline 在仅开启原生 feature 时仍能跑；验证仅开启 `zmemory` feature 时 tool 仍暴露且原生 startup/prompt 不会误触发；验证默认双开时两边互不影响。
- 手动检查：检查 `codex-rs/zmemory/README.md`、`docs/config.md` 与相关 prompt/feature 文案是否清楚区分原生 memory 与 `zmemory`；检查该计划阶段中的默认 `zmemory` 路径说明是否改为全局根模式，并知悉当前仓库后续已继续切到默认项目库。
- 未执行的验证（如有）：无。

## 风险与缓解
- 关键风险：feature 拆分后遗漏某些接线点，导致默认行为变化或测试静默失效。
- 触发信号：原生 memory 或 `zmemory` 的工具/启动测试在单独启停场景下失败；prompt 快照仍混入对方语义；配置解析仍沿用旧字段。
- 缓解措施：先梳理所有 `Feature::MemoryTool` / `generate_memories` / `use_memories` / 旧 `zmemory_path` 引用点并分组改动；用 feature on/off 定点测试覆盖隔离边界；同步更新文档与 schema，避免文档代码漂移。
- 回滚/恢复方案（如需要）：若阶段性改动导致默认行为不稳，可回滚到拆分前的 feature/config 接线，再分层重新提交；计划内不保留长期兼容层。

## 参考
- `codex-rs/features/src/lib.rs:621`
- `codex-rs/core/src/codex.rs:2061`
- `codex-rs/core/src/codex.rs:3540`
- `codex-rs/core/src/config/types.rs:420`
- `codex-rs/core/src/config/mod.rs:445`
- `codex-rs/core/src/tools/spec.rs:435`
- `codex-rs/core/src/tools/spec.rs:1861`
- `codex-rs/core/templates/memories/read_path.md:1`
- `codex-rs/zmemory/src/path_resolution.rs:33`
- `codex-rs/zmemory/README.md:29`
