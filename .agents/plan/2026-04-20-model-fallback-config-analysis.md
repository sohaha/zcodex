# 模型回退配置深度分析

## 背景
- 当前状态：
  `config.toml` 暴露了 `fallback_provider`、`fallback_model` 与 `fallback_providers` 三组入口；`core` 配置加载阶段会把 legacy 单回退入口解析成 `fallback_provider_id` / `fallback_provider` / `fallback_model`，并归一化进 `fallback_providers` 链路。
- 触发原因：
  用户要求先深入分析当前“模型回退”配置原理与功能逻辑。
- 预期影响：
  明确哪些行为是真正生效的回退，哪些只是同名但不同层面的兜底或告警，为后续 issue 拆分与执行提供依据。

## 目标
- 目标结果：
  形成一份可追溯的分析基线，说明配置入口、归一化规则、运行时切换链路、相关测试现状与验证阻塞。
- 完成定义（DoD）：
  已确认以下事实并可定位到代码：
  1. 配置层如何声明并归一化 `fallback_provider` / `fallback_model` / `fallback_providers`
  2. 运行时哪里负责切换 provider/model，实际是否接线
  3. “未知模型元数据回退”与“provider/model 请求回退”之间的区别
  4. `ModelReroute` 告警与配置回退的边界
  5. 当前仓库内自动化验证能否执行，以及阻塞点是什么
- 非目标：
  无代码修复、无配置改写、无新增文档、无行为变更。

## 范围
- 范围内：
  `codex-rs/config`、`codex-rs/core`、`codex-rs/models-manager` 中与模型回退配置相关的声明、解析、运行时调用点、测试与告警逻辑。
- 范围外：
  非模型回退语义的普通 fallback；与本任务无关的 app-server 线程恢复 fallback、文件系统 fallback、网络 fallback 细节；任何修复实现。

## 影响
- 受影响模块：
  `codex-rs/config/src/config_toml.rs`、`codex-rs/core/src/config/mod.rs`、`codex-rs/core/src/session/turn.rs`、`codex-rs/core/src/session/turn_context.rs`、`codex-rs/core/src/session/mod.rs`、`codex-rs/models-manager/src/manager.rs`、`codex-rs/models-manager/src/model_info.rs`、`codex-rs/core/tests/suite/websocket_fallback.rs`
- 受影响接口/命令：
  `config.toml` 的 `fallback_provider`、`fallback_model`、`fallback_providers`；会话采样请求提交流程；`env RUSTC_WRAPPER= cargo test -p codex-core request_fallback -- --nocapture`
- 受影响数据/模式：
  `ConfigToml`、`Config`、`FallbackProviderToml`、`FallbackProviderConfig`、`TurnContext`、`ModelInfo.used_fallback_model_metadata`
- 受影响用户界面/行为：
  可能影响真实请求是否会切到备用 provider/model；影响未知模型元数据告警；影响 server 侧 `ModelReroute` 告警的正确解读。

## 约束与依赖
- 约束（时间/兼容性/安全/性能/发布窗口等）：
  当前阶段是 `cadence-planning`，只产出计划；计划内容只写已确认事实。当前工作区存在无关脏改动，但未与本次计划文件冲突。当前 `codex-core` 测试验证被现有编译错误阻塞，不能把“测试未通过”误判为“回退逻辑已失效”。
- 外部依赖（系统/人员/数据/权限等）：
  需要后续在可编译基线或已修复测试基线下重跑最小验证命令，才能完成行为级确证。

## 实施策略
- 总体方案：
  先把“同名 fallback”拆成四层再分析：配置归一化、provider/model 请求回退、未知模型元数据回退、服务端模型改路由告警。只要四层边界清楚，后续 issue 才不会把不同问题混在一起。
- 关键决策：
  1. 把 `fallback_provider` + `fallback_model` 视为 legacy 单回退入口，`fallback_providers` 视为当前标准有序链
  2. 把 `used_fallback_model_metadata` 视为元数据兜底，而不是 provider/model 请求回退
  3. 把 `Session::maybe_warn_on_server_model_mismatch` 的 `ModelReroute` 视为服务端已改路由后的告警，不视为本地配置回退的实现证据
  4. 把 `next_fallback_turn_context()`、`should_retry_with_fallback_provider()`、`TurnContext::with_model_provider()` 当前“未被调用”的事实视为高优先级核验点
- 明确不采用的方案（如有）：
  不在计划阶段直接修改代码或先假设“测试一定代表真实行为”；不把 WebSocket->HTTPS 传输回退与 provider/model 回退混为一谈。

## 阶段拆分
### 配置层确权
- 目标：
  明确配置入口、解析规则、归一化顺序与候选链构造方式。
- 交付物：
  配置映射表与 legacy/chain 关系说明。
- 完成条件：
  已确认 `fallback_provider` 在未被 `fallback_providers` 覆盖时会被插入索引 0；已确认 `fallback_model` 仅附着于该 legacy 入口。
- 依赖：
  无

### 运行时链路确权
- 目标：
  明确 provider/model 回退的候选选择规则、跳过规则，以及实际是否接入主执行路径。
- 交付物：
  运行时链路说明，包含“意图逻辑”和“实际接线状态”。
- 完成条件：
  已确认候选模型优先级为 `fallback.model` -> `fallback.provider.model` -> `primary_requested_model`；已确认当前代码内 `next_fallback_turn_context()` / `should_retry_with_fallback_provider()` / `with_model_provider()` 未找到主链路调用点。
- 依赖：
  配置层确权

### 验证阻塞与后续 issue 入口
- 目标：
  记录现有测试与验证状态，形成后续 `cadence-issue-generation` 的边界。
- 交付物：
  验证阻塞说明、最小复现命令、后续 issue 候选范围。
- 完成条件：
  已确认仓库内存在专门测试期望 provider/model 回退行为；已确认最小验证命令当前被无关编译错误阻塞；已把后续 issue 边界收敛到“接线回退链路”或“清理失效配置/测试”二选一。
- 依赖：
  配置层确权、运行时链路确权

## 测试与验证
- 核心验证：
  静态验证代码引用与调用关系，核对配置声明、归一化实现、运行时 helper、元数据兜底与告警路径是否对应。
- 必过检查：
  `env RUSTC_WRAPPER= cargo test -p codex-core request_fallback -- --nocapture`
- 回归验证：
  `core/tests/suite/websocket_fallback.rs` 中 4 个 `request_fallback_*` 用例应能在可编译基线下完成行为级验证。
- 手动检查：
  1. 核对 `fallback_provider` 是否会被插入 `fallback_providers[0]`
  2. 核对 fallback model 解析优先级
  3. 核对 `ModelReroute` 是否只来自服务端模型不一致告警
  4. 核对 `used_fallback_model_metadata` 是否只在未知模型 slug 时置为 `true`
- 未执行的验证（如有）：
  行为级自动化验证尚未完成；当前命令被现有编译错误阻塞，命令输出显示 `codex-core` 测试目标存在大量与本任务无关的编译失败。

## 风险与缓解
- 关键风险：
  1. 配置看似完整，但 provider/model 回退链路可能未接入主请求路径，导致用户产生“已配置就会生效”的错误预期
  2. 测试文件宣称覆盖 request fallback，但当前仓库未处于可验证状态，容易让分析停留在静态层
  3. `ModelReroute` 与 `used_fallback_model_metadata` 名称上都像“回退”，容易被误读成同一功能
- 触发信号：
  1. `cargo test` 输出中 `next_fallback_turn_context`、`should_retry_with_fallback_provider`、`with_model_provider` 被报告为未使用
  2. `ztldr` 与文本检索均未找到这些 helper 的运行时调用点
  3. 仓库当前不存在对应 Markdown 文档说明这些配置字段
- 缓解措施：
  1. 在 issue 生成前先固定术语：配置回退、元数据回退、服务端改路由告警、传输层回退分开描述
  2. 后续 issue 必须先恢复最小可编译验证基线，再决定是接线还是清理
  3. 后续实现或清理必须同步补上用户可见文档与验证
- 回滚/恢复方案（如需要）：
  本阶段无代码变更；后续若进入执行阶段，回滚策略应以“恢复到当前实际行为”而不是“恢复到测试意图”为准。

## 参考
- `codex-rs/config/src/config_toml.rs:94`
- `codex-rs/config/src/config_toml.rs:98`
- `codex-rs/config/src/config_toml.rs:100`
- `codex-rs/core/src/config/mod.rs:260`
- `codex-rs/core/src/config/mod.rs:1898`
- `codex-rs/core/src/config/mod.rs:1932`
- `codex-rs/core/src/session/turn.rs:2315`
- `codex-rs/core/src/session/turn.rs:2365`
- `codex-rs/core/src/session/turn_context.rs:210`
- `codex-rs/models-manager/src/manager.rs:367`
- `codex-rs/models-manager/src/model_info.rs:76`
- `codex-rs/core/src/session/mod.rs:2241`
- `codex-rs/core/tests/suite/websocket_fallback.rs:249`
- `codex-rs/core/tests/suite/websocket_fallback.rs:320`
- `codex-rs/core/tests/suite/websocket_fallback.rs:442`
- `codex-rs/core/tests/suite/websocket_fallback.rs:548`
