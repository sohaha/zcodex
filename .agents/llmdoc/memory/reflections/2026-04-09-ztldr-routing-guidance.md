# 2026-04-09 ztldr 路由提示优化反思

## 背景
- 这轮任务表面上是“优化 ztldr 的工具描述或提示词”，但真正的问题不是文案不够好看，而是模型侧缺少足够强的路由决策信号。
- 起因之一是我前一轮在处理 `ztldr` 对外重命名时，没有先使用 `ztldr` 自己来做结构化影响面分析，反而过早收窄为字符串与契约替换，这暴露出 `ztldr` 当前提示层对模型的牵引力仍然不够。

## 这次学到的事实
- `ztldr` 的 prompt/description 触点至少有三层：
  1. 代码侧 tool description（`codex-rs/tools/src/tool_spec.rs`、`codex-rs/mcp-server/src/tldr_tool.rs`）
  2. 运行时拦截提示（`codex-rs/core/src/tools/rewrite/shell_search_rewrite.rs`）
  3. 文档侧 guidance（`docs/tldr-agent-first-guidance/tool-description.md`、`codex-rs/docs/codex_mcp_interface.md`）
- 对模型行为影响最大的不是接口参考文档，而是代码侧 description 与 shell search interception message；如果这两处仍然是“功能说明书语气”，模型就会继续用 broad grep/read 起手。
- `ztok` 的价值不只是做命令改写，更重要的是它对“为什么改写/为什么不改写”的边界表达非常清楚；`ztldr` 的提示层应借鉴这种边界清晰度，而不是只说“这是结构化分析工具”。
- `native-tldr` 的真实 `TldrToolAction` 远多于此前工具说明里列出的动作；如果 `tool_spec`、MCP tool description 和文档不以 `native-tldr/src/tool_api.rs` 为事实源，很容易再次出现动作口径漂移。

## 这次踩到的坑
- 仅靠定向 `cargo test -p codex-tools tool_spec` 无法覆盖 tool registry 的全局集合约束；我额外跑 `cargo nextest run -p codex-tools` 时，撞到了一个与本次改动不直接相关的 `tool_registry_plan` 断言失败，说明“局部测试通过”不等于“工具集整体没有其他历史问题”。
- 在 issue 状态回写时，把带双引号的命令直接塞进单行 TOML 字符串，导致 `notes` 解析失败；Cadence 的执行回写如果包含引号较多的命令，优先用多行基础字符串更稳。
- `cadence_validate.js` 的命令集只覆盖 `issue` 与 `execution-write`，不支持 `plan`；如果要在 Cadence 里做机械校验，先读脚本本身，不要按名字猜子命令。

## 后续建议
- 以后再做工具提示优化，先区分三类文档职责：
  - 代码侧 description：直接影响模型选工具
  - interception prompt：直接影响模型是否回退 broad grep/read
  - 接口文档：说明 contract，不承担主要 routing 责任
- 对 `ztldr` 这类多入口工具，新增或修改 action/参数描述时，必须先以 `native-tldr/src/tool_api.rs` 为唯一事实源，再同步 `tool_spec`、MCP 注册和文档。
- 若后续要继续提升 `ztldr` 使用率，优先增加或优化 routing/interception 层，而不是继续扩接口文档篇幅。
- 在 Cadence 执行回写里，任何包含引号、正则或命令行片段的 `notes` 都优先写多行字符串，避免 TOML 字面量转义出错。
