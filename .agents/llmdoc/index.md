# llmdoc 索引

## 用途
- 这个目录是当前仓库的稳定知识地图。
- 启动阅读从 `startup.md` 开始；临时调查草稿保留在系统临时目录，不并入这里。

## 分类
- `must/`：反复任务都要读的启动包。
- `overview/`：项目身份、边界和主要区域。
- `architecture/`：运行面、crate 分层、记忆与文档系统边界。
- `guides/`：高频执行工作流。
- `reference/`：常用命令、固定约束、稳定入口。
- `memory/`：决策、反思和文档缺口。

## 关键文档
- `.agents/llmdoc/startup.md`：启动阅读顺序。
- `.agents/llmdoc/overview/project-overview.md`：仓库定位、边界和主要区域。
- `.agents/llmdoc/architecture/runtime-surfaces.md`：CLI/TUI/app-server/MCP/zmemory/native-tldr 的职责分层。
- `.agents/llmdoc/architecture/rust-workspace-map.md`：Rust workspace 高价值 crate 地图。
- `.agents/llmdoc/architecture/memory-and-doc-systems.md`：`.ai`、`llmdoc`、`zmemory` 与 `.agents/*` 产物的分工。
- `.agents/llmdoc/guides/rust-change-loop.md`：在 `codex-rs` 做常规改动时的最小闭环。
- `.agents/llmdoc/guides/upstream-sync-preservation-rules.md`：同步 `openai/codex` 时区分本地分叉功能与 upstream 原生功能删除的判断顺序。
- `.agents/llmdoc/guides/ztldr-prompt-optimization.md`：优化 `ztldr` 工具描述、路由提示与相关文档时的事实源、触点与最小验证闭环。
- `.agents/llmdoc/reference/build-and-test-commands.md`：常用 `just`、`mise`、schema 和测试命令。
- `.agents/llmdoc/memory/doc-gaps.md`：后续应补强的文档空白。

## 最近三天反思
- 时间窗按当前日期 `2026-04-21` 计算，覆盖 `2026-04-19` 至 `2026-04-21`。
- 更早的历史反思直接到 `.agents/llmdoc/memory/reflections/` 按日期文件名检索。
- `.agents/llmdoc/memory/reflections/2026-04-21-tui-defaults-and-app-event-tests-need-end-to-end-coverage.md`：TUI 回归里若 `config_toml` 默认值和最终 `core::Config` 组装值脱节，单看反序列化测试会漏掉真实默认路径回归；同时 app 异步事件测试不能假设首个 `recv()` 就是目标事件，应该在时限内筛目标事件并先核对文案源头与断言是否同层。
- `.agents/llmdoc/memory/reflections/2026-04-21-subagent-config-reload-must-gate-on-enabled-project-layers.md`：子 agent 派生配置默认应保留 live `turn.config`；只有按新 `turn.cwd` 重载后确实启用了 project layer 且归一化后仍有实质差异时才切过去，同时 runtime provider 要同步回灌字段和 provider map。
- `.agents/llmdoc/memory/reflections/2026-04-21-models-endpoint-unsupported-state-must-persist-and-refresh-via-openai-fallback.md`：自定义 provider 缺少 `GET /models` 时，单进程熔断和 bundled catalog 只能止住当次重试，挡不住下个进程再次探测；unsupported 状态必须持久化，并且 `Online` 刷新要直接切到配置里的 `openai` provider 更新 cache，而不是每次启动都先打主 provider。
- `.agents/llmdoc/memory/reflections/2026-04-21-tui-status-tests-should-filter-branch-noise-and-keep-config-aliases.md`：`/status` 或 app 事件测试要过滤 `StatusLineBranchUpdated` 噪音；手动改 `codex_home` 且断言 sqlite state 时要同步 `sqlite_home`；状态栏 item id 改名要保留 legacy alias；依赖现有值提交的 prompt 要锁预填行为。
- `.agents/llmdoc/memory/reflections/2026-04-21-cli-localized-clap-version-help-must-preserve-streams.md`：CLI 汉化若改成手写 clap parse/error 输出，必须保留 `DisplayHelp` / `DisplayVersion` 走 stdout 的语义；否则 `install.sh` 这类 `2>/dev/null` 版本探测会把已存在的版本误判为空。
- `.agents/llmdoc/memory/reflections/2026-04-21-custom-provider-models-endpoint-failures-should-fuse-to-bundled-catalog.md`：自定义 provider 若不支持 `GET /models`，应把 404/405/501 视为能力缺失并在进程内熔断刷新，继续用 bundled catalog；不要静默回退到 OpenAI 远端目录掩盖兼容问题。
- `.agents/llmdoc/memory/reflections/2026-04-21-ztok-behavior-switch-should-bridge-config-and-bypass-session-cache.md`：`ztok` 新增行为模式时，应由 CLI 桥接配置而不是让 `ztok` 自己解析全局 config；`basic` 必须整条链路绕开 session dedup / near-diff / sqlite，会话复用里的 `summary` 还要单独收紧 signature 和 snapshot 边界。
- `.agents/llmdoc/memory/reflections/2026-04-21-ztok-runtime-settings-boundary-and-validation-split.md`：`ztok` 下一阶段实现时，应先把 runtime settings 收敛成单个 payload env，再把 session cache sqlite IO 从 dedup 编排层拆出去；若共享 `codex-core --lib` 校验被仓库既有失败阻塞，要在 Cadence notes 里把定向验证通过和外部阻塞明确拆开记录。
- `.agents/llmdoc/memory/reflections/2026-04-21-ztok-clean-worktree-validation-should-separate-baseline-failures-from-issue-state.md`：`ztok` 收口若遇到共享测试失败，先在干净 worktree 与纯净 `HEAD` 做对照，区分“当前 issue 未完成”和“当前分支基线失败”；同时把提交边界污染与功能完成状态分开审查。
- `.agents/llmdoc/memory/reflections/2026-04-21-ztok-session-cache-governance-should-pair-metadata-prune-and-operator-commands.md`：`ztok` 的 session cache 治理不应只补 schema version；要成组补 metadata、容量裁剪、inspect/clear 命令，以及损坏或 schema 演进失败时的显式回退合同。
- `.agents/llmdoc/memory/reflections/2026-04-21-ztok-decision-trace-coverage-must-match-wired-surfaces.md`：`ztok` 决策 trace 一旦接进多条压缩路径，验证覆盖也必须按接线面同步扩齐；`summary` 的 side channel 要复用稳定签名而不是回喷原始 shell 命令。
- `.agents/llmdoc/memory/reflections/2026-04-21-ztok-fetcher-trace-coverage-should-redact-url-source-labels.md`：`ztok` fetcher 输出复用共享 trace 后，要同时锁 URL source label 的 userinfo/query redaction、内部 URL raw JSON 合同，以及 `curl`/`wget -O -` 新入口自己的 trace 集成覆盖。
- `.agents/llmdoc/memory/reflections/2026-04-22-npm-only-republish-must-align-package-name-and-provenance-repository.md`：`npm-only` 复用既有 release tgz 时，既要修正包名，也要让 tarball 的 `package.json.repository.url` 与实际执行 `npm publish --provenance` 的 workflow 仓库一致；否则 npm 会因 provenance 校验返回 `E422`。
- `.agents/llmdoc/memory/reflections/2026-04-22-app-server-thread-history-must-filter-inter-agent-envelopes.md`：`core` 已经把 `InterAgentCommunication` 排除出可见 assistant 文本，但 app-server v2 若有旁路 reducer 直接消费 `EventMsg::AgentMessage`，也必须同步过滤 envelope，否则 thread history 和 TUI 会把内部 JSON 当正文显示。
- `.agents/llmdoc/memory/reflections/2026-04-22-ztldr-responses-schema-and-description-must-share-the-same-language-contract.md`：修 `ztldr` 误用时，不能只改 shell interception 或 MCP schema；还要检查 Responses API 共享序列化是否压平顶层 `oneOf`，并确保工具 description 的补救建议不再推荐同样缺 `language` 也无法运行的 action。
- `.agents/llmdoc/memory/reflections/2026-04-22-ztldr-shell-intercepts-must-not-suggest-semantic-without-language.md`：`ztldr` 误用不能只归因于文档；还要检查 shell interception 是否给模型生成了缺少必填 `language` 的 `semantic` 模板，并把参数契约提示、schema 暴露和 rewrite 建议一起收口。
- `.agents/llmdoc/memory/reflections/2026-04-22-federation-protocol-should-stay-separate-from-root-tree-semantics.md`：最小入侵 federation 应保持独立 `federation-protocol` crate，实例身份、信封和本地 state 契约不要复用 `/root/...`、`InterAgentCommunication` 或 `SessionSource::SubAgent` 语义。
- `.agents/llmdoc/memory/reflections/2026-04-22-federation-cli-should-layer-client-over-daemon-not-inline-socket-code.md`：`codex federation ...` 命令面应经由独立 `federation-client` 连接 daemon；CLI 只做解析、spawn 和输出，不应内联 socket 协议细节。
- `.agents/llmdoc/memory/reflections/2026-04-22-federation-bridge-should-attach-at-thread-start-and-propagate-through-cli-entrypoints.md`：主 `codex` 的 federation bridge 应挂在 `app-server thread/start` seam，把 `TextTask` 桥接成普通 `Op::UserTurn`，并确保 `tui`、`exec`、`resume/fork` wrapper 都把 federation 参数透传到同一个 thread/start 入口，而不是碰 `InterAgentCommunication` 或新增 `SessionSource::Federation`。
- `.agents/llmdoc/memory/reflections/2026-04-22-zmemory-canonical-identity-layer-should-follow-upstream-role-semantics.md`：对齐上游 `nocturne_memory` 时，`core://agent` / `core://my_user` 应按 identity / personality / bond anchor 理解，不能在本地治理或契约文案里误收窄成固定名字/称呼槽；本地若只治理 `core://agent/my_user`，也要明确这是当前实现边界，不是上游唯一规范。
- `.agents/llmdoc/memory/reflections/2026-04-22-tui-startup-latency-should-triage-post-start-prefetches-separately.md`：当 `thread/start` 已经很快时，启动优化要继续拆首帧后的后台预取；`account/rateLimits/read` 这类只为 `/status` 预热的远端请求应移出 startup 邻近路径，公告 tip 的 GitHub 预热也应后移到首帧之后。
- `.agents/llmdoc/memory/reflections/2026-04-22-zmemory-content-governance-should-start-with-service-rules-and-separate-workspace-baselines.md`：`zmemory` 内容治理首轮应先落到 `service` 层独立规则模块和统一 result contract，再让写入/doctor/review 复用；本地验证若被缺失的 workspace member 卡住，要把基线阻塞与功能改动分开记录。
- `.agents/llmdoc/memory/reflections/2026-04-19-ubuntu-macos-arm64-cross-cc-search-dirs-wrapper.md`：Linux 交叉构建 macOS arm64 时，不能只配 target-specific `CC/CXX`；还要兜住 PATH 里的裸 `cc` / `c++`，避免第三方 `build.rs` 把宿主 Linux linker path 注入 Apple 链接。
- `.agents/llmdoc/memory/reflections/2026-04-19-core-compact-localization-boundary.md`：`core` 压缩链路汉化时，应区分用户可见事件流与内部模板；模板 `md` 不应按 UI 文案翻译，验证上优先拿 `cargo check -p codex-core --lib` 证据并显式隔离仓库现有测试阻塞。
- `.agents/llmdoc/memory/reflections/2026-04-19-zoffsec-sync-skill-should-pin-reference-and-local-surface.md`：为参考型上游仓库补同步 skill 时，不能只写 upstream 地址；要同时固定首选参考文件、默认 selective sync 范围，以及本地必须保留的架构边界。
- `.agents/llmdoc/memory/reflections/2026-04-19-zoffsec-sync-baseline-must-not-retroactively-cover-pre-skill-work.md`：sync baseline 不能追认 skill 出现前的本地实现；没有 contemporaneous 状态锚点时，只能把后续真正做过 selective sync 的子能力标成对齐。
- `.agents/llmdoc/memory/reflections/2026-04-19-zoffsec-rollout-clean-must-rewrite-task-complete-fallback.md`：Codex rollout 清理不能只改 `agent_message`；若 replay 会读取 `task_complete.last_agent_message` 兜底，就必须一起改写。拒绝检测也应区分强短语全文匹配与弱关键词开头匹配。
- `.agents/llmdoc/memory/reflections/2026-04-19-upstream-sync-image-detail-and-mergeback-compile-gate.md`：upstream 同步不能只看冲突文件；共享模型字段扩展要补全局匹配/构造扫描，并把 merge-back 编译阻塞显式写入 `STATE.md`。
- `.agents/llmdoc/memory/reflections/2026-04-19-zmemory-ztldr-half-wired-build-warnings.md`：`zmemory` / `ztldr` 构建告警往往来自“实现已写但生产未接线”的 prompt/runtime/state 缺口，应优先核对接线 seam，而不是先怀疑 Cargo feature。
- `.agents/llmdoc/memory/reflections/2026-04-19-zmemory-ztldr-main-path-and-pending-input-gaps.md`：`zmemory recall` 和 `ztldr` 路由这类 turn 级派生逻辑，不能只接普通初始输入；还要核对主路径 producer、pending-input 和 mailbox-triggered turn 是否都覆盖。
- `.agents/llmdoc/memory/reflections/2026-04-19-zmemory-ztldr-finalization-needs-worktree-aware-git-root-and-subcommand-help-output-localization.md`：`zmemory` / `ztldr` 最后收口时，共享 `git-utils` 的 worktree root 解析和子命令 help 输出汉化都要单独验证，不能只看 crate 本体测试。
- `.agents/llmdoc/memory/reflections/2026-04-19-local-analysis-tools-must-be-wired-in-tool-plan-and-all-rs.md`：判断本地分析工具是否真正保留时，不能只看 crate/CLI；还要核对共享 `tool_registry_plan.rs` 的 spec+handler 接线，以及 `core/tests/all` 是否真的聚合对应 e2e。
- `.agents/llmdoc/memory/reflections/2026-04-19-tools-test-harness-must-track-shared-schema-and-wire-api-evolution.md`：`zmemory` / `ztldr` 运行时接线补齐后，若 `codex-tools` 测试 helper 没跟上 `wire_api`、`JsonSchema` enum 和 `ModelPreset` 字段演进，会把测试基座过期误判成特性未完成。
- `.agents/llmdoc/memory/reflections/2026-04-19-embedded-ztok-prompts-should-use-logical-shell-entrypoint.md`：Embedded ZTOK 提示不要再教 `"$CODEX_SELF_EXE" ztok err/test ...`；应改教逻辑 launcher 形式和专用 `ztok shell` 泛化入口，再用 `pending_input` snapshots 验证真实注入链路。
- `.agents/llmdoc/memory/reflections/2026-04-19-embedded-ztok-prompt-should-stay-compact-and-test-behavior.md`：常驻 Embedded ZTOK prompt 应优先压短，只保留用户可见边界与默认入口；`prompts_tests` 锁语义锚点，`pending_input` snapshots 只锁注入链路，不锁整句文案。
- `.agents/llmdoc/memory/reflections/2026-04-19-launcher-agnostic-ztok-validation-should-favor-tests-all-when-lib-tests-drift.md`：共享运行时的 launcher 无关改动如果被 `codex-core --lib` 既有漂移阻断，应改用 `tests/all` 精确集成用例为 prompt、shell route 和 pending-input snapshots 取证。
- `.agents/llmdoc/memory/reflections/2026-04-19-tui-localization-needs-source-test-and-snapshot-alignment.md`：TUI 汉化回归要同时更新源码字符串、直接断言和 snapshot；实验功能菜单项还要追到 `features` 元数据源头，不要误改视图层。
- `.agents/llmdoc/memory/reflections/2026-04-19-upstream-sync-localization-checks-must-cover-metadata-and-subcommand-output.md`：upstream sync 的中文化基线不能只盯 `main.rs` 或视图入口；还要覆盖 feature 元数据、onboarding/history 组件，以及子命令直接输出。
- `.agents/llmdoc/memory/reflections/2026-04-19-pending-input-routing-should-preserve-turn-intent.md`：mid-turn `pending_input` 的 routing side effects 不能整包重算默认 directives；要保留 turn 基线并按最新 steer 决议。
- `.agents/llmdoc/memory/reflections/2026-04-19-pending-input-test-verification-chain-and-ztok-boundary.md`：验证 `suite::pending_input` 时要先打通 `tests/all` 聚合编译面；grep 路由回归也应断言意图与工具种类，而不是写死 `ztok` 代理前的 stdout 形状。
- `.agents/llmdoc/memory/reflections/2026-04-19-upstream-sync-cli-surface-baseline-must-cover-entrypoints-and-help-localization.md`：upstream sync 的本地特性基线不能只保 crate 存在；`codex-rs/cli/src/main.rs` 的子命令注册、dispatch、help 汉化与本地别名也必须纳入 merge-back gate。
- `.agents/llmdoc/memory/reflections/2026-04-18-responses-replay-reasoning-content-strip.md`：Responses API replay 历史时不能回传 `reasoning.content`；应在出站输入整理层统一剥离 raw reasoning，并用请求级测试锁住。
- `.agents/llmdoc/memory/reflections/2026-04-18-inter-agent-envelope-visible-turn-item-leak.md`：inter-agent JSON envelope 不能泄露到 turn-item / last-agent-message 这类可见 assistant 文本提取层。
- `.agents/llmdoc/memory/reflections/2026-04-18-provider-config-log-redaction.md`：provider 配置日志不能直接打印完整 `Debug`；应统一走安全摘要视图，并把 token/header/query 这类任意字符串载荷一并纳入脱敏范围。
- `.agents/llmdoc/memory/reflections/2026-04-18-upstream-sync-state-anchor-and-runtime-sentinel-gaps.md`：upstream sync 的 `last_sync_commit` 必须锚定落地 sync 提交，`local_surface` 还要把事件桥接和 synthetic constructor 完整性纳入基线检查。
- `.agents/llmdoc/memory/reflections/2026-04-18-tui-startup-and-realtime-audio-localization.md`：TUI 汉化应按用户链路而不是按单文件收口，实时音频相关术语优先从共享枚举源头统一，同时记录当前 `fmt` / `lib test` 被仓库既有问题阻塞的验证边界。
- `.agents/llmdoc/memory/reflections/2026-04-18-provider-model-precedence-after-provider-selection.md`：provider 默认模型必须在最终 provider 选择之后解析，`-P` / `--model-provider` 切换 provider 时同样适用，并且要把仓库现有 `fmt` / lib test 阻塞与本次改动区分开。
- `.agents/llmdoc/memory/reflections/2026-04-18-upstream-sync-external-feature-inventory-and-mergeback-gate.md`：上游同步的本地分叉特性应拆成 `json` 权威基线、`discover/promote` 候选流程和 merge-back 审查门，且 `discover` 默认基线只能来自祖先关系仍有效的 `last_sync_commit`。
- `.agents/llmdoc/memory/reflections/2026-04-16-wire-api-terminal-signal-gating.md`：`wire_api = "anthropic"` / `"chat"` 的流式 parser 需要区分“已有可收敛输出的断流”和“无有效输出的提前关闭”，不能只靠终止信号或连接关闭单独判定。
- `.agents/llmdoc/memory/reflections/2026-04-16-anthropic-compat-done-and-top-level-error.md`：Anthropic 兼容 provider 可能用顶层 `error` 和 `[DONE]` 代替标准 `error` 事件与 `message_stop`，parser 需要显式识别这两类兼容信号。
- `.agents/llmdoc/memory/reflections/2026-04-16-ztldr-literal-search-and-generic-symbol-routing.md`：`ztldr search` 默认 literal、非法 regex 结构化失败，以及通用 symbol 回退 exact-text 路由的反思。
- `.agents/llmdoc/memory/reflections/2026-04-17-model-provider-to-tui-traversal-chain.md`：`model_providers` 配置经过模型元数据链路最终影响 TUI UI 行为的反思。
- `.agents/llmdoc/memory/reflections/2026-04-17-tui-chinese-text-and-format-consistency.md`：TUI 中文化持续完善、Line::from_iter 格式一致性和备份文件清理的反思。
- `.agents/llmdoc/memory/reflections/2026-04-17-tui-remote-session-model-provider-propagation.md`：TUI 远端 app-server session 不应因省略远端 `cwd` 而一并丢失 `model_provider` 的反思。

## 路由规则
- 每次进入仓库先读 `startup.md`。
- 触及 Rust 子系统前，先读 `architecture/rust-workspace-map.md`，再按具体运行面补读相关架构文档。
- 处理配置、记忆、cwd/project-scoped 行为前，先读 `architecture/memory-and-doc-systems.md`。
- 执行具体改动前，先读对应 `guides/`；重复踩过坑的工作流再补读 `memory/reflections/`。
- 临时调查草稿在 `/tmp/llmdoc/workspace-8af22c44f404/investigations/`。
