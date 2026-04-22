## 2026-04-22 `ztldr` function schema 顶层必须是纯 object，不能保留 top-level combinators

### 背景

这次现场报错比之前更具体：

- `Invalid schema for function 'ztldr': schema must have type 'object' and not have 'oneOf'/'anyOf'/'allOf'/'enum'/'not' at the top level`

这说明 provider 的 function schema 校验不只是要求顶层 `type: "object"`，还明确禁止顶层保留组合关键字。

### 这次确认的事实

- `codex-rs/tools/src/tool_spec.rs` 虽然能把 `JsonSchema::OneOf` 压平成 object，但如果继续为 `ztldr` 特判保留顶层 `oneOf`，provider 仍然会直接拒绝。
- `codex-rs/mcp-server/src/tldr_tool.rs` 之前也走了同样的思路：顶层 object 外面再挂一个 `oneOf`，这在当前 provider 规则下同样无效。
- 真正兼容的形状必须是：
  - 顶层 `type: "object"`
  - 顶层只放 `properties` / `required` / `additionalProperties` 这类普通 object schema 字段
  - `action` 变成顶层属性里的字符串枚举
  - action-specific 的强约束退回运行时解析和错误提示兜底

### 这次形成的原则

- `ztldr` 这类多 action 工具，只要最终会以 function tool 形式暴露给 provider，就不能在顶层继续保留 `oneOf` / `anyOf` / `allOf` / `enum` / `not`。
- Responses API 和 MCP 必须共享同一份“纯 object 顶层” contract；不能一条链路扁平化，另一条链路还偷偷保留 combinator。
- 回归测试不能再只验证“顶层是 object”或“还能看到 oneOf 变体”；必须显式断言顶层没有 `oneOf`，并锁住 `action` 顶层枚举和关键字段描述。

### 本次落地边界

- `codex-rs/tools/src/tool_spec.rs`
  - 移除 `ztldr` 保留顶层 `oneOf` 的特判，统一走顶层 object 扁平化。
- `codex-rs/mcp-server/src/tldr_tool.rs`
  - 顶层 schema 改为纯 object，不再输出 top-level `oneOf`。
- 测试
  - `codex-tools`、`codex-core`、`codex-mcp-server` 都改成断言：
    - 顶层 `type == "object"`
    - 顶层 `required == ["action"]`
    - 顶层不存在 `oneOf`
    - `properties.action.enum` 覆盖当前 action surface

### 后续检查点

以后再看到 provider 报 tool schema invalid，按这个顺序查：

1. 最终发给 provider 的 JSON 顶层是否仍带 `oneOf` / `anyOf` / `allOf` / `enum` / `not`
2. Responses 和 MCP 两条链路是否都已经扁平到纯 object 顶层
3. 是否还有旧测试只锁住“顶层 object + 保留 oneOf”这种已经失效的旧 contract
