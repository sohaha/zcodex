# ztldr 提示与文档优化指南

## 适用场景
- 当任务涉及 `ztldr` 的工具描述、路由提示、MCP 文档、agent-first 文档或相关测试时使用。
- 目标不是扩展 `native-tldr` 能力，而是让模型更稳定地在结构化代码问题上优先选择 `ztldr`。

## 核心原则
- 先以 `codex-rs/native-tldr/src/tool_api.rs` 的 `TldrToolAction` 与参数约束作为唯一事实源。
- 区分三类职责：
  - 代码侧 description：直接影响模型工具选择。
  - runtime interception prompt：直接影响 broad grep/read 是否被替换或软拦截。
  - 接口文档：说明 output schema、`degradedMode`、`structuredFailure` 等 contract，不承担主要路由职责。
- 如果要提高 `ztldr` 命中率，优先增强 description 与 interception prompt，而不是继续堆接口文档篇幅。
- 任何提示增强都要显式保留 raw escape hatch：regex、逐字文本核对、或用户明确要求 raw grep/read 时不强推 `ztldr`。
- 对降级和失败语义要前置到提示词里：`degradedMode` / `structuredFailure` 不能被模型当作正常成功静默吞掉。

## 推荐检查顺序
1. 读取 `codex-rs/native-tldr/src/tool_api.rs`，确认真实 action、参数和必填约束。
2. 读取 `codex-rs/tools/src/tool_spec.rs` 与 `codex-rs/mcp-server/src/tldr_tool.rs`，核对主 description 与参数描述是否一致。
3. 读取 `codex-rs/core/src/tools/rewrite/shell_search_rewrite.rs`，确认 interception message 是否说明：
   - 为什么拦截
   - 推荐 action 是什么
   - 哪些情况应保留 raw grep/read
   - `degradedMode` / `structuredFailure` 应如何解释
4. 最后同步 `docs/tldr-agent-first-guidance/tool-description.md` 与 `codex-rs/docs/codex_mcp_interface.md`。

## 最小验证闭环
- `cd /workspace/codex-rs && cargo test -p codex-tools tool_spec`
- `cd /workspace/codex-rs && cargo test -p codex-mcp-server tldr_tool`
- `cd /workspace/codex-rs && cargo test -p codex-core shell_search_rewrite`
- `cd /workspace && rg -n "ztldr|degradedMode|structuredFailure" docs/tldr-agent-first-guidance/tool-description.md codex-rs/docs/codex_mcp_interface.md`

## 常见陷阱
- 只改文档，不改代码侧 description，模型行为不会明显变化。
- 只改 tool description，不改 shell interception message，broad grep/read 起手问题仍会保留。
- 未以 `tool_api.rs` 为事实源，导致 action 列表和真实能力再次漂移。
- 在 Cadence issue `notes` 中写带引号的单行命令，导致 TOML 解析失败；这类内容优先改成多行字符串。

## 路由 contract 收敛（2026-04-09）
- 统一路由事实源已下沉到 `codex-rs/core/src/tools/rewrite/tldr_routing.rs`。
- 之后修改 `auto_tldr` / `read_gate` / `shell_search_rewrite` 路由行为时，先改该模块，再同步入口接线与测试。
- 避免在入口继续追加大段提示词；优先复用统一短解释模板。
