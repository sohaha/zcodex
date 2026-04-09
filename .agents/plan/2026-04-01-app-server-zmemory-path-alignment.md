# app-server zmemory 路径对齐 codex-cli

> 不适用项写 "无"，不要留空。
> 只写已确认事实；若信息不清楚或需要假设，先回到对话澄清，不要把未确认内容写进计划。

## 背景
- 当前状态：zmemory 动作层仅接收显式路径或默认 `$CODEX_HOME/zmemory/projects/<project-key>/zmemory.db`；app-server 通过 ConfigBuilder 加载配置并在工具 handler 中读取 `turn.config.zmemory.path` 传入动作层。
- 触发原因：用户在 app-server 环境期望使用项目内 `.codex/config.toml` 的 `[zmemory].path`（如 `/workspace/.agents/memory.db`），且行为应与 codex-cli 一致，但当前会话工具调用未对齐。
- 预期影响：zmemory 的实际 dbPath 与项目配置保持一致，避免落到默认库。

## 目标
- 目标结果：在 app-server 环境调用 zmemory 时，优先使用当前项目配置中的 `zmemory.path`，行为与 codex-cli 对齐。
- 完成定义（DoD）：
  - app-server 触发的 zmemory 调用在没有显式覆盖时，落到项目配置的 `zmemory.path`。
  - 能明确说明当前实现中“哪些环节决定 dbPath”，并输出可验证的路径解析依据。
- 非目标：
  - 不改动其它工具的行为；仅识别并告知可能存在类似配置注入的工具。

## 范围
- 范围内：
  - 分析 app-server 与 core 的 zmemory 调用链路与配置注入路径。
  - 仅针对 zmemory 的对齐方案设计与最小实现。
  - 列出可能同类的其它工具（仅告知，不改）。
- 范围外：
  - 其它工具的行为修复或统一。
  - 扩展 zmemory 的新功能或新增数据模型。

## 影响
- 受影响模块：
  - codex-rs/zmemory（路径解析与默认策略）
  - codex-rs/core 工具 handler（zmemory 调用注入点）
  - codex-rs/app-server（配置加载与工具调用链）
- 受影响接口/命令：
  - zmemory 工具调用（app-server 触发）
- 受影响数据/模式：
  - zmemory SQLite dbPath 选择逻辑
- 受影响用户界面/行为：
  - app-server 环境下 zmemory 读写目标库路径

## 约束与依赖
- 约束（时间/兼容性/安全/性能/发布窗口等）：
  - 不引入对其它工具的行为变更。
  - 不绕过现有配置层加载与信任机制。
- 外部依赖（系统/人员/数据/权限等）：
  - 需要现有项目配置（`.codex/config.toml`）可被加载并且项目可信。

## 实施策略
- 总体方案：
  - 先确认 app-server 调用链路中，是否稳定将 `turn.config.zmemory.path` 注入到 zmemory 动作层；若存在缺口，设计最小补丁保证 app-server 触发的 zmemory 调用对齐项目配置。
- 关键决策：
  - 仅针对 zmemory 增加最小对齐路径，不改变其他工具。
- 明确不采用的方案（如有）：
  - 不让 codex-zmemory crate 自行读取 config.toml（保持分层职责）。

## 阶段拆分

### 现状确认与缺口定位
- 目标：确认 app-server 触发 zmemory 时的配置注入与 dbPath 解析是否覆盖项目配置。
- 交付物：
  - 关键调用链说明（含引用位置）。
  - 明确导致未对齐的分支/条件。
- 完成条件：
  - 能指出具体注入点与缺口，且与当前观察一致。
- 依赖：无。

### 最小对齐实现
- 目标：在 app-server 触发的 zmemory 调用中对齐项目配置的 `zmemory.path`。
- 交付物：
  - 最小代码改动或配置注入方案。
  - 列出可能同类工具清单（只告知）。
- 完成条件：
  - zmemory 的实际 dbPath 可由 `system://workspace` 或等价诊断确认与项目配置一致。
- 依赖：上一阶段结论。

### 验证与回归
- 目标：验证对齐行为正确且不影响其它工具。
- 交付物：
  - 可执行的验证步骤或测试用例列表。
- 完成条件：
  - 至少一种稳定验证路径通过（例如工具输出 `dbPath` 与配置一致）。
- 依赖：实现完成。

## 测试与验证
- 核心验证：通过 app-server 触发 zmemory `system://workspace`，确认 `dbPath` 与 `/workspace/.codex/config.toml` 的 `zmemory.path` 一致。
- 必过检查：配置加载链路不报错；zmemory 调用成功。
- 回归验证：确认其他工具未被改动（仅代码层面检查）。
- 手动检查：读取 `system://workspace` 输出中的 `dbPath/source/reason`。
- 未执行的验证（如有）：无。

## 风险与缓解
- 关键风险：对齐逻辑引入额外配置分支，可能影响未来扩展。
- 触发信号：zmemory 输出的 `dbPath` 与期望不一致。
- 缓解措施：保持改动最小，优先复用现有 config 注入逻辑。
- 回滚/恢复方案（如需要）：恢复到当前仅依赖默认路径的行为。

## 参考
- codex-rs/zmemory/src/path_resolution.rs:29-93
- codex-rs/core/src/tools/handlers/zmemory.rs:27-60
- codex-rs/core/src/config/mod.rs:636-729
- codex-rs/core/src/config_loader/mod.rs:99-199
- codex-rs/app-server/src/codex_message_processor.rs:562-582
