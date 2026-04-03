# TUI Buddy AI Soul 扩展

> 不适用项写 `无`，不要留空。
> 只写已确认事实；若信息不清楚或需要假设，先回到对话澄清，不要把未确认内容写进计划。

## 背景
- 当前状态：TUI 已有 Buddy（`codex-rs/tui/src/buddy/*`），基于 seed 生成确定性 bones，物种/眼睛/帽子与台词为固定集合，反应气泡在 Buddy 区域内渲染；尚无 AI Soul、系统提示注入或上下文感知反应。
- 触发原因：用户要求对齐 claude-code-rev 的宠物能力差异，新增 AI Soul、系统提示集成、上下文感知反应、物种扩充与浮动气泡。
- 预期影响：配置结构与持久化、TUI Buddy 模型与渲染、核心指令拼装与模型调用链路、Buddy 相关测试与快照。

## 目标
- 目标结果：
  - AI Soul：全局生成并持久化到 `~/.codex/config.toml`，用于 Buddy 的名字与性格信息。
  - 系统提示集成：注入 companionIntroText，防止模型冒充宠物说话。
  - 上下文感知反应：允许调用模型生成反应台词，结合编程上下文与 AI Soul。
  - 物种扩充：从 8 种扩展到 18 种，补齐眼睛/帽子差距。
  - 浮动气泡：新增底部浮动气泡槽位用于 Buddy 反应。
- 完成定义（DoD）：
  - 新增功能全部可用且默认行为可控（有开关或明确触发）。
  - AI Soul 持久化与读取稳定，不破坏现有 config 兼容性。
  - Buddy UI 与文案更新对应快照覆盖。
- 非目标：时间门控、April Fools 彩蛋、物种名混淆策略。

## 范围
- 范围内：
  - `codex-rs/tui` Buddy 模型/渲染/命令交互与布局。
  - `codex-rs/core` 指令拼装与模型调用链路（用于系统提示与反应生成）。
  - `codex-rs/config` 与 schema 以支持 AI Soul 配置持久化。
- 范围外：
  - App-server v1 新接口扩展。
  - 与 Buddy 无关的 UI/交互重构。

## 影响
- 受影响模块：`codex-rs/tui`、`codex-rs/core`、`codex-rs/config`、配置 schema 生成。
- 受影响接口/命令：`/buddy` 系列命令、config.toml 的 `tui` 相关配置。
- 受影响数据/模式：`~/.codex/config.toml` 新增 Buddy AI Soul 持久化字段。
- 受影响用户界面/行为：Buddy 名字/性格显示、反应气泡位置与形态、物种与装饰显示。

## 约束与依赖
- 约束（时间/兼容性/安全/性能/发布窗口等）：
  - AI Soul 全局范围；持久化使用 `~/.codex/config.toml`。
  - 上下文反应允许模型调用，但需控制频率/成本并可回退为固定台词。
- 外部依赖（系统/人员/数据/权限等）：模型可用性与权限配置。

## 实施策略
- 总体方案：
  - 在配置层新增 Buddy 配置块，用于存储 AI Soul（名字/性格/元数据）与反应开关。
  - 在 core 指令拼装处注入 companionIntroText，并确保仅注入一次。
  - 新增 Buddy 观察者逻辑：采集编程上下文，调用模型生成短反应文本，并传递给 TUI。
  - 在 TUI Buddy 模型中引入 soul 覆盖 name/personality 展示，扩展物种/眼睛/帽子与 ASCII 精灵；新增底部浮动气泡渲染区。
- 关键决策：
  - AI Soul 全局、持久化于 config；bones 仍基于现有 seed 逻辑生成。
  - 反应生成采用模型调用，需设置冷却与失败回退。
- 明确不采用的方案（如有）：时间门控与物种名混淆不纳入本次改动。

## 阶段拆分

### 配置与 Soul 基础
- 目标：新增 AI Soul 配置结构与读写，确保默认兼容。
- 交付物：config 类型、schema、默认值与加载路径更新。
- 完成条件：AI Soul 可持久化/读取；无破坏性兼容问题。
- 依赖：`codex-rs/config` 与 `codex-rs/core` 配置加载路径。

### 指令集成与观察者
- 目标：注入 companionIntroText，新增上下文感知反应生成链路。
- 交付物：系统提示注入逻辑、反应生成 prompt/调用与节流。
- 完成条件：指令只注入一次；反应生成在失败时回退为固定台词。
- 依赖：core 指令/模型调用流程、Buddy 反应数据结构。

### TUI Buddy 扩展与浮动气泡
- 目标：物种/帽子/眼睛扩展与浮动气泡布局。
- 交付物：Buddy model/render 更新、ASCII 精灵与气泡新布局。
- 完成条件：宽窄终端均可显示；浮动气泡不破坏现有布局。
- 依赖：Buddy 渲染与 bottom pane 布局。

### 测试与回归
- 目标：更新快照与必要测试。
- 交付物：更新/新增 Buddy 快照，必要单测通过。
- 完成条件：`cargo nextest run -p codex-tui` 通过，快照更新完成。
- 依赖：TUI 测试与 insta 工具链。

## 测试与验证
- 核心验证：`cargo nextest run -p codex-tui`，更新 Buddy 相关快照。
- 必过检查：`just fmt`（Rust 改动后）。
- 回归验证：Buddy 显示/隐藏/抚摸/状态命令；窄终端显示与浮动气泡回退。
- 手动检查：启动 TUI，触发 Buddy 反应并观察气泡位置。
- 未执行的验证（如有）：无。

## 风险与缓解
- 关键风险：模型调用增加延迟或失败；浮动气泡影响布局稳定性。
- 触发信号：反应生成超时/失败率升高；底栏布局错位。
- 缓解措施：增加冷却与失败回退；提供禁用开关；保留窄终端降级渲染。
- 回滚/恢复方案（如需要）：关闭 Buddy 反应生成或回退到原固定台词逻辑。

## 参考
- `codex-rs/tui/src/buddy/model.rs`
- `codex-rs/tui/src/buddy/render.rs`
- `codex-rs/tui/src/buddy/mod.rs`
- `codex-rs/tui/src/chatwidget.rs`
- `codex-rs/tui/src/bottom_pane/mod.rs`
- `codex-rs/config/src/types.rs`
- `codex-rs/core/src/config/mod.rs`
- https://github.com/oboard/claude-code-rev/raw/refs/heads/main/src/buddy/companion.ts
- https://github.com/oboard/claude-code-rev/raw/refs/heads/main/src/buddy/prompt.ts
- https://github.com/oboard/claude-code-rev/raw/refs/heads/main/src/buddy/types.ts
