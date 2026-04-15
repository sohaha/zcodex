# 计划：ztldr `--lang` 参数可选化 + 文件后缀自动识别

## 目标

将 ztldr 子命令和 MCP 工具中当前必需的 `--lang` / `language` 参数改为可选。当用户未显式传递语言时，通过文件路径后缀自动推断。显式传递时仍以用户指定为准。**不修改工具的 description/用法提示词**。

## 推断策略

1. 显式传递 `lang` / `language` → 直接使用
2. 未传递 + 有可用的 `path` → `SupportedLanguage::from_path()` 推断
3. 未传递 + 无 `path`（或后缀不受支持） → 报错，提示显式传递

**不做项目根目录扫描**：根目录文件以配置文件为主，推断结果不可靠，猜错比报错代价更高。

## 现状分析

### CLI 子命令（`codex-rs/cli/src/tldr_cmd.rs`）

| 子命令 | `--lang` 现状 | 有路径参数？ | 改动后 |
|--------|-------------|------------|-------|
| Structure/Context/Impact/Calls/Dead/Arch/Cfg/Dfg | 必需 | 否（`symbol`） | 可选，无 `lang` 时报错 |
| Importers | 必需 | 否（`module`） | 可选，无 `lang` 时报错 |
| Semantic | 必需 | 否（`query`） | 可选，无 `lang` 时报错 |
| ChangeImpact | 必需 | 是（`paths`） | 可选，从 `paths[0]` 推断 |
| Extract/Imports | 已可选 | 是（`path`） | 不变 |
| Slice | 已可选 | 是（`path`） | 不变 |
| Search | 已可选 | — | 不变 |
| Diagnostics | 已可选 | 是（`path`） | 不变 |
| Doctor | 已可选 | — | 不变 |

### MCP 工具（`codex-rs/native-tldr/src/tool_api.rs`）

`TldrToolCallParam.language` 类型已是 `Option<TldrToolLanguage>`。

- `required_language(&args)` — 11 处调用。改为 `resolve_language(&args, path_hint)`，支持从 `args.path` 推断。
- `required_or_inferred_language(&args, path)` — 4 处调用，已有正确推断逻辑。统一合并到 `resolve_language`。

## 涉及文件

### 1. `codex-rs/cli/src/tldr_cmd.rs`

- `TldrAnalyzeCommand.lang`：`CliLanguage` → `Option<CliLanguage>`
- `TldrImportersCommand.lang`：同上
- `TldrSemanticCommand.lang`：同上
- `TldrChangeImpactCommand.lang`：同上
- `run_analysis_command()`：从 `cmd.lang.into()` 改为解析 `Option`，`None` 报错
- `run_importers_command()`：同上
- `run_semantic_command()`：同上
- `run_change_impact_command()`：`None` 时从 `cmd.paths[0]` 推断

### 2. `codex-rs/native-tldr/src/tool_api.rs`

- 合并 `required_language` 和 `required_or_inferred_language` 为 `resolve_language(args, path_hint: Option<&str>)`
- 推断逻辑：显式 > path_hint 推断 > 报错
- 所有 15 处调用点统一改用 `resolve_language`

### 3. `codex-rs/native-tldr/src/lang_support/mod.rs`

- 不需要新增函数，`SupportedLanguage::from_path()` 已存在且足够

## 不涉及的文件

- MCP 工具 description（`tldr_tool.rs` 的 `create_tool_for_tldr_tool_call_param`）
- `codex-rs/mcp-server/src/message_processor.rs`
- `codex-rs/core/src/tools/handlers/tldr.rs`

## 验证

1. `cargo nextest run -p codex-cli --test tldr`
2. `cargo nextest run -p codex-native-tldr`
3. `cargo nextest run -p codex-mcp-server`
