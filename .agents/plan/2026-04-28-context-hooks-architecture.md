# Context Hooks 内置架构

## 背景
- 当前状态：上一轮中断前已留下未完成的 `codex-rs/context-hooks/` 草稿、配置字段草稿和 `zmemory` `session` 域草稿改动；仓库当前没有可引用的 `codex-rs/PRD-context-hooks.md`，执行阶段必须以本计划、现有代码 seam 和当前 diff 审查结果为准，先分类保留、调整、拆分或重写草稿。
- 触发原因：需要参考 `https://github.com/mksglu/context-mode/`，基于当前仓库已有能力尽量复用，并设计可落地的内置功能，而不是依赖外部 Node.js MCP server。
- 预期影响：提升 session resume、context compaction 后的连续性；减少重复探索、状态遗忘和错误上下文丢失；默认启用内置 context hooks，但必须提供显式配置开关用于关闭，并确保失败开放不阻断现有 hooks、zmemory、工具执行链路。

## 目标
- 目标结果：形成并实施一个纯 Rust、默认开启且可显式关闭、复用现有 `codex-hooks`、`codex-zmemory`、`codex-utils-output-truncation` 和 `codex-core` seam 的内置 context hooks 第一阶段架构。
- 完成定义（DoD）：未显式配置时内置 context hooks 默认启用；`[context_hooks] enabled = false` 时完全关闭记录与注入；工具调用事件先脱敏再截断并记录到可搜索的 `zmemory` `session://...` 域；恢复或压缩相关入口能基于当前 session scope 生成结构化 session snapshot 并注入模型上下文；配置 schema、文档和测试均更新并通过定向验证。
- 非目标：不引入外部 Node.js 运行时；不替换现有用户自定义 hook command 框架；不新建独立存储引擎；不在第一批实现 URL fetch/index；不在第一批实现 `ctx_batch_execute` / `ctx_search`；不把主要业务逻辑继续堆入 `codex-core`。

## 范围
- 范围内：新增或收敛 `codex-context-hooks` 小 crate；新增 `[context_hooks]` 配置并使未显式配置时默认启用；实现 `PostToolUse` 事件记录；实现 resume/compact 可用的 session-scoped snapshot；补齐工具输入/输出脱敏、截断和失败开放；更新 schema、文档、测试和 `.codex/skills/upgrade-rtk/SKILL.md` 中与上下文连续性相关的升级注意事项。
- 范围外：完整照搬 `context-mode` 的 `ContentStore`、Node preload、trigram/fuzzy/proximity 搜索层；外部 MCP server 兼容层；批量 URL 抓取与 HTML 转 Markdown；`ctx_batch_execute` / `ctx_search` 工具注册与执行器复用实现；任何新的 approval/sandbox 执行路径。

## 影响
- 受影响模块：`codex-rs/context-hooks`、`codex-rs/core`、`codex-rs/config`、`codex-rs/zmemory`、`docs/`、`.codex/skills/upgrade-rtk/SKILL.md`、`codex-rs/core/config.schema.json`。
- 受影响接口/命令：`~/.codex/config.toml` 的 `[context_hooks]` 配置；内部 `PostToolUse` / `SessionStart` 接线；compaction/initial context 注入 seam；`zmemory` 的 `session://...` URI 使用。
- 受影响数据/模式：新增 `session://...` 事件记录域；需要保证不会污染既有 `core://`、`project://`、`notes://` 用户记忆；第一阶段不新增 `batch://...` 数据域。
- 受影响用户界面/行为：默认开启后，恢复会话或压缩后继续时可能出现模型可见的 context snapshot；用户可通过 `[context_hooks] enabled = false` 显式关闭。

## 约束与依赖
- 约束（时间/兼容性/安全/性能/发布窗口等）：必须默认开启并支持 `[context_hooks] enabled = false` 显式关闭；`PostToolUse` 记录失败必须失败开放，不阻断工具返回；工具输入/输出必须先脱敏再截断，至少覆盖 secret/token/key/header、URL userinfo/query、认证片段和常见凭证字段；snapshot 必须有 token budget 且不得全域扫描 `session` domain；Rust 改动后运行 `just fmt`；配置改动后运行 `just write-config-schema`；依赖改动后按仓库规则刷新 Bazel lock；不得修改 `CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR` 或 `CODEX_SANDBOX_ENV_VAR` 相关代码。
- 外部依赖（系统/人员/数据/权限等）：无外部账号、API key 或第三方服务依赖；参考仓库 `mksglu/context-mode` 只作为架构输入，不作为运行时依赖。

## 实施策略
- 总体方案：采用“小 crate 承载行为 + core 薄接线 + zmemory 持久化”的分层架构。第一阶段只完成事件记录与 snapshot 注入闭环；batch execute/search 另起后续计划，避免在执行器复用策略未明确前扩大 approval/sandbox 面。
- 关键决策：`codex-context-hooks` 负责事件抽取、脱敏、事件格式化、优先级、预算裁剪、zmemory 参数构造和 snapshot 格式；`codex-core` 只负责从现有 session/turn/hook runtime 提供上下文并调用窄入口；`session://{session_id}/events/{turn_id}/{tool_use_id}` 用于 session event；snapshot 构建必须按当前 session scope 查询或读取，禁止先导出整个 `session` domain 再内存过滤；高频写入第一版必须失败开放、可观察并设置明确超时/耗时观测点，若后续性能不达标再引入 bounded async queue。
- 明确不采用的方案（如有）：不采用外部 `context-mode` Node server；不采用新的 SQLite schema 作为第一版；不直接公开 `zmemory/src/service/*` 的 `pub(crate)` 内部 API；不只用 BM25 top-N 生成 snapshot，因为恢复上下文需要结构化状态而不是松散相关片段。

## 方案选择
- 选择方案：第一阶段内置闭环。复用 `PostToolUse` / `SessionStart`、`zmemory` `session://...`、additional context 注入和 output truncation；新增 `codex-context-hooks` 承载行为，`core` 只做窄接线。
- 不选最小方案：只记录不注入无法满足 resume/compact 连续性目标。
- 不选扩展方案：把 `ctx_batch_execute` / `ctx_search` 并入第一阶段会额外触碰 `codex-tools`、`ToolHandlerKind`、shell/unified exec、approval 和 sandbox，回滚与验证成本显著高于当前目标。

## 攻击面校验
- 依赖失败：`zmemory` 写入或读取失败时记录 warning 并继续工具返回、resume 或 compact；不得把内置 context hooks 失败升级为用户 turn 失败。
- 规模爆炸：snapshot 只允许 session-scoped 查询/读取，配置 `max_events_per_snapshot` 与 `snapshot_token_budget` 双重裁剪；禁止 `Export(domain = "session")` 全域扫描。
- 回滚成本：运行时可用 `[context_hooks] enabled = false` 关闭；代码层可回滚 `core` 接线与 `context-hooks` crate；`session://...` 数据可通过既有 `zmemory delete-path` 清理。
- 前提坍塌：最脆弱前提是默认记录工具 IO 不泄露敏感信息；因此脱敏必须在截断和写入之前执行，并作为第一阶段必过测试。

## 阶段拆分

### 架构与配置基线
- 目标：建立 `codex-context-hooks` 的 crate 边界、配置入口、默认开启行为和显式关闭路径。
- 交付物：workspace member、crate skeleton、`ContextHooksToml` / runtime config、`context_hooks.enabled` 默认 true、`enabled = false` 关闭路径、schema 更新、当前未完成草稿 diff 的保留/重做/拆分决策记录。
- 完成条件：`cargo check` 或定向测试能证明新 crate 与配置编译通过；未配置 `[context_hooks]`、配置空 `[context_hooks]`、配置 `[context_hooks] enabled = true` 时均启用；`[context_hooks] enabled = false` 时不记录、不注入；schema 包含 `[context_hooks]`。
- 依赖：现有 `codex-config`、`codex-core` 配置加载链和 `just write-config-schema`。

### Session 事件记录
- 目标：启用后把 `PostToolUse` 事件写入 `zmemory` session 域，并保持工具调用主路径失败开放。
- 交付物：事件分类器、脱敏器、markdown 事件格式、URI 构造、输出截断、zmemory create 调用、core hook runtime 接线、单测与集成测试。
- 完成条件：默认配置下成功和失败工具调用均能形成可搜索 `session://...` 记录；记录内容不包含已知 secret/token/header/URL query/userinfo 测试样本；`[context_hooks] enabled = false` 时无写入；zmemory 写失败只产生可观测 warning，不改变工具输出。
- 依赖：`codex_hooks::PostToolUseRequest`、`codex_zmemory::tool_api::run_zmemory_tool_with_context`、turn cwd 下的 project-scoped zmemory config 解析。

### Snapshot 注入与压缩恢复
- 目标：恢复或压缩相关入口能生成结构化 session snapshot 并注入模型上下文。
- 交付物：session-scoped snapshot builder、优先级排序、token budget 裁剪、`SessionStart(resume)` 接线、compaction/initial context seam 审查与必要接线、稳定 marker 或稳定标题策略。
- 完成条件：已有当前 session events 时，resume/compact 后模型可见 history 包含结构化 snapshot；无 events 或读取失败时不注入且不阻断会话；snapshot 不超过配置预算；测试证明不会读取其他 session 的 `session://...` 记录，也不会通过全域 domain export 构建 snapshot。
- 依赖：`core/src/hook_runtime.rs` 的 additional context 注入链、`core/src/compact_remote.rs` 或相邻 compaction 处理链、`codex-utils-output-truncation`。

### 后续 Batch Execute/Search 计划
- 目标：仅产出后续独立计划，不在第一阶段实现 `ctx_batch_execute` 或 `ctx_search`。
- 交付物：记录后续计划边界：工具 spec、`ToolHandlerKind`、`core` handler、shell/unified exec 复用、approval/sandbox 不变量、`batch://...` 数据域和验证矩阵。
- 完成条件：第一阶段代码中没有注册 `ctx_batch_execute` / `ctx_search`，没有新增 batch 执行路径；文档明确这是第二阶段工作。
- 依赖：第一阶段 session 记录与 snapshot 注入闭环稳定后再评估。

### 文档、技能治理与收口
- 目标：补齐用户配置说明、开发边界说明、测试和回归 gates。
- 交付物：`docs/` 配置文档、`.codex/skills/upgrade-rtk/SKILL.md` 中与 context hooks / session continuity / ztok-rtk 升级边界相关的指引更新、测试覆盖、性能验证记录、llmdoc 反思或稳定文档更新建议。
- 完成条件：文档说明默认开启、关闭方式、数据位置、限制和清理方式；`upgrade-rtk` skill 反映本任务完成后的新增内置上下文能力与升级注意事项；定向测试通过；已明确未覆盖的全量验证与剩余风险。
- 依赖：前序阶段实现结果和验证输出。

## 测试与验证
- 核心验证：`cargo nextest run -p codex-context-hooks`；`cargo nextest run -p codex-core` 中 context hooks 定向用例；`cargo nextest run -p codex-zmemory` 中 session 域相关用例。
- 必过检查：`just fmt`；配置变更后 `just write-config-schema`；若修改 Rust dependency 或 lockfile，则运行 `just bazel-lock-update` 和 `just bazel-lock-check`。
- 回归验证：默认开启时现有 hooks 测试不变且 context hooks 失败开放；显式关闭时不记录、不注入；缺省配置与空 `[context_hooks]` 均启用；`PostToolUse` additional context 既有行为不被破坏；zmemory `system://workspace` 与 project-scoped path 测试继续覆盖 turn cwd override；snapshot 仅包含当前 session；脱敏测试覆盖 token/header/URL query/userinfo；TUI 或用户可见输出若变化则补 snapshot。
- 手动检查：默认配置下执行一次 shell/tool 调用，确认 `zmemory` 能搜索到 `session://...`；设置 `[context_hooks] enabled = false` 后确认不再写入；恢复同一 session 时确认 snapshot 标题、事件数、预算说明和完整历史检索提示存在；任务完成后回读 `.codex/skills/upgrade-rtk/SKILL.md`，确认新增指引与实际实现一致。
- 未执行的验证（如有）：规划阶段不执行代码测试；执行阶段按 issue 拆分逐项运行。

## 风险与缓解
- 关键风险：默认开启导致更多用户路径暴露写入延迟或 snapshot 噪音；工具输入/输出写入 durable memory 前脱敏不足；`PostToolUse` 同步 SQLite 写入影响工具延迟；`SessionStart` 全域扫描 `session` 记录导致恢复慢；additional context 无稳定 marker 导致后续难以识别；上一轮未完成草稿改动与最终架构不一致；当前工作区存在与本计划不直接相关的改动，提交边界可能被污染。
- 触发信号：PostToolUse p99 超过目标；脱敏测试样本出现在 `session://...` 记录；snapshot 构建超过预算或注入重复；snapshot 包含其他 session 的记录；core 测试中 turn cwd override 读取错误 zmemory path；`git diff` 中现有草稿无法通过编译或测试；提交前 staged 文件超出本计划范围。
- 缓解措施：写入失败开放并记录 warning；写入前统一脱敏并用测试锁定；保留 `[context_hooks] enabled = false` 快速关闭路径；必要时引入 bounded async queue；snapshot 限制 `max_events_per_snapshot` 和 token budget；snapshot builder 必须 session-scoped；抽取共享 zmemory config resolver 避免第三份 project-scoped 逻辑；执行前先审查并修正草稿 diff；提交前只精准暂存本计划相关文件。
- 回滚/恢复方案（如需要）：用户可通过 `[context_hooks] enabled = false` 关闭运行时行为；代码层可回滚 runtime 接线；删除或禁用 `[context_hooks]` 后不再写入/注入；`session://` 数据可用既有 `zmemory delete-path` 清理；新 crate 可按阶段独立回退。

## 执行前 Gate
- Diff 分类：先检查当前 `git status --short` 与 `git diff`，将 `codex-rs/context-hooks/`、context hooks 配置、`zmemory` `session` 域相关改动列为候选保留；将 shutdown rollout 顺序、`zinit_cmd` 或其他无关改动拆出本任务提交边界。
- 默认启用修正：若现有草稿仍把 `ContextHooksConfig::from_toml(None)` 或空 `[context_hooks]` 解析为 disabled，必须先修正并补测试。
- 安全修正：若现有草稿仍直接 pretty-print 原始 `tool_input` / `tool_response` 后截断，必须改为先脱敏再截断。
- Snapshot 修正：若现有草稿仍通过 `Export(domain = "session")` 全域导出构建 snapshot，必须改为 session-scoped 查询或读取。
- 接线修正：`core` 接线只允许调用窄入口，不在 `core` 内实现事件格式化、脱敏或 snapshot 排序逻辑。

## 参考
- `codex-rs/hooks/src/events/post_tool_use.rs`
- `codex-rs/hooks/src/events/session_start.rs`
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
- `https://github.com/mksglu/context-mode/`
