# codex-native-tldr

`codex-native-tldr` 是面向 `codex-cli` 的原生代码上下文分析骨架 crate。

当前阶段提供：

- 统一的分析引擎入口 `TldrEngine`
- AST / 调用图 / CFG / DFG / PDG 的占位 API
- 语言注册表骨架
- daemon / MCP / semantic 配置骨架

后续阶段会在此 crate 上继续补充：

- 七种首批语言的 tree-sitter 解析链
- `codex tldr ...` CLI 接入
- daemon / session 缓存
- MCP tool 集成
- semantic search feature gate
