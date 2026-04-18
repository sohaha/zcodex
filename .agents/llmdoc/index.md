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
- 时间窗按当前日期 `2026-04-18` 计算，覆盖 `2026-04-16` 至 `2026-04-18`。
- 更早的历史反思直接到 `.agents/llmdoc/memory/reflections/` 按日期文件名检索。
- `.agents/llmdoc/memory/reflections/2026-04-18-tui-startup-and-realtime-audio-localization.md`：TUI 汉化应按用户链路而不是按单文件收口，实时音频相关术语优先从共享枚举源头统一，同时记录当前 `fmt` / `lib test` 被仓库既有问题阻塞的验证边界。
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
