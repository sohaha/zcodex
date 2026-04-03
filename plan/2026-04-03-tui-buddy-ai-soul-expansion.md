# TUI Buddy AI Soul 扩展

> 不适用项写 `无`，不要留空。
> 只写已确认事实；若信息不清楚或需要假设，先回到对话澄清，不要把未确认内容写进计划。

## 背景
- 当前状态：主线 TUI 已有 Buddy（`codex-rs/tui/src/buddy/*`），物种/眼睛/帽子与台词为固定集合；当前 worktree 已新增 AI Soul、系统提示注入、上下文感知反应、物种扩充与浮动气泡相关改动，但尚未完成验证与快照更新。
- 触发原因：用户要求对齐 claude-code-rev 的宠物能力差异，新增 AI Soul、系统提示集成、上下文感知反应、物种扩充与浮动气泡。
- 预期影响：配置结构与持久化、TUI Buddy 模型与渲染、核心指令拼装与模型调用链路、Buddy 相关测试与快照、app-server 通知协议。

## 目标
- 目标结果：
  - AI Soul：自动生成并持久化到 `~/.codex/config.toml`，用于 Buddy 名字与性格。
  - 系统提示集成：注入 `companionIntroText`，防止模型冒充宠物说话。
  - 上下文感知反应：允许调用模型生成反应台词，结合编程上下文与 AI Soul，失败回退固定台词。
  - 物种扩充：从 8 种扩展到 18 种，补齐眼睛/帽子差距。
  - 浮动气泡：新增底部浮动气泡槽位用于 Buddy 反应（宽屏显示、窄屏降级）。
- 完成定义（DoD）：
  - AI Soul 持久化与读取稳定，不破坏现有 config 兼容性。
  - 系统提示注入仅一次且可控；反应生成有失败回退。
  - Buddy UI 更新完成并有对应快照覆盖。
  - `just fmt` 与相关 crate 测试通过，schema 与通知协议产物更新完成。
- 非目标：时间门控、April Fools 彩蛋、物种名混淆策略。

## 范围
- 范围内：
  - `codex-rs/tui` Buddy 模型/渲染/命令交互与布局。
  - `codex-rs/core` 指令拼装、模型调用链路与 Buddy 观察者逻辑。
  - `codex-rs/config` 与 schema 用于 AI Soul 配置持久化。
  - `codex-rs/protocol` 与 app-server v2 通知协议接入。
- 范围外：
  - App-server v1 新接口扩展。
  - 与 Buddy 无关的 UI/交互重构。

## 影响
- 受影响模块：`codex-rs/tui`、`codex-rs/core`、`codex-rs/config`、`codex-rs/protocol`、`codex-rs/app-server-protocol`、`codex-rs/app-server`。
- 受影响接口/命令：Buddy 相关 TUI 展示与 app-server v2 通知。
- 受影响数据/模式：`~/.codex/config.toml` 新增 Buddy AI Soul 持久化字段。
- 受影响用户界面/行为：Buddy 名字/性格显示、反应气泡位置与形态、物种与装饰显示。

## 约束与依赖
- 约束（时间/兼容性/安全/性能/发布窗口等）：
  - AI Soul 全局范围，持久化使用 `~/.codex/config.toml`。
  - 上下文反应允许模型调用，但需控制频率/成本并可回退固定台词。
- 外部依赖（系统/人员/数据/权限等）：模型可用性与权限配置、`cargo-insta` 工具链。

## 实施策略
- 总体方案：
  - 校对并完善已实现的 Buddy AI Soul 与反应逻辑，确保配置持久化、系统提示注入与事件分发一致。
  - 完成 TUI 侧物种/眼睛/帽子扩展与浮动气泡渲染，确保窄终端降级不破坏布局。
  - 完成协议与 schema 产物更新，并通过快照/测试验证。
- 关键决策：
  - AI Soul 全局持久化于 config；bones 仍基于现有 seed 逻辑生成。
  - 反应生成采用模型调用，并在失败时回退固定台词。
- 明确不采用的方案（如有）：时间门控与物种名混淆不纳入本次改动。

## 阶段拆分

### 配置/协议与核心逻辑核对
- 目标：确认 AI Soul 配置结构、系统提示注入与反应生成链路完整可控。
- 交付物：config 类型与 schema、app-server v2 通知定义、Buddy observer 逻辑与事件路由。
- 完成条件：配置读写与事件分发可用；系统提示仅注入一次；反应有失败回退。
- 依赖：`codex-rs/config`、`codex-rs/core`、`codex-rs/protocol`、`codex-rs/app-server-protocol`。

### TUI Buddy 扩展与浮动气泡
- 目标：完成物种/帽子/眼睛扩展与浮动气泡布局。
- 交付物：Buddy model/render 更新、ASCII 精灵与气泡新布局。
- 完成条件：宽窄终端均可显示；浮动气泡不破坏现有布局。
- 依赖：Buddy 渲染与 bottom pane 布局。

### 验证与回归
- 目标：通过格式化、快照与相关测试。
- 交付物：更新/新增 Buddy 快照与 schema 产物。
- 完成条件：相关命令通过且快照已接受。
- 依赖：TUI 测试与 insta 工具链。

## 测试与验证
- 核心验证：`cargo nextest run -p codex-core`、`cargo nextest run -p codex-tui`、`cargo nextest run -p codex-app-server-protocol`。
- 必过检查：`just fmt`（在 `codex-rs` 目录）。
- 回归验证：Buddy 显示/隐藏/抚摸/状态命令；窄终端显示与浮动气泡降级。
- 手动检查：启动 TUI，触发 Buddy 反应并观察气泡位置与内容。
- 未执行的验证（如有）：无。

## 风险与缓解
- 关键风险：模型调用增加延迟或失败；浮动气泡影响布局稳定性。
- 触发信号：反应生成超时/失败率升高；底栏布局错位。
- 缓解措施：增加冷却与失败回退；提供禁用开关；保留窄终端降级渲染。
- 回滚/恢复方案（如需要）：关闭 Buddy 反应生成或回退到原固定台词逻辑。

## 参考
- `codex-rs/core/src/buddy.rs`
- `codex-rs/core/src/tasks/mod.rs`
- `codex-rs/tui/src/buddy/model.rs`
- `codex-rs/tui/src/buddy/render.rs`
- `codex-rs/tui/src/buddy/mod.rs`
- `codex-rs/tui/src/bottom_pane/mod.rs`
- `codex-rs/tui/src/chatwidget.rs`
- `codex-rs/app-server-protocol/src/protocol/v2.rs`
- https://github.com/oboard/claude-code-rev/raw/refs/heads/main/src/buddy/companion.ts
- https://github.com/oboard/claude-code-rev/raw/refs/heads/main/src/buddy/prompt.ts
