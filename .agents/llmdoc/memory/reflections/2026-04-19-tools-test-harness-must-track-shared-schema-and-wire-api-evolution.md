# 2026-04-19 tools 测试基座必须跟上共享 schema 与 wire_api 演进，否则会误判本地特性“未完成”

- 这轮继续收口 `zmemory` / `ztldr` 时，主链路其实已经打通：
  - `tool_registry_plan.rs` 已注册 `ztldr` / `zmemory` 的 spec + handler。
  - `core/tests/all` 也已聚合 `tldr_e2e` / `zmemory_e2e`。
  - `codex ztldr --help` 已支持 `--language`，并显示 `[别名： --lang]`。
- 但 `codex-tools` 的旧单测基座没有跟上共享类型演进，导致收尾阶段会出现大量“看起来像功能没补完”的假信号，主要有三类：
  - `ToolsConfigParams` 新增 `wire_api` 后，老测试仍按旧字段构造。
  - `JsonSchema` 从“平铺字段结构体式访问”演进为 enum 后，老 helper 还在直接访问 `schema_type`、`properties`、`description`、`items`、`any_of`。
  - `ModelPreset` 新增 `skip_reasoning_popup` 后，测试构造器没有补全字段。

## 结论

- 判断本地分叉能力是否“真正完成”时，不能只看运行时和 e2e 是否通过。
- 若 `codex-tools` 里的测试 helper 仍停留在旧 API，会把“测试基座过期”误读成“功能实现不完整”。
- 这类问题应该优先在测试辅助层收口，而不是反向给生产代码加兼容层。

## 这轮有效做法

- 在 `tools/src/agent_tool_tests.rs` 里只补测试构造缺失字段，不动生产类型。
- 在 `tools/src/tool_registry_plan_tests.rs` 里：
  - 统一补 `wire_api: WireApi::Responses`。
  - 把 object/string/schema-strip helper 全部改成按 `JsonSchema` enum 匹配。
  - 去掉对 `ToolsConfigParams::default()` / `ToolRegistryPlanParams::default()` 的错误假设，改为本地显式 helper。
- 这样可以把漂移集中在测试层修掉，不扩大到运行时实现。

## 后续规则

- 以后只要共享工具层新增：
  - 配置字段
  - schema 表示方式
  - 模型元数据字段
- 就要同时检查：
  - `tools/src/*_tests.rs` 里的构造 helper
  - `tool_registry_plan_tests.rs` 对 schema 的断言方式
  - 是否还有假设 `Default` 的旧测试便利写法
- 对本地特性回归，推荐验证顺序：
  1. 先看运行时 spec/handler 接线。
  2. 再看 `tests/all` 聚合 e2e。
  3. 最后收口 `codex-tools` 的测试基座和 `just fix -p codex-tools`。
