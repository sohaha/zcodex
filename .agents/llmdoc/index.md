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
- `.agents/llmdoc/memory/reflections/2026-04-09-ztldr-routing-contract-unification.md`：ztldr 路由 contract 一次性收敛反思。
- `.agents/llmdoc/memory/reflections/2026-04-09-ztldr-query-signal-observability.md`：ztldr query signal 落到统一日志面的反思。
- `.agents/llmdoc/memory/reflections/2026-04-09-ztldr-tool-route-metrics.md`：ztldr route 指标化与聚合面的反思。
- `.agents/llmdoc/memory/reflections/2026-04-09-ztldr-real-query-matrix.md`：ztldr 真实 query 分类矩阵的回归策略反思。
- `.agents/llmdoc/memory/reflections/2026-04-09-ztldr-routing-switches.md`：ztldr 路由开关语义的回归边界反思。
- `.agents/llmdoc/memory/reflections/2026-04-09-ztldr-project-corpus-summary.md`：基于当前项目真实 query 样本的 summary 回归反思。
- `.agents/llmdoc/memory/reflections/2026-04-09-ztldr-shell-corpus-summary.md`：基于当前项目 shell 查询样本的 summary 回归反思。
- `.agents/llmdoc/memory/reflections/2026-04-09-ztldr-grep-corpus-summary.md`：基于当前项目 grep rewrite 样本的 summary 回归反思。
- `.agents/llmdoc/memory/reflections/2026-04-09-ztldr-shared-test-corpus.md`：ztldr 三层测试共享样本事实源的反思。
- `.agents/llmdoc/memory/reflections/2026-04-09-ztldr-shared-summary-helpers.md`：ztldr 三层 summary 测试共享标签与计数 helper 的反思。
- `.agents/llmdoc/memory/reflections/2026-04-09-ztldr-real-query-matrix-facts.md`：ztldr 真实 query matrix 收敛到共享事实源的反思。
- `.agents/llmdoc/memory/reflections/2026-04-09-ztldr-shared-shell-fixtures.md`：ztldr shell 命令变体收敛到共享 fixture 的反思。
- `.agents/llmdoc/memory/reflections/2026-04-09-ztldr-shared-grep-tool-calls.md`：ztldr grep tool call builder 收敛的反思。
- `.agents/llmdoc/memory/reflections/2026-04-09-ztldr-shared-grep-payloads.md`：ztldr grep JSON payload 收敛到共享 helper 的反思。
- `.agents/llmdoc/memory/reflections/2026-04-09-ztldr-daemon-socket-path-length.md`：macOS 上 ztldr daemon 因 Unix socket 路径超长而启动失败的根因与修法反思。
- `.agents/llmdoc/memory/reflections/2026-04-09-ctf-resume-clean-then-resume.md`：CTF 会话在复用现有 resume 选择器时接入 clean-then-resume 的实现与验证阻塞反思。
- `.agents/llmdoc/memory/reflections/2026-04-10-clouddev-mise-copy-on-write-mask.md`：Clouddev 把 `/root/.local/bin` 与 mise 数据目录挂成 copy-on-write 后遮住镜像预装工具的反思。
- `.agents/llmdoc/memory/reflections/2026-04-10-build-self-heals-missing-sccache-wrapper.md`：构建脚本在 Clouddev 预设 `RUSTC_WRAPPER=sccache` 但二进制缺失时主动自愈的反思。
- `.agents/llmdoc/memory/reflections/2026-04-10-buddy-snapshot-accept-scope.md`：在 dirty monorepo 中只接收目标 snapshot，避免 `cargo insta accept` 扩大变更边界的反思。
- `.agents/llmdoc/memory/reflections/2026-04-10-core-tests-network-sandbox-split.md`：在 sandbox 环境下拆分 `codex-core` 测试以兼顾禁网与环境变量断言的回路反思。
- `.agents/llmdoc/memory/reflections/2026-04-10-upstream-sync-code-mode-output-schema-local-preservation.md`：同步上游 code-mode `output_schema` 时，先验本地单工具描述行为再决定是否接收上游测试断言的反思。
- `.agents/llmdoc/memory/reflections/2026-04-11-upstream-sync-native-feature-revert-triage.md`：同步时先确认功能来源，避免把 upstream 已回滚的原生功能误保留成本地分叉。
- `.agents/llmdoc/memory/reflections/2026-04-12-upstream-sync-state-auditability.md`：同步提交若不是 merge-parent 型，必须靠提交正文与 `STATE.md` 保持 upstream 基线可审计。
- `.agents/llmdoc/memory/reflections/2026-04-12-upstream-sync-toolname-cli-test-loop.md`：同步 ToolName / namespaced MCP tools 后，如何分流 `cargo` 环境变量噪音、`cli_stream` 缺少 `codex` 二进制，以及高并发 nextest timeout 的反思。
- `.agents/llmdoc/memory/reflections/2026-04-12-remote-compaction-nonblocking-connectors.md`：远程 compaction 仍卡住时，要继续检查 `build_initial_context()` 和 connector 指令链路是否还在走阻塞版 MCP tools。
- `.agents/llmdoc/memory/reflections/2026-04-12-core-nextest-mcp-timeout-triage.md`：`codex-core` 中依赖 MCP tool ready 的测试若只在 nextest 30 秒 case timeout 下失败，先用定向 `cargo test --exact` 区分 runner 时限和真实回归。
- `.agents/llmdoc/memory/reflections/2026-04-09-ztldr-semantic-cache-runtime-dir.md`：ztldr semantic cache 迁出项目根 `.tldr/` 的排查与落地反思。
- `.agents/llmdoc/memory/reflections/2026-04-13-zmemory-relative-path-config-deserialization.md`：`zmemory.path` 若需要按 repo root / cwd 解析，就不能在通用配置层提前反序列化成 `AbsolutePathBuf`。
- `.agents/llmdoc/memory/reflections/2026-04-13-inline-resize-clear-order.md`：inline viewport 因 resize 下移时，必须先按旧 origin 清屏，否则旧 frame 会残留在新 viewport 上方。
- `.agents/llmdoc/memory/reflections/2026-04-13-inline-resize-screen-delta-fallback.md`：inline viewport 在 cursor 行未变化时，必须回退到 screen delta，否则 tmux/CPR 异常时会丢失 viewport 重定位。

- `.agents/llmdoc/memory/reflections/2026-04-12-cli-startup-error-chain-visibility.md`：CLI 入口只打印最外层 anyhow 错误会隐藏 TUI bootstrap 根因，需显式展开 error chain。
- `.agents/llmdoc/memory/reflections/2026-04-12-windows-native-tldr-daemon-first.md`：Windows 并不需要独立 native-tldr 安装物，缺的是非 Unix daemon-first 的 TCP endpoint metadata 与生命周期接线。
- `.agents/llmdoc/memory/reflections/2026-04-12-windows-installer-bundle-parity.md`：Windows `install.ps1` 收口到 npm bundle/vendor 分发语义，并补齐 `CODEX_BASE_URL` / `CODEX_INSTALL_DIR` 对齐点的反思。
- `.agents/llmdoc/memory/reflections/2026-04-13-ztok-find-rewrite-boundary.md`：shell 自动重写不能越过 `ztok find` 的能力边界，rewrite 层应复用运行时不支持参数事实源的反思。

## 路由规则
- 每次进入仓库先读 `startup.md`。
- 触及 Rust 子系统前，先读 `architecture/rust-workspace-map.md`，再按具体运行面补读相关架构文档。
- 处理配置、记忆、cwd/project-scoped 行为前，先读 `architecture/memory-and-doc-systems.md`。
- 执行具体改动前，先读对应 `guides/`；重复踩过坑的工作流再补读 `memory/reflections/`。
- 临时调查草稿在 `/tmp/llmdoc/workspace-8af22c44f404/investigations/`。
