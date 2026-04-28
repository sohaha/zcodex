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
