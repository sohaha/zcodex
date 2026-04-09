# zmemory 内置 system 视图支持用户配置

> 不适用项写 `无`，不要留空。
> 只写已确认事实；若信息不清楚或需要假设，先回到对话澄清，不要把未确认内容写进计划。

## 背景
- 当前状态：`zmemory` 的 runtime profile 里，`validDomains` 与 `coreMemoryUris` 由 `codex-rs/zmemory/src/config.rs` 中的环境变量 `VALID_DOMAINS` / `CORE_MEMORY_URIS` 及其硬编码默认值驱动；`system://workspace` 返回当前 runtime 值，`system://defaults` 返回产品默认值，当前 `[zmemory]` 配置块只支持 `path`，还不支持用户在 `config.toml` 中声明编码记忆域与 boot anchors。
- 触发原因：用户明确要求把 `system://workspace` / `system://defaults` / `system://boot` 这类内置 system 视图所依赖的编码记忆配置开放给用户自定义，并参考一套“编码协作型长期记忆”方案：少量高价值 boot、显式 `core/project/notes` 域、项目知识按需召回，而不是把大量项目知识预塞进 boot。
- 预期影响：需要扩展 `[zmemory]` 配置面、调整 runtime profile 装配优先级、明确 `system://workspace` 与 `system://defaults` 如何表达“用户配置后的当前事实”与“产品内置默认事实”的边界，并同步文档与测试。

## 目标
- 目标结果：用户可以通过 `~/.codex/config.toml` 的 `[zmemory]` 配置块声明自己的 `valid_domains` 与 `core_memory_uris`，使 `system://workspace` / `system://boot` / `stats` / `doctor` 反映当前用户配置；同时保留 `system://defaults` 作为产品内置默认事实视图，不与用户覆盖值混淆。
- 完成定义（DoD）：`[zmemory]` 至少支持 `path`、`valid_domains`、`core_memory_uris`；未配置时继续使用产品默认值；配置后 `system://workspace.validDomains` / `coreMemoryUris` / `boot` 以及 `stats` / `doctor` 的相关行为与用户设置一致；`system://defaults` 仍明确报告仓库内置默认值；README / `docs/config.md` / `docs/zmemory.md` 给出编码记忆推荐配置示例与 system view 判读方法。
- 非目标：本轮不把 `system://workspace` / `system://defaults` 变成任意用户自定义 URI；不引入新的 system view 命名空间；不自动初始化用户提供的 `core://...` / `project://...` / `notes://...` 节点内容；不在本轮重写 recall 策略或新增自动 profile 选择机制。

## 范围
- 范围内：`codex-rs/core/src/config/*` 中 `[zmemory]` 配置类型与 schema；`codex-rs/zmemory/src/config.rs` 的 runtime settings 装配；`codex-rs/zmemory/src/system_views.rs` / `service.rs` / `tool_api.rs` 对 system views 与 boot/profile 事实的输出；`docs/config.md`、`docs/zmemory.md`、`codex-rs/zmemory/README.md` 的说明与示例；必要的 core/zmemory/CLI 测试。
- 范围外：改造 `system://index` / `system://recent` / `system://alias` 的结构；新增 profile 模板市场；把用户推荐配置自动写入现有配置文件；迁移历史记忆内容到新的 domain 结构。

## 影响
- 受影响模块：`codex-rs/core/src/config/types.rs`、`codex-rs/core/src/config/mod.rs`、`codex-rs/core/config.schema.json`、`codex-rs/zmemory/src/config.rs`、`codex-rs/zmemory/src/system_views.rs`、`codex-rs/zmemory/src/service.rs`、相关测试与文档。
- 受影响接口/命令：`[zmemory]` 配置块；`codex zmemory read system://workspace --json`；`codex zmemory read system://defaults --json`；`codex zmemory read system://boot --json`；`codex zmemory stats --json`；`codex zmemory doctor --json`。
- 受影响数据/模式：`validDomains`、`coreMemoryUris`、boot contract 与 runtime profile 的装配来源；配置优先级（用户配置 vs 环境变量 vs 产品默认值）；system 视图中的“默认值”与“当前值”语义边界。
- 受影响用户界面/行为：用户不再只能靠环境变量改 `zmemory` 的域与 boot；system 视图会更清楚地区分“内置默认事实”与“当前用户配置事实”；文档将提供面向编码记忆的推荐配置样板。

## 约束与依赖
- 约束（时间/兼容性/安全/性能/发布窗口等）：只处理 `zmemory`；必须避免影响原生 `memories`；尽量复用现有 `[zmemory]` 配置块而不是新增平行入口；已有环境变量能力若保留，必须定义清晰优先级并更新文档；system 视图必须继续可解释，不能把“产品默认”与“当前配置”混成一团。
- 外部依赖（系统/人员/数据/权限等）：无额外外部系统依赖；依赖仓库现有 config schema、core/zmemory 测试与文档更新流程。

## 实施策略
- 总体方案：先扩展 `[zmemory]` 配置块，使用户可在 `config.toml` 中声明 `valid_domains` 与 `core_memory_uris`；再把 `codex-zmemory` runtime settings 的装配从“仅环境变量 + 硬编码默认”改为“config.toml 优先，必要时再兼容 env，最后回退到产品默认”；随后对齐 `system://workspace` / `system://boot` / `system://defaults` 的事实输出与文档、测试。
- 关键决策：用户配置入口放在现有 `[zmemory]` 块，而不是新增顶层配置；`system://workspace` 始终报告当前实际生效的 runtime profile；`system://defaults` 保持为产品内置默认事实视图，不改成“用户默认值”；文档中补一套面向编码协作记忆的推荐配置示例（例如 `core,project,notes` 与 3 条 boot anchors），但不把这套示例直接强改成新的产品默认值。
- 明确不采用的方案（如有）：不把 `system://workspace` / `system://defaults` 本身做成可重命名或可关闭的视图；不只靠 `.env` 暴露配置而跳过 `config.toml`；不把大量项目知识节点直接塞进默认 `CORE_MEMORY_URIS`；不让 `system://defaults` 反映用户覆盖后的当前值。

## 阶段拆分

### 阶段一：扩展配置与运行时装配
- 目标：让 `[zmemory]` 能承载 runtime profile，并明确优先级。
- 交付物：新的 `ZmemoryToml` / `ZmemoryConfig` 字段、schema 更新、`codex-zmemory` runtime settings 装配逻辑调整、对应配置测试。
- 完成条件：`[zmemory].valid_domains` 与 `[zmemory].core_memory_uris` 能被解析并传入 `codex-zmemory`；未配置时仍能得到当前产品默认；优先级行为有测试覆盖。
- 依赖：现有 `[zmemory]` 配置块与 `codex-zmemory` settings 装配逻辑。

### 阶段二：对齐 system views 与诊断合同
- 目标：让内置 system 视图正确表达“当前配置事实”与“产品默认事实”。
- 交付物：更新后的 `system://workspace` / `system://boot` / `system://defaults` / `stats` / `doctor` 输出与断言。
- 完成条件：用户配置 `valid_domains` / `core_memory_uris` 后，`system://workspace` 和 `system://boot` 能如实反映当前值；`system://defaults` 继续只报告产品内置默认值；相关单测/e2e 覆盖通过。
- 依赖：阶段一完成后的 runtime profile 装配。

### 阶段三：文档与推荐配置收口
- 目标：把“编码记忆使用配置建议”映射到当前仓库文档与示例。
- 交付物：更新后的 `docs/config.md`、`docs/zmemory.md`、`codex-rs/zmemory/README.md`，必要时补 `docs/example-config.md` 示例片段。
- 完成条件：文档明确说明 `[zmemory]` 的新字段、system views 的判读方式、推荐的编码记忆 profile，以及“启动只加载少量高价值 boot”的建议。
- 依赖：阶段一、阶段二完成。

## 测试与验证
- 核心验证：`cargo test -p codex-zmemory --quiet`；`cargo test -p codex-core config::tests:: --quiet`；`cargo test -p codex-core --test all suite::zmemory_e2e:: --quiet`。
- 必过检查：如变更了 `ConfigToml` / `ZmemoryToml` / schema，则运行 `just write-config-schema`；Rust 代码改动后运行 `just fmt`；必要时运行与 `zmemory` 配置或 system view 相关的定向测试。
- 回归验证：验证未配置 `[zmemory].valid_domains` / `core_memory_uris` 时行为保持当前默认；验证显式配置后 `system://workspace` / `system://boot` 变更而 `system://defaults` 不漂移；验证 `system` 域仍为保留只读域。
- 手动检查：核对 `docs/config.md`、`docs/zmemory.md`、`codex-rs/zmemory/README.md` 是否清楚写出推荐的编码记忆配置、默认项目库语义、全局库显式配置方式，以及 `system://workspace` vs `system://defaults` 的区别。
- 未执行的验证（如有）：无。

## 风险与缓解
- 关键风险：把“用户当前配置”误写进 `system://defaults`，导致默认事实与运行时事实混淆；或配置优先级设计不清，破坏现有 env 使用者。
- 触发信号：`system://defaults` 在用户改配置后跟着变化；未配置场景的 `validDomains` / `coreMemoryUris` 与当前基线不一致；CLI / e2e 断言在默认场景下回归失败。
- 缓解措施：先固定语义边界——`system://workspace` 报当前值，`system://defaults` 报产品默认值；在计划执行时先补配置优先级测试，再改 system views；文档中显式写明 precedence 与推荐配置只是示例而非强制默认。
- 回滚/恢复方案（如需要）：若配置优先级或 system view 合同引发回归，可先回退到只保留现有 `path` 配置与 env 驱动的 runtime profile，再单独重做 `[zmemory]` 的 profile 扩展。

## 参考
- `codex-rs/zmemory/src/config.rs:1`
- `codex-rs/zmemory/src/system_views.rs:1`
- `codex-rs/core/src/config/types.rs:513`
- `docs/config.md:39`
- `docs/zmemory.md:1`
- `codex-rs/zmemory/README.md:1`
