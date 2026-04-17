# 过滤不支持的工具类型

## 背景
- 当前状态：web_search 工具在所有 wire_api 模式下都会被注册到工具注册表
- 触发原因：当使用 `wire_api = "chat"` 或 `wire_api = "anthropic"` 时，模型会返回错误："chat completions does not support tool type web_search; use wire_api = \"responses\" for hosted tools"
- 预期影响：防止在不兼容的 API 模式下注册不支持的工具类型

## 目标
- 目标结果：web_search 工具仅在 `wire_api = "responses"` 时注册
- 完成定义（DoD）：
  - 修改 `tool_registry_plan.rs` 中的工具注册逻辑
  - 添加 wire_api 检查条件
  - 添加单元测试验证过滤逻辑
  - 运行测试确保修复有效
- 非目标：不修改其他工具的注册逻辑

## 范围
- 范围内：
  - `codex-rs/tools/src/tool_registry_plan.rs` 中的 web_search 工具注册
  - `codex-rs/tools/src/tool_registry_plan_tests.rs` 添加测试用例
- 范围外：
  - 其他工具类型的过滤逻辑
  - wire_api 的实现细节

## 影响
- 受影响模块：`codex-tools` crate
- 受影响接口/命令：所有使用 wire_api=chat 或 wire_api=anthropic 的会话
- 受影响数据/模式：工具注册表
- 受影响用户界面/行为：在非 Responses API 模式下将不显示 web_search 工具

## 约束与依赖
- 约束：
  - 必须保持向后兼容性
  - 不能影响现有 Responses API 用户的工具可用性
  - 修改必须通过所有现有测试
- 外部依赖：无

## 实施策略
- 总体方案：在 `build_tool_registry_plan` 函数中，为 web_search 工具注册添加 wire_api 检查条件
- 关键决策：
  - 使用 `matches!(config.wire_api, WireApi::Responses)` 作为条件
  - 仅当 wire_api 为 Responses 时才调用 `create_web_search_tool`
- 明确不采用的方案：不修改工具创建函数签名，保持接口稳定

## 阶段拆分

### 阶段 1：修改工具注册逻辑
- 目标：添加 wire_api 检查条件
- 交付物：
  - 修改后的 `tool_registry_plan.rs`
  - 添加条件检查 `if matches!(config.wire_api, WireApi::Responses)`
- 完成条件：代码修改完成并通过编译
- 依赖：无

### 阶段 2：添加测试用例
- 目标：验证过滤逻辑正确性
- 交付物：
  - 测试用例覆盖三种 wire_api 模式（Responses、Chat、Anthropic）
  - 验证 web_search 工具仅在 Responses 模式下注册
- 完成条件：测试用例通过
- 依赖：阶段 1 完成

### 阶段 3：运行验证
- 目标：确保修复有效且无回归
- 交付物：
  - 运行相关测试套件
  - 确认所有测试通过
- 完成条件：所有测试通过
- 依赖：阶段 2 完成

## 测试与验证

- 核心验证：
  - 单元测试：`tool_registry_plan_tests.rs` 中添加测试验证 wire_api 过滤
  - 集成测试：运行 `cargo nextest run -p codex-tools`
- 必过检查：
  - 现有测试必须全部通过
  - 新测试用例必须通过
- 回归验证：
  - 运行 `cargo nextest run -p codex-core` 确保核心功能未受影响
- 手动检查：无
- 未执行的验证：无

## 风险与缓解
- 关键风险：
  - 可能影响依赖 web_search 工具的现有功能
- 触发信号：
  - 现有测试失败
  - 运行时工具不可用错误
- 缓解措施：
  - 仔细审查所有相关测试
  - 确保仅在非兼容模式下过滤
- 回滚/恢复方案：通过 git revert 撤销更改

## 参考
- `/workspace/codex-rs/tools/src/tool_registry_plan.rs:235-245`（web_search 工具注册位置）
- `/workspace/codex-rs/tools/src/tool_config.rs:234`（ToolsConfig.wire_api 字段）
- `/workspace/codex-rs/model-provider-info/src/lib.rs:35-43`（WireApi 枚举定义）
