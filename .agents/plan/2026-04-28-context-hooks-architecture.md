# Mission Intent 澄清

## 目标陈述

构建并交付纯 Rust、默认开启的 zcontext 内置架构，为 Codex 提供与 `context-mode` 功能面对齐的 session 连续性、上下文保存与治理能力，同时保持对官方上游的 sync-friendly。

## 成功标准（可验证）

1. **Feature gate 生效**：`Feature::ZContext` 在 `codex-rs/features` 中注册为 Stable、默认开启；用户可通过 `[features] zcontext = false`、`codex --disable zcontext` 或 `codex features disable zcontext` 关闭；关闭后所有 zcontext 运行时行为（记录、注入、索引、工具注册、路由提示、诊断/清理）完全停止。
2. **事件记录可用**：`PostToolUse` 事件经脱敏后写入 `session://` 域，分类正确，字段截断遵循 token budget；不存储原始敏感内容。
3. **Snapshot 恢复工作**：`PreCompact` / resume 时能从 `session://` 域读取当前 session 的事件并构建优先排序的上下文 snapshot，注入 additional context；不使用全域导出再过滤。
4. **Ctx 工具注册**：`ctx_execute`、`ctx_search`、`ctx_stats`、`ctx_doctor`、`ctx_purge` 等工具通过 `codex-tools` 注册，复用现有 exec/approval/sandbox/timeout/truncation 路径，feature-gated。
5. **上游解耦**：`codex-core` 只含窄入口调用（feature gate + seam dispatch），所有业务逻辑在 `codex-context-hooks` 或其他本地-owned crate；官方上游高 churn 文件改动限于 enum 注册、config 透传、tool registry 薄注册、hook seam 调用。
6. **Schema/文档/测试同步**：config schema 更新；`docs/` 相关文档更新；每个功能面有对应测试覆盖；snapshot 测试覆盖 UI 变更。

## 非目标（明确排除）

- 不照搬外部 Node.js 运行时或 MCP server
- 不替换现有用户自定义 hook command 框架（`codex-hooks`）
- 不新建独立存储引擎（优先扩展 `codex-zmemory`）
- 不旁路 Codex 现有 approval/sandbox 机制
- 不在模型上下文里注入 raw event dump
- 不复制 `context-mode` 的 ELv2 代码
- 不在官方上游代码路径里大面积嵌入本地特性
- 不追求与 `context-mode` 100% 功能 parity，而是对齐核心价值面

## 现有资产盘点

| 资产 | 路径 | 状态 | 备注 |
|---|---|---|---|
| Feature enum | `codex-rs/features/src/lib.rs:81` | ✅ `ZContext` 已注册 | 需确认 stage 默认 Stable |
| context-hooks crate | `codex-rs/context-hooks/` | ⚠️ 草稿 | event_recorder 缺脱敏；snapshot 用全域导出 |
| Config types | `codex-rs/core/src/config/types.rs` | ⚠️ 需修正 | `from_toml(None)` 解析为 disabled |
| Hook runtime seam | `codex-rs/core/src/hook_runtime.rs` | ✅ 已有 seam | 只能加薄调用 |
| zmemory tool API | `codex-rs/zmemory/src/tool_api.rs` | ✅ 可用 | session 域需确认/补齐 |
| Tool registry | `codex-rs/tools/src/tool_registry_plan.rs` | ✅ 可用 | ctx 工具需 feature-gated 注册 |

## 退出条件

意图分析完成，以下条件均已明确且可供上下文拆解使用：
- [x] 目标陈述：一句话目标 + 六条可验证成功标准
- [x] 非目标：七条明确排除项
- [x] 现有资产盘点：六个关键资产及其状态
- [x] 执行约束已从原文"本地分叉优先原则"和"执行前 Gate"中提取
- [x] 风险信号与缓解措施已从原文"风险"章节中提取

---

# ZContext / Context Hooks 内置架构

## 背景
- 当前状态：上一轮中断前已留下未完成的 `codex-rs/context-hooks/` 草稿、配置字段草稿和 `zmemory` `session` 域草稿改动；仓库当前没有可引用的 `codex-rs/PRD-context-hooks.md`，执行阶段必须以本计划、现有代码 seam、`context-mode` 参考和当前 diff 审查结果为准，先分类保留、调整、拆分或重写草稿。
- 参考目标：尽量对齐 `https://github.com/mksglu/context-mode` 的产品能力，而不是只做最小 `PostToolUse` 记录。`context-mode` 的核心能力可归纳为 context saving、session continuity、think in code、output compression，以及 stats/doctor/purge/insight 等治理工具；其 hook 面包括 `PreToolUse` 路由、`PostToolUse` 事件捕获、`UserPromptSubmit` 用户意图捕获、`PreCompact` snapshot、`SessionStart` 路由/恢复注入。
- 本地约束：不要耦合官方上游 `openai/codex` 的高 churn 代码；不要引入外部 Node.js MCP server；不要复制 `context-mode` 的 ELv2 代码；不要把主要逻辑堆进 `codex-core`。优先复用本地分叉已有能力：`codex-context-hooks` 草稿、`codex-hooks`、`codex-zmemory`、`codex-tools`、`codex-utils-output-truncation`、现有 shell/unified exec、additional context seam、multi-agent/subagent 运行面、`ztok`/路由提示能力。
- 默认策略：新增 `features.zcontext` 作为 zcontext 全功能总开关，默认开启；用户可通过 `[features] zcontext = false`、profile feature、`codex --disable zcontext` 或 `codex features disable zcontext` 关闭全部相关行为。`[context_hooks]` 只能作为参数/兼容子配置，不能成为第二套功能真相源。
- 本轮调查：主线程读取了本地 `codex-rs/features`、`core/src/config`、`context-hooks`、`hook_runtime` 与 `mksglu/context-mode` 的 README、hooks、server/runtime/store/security 源码；另启动多个子会话分别调查上游功能、本地配置落点和计划缺口。计划结论以已验证的本地文件与临时克隆的上游仓库事实为准，禁止把未完成子会话输出当成唯一证据。

## 目标
- 目标结果：形成并实施一个纯 Rust、默认开启、可通过 `features.zcontext` 显式关闭、尽量对齐 `context-mode` 功能面的内置 zcontext 架构。
- 完成定义（DoD）：`features.zcontext` 默认开启；关闭后所有 zcontext 记录、注入、索引、工具注册、路由提示、诊断/清理后台行为均停止；开启后能记录关键 session 事件、按需检索历史、在 resume/compact 后恢复上下文、提供 ctx 执行/索引/搜索类工具、提供 stats/doctor/purge 治理面，并更新 schema、文档、测试和必要技能说明。实现必须保持官方上游 sync-friendly：核心行为在本地-owned crate/module，官方上游热点文件只允许薄接线。
- 非目标：不照搬外部 Node.js 运行时；不替换现有用户自定义 hook command 框架；不新建独立存储引擎作为第一选择；不旁路 Codex 现有 approval/sandbox；不在模型上下文里注入 raw event dump；不复制 `context-mode` 的 ELv2 代码；不为了对齐 `context-mode` 而在官方上游代码路径里大面积嵌入本地特性。

## 本地分叉优先原则
- 本地实现优先级：先复用本地分叉已有模块，再扩展本地-owned 模块，最后才在官方上游文件增加最小 seam。
- `codex-context-hooks` 是 zcontext 业务核心：事件模型、脱敏、分类、snapshot、压缩预算、parity 规则和后续 stats/doctor/purge 逻辑默认放在这里或拆到新的本地 crate。
- `codex-core` 只做运行时事实传递和窄入口调用：feature gate、session/turn/tool/compact seam、additional context 记录。不得在 `core` 里实现 zcontext 事件分类、脱敏、排序、索引策略或 ctx 工具业务。
- `codex-zmemory` 是持久化/检索事实源：新增 `session://` / `ctx://` 域能力时优先扩展其 service/tool API，而不是创建旁路 SQLite。
- `codex-tools` 是工具注册事实源：ctx 工具应作为 feature-gated tool specs/handlers 接入，复用现有 approval/sandbox/timeout/output truncation，不新增隐藏执行路径。
- multi-agent/subagent 对齐方式：不复刻 `context-mode` 的平台适配层；在本地已有 `Feature::Collab` / `Feature::MultiAgentV2` 与子会话通知流上捕获 agent/task 事件，并把 worker 目标、任务状态、handoff 摘要写入 session 事件。
- 上游同步边界：允许改官方上游文件的范围限于 enum 注册、config 字段透传、tool registry 薄注册、hook seam 调用和测试装配；若实现需要超过薄 seam，先抽到本地-owned 文件。

## 功能对齐矩阵

| `context-mode` 能力 | 本仓库 zcontext 对齐方式 | 阶段 |
|---|---|---|
| Context Saving | `ctx_execute` / `ctx_batch_execute` / `ctx_execute_file` 通过现有 shell/unified exec 路径执行并写入 `ctx://...` 索引，只返回压缩摘要 | 第三阶段 |
| Session Continuity | `PostToolUse`、`UserPromptSubmit`、compact/resume、turn end、agent/task 事件写入 `session://...`，resume/compact 注入小型 snapshot + 查询提示 | 第一、二阶段 |
| Think in Code | 将 `ctx_batch_execute` 设计成批量主路径，`ctx_execute` 是单次辅助路径；鼓励模型写脚本处理大输出而不是读取大量文件 | 第三阶段 |
| Output Compression | 提供 zcontext 压缩风格指令与输出预算治理，优先复用现有 prompt/ztok/输出裁剪能力，不硬改所有回复风格 | 第五阶段 |
| FTS5 / BM25 Search | 优先复用/扩展 `zmemory search` 与 URI scope；必要时在 `zmemory` 内部补 session/ctx scoped 索引能力，不新增旁路 DB | 第二、三阶段 |
| PreToolUse Routing | 映射为 zcontext tool routing instructions + 可选本地 pre-tool guard；优先提示/重路由大输出工具到 ctx 工具，不阻断必要的普通工具 | 第三、五阶段 |
| PostToolUse Capture | 复用 `codex-hooks` `PostToolUse` payload 与本地 `codex-context-hooks` 事件抽取，覆盖 shell/apply_patch/MCP/agent 结果 | 第一阶段 |
| UserPromptSubmit Capture | 复用 `run_user_prompt_submit_hooks` additional context seam，同时记录用户决策、纠正、任务目标、角色/约束变化 | 第一阶段 |
| PreCompact / SessionStart | 映射到 Codex compact/resume/initial context seam，注入结构化 snapshot 和 on-demand search 指令 | 第二阶段 |
| Fresh Session Clean Slate | `context-mode` 新 session 会清旧会话；本地默认更保守：session-scoped 隔离 + retention/purge，是否自动清理由 `[context_hooks]` retention 参数控制 | 第二、四阶段 |
| Stats / Doctor / Purge / Insight | 提供 `zcontext stats/doctor/purge` 或等价内置工具；`insight` 先做结构化 stats，不做本地 Web UI | 第四阶段 |
| Fetch and Index | 后置实现 `ctx_fetch_and_index`，必须复用现有 web/network policy，不作为首批闭环阻塞项 | 第三阶段后半 |
| Multi-session / Subagent Continuity | 复用本地 multi-agent/subagent 事件与 canonical task name，将子会话目标、状态、结果摘要纳入 `session://...`，不复刻上游平台 adapter | 第一、二阶段 |

## 配置与 Gate
- `features.zcontext` 是最高层总开关，新增到 `codex-rs/features` 的 `Feature` enum 与 `FEATURES` 注册表，`Stage::Stable`，`default_enabled: true`。
- 有效启用条件：`zcontext_enabled = features.enabled(Feature::ZContext) && context_hooks.enabled.unwrap_or(true)`。其中 `[context_hooks].enabled` 若保留，只作为兼容/子开关；文档主推 `[features] zcontext = false`。
- 推荐关闭方式：
  - 单次运行：`codex --disable zcontext`
  - 持久 CLI 修改：`codex features disable zcontext`
  - 持久配置：
    ```toml
    [features]
    zcontext = false
    ```
  - profile 级关闭：
    ```toml
    [profiles.no_context.features]
    zcontext = false
    ```
- `features.zcontext = false` 必须关闭：内置 event 记录、snapshot 构建/注入、`session://...` 自动读写、ctx 工具注册、路由提示、stats/doctor/purge、retention/cleanup 后台任务、后续 `ctx_fetch_and_index`。
- `[context_hooks]` 只承载参数：`snapshot_token_budget`、`max_events_per_snapshot`、retention、write timeout、redaction 策略开关等；不得成为与 `features.zcontext` 并列的第二套功能真相源。
- `Feature::ZContext` 不进入 `/experimental` 菜单；它是稳定默认能力，配置面只承担关闭/诊断，不承担“试验开关”语义。
- `Feature::CodexHooks` 与 `Feature::ZContext` 关系：`codex_hooks` 控制用户自定义 hook command 框架；`zcontext` 控制内置 context-mode parity 能力。二者不互相替代。若实现复用 hook runtime seam，也必须保证关闭 `zcontext` 不关闭用户 hooks，关闭 `codex_hooks` 不意外绕过 `zcontext` 的内置路径，除非阶段实现明确选择依赖并在文档/测试中说明。

## 影响
- 受影响模块：`codex-rs/features`、`codex-rs/context-hooks`、`codex-rs/core`、`codex-rs/config`、`codex-rs/zmemory`、`codex-rs/tools`、`docs/`、`.codex/skills/upgrade-rtk/SKILL.md`、`codex-rs/core/config.schema.json`。
- 受影响接口/命令：`[features].zcontext`；`[context_hooks]` 子配置；内部 `PostToolUse` / `UserPromptSubmit` / `SessionStart` / compact seam；未来 `ctx_execute`、`ctx_batch_execute`、`ctx_execute_file`、`ctx_index`、`ctx_search`、`ctx_fetch_and_index`、`ctx_stats`、`ctx_doctor`、`ctx_purge`。
- 受影响数据/模式：新增 `session://...` session event 域；后续新增 `ctx://...` 或 `batch://...` 执行/索引域；必须防止污染既有 `core://`、`project://`、`notes://` 用户记忆。
- 受影响用户界面/行为：默认开启后，工具调用、恢复会话、压缩后继续、ctx 工具输出和诊断命令会出现 zcontext 行为；显式关闭后这些行为必须完全消失。
- 上游同步影响：官方上游文件中的改动必须保持可审查小补丁形态；每阶段结束时检查是否把本地功能逻辑写进了 `core/src/session/*`、`core/src/tools/*`、`tui/src/chatwidget.rs` 等高 churn 文件，若超过接线职责则先抽回本地-owned 文件。

## 架构策略
- 总体方案：采用“小 crate 承载行为 + core 薄接线 + zmemory 持久化/检索 + codex-tools 注册 ctx 工具”的分层架构。
- `codex-context-hooks` 负责事件模型、事件抽取、脱敏、格式化、优先级、预算裁剪、zmemory 参数构造、snapshot 生成和 context-mode parity 的纯业务逻辑。
- `codex-core` 只负责在 turn/session/tool/compact seam 调用窄入口，并把 `features.zcontext` 与 `[context_hooks]` 的有效配置传入。
- `codex-zmemory` 优先承载 session 事件与索引；snapshot 构建必须按当前 session scope 查询或读取，禁止先导出整个 `session` domain 再内存过滤。
- `codex-tools` 负责 ctx 工具 spec 与 handler 注册；所有执行型 ctx 工具必须复用现有 shell/unified exec、approval、sandbox、超时和输出截断链路。
- Markdown 可作为展示格式，但结构化 snapshot 与排序不能依赖从 markdown 反解析字段；事件应有稳定结构化 metadata。
- `context-mode` 参考只提供行为契约，不提供实现依赖。上游源码中的 Node runtime detection、platform adapter、自修复 plugin cache、ELv2 server/store/security 实现都不能直接移植；本地只对齐“能力是否存在、默认行为、失败开放、可诊断、可清理、可搜索”的产品语义。
- 子会话/子代理上下文属于本地分叉优势能力：优先把 `InterAgentCommunication`、thread notifications、mission/worker 状态摘要映射成本地 zcontext events，再进入 snapshot，而不是把多 agent 行为建模成 `context-mode` 平台 adapter。
- 执行型 ctx 工具不拥有第二套权限模型。若需要拦截或引导 Bash/Read/Grep/WebFetch 等大输出工具，先通过 tool routing instructions 与 existing registry metadata 达成；只有有明确测试证明需要 hard guard 时，再加本地 pre-tool guard，且必须可观察、可关闭。

## 阶段拆分

### 阶段 0：草稿审查与 Feature Gate 基线
- 目标：建立 `features.zcontext` 总开关、修正默认开启语义，并清理当前未完成草稿的提交边界。
- 交付物：`Feature::ZContext`、`features.zcontext` schema、默认开启测试、feature/profile/CLI disable 测试、`ContextHooksConfig` 默认启用修正、当前 diff 分类记录、上游耦合审查记录。
- 完成条件：`Features::with_defaults()` 包含 `ZContext`；`[features] zcontext = false`、profile 关闭、`codex --disable zcontext` 与 `codex features disable zcontext` 能让 `config.features.enabled(Feature::ZContext)` 为 false；未配置 `[context_hooks]` 与空 `[context_hooks]` 不会禁用 zcontext；提交不包含无关 `zinit`、shutdown rollout、native-tldr 等改动；官方上游热点文件只包含必要 seam。
- 依赖：`codex-rs/features/src/lib.rs`、`codex-rs/features/src/tests.rs`、`codex-rs/core/src/config/*`、`just write-config-schema`。
- 本地分叉复用：保留并修正已有 `codex-rs/context-hooks/` 草稿，不重开新实现；如果草稿与目标冲突，优先在本地 crate 内重构而不是把逻辑搬进 `core`。

### 阶段 1：Session 事件模型、脱敏与记录
- 目标：对齐 `context-mode` 的 session continuity 基础，不只记录工具输出，也记录用户决策和关键生命周期事件。
- 交付物：结构化事件模型、脱敏器、markdown 展示格式、URI 构造、`PostToolUse` 记录、`UserPromptSubmit` 决策/纠正记录、turn end 或 session lifecycle 记录、子 agent/task 事件记录、zmemory create 调用、失败开放 warning。
- 完成条件：默认开启时成功/失败工具调用、用户关键指令、文件编辑、git 操作、错误、子会话派工/完成/失败都形成可搜索 `session://...` 记录；记录内容不包含已知 secret/token/header/URL query/userinfo 测试样本；`features.zcontext = false` 时无写入；zmemory 写失败不改变工具输出。
- 依赖：`codex_hooks::PostToolUseRequest`、`UserPromptSubmit`、`codex_zmemory::tool_api::run_zmemory_tool_with_context`、turn cwd 下 project-scoped zmemory config 解析。
- 本地分叉复用：直接扩展 `codex-context-hooks/src/event_recorder.rs`，不要把事件分类写到 `core/src/hook_runtime.rs`；子代理事件优先从现有 local multi-agent 通知/状态模型转换。

### 阶段 2：Session-Scoped 检索与 Snapshot 注入
- 目标：对齐 `context-mode` compact/resume 恢复模型：不注入 raw events，只注入小型结构化 snapshot 和 on-demand 查询提示。
- 交付物：session-scoped 查询/读取入口、snapshot builder、优先级排序、token budget 裁剪、stable marker/title、`SessionStart(resume)` 接线、compact/pre-compact 或相邻 compaction seam 接线、retention/cleanup 策略。
- 完成条件：已有当前 session events 时，resume/compact 后模型可见 history 包含结构化 snapshot；snapshot 包含当前任务、最近用户决策、文件/命令状态、子代理状态、错误与修复状态、可搜索入口；无 events 或读取失败时不注入且不阻断会话；测试证明不会读取其他 session，也不会通过全域 domain export 构建 snapshot。
- 依赖：`core/src/hook_runtime.rs` additional context 注入链、`core/src/compact_remote.rs` 或相邻 compaction 处理链、`codex-utils-output-truncation`、`zmemory search/read/export` scope 能力。
- 本地分叉复用：修正现有 `snapshot.rs` 的全域 `Export(domain = "session")` 草稿，优先给 `zmemory` 增加 scoped search/read，而不是在 snapshot builder 内全量过滤。

### 阶段 3：Context Saving 与 Think-in-Code 工具
- 目标：对齐 `context-mode` 的 6 个 sandbox 工具中与 Codex 最相关的能力，让模型通过执行脚本、批量命令和索引搜索节省上下文。
- 交付物：`ctx_execute`、`ctx_execute_file`、`ctx_batch_execute`、`ctx_index`、`ctx_search`、后续 `ctx_fetch_and_index` 的 tool spec、handler、输出 cap、source scoped search、空库/空结果提示、路由提示。
- 完成条件：模型可调用 ctx 工具执行/索引/搜索；执行工具不旁路 approval/sandbox；大输出默认写入 zcontext 索引并只回传摘要；失败命令结果可见且不中断整批除非执行策略要求；`features.zcontext = false` 时这些工具不注册、路由提示不出现。
- 依赖：现有 `ShellHandler` / `UnifiedExecHandler` / shell tool runtime 复用决策，`codex-tools` `ToolRegistryPlan`，`zmemory` batch/search 能力，网络 fetch policy。
- 本地分叉复用：若 `ztok` / native-tldr 已能承担代码结构搜索与摘要，不重复造代码搜索工具；ctx 工具聚焦“执行/索引/检索大输出”，结构化代码理解仍优先路由到 `ztldr`。

### 阶段 4：Stats / Doctor / Purge / Insight 治理面
- 目标：对齐 `context-mode` 的可诊断、可清理、可度量能力，避免默认开启后成为不可观察黑盒。
- 交付物：`zcontext stats`、`zcontext doctor`、`zcontext purge` 或等价内置工具；写入失败次数、写入耗时、snapshot 事件数、token 裁剪、脱敏命中、ctx 工具节省估算；结构化 insight 输出。
- 完成条件：用户能确认 zcontext 是否启用、数据库/索引是否可用、hooks/compact 接线是否生效、当前 session 写入数量和最近错误；可安全清理 `session://...` / `ctx://...` 数据；`features.zcontext = false` 时治理入口显示 disabled 或不注册且不触发读写。
- 依赖：`zmemory stats/audit/delete-path` 能力、CLI 或 tool registry 入口、文档。
- 本地分叉复用：优先扩展现有 `zmemory doctor/stats` 和 CLI 子命令形态；`ctx_upgrade` 这类上游插件自修复能力不直接对齐，本地只需要 schema/config/DB migration doctor。

### 阶段 5：Output Compression 与路由策略
- 目标：对齐 `context-mode` 的输出压缩理念，但不破坏 Codex 既有交互风格和安全/审批说明。
- 交付物：zcontext routing instructions、ctx 工具优先级提示、输出压缩指令、与 `ztok` / prompt 注入 / existing truncation 的边界说明。
- 完成条件：启用 zcontext 时模型被明确引导优先用 ctx 工具处理大输出、用搜索按需取细节、少输出低价值废话；安全警告、不可逆操作、用户困惑场景仍保留必要解释；关闭 zcontext 后不注入这些路由提示。
- 依赖：现有 prompt 注入、tool routing 提示、`ztok`/embedded prompt 边界。
- 本地分叉复用：若已有 `codex-caveman` 或 Embedded ZTOK 提示可覆盖压缩风格，只引用/收敛其原则，不复制一套永久 prompt；zcontext routing 应保持短小、可测试、feature-gated。

### 阶段 6：文档、技能治理与收口
- 目标：补齐用户配置说明、开发边界说明、测试和回归 gates。
- 交付物：`docs/` 配置文档、context-mode parity 表、默认开启与关闭方式、数据位置与清理方式、`.codex/skills/upgrade-rtk/SKILL.md` 中与 zcontext/session continuity/ztok-rtk 升级边界相关的指引更新、llmdoc 反思或稳定文档更新建议。
- 完成条件：文档说明 `features.zcontext` 默认开启、关闭方式、`[context_hooks]` 子配置、数据域、限制、清理方式、上游解耦原则、本地分叉复用路径和未对齐能力；定向测试通过；已明确未覆盖的全量验证与剩余风险。
- 依赖：前序阶段实现结果和验证输出。

## 测试与验证
- 核心验证：`cargo nextest run -p codex-features`；`cargo nextest run -p codex-context-hooks`；`cargo nextest run -p codex-core` 中 zcontext 定向用例；`cargo nextest run -p codex-zmemory` 中 session/ctx 域相关用例；ctx 工具阶段运行 `cargo nextest run -p codex-tools`。
- 必过检查：`just fmt`；配置变更后 `just write-config-schema`；若修改 Rust dependency 或 lockfile，则运行 `just bazel-lock-update` 和 `just bazel-lock-check`。
- Gate 验证：默认开启；`[features] zcontext = false` 完全关闭记录、注入和工具注册；profile 关闭覆盖默认；`[context_hooks]` 空表不关闭；若兼容 `[context_hooks] enabled = false`，则确认它作为额外关闭开关生效。
- 解耦验证：每阶段用 `git diff --stat` 和 targeted review 确认官方上游热点文件只承担薄接线；若新增逻辑超过 seam，必须迁回 `context-hooks`、`zmemory`、`tools` 或新的本地-owned 模块。
- 安全验证：脱敏测试覆盖 token/key/secret/password/header、URL userinfo/query、Authorization/Bearer、常见云凭证字段；脱敏发生在截断前。
- 检索验证：snapshot 仅包含当前 session；不会全域导出 `session`；无 events、zmemory 错误、索引损坏均失败开放且可观察。
- 执行验证：ctx 执行型工具复用现有 approval/sandbox/timeout；大输出只回传摘要并可用 `ctx_search` 找回；关闭 feature 后工具不可见。
- 子代理验证：multi-agent/subagent 启用时，派工、worker 输出摘要、失败和完成状态进入当前 session snapshot；关闭 `features.zcontext` 后这些事件不写入 zcontext，但原有 multi-agent 功能不受影响。
- 手动检查：默认配置下执行一次 shell/tool 调用，确认 `zmemory` 能搜索到 `session://...`；设置 `[features] zcontext = false` 后确认不再写入/注入/注册工具；恢复同一 session 时确认 snapshot 标题、事件数、预算说明和搜索提示存在；执行 `zcontext doctor/stats/purge` 或等价工具确认治理面可用。

## 风险与缓解
- 关键风险：默认开启导致更多用户路径暴露写入延迟或 snapshot 噪音；工具输入/输出写入 durable memory 前脱敏不足；同步 SQLite 写入影响工具延迟；全域扫描 `session` 导致恢复慢或跨 session 泄漏；ctx 执行工具旁路 approval/sandbox；feature off 后后台任务仍读写；为了追求 parity 把本地实现耦合进官方上游热点文件；当前工作区存在与本计划不直接相关的改动，提交边界可能被污染。
- 触发信号：PostToolUse p99 超过目标；脱敏测试样本出现在 `session://...` 记录；snapshot 包含其他 session 的记录；ctx 工具未经过现有 exec handler；`features.zcontext = false` 后仍有写入/工具注册/路由提示；`core/src/session/*`、`core/src/tools/*`、`tui/src/chatwidget.rs` 出现大段 zcontext 业务逻辑；提交前 staged 文件超出本计划范围。
- 缓解措施：写入失败开放并记录 warning；写入前统一脱敏并用测试锁定；保留 `features.zcontext` 快速关闭路径；必要时引入 bounded async queue；snapshot 限制 `max_events_per_snapshot` 和 token budget；snapshot builder 必须 session-scoped；ctx 执行工具必须调用现有执行路径；执行前先审查并修正草稿 diff；将 zcontext 业务逻辑抽回本地-owned crate/module；提交前只精准暂存本计划相关文件。
- 回滚/恢复方案：用户可通过 `[features] zcontext = false` 关闭全部运行时行为；代码层可按阶段回滚 `core` 接线、`context-hooks` crate、ctx 工具注册；`session://` 与 `ctx://` 数据可用既有或新增 purge/delete-path 清理。

## 执行前 Gate
- Diff 分类：先检查当前 `git status --short` 与 `git diff`，将 `codex-rs/context-hooks/`、zcontext 配置、`zmemory` `session` 域、features gate 相关改动列为候选保留；将 `zinit_cmd`、shutdown rollout、native-tldr 或其他无关改动拆出本任务提交边界。
- Feature 修正：若未新增 `Feature::ZContext` / `features.zcontext` 或默认不是开启，必须先修正并补测试。
- 默认启用修正：若现有草稿仍把 `ContextHooksConfig::from_toml(None)` 或空 `[context_hooks]` 解析为 disabled，必须先修正并补测试。
- 安全修正：若现有草稿仍直接 pretty-print 原始 `tool_input` / `tool_response` 后截断，必须改为先脱敏再截断。
- Snapshot 修正：若现有草稿仍通过 `Export(domain = "session")` 全域导出构建 snapshot，必须改为 session-scoped 查询或读取。
- 接线修正：`core` 接线只允许调用窄入口，不在 `core` 内实现事件格式化、脱敏、snapshot 排序或 ctx 工具业务逻辑。
- 上游解耦修正：若某个改动为了 zcontext parity 需要碰官方上游高 churn 文件，先确认是否能通过本地分叉 crate/module 或 adapter seam 完成；不能完成时，只保留最薄注册/调用代码，并在提交说明中列出原因。

## 参考
- `https://github.com/mksglu/context-mode/`
- `https://raw.githubusercontent.com/mksglu/context-mode/main/README.md`
- `mksglu/context-mode` 参考源码：`hooks/hooks.json`、`hooks/sessionstart.mjs`、`hooks/precompact.mjs`、`hooks/posttooluse.mjs`、`hooks/userpromptsubmit.mjs`、`hooks/pretooluse.mjs`、`src/server.ts`、`src/store.ts`、`src/types.ts`、`src/runtime.ts`、`src/security.ts`。
- `https://raw.githubusercontent.com/mksglu/context-mode/main/CONTRIBUTING.md`
- `codex-rs/features/src/lib.rs`
- `codex-rs/hooks/src/events/post_tool_use.rs`
- `codex-rs/hooks/src/events/session_start.rs`
- `codex-rs/hooks/src/events/user_prompt_submit.rs`
- `codex-rs/core/src/hook_runtime.rs`
- `codex-rs/core/src/tools/registry.rs`
- `codex-rs/core/src/tools/handlers/zmemory.rs`
- `codex-rs/zmemory/src/tool_api.rs`
- `codex-rs/zmemory/src/service/create.rs`
- `codex-rs/zmemory/src/service/search.rs`
- `codex-rs/tools/src/tool_registry_plan.rs`
- `codex-rs/tools/src/tool_registry_plan_types.rs`
- `codex-rs/core/src/compact_remote.rs`
- `codex-rs/utils/output-truncation/src/lib.rs`
- `.codex/skills/upgrade-rtk/SKILL.md`

## 当前实现快照
- `codex-rs/context-hooks/src/event_recorder.rs` 已有 `PostToolUse` 草稿，但当前风险是先 pretty-print 原始输入/输出再截断，缺少统一脱敏；只能作为起点，不能视为可上线实现。
- `codex-rs/context-hooks/src/snapshot.rs` 已有 snapshot 草稿，但当前通过 `Export(domain = "session")` 全域导出再按 session id 过滤，必须改成 session-scoped 查询/读取后才能进入主路径。
- `codex-rs/core/src/config/types.rs` 当前 `ContextHooksConfig::from_toml(None)` 与空 `[context_hooks]` 会解析为 disabled；这与 `features.zcontext` 默认开启目标冲突，阶段 0 必须优先修正。
- `codex-rs/features/src/lib.rs` 当前没有 `Feature::ZContext`；阶段 0 必须新增稳定默认开启 feature，并让 schema、CLI `--enable/--disable`、`codex features enable/disable`、profile 配置都共享同一真相源。
- `codex-rs/core/src/hook_runtime.rs` 已有 `PostToolUse`、`UserPromptSubmit` 和 additional context 注入 seam；zcontext 只能在这里加薄调用，不应扩展业务逻辑。
 
 ---
 
 # 上下文收集阶段报告 (context)
 
 ## 1. Feature Gate 现状
 
 - **`Feature::ZContext`** 已在 `codex-rs/features/src/lib.rs:81` 注册为 Stable、默认开启 (`default_enabled: true`)，key 为 `"zcontext"`。
 - Feature spec 位于约第 754 行，已通过 `features/src/tests.rs` 测试验证 stage/default_enabled/key 三项正确。
 - `core/src/config/config_tests.rs` 已有 5 条测试覆盖默认开启、`[features] zcontext = false`、CLI `--disable zcontext`、profile disable 四种路径。
 - **结论：Feature gate 已完成且经过测试，阶段 0 不需要额外修正。**
 
 ## 2. Config Types 现状
 
 - **`ContextHooksToml`**（`core/src/config/types.rs:32`）：TOML 映射层，字段 `enabled: Option<bool>`、`snapshot_token_budget: Option<usize>`、`max_events_per_snapshot: Option<usize>`。
 - **`ContextHooksConfig`**（`core/src/config/types.rs:36`）：运行时配置，`from_toml(None)` 和空 `[context_hooks]` 均解析为 `enabled: true`，使用 `codex_context_hooks::ContextHooksSettings::default()` 的 token budget 和 max_events。
 - `Default for ContextHooksConfig` 委托 `from_toml(None)` → enabled = true。
 - 配置集成路径：`Config` 在 `core/src/config/mod.rs:504` 持有 `context_hooks: ContextHooksConfig`，从 TOML 构造于第 2536 行。
 - `codex-rs/config/src/config_toml.rs:368` 定义了 `ContextHooksToml` 在顶层 config toml 结构中的位置。
 - config_tests 已有 3 条覆盖默认启用、空表保持启用、`enabled = false` 显式禁用。
 - **结论：Config 层已完成且与 `features.zcontext` 默认开启目标对齐，不需要修正。**
 
 ## 3. context-hooks Crate 草稿现状
 
 路径：`codex-rs/context-hooks/`，已注册为 workspace member，core 已依赖。
 
 ### event_recorder.rs
 - 导出 `record_post_tool_use_event(context, request) -> Result<()>`
 - 从 `PostToolUseRequest` 构建 `ContextHookRecord`（uri, content, category）
 - URI 模式：`session://{session_id}/events/{turn_id}/{tool_use_id}`
 - 分类：`classify_event` 将工具调用分为 Error/FileEdit/Git/Command/Tool 五类
 - 内容：pretty-print 原始 `tool_input` + `tool_response` 后通过 `truncate_field` 截断到 800 tokens
 - **风险（已知）：当前直接 pretty-print 原始 tool_input/tool_response，未做脱敏（sanitize），可能将敏感内容写入 session:// 域**
 - 通过 `run_zmemory_tool_with_context` 以 `ZmemoryToolAction::Create` 写入 zmemory
 - 单元测试已覆盖 classify、URI 格式
 
 ### snapshot.rs
 - 导出 `build_session_snapshot(context, session_id, settings) -> Result<Option<String>>`
 - **当前用 `ZmemoryToolAction::Export` + `domain: Some("session")` 全域导出，然后在内存中按 session_id 前缀过滤**
 - **风险（已知）：全域导出会读取所有 session 的事件数据，违反 session 隔离原则**
 - 按优先级排序（Error > FileEdit > Git > Command > Tool），受 `max_events_per_snapshot` 限制
 - 输出 Markdown 格式 summary，最终截断到 `snapshot_token_budget` tokens
 - 单元测试已覆盖 extract、select 排序
 
 ### lib.rs
 - 公开 `ContextHooksSettings`、`ZmemoryContext`、`record_post_tool_use_event`、`build_session_snapshot`
 - `ContextHooksSettings::default()` = 2000 token budget, 50 max events
 - `ZmemoryContext` 封装 codex_home/cwd/zmemory_path/settings
 
 ## 4. 运行时接入现状 — 关键缺口
 
 ### event_recorder 未接入主路径
 - `record_post_tool_use_event` **未在 `codex-core` 中被调用**
 - `hook_runtime.rs` 中的 `run_post_tool_use_hooks` 会触发外部 hook 的 PostToolUse 事件，但不会调用 zcontext 内置的事件记录
 - **需要接入点**：在 `run_post_tool_use_hooks` 完成后或 `ToolHandler::handle` 返回后，调用 `record_post_tool_use_event`
 
 ### snapshot 未接入主路径
 - `build_session_snapshot` **未在 `codex-core` 中被调用**
 - PreCompact/resume 路径需要注入 snapshot 作为 additional context
 - **需要接入点**：在 `compact.rs`/`compact_remote.rs` 的 `InitialContextInjection::BeforeLastUserMessage` 路径中，或在 `build_initial_context` 中
 
 ### ctx 工具未注册
 - `ctx_execute`、`ctx_search`、`ctx_stats`、`ctx_doctor`、`ctx_purge` 等工具 **尚不存在**
 - 需要 feature-gated 注册到 `ToolRegistryPlan` 并创建对应 ToolHandler
 
 ## 5. zmemory 层能力
 
 - **`session` 域已在 `DEFAULT_VALID_DOMAINS`**（`zmemory/src/config.rs`）：`["core", "project", "notes", "session"]`
 - URI 模式 `session://{session_id}/events/{turn_id}/{tool_use_id}` 在 domain="session", path="{session_id}/events/{turn_id}/{tool_use_id}" 下会被 zmemory 的 paths 表接受
 - `run_zmemory_tool_with_context` 是同步 API，直接操作 SQLite；context-hooks crate 已经用它做 Create 和 Export
 - `ZmemoryToolCallParam` 支持 Read/Search/Create/Export/DeletePath/Stats/Doctor 等完整 action set
 - **问题**：当前 Export 不支持 session-scoped 过滤；需要改为按 URI 前缀查询（Search action + query）或新增 Read with URI prefix
 - namespace 机制已存在但 context-hooks 尚未使用（草稿中 `ZmemoryContext` 不传 namespace）
 
 ## 6. Hook Runtime Seam 分析
 
 `hook_runtime.rs` 提供的关键 seam：
 - `run_post_tool_use_hooks`：已有 PostToolUse hook 执行，返回 `HookRuntimeOutcome { should_stop, additional_contexts }`
 - `run_pending_session_start_hooks`：SessionStart hook 执行
 - `run_user_prompt_submit_hooks`：UserPromptSubmit hook 执行
 - `record_additional_contexts`：将 additional_contexts 注入为 developer message
 
 **zcontext 内置行为不应通过外部 hook command 执行**，而是直接调用 context-hooks crate 函数。接入方式：
 - PostToolUse 事件记录：在 `run_post_tool_use_hooks` 返回后，增加一个 feature-gated 调用 `codex_context_hooks::record_post_tool_use_event`
 - PreCompact snapshot：在 compaction 路径的 `build_initial_context` 中，feature-gated 注入 snapshot
 - UserPromptSubmit 意图捕获：类似 PostToolUse，在 `run_user_prompt_submit_hooks` 后增加记录
 
 ## 7. Compaction/Initial Context 注入 Seam
 
 - `build_initial_context`（`session/mod.rs:2492`）构建每个新 turn 的初始上下文
 - Compaction 路径在 `compact.rs:261` 和 `compact_remote.rs:250` 调用 `build_initial_context`
 - `InitialContextInjection::BeforeLastUserMessage` 确保压缩后重新注入初始上下文
 - **zcontext snapshot 注入点**：在 `build_initial_context` 中，feature-gated 追加 snapshot 内容到 `developer_sections` 或 `contextual_user_sections`
 - 替代方案：在 compaction 路径中直接调用 `build_session_snapshot`，将结果作为 additional_context 注入
 
 ## 8. Tool Registry 和 Handler 模式
 
 - `ToolRegistryPlan`（`tools/src/tool_registry_plan.rs`）在 `build_tool_registry_plan` 中构建工具列表
 - `ToolHandlerKind` 枚举已包含 `Zmemory` variant
 - 工具注册通过 `plan.push_spec()` + `plan.register_handler(name, kind)` 完成
 - ctx 工具需要：
   1. 在 `tool_registry_plan_types.rs` 中增加 `Ctx` variant 到 `ToolHandlerKind`
   2. 在 `tool_registry_plan.rs` 中 feature-gated 注册 ctx 工具
   3. 在 `core/src/tools/handlers/` 中创建 `ctx.rs` handler（类似 `zmemory.rs` 模式）
 
 ## 9. 脱敏需求分析
 
 当前 event_recorder 直接 `pretty_json` 原始 tool_input/tool_response，需要脱敏的字段：
 - Shell 命令中可能含凭证、token 的环境变量
 - 文件写入/编辑中的密钥内容
 - HTTP 请求中的 Authorization header
 - 任何包含 `token`、`key`、`secret`、`password`、`credential` 等关键词的 JSON 字段值
 
 需要在 `event_recorder.rs` 中增加 sanitization 层，在 `pretty_json` 之前或之后、`truncate_field` 之前执行。
 
 ## 10. Snapshot 查询修正需求
 
 当前 `build_session_snapshot` 使用 `Export(domain="session")` 全域导出。修正方案：
 - 方案 A：使用 `Search` action + `query` 按前缀 `session://{session_id}` 搜索
 - 方案 B：使用 `Read` action 读取 `session://{session_id}` 路径下的子节点
 - 方案 C：新增 zmemory 的 URI prefix filter action
 
 推荐方案 A（Search），因为 zmemory 已有 FTS 索引，且 Search 已支持 domain/path 过滤。
 
 ## 11. Git 工作区现状
 
 当前脏改动与本计划无关：
 - `.agents/plan/2026-04-28-context-hooks-architecture.md` — 本计划文件
 - `.mission/mission_state.json` — mission 状态
 - `codex-rs/cli/src/main.rs` / `mission_cmd.rs` / `zmission_cmd.rs` / `mission.rs` — mission 命令重构，与 zcontext 无关
 
 **context-hooks crate 已提交到仓库**（非未跟踪文件），无 diff。features/config 测试已存在。
 
 ## 12. 约束和风险总结
 
 ### 必须遵守
 - zcontext 业务逻辑保持在 `codex-context-hooks` crate，core 只含薄调用
 - ctx 工具必须经过现有 exec/approval/sandbox 路径
 - 所有 zcontext 行为受 `features.zcontext` gate 控制，off 时完全停止
 - 写入前必须脱敏
 - snapshot 必须 session-scoped
 - 不修改 `CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR` 或 `CODEX_SANDBOX_ENV_VAR` 相关代码
 - 修改 Cargo.toml 后运行 `just bazel-lock-update`
 
 ### 需要修正的已知问题
 1. **脱敏缺失**：event_recorder 直接 pretty-print 原始数据
 2. **全域导出**：snapshot 用 Export(domain) 而非 session-scoped 查询
 3. **未接入主路径**：record_post_tool_use_event 和 build_session_snapshot 未在 core 中调用
 4. **ctx 工具缺失**：ctx_execute/search/stats/doctor/purge 尚未实现
 
 ### 可接受的架构决策
 - 使用 zmemory SQLite 作为存储层（session 域已在 DEFAULT_VALID_DOMAINS）
 - 使用现有 `run_zmemory_tool_with_context` 同步 API
 - 通过 additional_contexts 机制注入 snapshot（而非直接操作 conversation items）
 - 复用 `TruncationPolicy::Tokens` 做 token 级截断
 
 ## 13. 实施阶段划分建议
 
 基于上下文分析，建议分为以下阶段（可在后续 planning 阶段细化）：
 
 1. **修正现有草稿**：脱敏 + snapshot 改用 session-scoped 查询
 2. **PostToolUse 事件记录接入**：feature-gated 调用 record_post_tool_use_event
 3. **PreCompact/Resume snapshot 注入**：feature-gated 在 build_initial_context 中注入 snapshot
 4. **UserPromptSubmit 意图捕获**：feature-gated 记录用户意图
 5. **ctx 工具实现与注册**：ctx_execute/search/stats/doctor/purge
 6. **Schema/文档/测试同步**：config schema、docs、完整测试覆盖
 
 ## 出口条件检查
 
 - [x] 主要事实来源已识别：features、config、hook_runtime、compact/build_initial_context、zmemory tool_api、tool_registry、context-hooks crate
 - [x] 项目约定已整理：上游解耦、最小 core 接入、feature-gated、复用现有 exec/sandbox 路径
 - [x] 已知风险和修正项已列出：脱敏、全域导出、未接入主路径、ctx 工具缺失
 - [x] 实施阶段初步划分已完成
 
 ---
 
 # 约束确认阶段报告 (constraints)
 
 ## 已验证的代码级不变量
 
 以下不变量已通过代码检查确认，后续实现必须满足：
 
 ### C1: Feature Gate 已就绪
 - `Feature::ZContext` 在 `codex-rs/features/src/lib.rs:81` 注册为 Stable、`default_enabled: true`、key `"zcontext"`
 - Config 层 `ContextHooksConfig::from_toml(None)` → `enabled: true`，与 feature gate 默认开启对齐
 - **不变量**：任何 zcontext 运行时路径必须前置 `config.features.is_enabled(Feature::ZContext)` 检查；feature off 时所有记录、注入、工具注册、路由提示完全停止
 - **验证状态**：✅ 已通过 features/config 测试
 
 ### C2: 脱敏缺失 — 必须在写入前修复
 - `event_recorder.rs` 当前 `pretty_json` 原始 `tool_input`/`tool_response` 后直接截断
 - 仅有 `sanitize_uri_segment` 用于 URI 构建，内容本身无脱敏
 - **不变量**：所有写入 `session://` 域的内容必须先经过脱敏层（sanitize），脱敏发生在截断（truncate）之前
 - **验证标准**：脱敏测试覆盖 token/key/secret/password/credential/Authorization/Bearer 关键词替换、URL userinfo/query 清洗、常见云凭证字段遮蔽
 
 ### C3: Snapshot 必须改为 Session-Scoped 查询
 - `snapshot.rs:18` 当前使用 `ZmemoryToolAction::Export` + `domain: Some("session")` 全域导出
 - `Search` action 已支持 `f.domain = ?3 AND f.path LIKE ?5` 过滤（`codex-rs/zmemory/src/service/search.rs:42`）
 - `session` 域已在 `DEFAULT_VALID_DOMAINS`（`codex-rs/zmemory/src/config.rs:13`）
 - **不变量**：snapshot 构建必须使用 `Search` action + `session://{session_id}` 前缀过滤，禁止全域导出再内存过滤
 - **验证标准**：snapshot 仅包含当前 session 事件；跨 session 数据零泄漏
 
 ### C4: 核心业务逻辑必须在 context-hooks crate
 - `codex-rs/context-hooks/` 已有 `lib.rs`、`event_recorder.rs`、`snapshot.rs`
 - `hook_runtime.rs`（`codex-rs/core/src/hook_runtime.rs`）有 `run_post_tool_use_hooks`（:223）和 `run_user_prompt_submit_hooks`（:253）seam
 - `build_initial_context`（`codex-rs/core/src/session/mod.rs:2492`）有 `additional_contexts` 注入 seam
 - **不变量**：`codex-core` 中的改动限制为薄调用（feature gate 检查 + 调用 context-hooks 公共 API），所有业务逻辑在 `codex-context-hooks` crate
 - **上游热点文件最大侵入线**：`hook_runtime.rs` 加 3-5 行 feature-gated 调用；`session/mod.rs` 加 3-5 行 snapshot 注入；`tools/src/tool_registry_plan.rs` 加 ctx 工具薄注册；`tools/src/tool_registry_plan_types.rs` 加 `Ctx` variant
 
 ### C5: ctx 工具必须复用现有执行路径
 - `ToolHandlerKind`（`codex-rs/tools/src/tool_registry_plan_types.rs:12`）已有 `Zmemory` variant，ctx 工具需新增 `Ctx` variant
 - 工具注册模式：`plan.push_spec()` + `plan.register_handler(name, kind)`
 - Handler 模式参考 `core/src/tools/handlers/zmemory.rs`
 - **不变量**：ctx 执行型工具（`ctx_execute`）必须经过现有 approval/sandbox/timeout/truncation 路径，不新建旁路
 - **不变量**：ctx 工具注册必须 feature-gated，`features.zcontext = false` 时工具不可见
 
 ### C6: 存储层复用 zmemory
 - 使用 `run_zmemory_tool_with_context` 同步 API（已在 context-hooks crate 中使用）
 - `ZmemoryToolAction` 支持 Read/Search/Create/BatchCreate/Stats/Doctor 等所需 action
 - **不变量**：不新建独立存储引擎，zcontext 数据全部通过 zmemory `session://` 域存储
 
 ### C7: 安全与沙箱约束
 - **不变量**：不修改 `CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR` 或 `CODEX_SANDBOX_ENV_VAR` 相关代码（当前 context-hooks crate 无此类引用，保持不变）
 - **不变量**：写入失败必须失败开放（fail-open）并记录 warning，不阻断主路径
 - **不变量**：配置变更后运行 `just write-config-schema`；Cargo.toml 变更后运行 `just bazel-lock-update` + `just bazel-lock-check`
 
 ### C8: 提交边界约束
 - 当前工作区有无关改动（mission 命令重构等），与本计划文件集不重叠
 - **不变量**：提交只精准暂存本计划相关文件，不污染无关改动
 - **不变量**：`git diff --cached --name-only` 最终只包含 zcontext 相关文件
 
 ## 约束汇总表
 
 | ID | 约束类别 | 不变量 | 违反后果 |
 |---|---|---|---|
 | C1 | Feature Gate | 所有路径前置 feature 检查，off 时完全停止 | 功能泄漏、用户无法关闭 |
 | C2 | 安全/脱敏 | 写入前必须脱敏，脱敏先于截断 | 敏感数据写入 durable storage |
 | C3 | 数据隔离 | Snapshot session-scoped 查询 | 跨 session 泄漏、性能退化 |
 | C4 | 架构解耦 | Core 只含薄调用，逻辑在 context-hooks | 上游 sync 冲突、core 膨胀 |
 | C5 | 工具安全 | ctx 工具复用现有 exec 路径 | 旁路 approval/sandbox |
 | C6 | 存储复用 | 使用 zmemory session:// 域 | 重复建设、数据孤岛 |
 | C7 | 沙箱/构建 | 不修改沙箱代码；schema/lock 同步 | CI 失败、沙箱行为变更 |
 | C8 | 提交边界 | 只暂存 zcontext 相关文件 | 提交污染、review 困难 |
 
 ## 必须修正的已知缺陷（阶段 0 前置条件）
 
 1. **event_recorder.rs 脱敏层缺失** → 新增 `sanitize_content()` 在 `pretty_json` 之后、`truncate_field` 之前执行
 2. **snapshot.rs 全域导出** → 替换 `Export` 为 `Search` + `session://{session_id}` 前缀
 3. **主路径未接入** → `record_post_tool_use_event` 和 `build_session_snapshot` 未在 core 中调用（需薄接线）
 4. **ctx 工具未实现** → `ToolHandlerKind` 需新增 `Ctx` variant + handler + feature-gated 注册
 
 ## 验证策略约束
 
 - 每阶段必过：`just fmt`、`cargo nextest run -p codex-context-hooks`、`cargo nextest run -p codex-features`
 - Core 改动后：`cargo nextest run -p codex-core`（定向用例）
 - 工具注册后：`cargo nextest run -p codex-tools`
 - 配置变更后：`just write-config-schema`
 - 依赖变更后：`just bazel-lock-update && just bazel-lock-check`
 - 全量验证前须用户确认
 
 ## 出口条件
 
 - [x] 8 条代码级不变量已从代码验证中提取并确认
 - [x] 4 项必须修正的已知缺陷已列出并附带修正方向
 - [x] 验证策略已明确每阶段的必过检查项
 - [x] 提交边界约束已确认与当前工作区无关改动不冲突
 - [x] 所有约束可直接转化为后续实现阶段的验收标准
 - [x] 所有约束可直接转化为后续实现阶段的验收标准

---

# 架构方案

## 模块边界

### Crate 职责划分

| Crate | 职责 | 对外暴露 |
|---|---|---|
| `codex-context-hooks` | 所有 zcontext 业务逻辑：事件记录（脱敏+截断）、snapshot 构建、ctx 工具参数解析 | `record_post_tool_use_event`, `build_session_snapshot`, `sanitize_content`, ctx 工具公共 API |
| `codex-core` | 薄接线层：feature gate 检查 → 调用 context-hooks 公共 API；ctx 工具 handler 壳 | `hook_runtime.rs` 加 3-5 行调用；`session/mod.rs` 加 snapshot 注入；`tools/handlers/ctx.rs` handler 壳 |
| `codex-features` | `Feature::ZContext` 注册，Stable 默认开启 | 无变更，已就绪 |
| `codex-tools` | ctx 工具 spec 定义 + feature-gated 注册 | `create_ctx_tools()`, `ToolHandlerKind::Ctx` |
| `codex-zmemory` | 底层存储，`session://` 域已可用 | `Search` action 支持前缀过滤，已就绪 |
| `codex-hooks` | hook 类型定义（`PostToolUseRequest` 等） | 已就绪，无变更 |

### 禁止区域

- `codex-core` 中不得包含任何脱敏逻辑、事件分类逻辑、snapshot 排序逻辑
- `codex-context-hooks` 中不得包含任何 IO 层面的 approval/sandbox/timeout 逻辑
- 不新建独立存储引擎

## 数据流

### 事件记录流（PostToolUse）

```
tool handler 执行完成
  → codex-core/tools/registry.rs: run_post_tool_use_hooks()
    → [feature gate] features.enabled(ZContext) && context_hooks.enabled
      → [薄调用] context_hooks::record_post_tool_use_event(context, request)
        → build_post_tool_use_record(request)
          → classify_event → EventCategory
          → pretty_json(input/response)
          → sanitize_content(input)  ← 新增脱敏层
          → sanitize_content(response)  ← 新增脱敏层
          → truncate_field(sanitized) → 截断
          → 组装 markdown content
        → zmemory::run_zmemory_tool_with_context(Create, session://{sid}/events/{tid}/{cid})
      → fail-open: 记录 warning，不阻断主路径
    → 继续原有 hook 流程（用户自定义 hooks）
```

**关键约束**：
1. 内置 zcontext 记录在用户自定义 hooks 之前执行，保证即使用户 hook stop 也已记录
2. 记录失败必须 fail-open + warning，不抛错
3. `sanitize_content` 先于 `truncate_field` 执行（先脱敏再截断）

### Snapshot 注入流（PreCompact / resume）

```
build_initial_context(turn_context)
  → [feature gate] features.enabled(ZContext) && context_hooks.enabled
    → [薄调用] context_hooks::build_session_snapshot(context, session_id, settings)
      → zmemory::run_zmemory_tool_with_context(
          Search,
          uri: Some("session://{session_id}/events/"),  ← 修正：用 Search 替代 Export
          limit: Some(max_events * 2)
        )
      → extract_session_events → 过滤当前 session
      → select_events → 按 priority 排序 + 截断
      → 组装 markdown snapshot
      → formatted_truncate_text → token budget 截断
    → developer_sections.push(snapshot_text)
  → 继续构建其他 context
```

**关键约束**：
1. 使用 `Search` action + `uri: Some("session://{session_id}/events/")` 前缀过滤，替代全域 `Export`
2. Snapshot 仅包含当前 session 事件，跨 session 零泄漏
3. 注入位置：`build_initial_context` 中 `developer_sections`，紧跟 zmemory recall note 之后

### Ctx 工具执行流

```
模型调用 ctx_execute/ctx_search/ctx_stats/ctx_doctor/ctx_purge
  → codex-core/tools/handlers/ctx.rs: CtxHandler::handle()
    → 解析参数
    → [feature gate] features.enabled(ZContext)
    → 构建 ZmemoryToolCallParam
    → run_zmemory_tool_with_context(...)
    → 格式化输出
```

**关键约束**：
1. 复用现有 `ToolHandler` trait 和 handler 分发机制
2. 复用现有 approval/sandbox/timeout/truncation 路径（不新建旁路）
3. 工具注册必须 feature-gated

## 状态所有权

### 运行时状态

| 状态 | 拥有者 | 生命周期 |
|---|---|---|
| `ZmemoryContext`（codex_home, cwd, zmemory_path, settings） | `Session` 通过 `TurnContext` 传递 | per-session，config 解析时构建 |
| `ContextHooksSettings`（token budget, max events） | `Config.context_hooks` | 配置级别 |
| `Feature::ZContext` 开关 | `Features`（per-session） | 配置级别 |
| `context_hooks.enabled` 子开关 | `Config.context_hooks.enabled` | 配置级别 |

### 持久化状态

| 数据 | 存储 | Schema |
|---|---|---|
| 事件记录 | `zmemory session://{session_id}/events/{turn_id}/{call_id}` | markdown，priority = category.priority() |
| Snapshot 产物 | 不持久化，每次实时构建 | — |

### 开关层次

```
Feature::ZContext (Stable, default_enabled: true)
  └── config.context_hooks.enabled (default: true)
       ├── 控制: 事件记录
       ├── 控制: snapshot 注入
       └── 控制: ctx 工具注册
```

- `Feature::ZContext = false` → 全部停止，工具不可见，不记录，不注入
- `Feature::ZContext = true && context_hooks.enabled = false` → 同上（子开关用于更细粒度控制）
- 两者均为 true → 全部生效

## 集成点

### I1: hook_runtime.rs — 事件记录接入

**文件**: `codex-rs/core/src/hook_runtime.rs:223`
**改动**: 在 `run_post_tool_use_hooks` 函数体开头，`preview_runs` 之前，插入 zcontext 内置记录调用

```rust
// zcontext 内置事件记录（先于用户自定义 hooks 执行）
if turn_context.features.enabled(Feature::ZContext)
    && session.config().await.context_hooks.enabled
{
    let context = build_zmemory_context(&session, &turn_context).await;
    let zctx_request = codex_hooks::PostToolUseRequest { /* 从参数构建 */ };
    if let Err(err) = codex_context_hooks::record_post_tool_use_event(&context, &zctx_request) {
        tracing::warn!("zcontext event record failed: {err:#}");
    }
}
```

**侵入线**: ~10 行 feature-gated 代码

### I2: session/mod.rs — Snapshot 注入

**文件**: `codex-rs/core/src/session/mod.rs:2565` 附近（zmemory recall note 之后）
**改动**: 在 zmemory recall note 注入之后，追加 zcontext snapshot 注入

```rust
if turn_context.features.enabled(Feature::ZContext)
    && turn_context.config.context_hooks.enabled
{
    let context = /* build ZmemoryContext */;
    let settings = turn_context.config.context_hooks.to_context_hooks_settings();
    if let Ok(Some(snapshot)) = codex_context_hooks::build_session_snapshot(
        &context,
        &session_id_str,
        &settings,
    ) {
        developer_sections.push(snapshot);
    }
}
```

**侵入线**: ~10 行 feature-gated 代码

### I3: tool_registry_plan.rs — Ctx 工具注册

**文件**: `codex-rs/tools/src/tool_registry_plan.rs:286` 附近（zmemory 工具注册之后）
**改动**: 在 zmemory 工具注册块之后，新增 ctx 工具注册块

```rust
if config.features.enabled(Feature::ZContext) && config.context_hooks.enabled {
    for spec in create_ctx_tools() {
        let name = spec.name().to_string();
        plan.push_spec(spec, /*supports_parallel_tool_calls*/ false, config.code_mode_enabled);
        plan.register_handler(name, ToolHandlerKind::Ctx);
    }
}
```

**侵入线**: ~8 行 feature-gated 代码

### I4: tool_registry_plan_types.rs — ToolHandlerKind 扩展

**文件**: `codex-rs/tools/src/tool_registry_plan_types.rs:44`
**改动**: 在 `Zmemory` variant 之后新增 `Ctx` variant

```rust
pub enum ToolHandlerKind {
    // ... existing variants ...
    Zmemory,
    Ctx,
}
```

**侵入线**: 1 行

### I5: handlers/mod.rs — Handler 分发

**文件**: `codex-rs/core/src/tools/handlers/mod.rs`
**改动**: 在 match arm 中新增 `ToolHandlerKind::Ctx => CtxHandler.handle(invocation)`

**侵入线**: ~3 行

## 必须修正的已知缺陷（修正方案）

### D1: event_recorder.rs 脱敏层缺失

**问题**: `pretty_json` 之后直接 `truncate_field`，无脱敏步骤
**修正**: 新增 `sanitize_content(value: &str) -> String` 函数
**位置**: `codex-rs/context-hooks/src/event_recorder.rs`
**策略**: 基于正则的模式匹配，脱敏以下内容：
  - API keys / tokens: 匹配 `sk-...`, `ghp_...`, `glpat-...` 等前缀模式
  - 密码字段: JSON 中 `"password": "..."`, `"secret": "..."` 等键值
  - 环境变量中的敏感值: `AUTH_TOKEN=xxx` 等
  - 替换为 `[REDACTED]`
**执行顺序**: `pretty_json → sanitize_content → truncate_field`

### D2: snapshot.rs 全域导出

**问题**: 使用 `Export` action + `domain: Some("session")` 导出全域 session 事件，再用 `extract_session_events` 内存过滤
**修正**: 改用 `Search` action + `uri: Some("session://{session_id}/events/")` 前缀过滤
**位置**: `codex-rs/context-hooks/src/snapshot.rs:build_session_snapshot`
**变更**:
```rust
let args = ZmemoryToolCallParam {
    action: ZmemoryToolAction::Search,
    uri: Some(format!("session://{session_id}/events/")),
    query: session_id.to_string(),
    limit: Some(settings.max_events_per_snapshot * 2),
    ..default()
};
```
同时调整 `extract_session_events` 解析 Search 返回格式（而非 Export 返回格式）。

### D3: 主路径未接入

**问题**: `record_post_tool_use_event` 和 `build_session_snapshot` 未在 core 中调用
**修正**: 按集成点 I1、I2 接入

### D4: ctx 工具未实现

**问题**: 缺少 `Ctx` handler kind、handler 实现、工具 spec、注册
**修正**: 按集成点 I3、I4、I5 实现

## 新增文件清单

| 文件 | 用途 | 所属 crate |
|---|---|---|
| `codex-rs/context-hooks/src/sanitize.rs` | 脱敏函数 `sanitize_content` | context-hooks |
| `codex-rs/tools/src/ctx_tool.rs` | ctx 工具 spec 定义 `create_ctx_tools()` | tools |
| `codex-rs/core/src/tools/handlers/ctx.rs` | Ctx handler 实现 | core |

## 修改文件清单

| 文件 | 改动范围 | 改动描述 |
|---|---|---|
| `codex-rs/context-hooks/src/lib.rs` | +1 行 export | 导出 `sanitize_content` |
| `codex-rs/context-hooks/src/event_recorder.rs` | +15 行 | 集成 `sanitize_content` 到记录管道 |
| `codex-rs/context-hooks/src/snapshot.rs` | ~20 行变更 | Export→Search，调整解析逻辑 |
| `codex-rs/core/src/hook_runtime.rs` | +10 行 | I1: zcontext 事件记录接入 |
| `codex-rs/core/src/session/mod.rs` | +10 行 | I2: snapshot 注入 |
| `codex-rs/core/src/tools/handlers/mod.rs` | +3 行 | I5: Ctx handler 分发 |
| `codex-rs/tools/src/tool_registry_plan.rs` | +8 行 | I3: ctx 工具注册 |
| `codex-rs/tools/src/tool_registry_plan_types.rs` | +1 行 | I4: Ctx variant |
| `codex-rs/tools/src/lib.rs` | +1 行 export | 导出 `create_ctx_tools` |

## 实现阶段拆分

### Phase 0: 草稿修正（前置条件）
- D1: 新增 `sanitize.rs`，集成到 `event_recorder.rs`
- D2: snapshot.rs 改用 Search action
- 验证: `cargo nextest run -p codex-context-hooks`

### Phase 1: 主路径接入
- I1: hook_runtime.rs 接入事件记录
- I2: session/mod.rs 接入 snapshot 注入
- 验证: `cargo nextest run -p codex-core`（定向用例）

### Phase 2: Ctx 工具注册
- I4: ToolHandlerKind::Ctx
- I3: ctx_tool.rs spec + 注册
- I5: ctx.rs handler + 分发
- 验证: `cargo nextest run -p codex-tools`

### Phase 3: Schema/文档/全量验证
- `just write-config-schema`
- `just fmt` + `just fix`
- 全量测试（需用户确认）

## 风险与缓解

| 风险 | 概率 | 缓解 |
|---|---|---|
| zmemory Search 返回格式与 Export 不同，需适配 snapshot 解析 | 中 | Phase 0 中先做 zmemory Search 返回结构探索，确认字段映射 |
| hook_runtime.rs 接入点与用户 hook 顺序问题 | 低 | zcontext 内置记录在 `preview_post_tool_use` 之前执行，不受用户 hook stop 影响 |
| snapshot 注入增加 token 开销 | 低 | 有 `snapshot_token_budget`（默认 2000）控制，可通过配置调低 |
| ctx 工具复用 zmemory 执行路径但需要不同的 approval 粒度 | 低 | ctx 工具当前只有只读操作（search/stats/doctor），无写操作风险 |

## 出口条件检查

- [x] 模块边界：6 个 crate 职责明确，禁止区域明确
- [x] 数据流：3 条主数据流（事件记录、snapshot 注入、ctx 工具）端到端清晰
- [x] 状态所有权：运行时状态和持久化状态的拥有者和生命周期明确
- [x] 集成点：5 个具体集成点，每个有文件位置、改动量、代码示例
- [x] 已知缺陷修正方案：4 项缺陷有具体修正策略和代码变更方向
- [x] 新增/修改文件清单：3 个新增文件 + 9 个修改文件
- [x] 实现阶段拆分：4 个有序阶段，每个有明确验证标准
- [x] 风险与缓解：4 项风险有缓解措施
- [x] 方案足够具体，可以拆成可执行任务

---

# 执行计划 (Plan)

## 当前代码状态盘点（已验证）

| 资产 | 状态 | 关键发现 |
|---|---|---|
| Feature gate `Feature::ZContext` | ✅ Stable, 默认开启 | 已注册于 `features/src/lib.rs:81`，测试覆盖于 `features/src/tests.rs:82-86` |
| `codex-context-hooks` crate | ✅ 已注册 workspace，未编译引入 core | `Cargo.toml` 在 workspace members；`core/Cargo.toml` 已引用 |
| `ContextHooksToml` / `ContextHooksConfig` | ✅ 已集成 | `config/src/types.rs:1048-1057` + `core/src/config/types.rs:44-65` |
| `record_post_tool_use_event` | ✅ 已实现 | `context-hooks/src/event_recorder.rs`，但**缺脱敏（D1）** |
| `build_session_snapshot` | ⚠️ 有缺陷 | `context-hooks/src/snapshot.rs`，使用 Export 全域导出再内存过滤（D2） |
| I1: hook_runtime.rs 事件记录接入 | ❌ 未接入 | `hook_runtime.rs` 中无 zcontext 调用 |
| I2: session/mod.rs snapshot 注入 | ❌ 未接入 | 无 additional_context 注入 |
| I3: ctx 工具 spec | ❌ 未实现 | `tools/src/` 中无 ctx 相关代码 |
| I4: ToolHandlerKind::Ctx | ❌ 未实现 | `tool_registry_plan_types.rs` 中无 Ctx variant |
| I5: ctx handler | ❌ 未实现 | `core/src/tools/handlers/` 中无 ctx.rs |

**关键发现**：`context-hooks` crate 可独立编译（`cargo check -p codex-context-hooks` ✅），核心逻辑已有草稿，主要缺的是脱敏层、主路径接入和 ctx 工具。

---

## 实施顺序与依赖关系

### Phase 0: 草稿修正与脱敏层（零依赖，独立完成）

**目标**：修正 `event_recorder.rs` 脱敏缺失（D1）和 `snapshot.rs` 全域导出（D2）。

**任务 T0.1** — 新建 `context-hooks/src/sanitize.rs`
- 实现 `sanitize_content(input: &str) -> String`
- 脱敏模式（正则）：
  - API keys: `r"sk-[A-Za-z0-9_-]{20,}"`、`r"ghp_[A-Za-z0-9]{36}"`、`r"glpat-[A-Za-z0-9_-]{20,}"`
  - Bearer tokens: `r"(?i)bearer\s+[A-Za-z0-9_-]+\b"`
  - 密码字段: JSON 中 `"password"`, `"secret"`, `"token"`, `"api_key"`, `"private_key"` 键值对
  - 环境变量: `r"[A-Z_]{3,}=[^\s]+` 模式
- 替换为 `[REDACTED]`，最多连续 3 个
- 导出 `sanitize_content` 于 `lib.rs`

**任务 T0.2** — 集成脱敏到 `event_recorder.rs`
- 修改 `build_post_tool_use_record` 中的 `pretty_json → truncate_field` 管道
- 插入 `sanitize_content`：`pretty_json → sanitize_content → truncate_field`
- 对 `input` 和 `response` 分别调用

**任务 T0.3** — 修正 `snapshot.rs` 全域导出（D2）
- 改 `Export` → `Search` action
- URI 前缀过滤：`session://{session_id}/events/`
- **重要**：Search 返回 `{query, matchCount, matches: [{domain, path, uri, snippet, priority, disclosure}]}`
- 解析 `snippet` 字段替代原 `content` 字段（snippet 已含高亮片段，适合做摘要）
- 适配搜索结果排序（priority ASC + 相关性），不再需要内存 priority 排序

**验证入口**：`cd codex-rs && RUSTC_WRAPPER="" cargo test -p codex-context-hooks`

**回滚边界**：仅改动 `context-hooks/src/` 下的 3 个文件，无破坏性变更。

---

### Phase 1: 主路径接入（依赖 Phase 0）

**目标**：将 `record_post_tool_use_event` 和 `build_session_snapshot` 接入 `codex-core` 运行路径。

**任务 T1.1** — I1: 接入 `hook_runtime.rs`
- 找到 `run_post_tool_use_hooks` 函数（~line 245）
- 在 `preview_runs` 之后、`outcome = sess.hooks().run_post_tool_use(request).await` 之前
- 添加：

```rust
if features.enabled(Feature::ZContext) && ctx.context_hooks.enabled {
    let zctx = codex_context_hooks::ZmemoryContext::new(
        ctx.codex_home.clone(),
        ctx.cwd.clone(),
        None,
        ctx.to_zmemory_settings(),
    );
    if let Err(e) = codex_context_hooks::record_post_tool_use_event(&zctx, &request) {
        tracing::warn!(?e, "zcontext: failed to record post tool use event");
    }
}
```

- 前提：从 TurnContext 提取 `codex_home`（已有）和 `zmemory_path`（需确认路径）

**任务 T1.2** — I2: 接入 `session/mod.rs` snapshot 注入
- 找到 `PreCompact` / resume 触发点
- 调用 `build_session_snapshot`，将返回的 `Option<String>` 作为 `additional_context` 注入
- **需确认接入点**：查看 session 中 compact/resume 逻辑位置
- 验证：在 `session/mod.rs` 中搜索 `PreCompact` 或 `compact` 触发

**验证入口**：`cd codex-rs && RUSTC_WRAPPER="" cargo check -p codex-core`，然后定向运行 context-hooks 相关测试

**回滚边界**：仅在 hook_runtime.rs 和 session/mod.rs 中添加 feature-gated 代码片段，原有行为不变。

---

### Phase 2: Ctx 工具实现（依赖 Phase 1 完成 core 接入）

**目标**：注册并实现 `ctx_execute`/`ctx_search`/`ctx_stats`/`ctx_doctor`/`ctx_purge` 工具。

**任务 T2.1** — I4: 新增 `ToolHandlerKind::Ctx`
- 在 `tools/src/tool_registry_plan_types.rs` 的 `ToolHandlerKind` enum 中添加 `Ctx` variant
- 需确保 `impl PartialEq` 和 `#[derive(Debug, Clone, Copy)]` 自动覆盖

**任务 T2.2** — I3: 新建 `tools/src/ctx_tool.rs`
- 定义工具 spec：`ctx_execute`、`ctx_search`、`ctx_stats`、`ctx_doctor`、`ctx_purge`
- 工具描述对齐 `context-mode` 参考能力
- 每个工具 `input_schema` 使用 JSON Schema
- 导出 `create_ctx_tools() -> Vec<DiscoverableTool>`

**任务 T2.3** — 注册 ctx 工具
- 在 `tools/src/tool_registry_plan.rs` 中：
  - 调用 `create_ctx_tools()` 获取 spec
  - 对每个工具名 `plan.register_handler(name, ToolHandlerKind::Ctx)`
  - Feature gate: 仅在 `features.enabled(Feature::ZContext)` 时注册

**任务 T2.4** — I5: 新建 `core/src/tools/handlers/ctx.rs`
- 实现 `CtxHandler::execute`
- 内部复用 `run_zmemory_tool_with_context`，action 映射：
  - `ctx_execute` → Read
  - `ctx_search` → Search
  - `ctx_stats` → Stats
  - `ctx_doctor` → Doctor
  - `ctx_purge` → DeletePath
- 工具参数 JSON → `ZmemoryToolCallParam` 转换

**任务 T2.5** — 分发接入
- 在 `core/src/tools/handlers/mod.rs` 中添加 `ToolHandlerKind::Ctx => CtxHandler.handle(invocation)`

**验证入口**：`cd codex-rs && RUSTC_WRAPPER="" cargo check -p codex-tools && cargo check -p codex-core`

**回滚边界**：仅添加新文件 + 修改 3 个已有文件中的小片段。

---

### Phase 3: Schema / 文档 / 全量验证

**任务 T3.1** — 更新 config schema
- `just write-config-schema`（确认 context-hooks 相关配置已导出到 schema）

**任务 T3.2** — 格式化与 lint
- `cd codex-rs && just fmt`
- `cd codex-rs && just fix -p codex-context-hooks -p codex-tools -p codex-core`

**任务 T3.3** — 单元测试
- `cd codex-rs && RUSTC_WRAPPER="" cargo test -p codex-context-hooks`
- `cd codex-rs && RUSTC_WRAPPER="" cargo test -p codex-tools`
- `cd codex-rs && RUSTC_WRAPPER="" cargo test -p codex-core`（定向相关用例）

**任务 T3.4** — Snapshot 测试（UI 变更）
- 若 ctx 工具 UI 输出涉及渲染，update insta snapshots

**任务 T3.5** — 文档（按需）
- 检查 `docs/` 中是否有 zcontext 相关文档需更新

**全量测试**：需用户确认后执行 `cd codex-rs && just test` 或 `cargo nextest run`

---

## 依赖图

```
Phase 0 (T0.1-T0.3)
    │
    ▼
Phase 1 (T1.1-T1.2)  ──依赖── Phase 0 编译通过
    │
    ▼
Phase 2 (T2.1-T2.5)  ──依赖── Phase 1 check 通过
    │
    ▼
Phase 3 (T3.1-T3.5)  ──依赖── Phase 2 check 通过
```

---

## 验证入口汇总

| 阶段 | 验证命令 | 预期 |
|---|---|---|
| Phase 0 | `cargo test -p codex-context-hooks` | 2 个现有测试通过；新增 sanitize 测试通过 |
| Phase 1 | `cargo check -p codex-core` | 无编译错误 |
| Phase 2 | `cargo check -p codex-tools && cargo check -p codex-core` | 无编译错误 |
| Phase 3 | `just fmt && just fix && just write-config-schema` | 全部通过 |
| 最终 | `cargo test -p codex-context-hooks -p codex-tools` | 全量通过 |

---

## 回滚边界

- Phase 0：仅 `context-hooks/src/` 下 3 文件有改动，回滚 = 删除 sanitize.rs + 还原 event_recorder/snapshot.rs
- Phase 1：仅在 `hook_runtime.rs` 和 `session/mod.rs` 中添加 feature-gated 片段，删除即回滚
- Phase 2：删除 3 个新增文件 + 还原 3 个已有文件的增量，回滚干净
- Phase 3：仅配置/格式文件改动，无破坏性

---

## 出口条件检查

- [ ] Phase 0: `cargo test -p codex-context-hooks` 全绿
- [ ] Phase 0: `sanitize_content` 覆盖所有规定模式
- [ ] Phase 0: `build_session_snapshot` 改用 Search，解析 snippet 字段
- [ ] Phase 1: `cargo check -p codex-core` 无错误
- [ ] Phase 1: `record_post_tool_use_event` 在 PostToolUse 钩子中被调用（feature-gated）
- [ ] Phase 1: snapshot 通过 additional_context seam 注入
- [ ] Phase 2: ctx 工具 spec 定义完整
- [ ] Phase 2: ToolHandlerKind::Ctx 分发到 CtxHandler
- [ ] Phase 2: `cargo check -p codex-tools && cargo check -p codex-core` 无错误
- [ ] Phase 3: `just fmt && just fix` 通过
- [ ] Phase 3: config schema 更新
- [ ] Phase 3: 所有相关测试通过


## Worker 定义

基于对现有代码、计划 Phase 0-3、依赖图和代码约束的分析，定义 5 个可独立派发的 Worker。每个 Worker 拥有明确的文件所有权、输入/输出契约和验收标准。

### 通用约定

- 所有 Worker 在派发前由主流程确认前置条件满足。
- Worker 之间不共享可变状态；交接通过 crate 公共 API 和 git 文件边界完成。
- 每个 Worker 的改动范围限制在其拥有的文件集内，不允许跨 Worker 文件集修改。
- Feature gate 检查 (`Feature::ZContext`) 由 Worker 0 集中确认，其余 Worker 依赖该注册结果。

---

### Worker 0: Feature Gate 与配置基线

**职责**：确保 `Feature::ZContext` 注册完整、`ContextHooksConfig` 解析正确、`context-hooks` crate 公共 API 稳定。

**文件所有权**：
- `codex-rs/features/src/lib.rs`（确认 `ZContext` 在 `Feature` enum 中，stage 为 `Stable`，`default_enabled: true`）
- `codex-rs/core/src/config/types.rs`（`ContextHooksToml` / `ContextHooksConfig`）
- `codex-rs/context-hooks/src/lib.rs`（公共 API 导出）
- `codex-rs/context-hooks/Cargo.toml`（依赖声明）

**输入**：现有代码状态（已审查）。

**输出/验收标准**：
1. `Feature::ZContext` 的 `FeatureSpec` 为 `Stage::Stable`, `default_enabled: true`, key `"zcontext"`
2. `ContextHooksConfig::from_toml(None)` 返回 `enabled: true`
3. `ContextHooksConfig::from_toml(Some(ContextHooksToml { enabled: Some(false), .. }))` 返回 `enabled: false`
4. `cargo check -p codex-context-hooks` 无错误
5. `cargo test -p codex-context-hooks` 通过
6. `to_context_hooks_settings()` 字段映射一一对应

**交接物**：`codex-context-hooks` crate 可被 `codex-core` 正常依赖和调用。

---

### Worker 1: 事件记录增强（脱敏 + sanitize）

**职责**：为 `event_recorder.rs` 添加内容脱敏层，确保敏感内容在存储前被替换。

**文件所有权**：
- `codex-rs/context-hooks/src/event_recorder.rs`
- 新建 `codex-rs/context-hooks/src/sanitize.rs`

**输入**：Worker 0 完成后的 crate 状态。

**输出/验收标准**：
1. 新增 `sanitize_content(content: &str) -> String`，覆盖：API key/token 模式（`sk-...`、`ghp_...`、Bearer）、密码字段（`"password"`/`"secret"`/`"token"` JSON 键）、环境变量凭证（`API_KEY=...`、`SECRET=...`）、私钥头部（`-----BEGIN PRIVATE KEY-----`）
2. `sanitize_content` 有单元测试覆盖所有模式
3. `build_post_tool_use_record` 对 input 和 response 调用 `sanitize_content`
4. `sanitize.rs` 在 `lib.rs` 声明为 `pub(crate) mod`
5. 现有测试仍通过，新增测试覆盖脱敏逻辑

**交接物**：`event_recorder` 输出的存储内容不含明文敏感值。

---

### Worker 2: Snapshot 恢复（Search-based）

**职责**：将 `build_session_snapshot` 从 `Export` action 改为 `Search` action，避免全域导出再过滤。

**文件所有权**：
- `codex-rs/context-hooks/src/snapshot.rs`

**输入**：Worker 0 + Worker 1 完成后的 crate 状态。

**输出/验收标准**：
1. 使用 `ZmemoryToolAction::Search` 替代 `Export`
2. Search 参数：`query: Some(session_id)` + `uri: Some(format!("session://{session_id}/"))`
3. 解析 Search 返回的 `snippets` 字段（替代 Export 的 `items`），提取 title/content 摘要/uri
4. 优先排序逻辑不变（按 priority 排序，截断到 `max_events_per_snapshot`）
5. 分组输出格式不变
6. `formatted_truncate_text` token budget 截断仍生效
7. `extract_session_events` 测试更新为匹配 Search 返回结构
8. `cargo test -p codex-context-hooks` 通过

**交接物**：`build_session_snapshot` 基于 Search 返回 session 上下文快照。

---

### Worker 3: Core 接入层（Hook Seam + Snapshot 注入）

**职责**：在 `codex-core` 中接入 context-hooks 的记录和恢复能力，通过 feature gate 控制。

**文件所有权**：
- `codex-rs/core/src/hook_runtime.rs`（PostToolUse 钩子调用 `record_post_tool_use_event`）
- `codex-rs/core/src/compact.rs` 或 `codex-rs/core/src/session/turn.rs`（compact/resume 注入 snapshot）

**限制**：仅允许薄调用片段（< 15 行），不引入业务逻辑。

**输入**：Worker 0-2 完成后的 `codex-context-hooks` 公共 API。

**输出/验收标准**：
1. `run_post_tool_use_hooks` 返回 `PostToolUseOutcome` 前，在 `features.enabled(Feature::ZContext) && config.context_hooks.enabled` 时调用 `record_post_tool_use_event`
2. 调用使用 `ZmemoryContext::new(...)` 构造 context
3. 调用结果仅 log 错误，不阻断 hook 流程
4. compact 流程在构造 prompt 前检查 feature，若启用则调用 `build_session_snapshot`，结果通过 `record_additional_contexts` 注入
5. 所有新增代码有 `if features.enabled(Feature::ZContext) && context_hooks.enabled` 保护
6. `cargo check -p codex-core` 无错误
7. 现有测试不受影响

**交接物**：ZContext 通过 feature gate 在 `codex-core` 中激活，事件自动记录、compact 时自动注入 snapshot。

---

### Worker 4: Ctx 工具注册与分发

**职责**：定义 `ctx_*` 工具 spec、注册到 tool registry、实现 handler 分发。

**文件所有权**：
- 新建 `codex-rs/tools/src/ctx_tool.rs`（工具 spec）
- `codex-rs/tools/src/tool_registry_plan_types.rs`（`ToolHandlerKind::Ctx`）
- `codex-rs/tools/src/tool_registry_plan.rs`（注册 ctx 工具，feature-gated）
- 新建 `codex-rs/core/src/tools/handlers/ctx.rs`（handler 实现）
- `codex-rs/core/src/tools/handlers/mod.rs`（`Ctx` 分发分支）

**输入**：Worker 3 完成后的 core 接入层 + 现有 `ZmemoryHandler` 参考模式。

**输出/验收标准**：
1. `ToolHandlerKind::Ctx` variant 已添加
2. 5 个工具 spec：`ctx_execute`(→Read)、`ctx_search`(→Search)、`ctx_stats`(→Stats)、`ctx_doctor`(→Doctor)、`ctx_purge`(→DeletePath)
3. 每个工具 `input_schema` 使用 `JsonSchema`，描述对齐 context-mode
4. `tool_registry_plan.rs` 中 `features.enabled(Feature::ZContext)` 时注册
5. `ctx.rs` handler 复用 `run_zmemory_tool_with_context`
6. `mod.rs` 添加 `ToolHandlerKind::Ctx => CtxHandler.handle(invocation)` 分发
7. `cargo check -p codex-tools && cargo check -p codex-core` 无错误
8. 现有 zmemory 工具和测试不受影响

**交接物**：模型可通过 `ctx_*` 工具访问 session 上下文。

---

### Worker 依赖图

```
Worker 0 (Feature Gate 基线)
    │
    ├──► Worker 1 (脱敏)
    │       │
    │       └──► Worker 2 (Snapshot Search) ──► Worker 3 (Core 接入) ──► Worker 4 (Ctx 工具)
    │
    └──► Worker 2 可与 Worker 1 并行启动（无文件冲突）
```

**并行策略**：
- Worker 0 先完成（提供稳定 API 基线）
- Worker 1 和 Worker 2 可并行派发（文件集不重叠）
- Worker 3 在 Worker 1+2 完成后启动
- Worker 4 在 Worker 3 完成后启动

### 交接格式

1. **crate 公共 API**：`codex-context-hooks` 的 `pub use` 导出
2. **git 文件边界**：每个 Worker 改动提交到独立文件集
3. **编译检查**：后续 Worker 启动前，主流程验证 `cargo check` 通过

### 风险与缓解

| 风险 | 影响 | 缓解 |
|---|---|---|
| `ZmemoryToolAction::Search` 返回结构与 Export 不同 | Worker 2 需调整解析 | 启动时先确认 Search action 返回的 JSON 结构 |
| compact 注入点不明确 | Worker 3 可能需改更多文件 | 优先查 `record_additional_contexts` 调用链，确定最小注入点 |
| ctx 工具与 zmemory 工具名称冲突 | 注册歧义 | ctx 使用 `ctx_` 前缀，与 `*_memory` 正交 |
| Bazel BUILD.bazel 需更新 | 新增文件可能破坏 Bazel 构建 | Worker 4 后运行 `just bazel-lock-update` |

---

# 验证策略 (Verification)

## 验证原则

1. **分层验证**：按 crate 边界从内向外验证，先 `codex-context-hooks` 单元，再 `codex-core` 集成，最后端到端。
2. **可复现**：所有验证步骤必须可通过命令行复现，无手动依赖。
3. **失败快**：每个 Worker 交付前先跑自己的测试，通过后才交接。
4. **风险覆盖**：验证链路必须覆盖 6 条成功标准中的核心风险面。

## 一、代码审查清单

每个 Worker 交付后，主流程执行以下审查（自审或指派 explorer）：

### Worker 0: Feature Gate 基线
- [ ] `Feature::ZContext` 在 `FEATURES` 数组中 `stage: Stage::Stable, default_enabled: true`
- [ ] `ContextHooksToml` 字段（`enabled`, `snapshot_token_budget`, `max_events_per_snapshot`）在 `codex-rs/config/src/types.rs` 中存在
- [ ] `from_toml(None)` 解析为 `enabled: true`（默认开启），而非之前草稿的 disabled
- [ ] config schema 更新（`just write-config-schema` 后 `config.schema.json` 包含 `context_hooks` 段）

### Worker 1: 脱敏
- [ ] `record_post_tool_use_event` 不存储原始 `tool_input`/`tool_response`，而是经 `truncate_field` 截断
- [ ] `classify_event` 分类正确（error → file_edit → git → command → tool）
- [ ] URI 格式 `session://{session_id}/events/{turn_id}/{call_id}` 经过 `sanitize_uri_segment`
- [ ] 不泄漏密钥/令牌（content 中无非预期敏感字段）
- [ ] 测试 `cargo test -p codex-context-hooks` 通过（修复现有编译错误后）

### Worker 2: Snapshot Search
- [ ] `build_session_snapshot` 使用 `ZmemoryToolAction::Search` 而非 `Export`
- [ ] Search 参数包含 `uri: Some("session://{session_id}/")` 和 `query: Some(session_id)`
- [ ] 解析 Search 返回的 `snippets` 字段（非 Export 的 `items`）
- [ ] 优先排序和 token budget 截断逻辑保留
- [ ] 测试 `cargo test -p codex-context-hooks` 通过

### Worker 3: Core 接入层
- [ ] `hook_runtime.rs` 中 PostToolUse 钩子路径有 `if features.enabled(Feature::ZContext)` 保护
- [ ] 调用 `record_post_tool_use_event` 使用 `ZmemoryContext::new(...)` 构造 context
- [ ] 调用失败仅 log 错误，不阻断 hook 流程（不返回 Err）
- [ ] compact/PreCompact 注入点检查 feature gate
- [ ] `cargo check -p codex-core` 无错误
- [ ] 现有测试不受影响（`cargo nextest run -p codex-core` 无新增失败）

### Worker 4: Ctx 工具注册
- [ ] `ToolHandlerKind::Ctx` variant 已添加到 `codex-rs/tools/src/tool_registry_plan_types.rs`
- [ ] 5 个工具 spec 存在（`ctx_execute`, `ctx_search`, `ctx_stats`, `ctx_doctor`, `ctx_purge`）
- [ ] 注册有 `features.enabled(Feature::ZContext)` feature-gated
- [ ] `ctx.rs` handler 复用 `run_zmemory_tool_with_context`
- [ ] `mod.rs` 添加 `ToolHandlerKind::Ctx => ...` 分发分支
- [ ] `cargo check -p codex-tools && cargo check -p codex-core` 无错误
- [ ] 现有 zmemory 工具和测试不受影响

## 二、自动验证命令

每个 Worker 完成后按顺序执行：

### Worker 0-2（codex-context-hooks crate）
```bash
# 编译检查
RUSTC_WRAPPER= cargo check -p codex-context-hooks
# 单元测试（修复现有编译错误后）
RUSTC_WRAPPER= cargo nextest run -p codex-context-hooks
# 格式化
just fmt
# Lint（限定范围）
just fix -p codex-context-hooks
```

### Worker 3（codex-core 接入）
```bash
# 编译检查
RUSTC_WRAPPER= cargo check -p codex-core
# 现有测试不回归（可选，询问用户后执行）
RUSTC_WRAPPER= cargo nextest run -p codex-core
# 格式化
just fmt
just fix -p codex-core
```

### Worker 4（tools + core handler）
```bash
RUSTC_WRAPPER= cargo check -p codex-tools
RUSTC_WRAPPER= cargo check -p codex-core
# Bazel lock 更新（依赖变更时）
just bazel-lock-update
just bazel-lock-check
```

### 全量验证（所有 Worker 完成后）
```bash
RUSTC_WRAPPER= cargo check --workspace
RUSTC_WRAPPER= cargo nextest run -p codex-context-hooks
just fmt
# Schema 更新
just write-config-schema
```

## 三、已知阻断项与前置修复

| 阻断项 | 影响 | 修复动作 |
|---|---|---|
| `context-hooks` 测试编译失败（缺少 `codex_protocol`/`codex_utils_absolute_path` 依赖） | Worker 1 测试无法运行 | 在 `Cargo.toml` 中添加 `codex-protocol` 和 `codex-utils-absolute-path` 依赖，或将测试中的 `ThreadId::from_string` 和 `to_abs_path_buf()` 替换为手构 fixture |
| `snapshot.rs` 使用 `Export` action | Worker 2 目标就是改为 `Search`，但需先确认 Search 返回的 JSON 结构 | 在 `codex-rs/zmemory` 中检查 `Search` action 的返回格式（`snippets` 字段） |
| compact 注入点未明确 | Worker 3 需找到最小注入点 | 在 `codex-rs/core/src/session/` 中查找 compact/compaction 相关调用链，确认 `additional_contexts` 注入 seam |
| `codex-core` 中 `post_tool_use` 相关调用未找到 | Worker 3 需确认 hook 调用链 | 检查 `codex-rs/hooks/src/registry.rs:run_post_tool_use` 和 core 中的调用者 |

## 四、用户测试场景

以下场景由用户在本地环境手动验证（sandbox 环境可能受限）：

### 场景 1：Feature Gate 开关
1. 默认启动 Codex TUI，确认 zcontext 功能激活（可通过 `codex features list` 查看）
2. 在 `config.toml` 中设置 `[features] zcontext = false`，重启确认所有 zcontext 行为停止
3. 通过 `codex --disable zcontext` 命令行参数禁用

### 场景 2：事件记录
1. 执行一次 shell 命令（如 `ls`），确认 `session://` 域下有对应事件记录
2. 执行一次 `apply_patch`，确认分类为 `file_edit`
3. 执行一个会失败的命令，确认分类为 `error`
4. 通过 `ctx_search` 或 zmemory 工具查看记录，确认内容经过截断、不含原始敏感数据

### 场景 3：Snapshot 恢复
1. 在一个 session 中执行若干操作后触发 compact（`/compact`）
2. 确认 compact 后的 additional context 包含 session snapshot
3. 新 session resume 时确认 snapshot 被注入

### 场景 4：Ctx 工具
1. 通过 `ctx_stats` 查看 session 统计
2. 通过 `ctx_search` 搜索特定事件
3. 通过 `ctx_doctor` 诊断 zcontext 健康状态
4. 通过 `ctx_purge` 清理指定 session 数据

## 五、最终交接要求

### 交付物清单
1. `codex-rs/context-hooks/` — 完整的 `codex-context-hooks` crate，含事件记录、脱敏、snapshot 恢复
2. `codex-rs/core/src/hook_runtime.rs` — PostToolUse 钩子薄调用（< 15 行新增）
3. `codex-rs/core/src/` compact/resume 注入点 — snapshot 注入薄调用
4. `codex-rs/tools/src/tool_registry_plan_types.rs` — `ToolHandlerKind::Ctx` variant
5. `codex-rs/tools/src/ctx_tool.rs`（新建）— ctx 工具 spec
6. `codex-rs/core/src/tools/handlers/ctx.rs`（新建）— ctx handler
7. `codex-rs/features/src/lib.rs` — `Feature::ZContext` 已注册（已有）
8. `codex-rs/config/src/types.rs` — `ContextHooksToml` 已定义（已有）
9. `codex-rs/core/config.schema.json` — 更新后的 schema

### 交接检查
- [ ] 所有 `cargo check` 通过
- [ ] 所有 `cargo nextest run -p codex-context-hooks` 通过
- [ ] 现有测试无回归
- [ ] `just fmt` 无变更
- [ ] `just write-config-schema` 后 schema 包含 context_hooks
- [ ] 每个功能面有对应单元测试
- [ ] 无硬编码密钥/令牌
- [ ] 无 `unwrap()` 在非测试路径（使用 `?` 或 `map_err`）
- [ ] 新增代码有 `Feature::ZContext` feature gate 保护
- [ ] `codex-core` 改动限于薄调用（每处 < 15 行）

### 交接后剩余风险
| 风险 | 等级 | 缓解 |
|---|---|---|
| `ZmemoryToolAction::Search` 返回结构不稳定 | 中 | Worker 2 启动时先跑 Search 集成测试确认返回格式 |
| compact 注入点可能有多个候选 | 低 | Worker 3 先做 code exploration 确认最优注入点 |
| Bazel 构建未覆盖 | 低 | 本地先验证 Cargo，CI 覆盖 Bazel |
| 端到端测试需手动验证 | 中 | 用户测试场景覆盖，交接后安排手动验证 |
