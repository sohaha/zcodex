# 上游同步后清理与对齐

> 不适用项写无，不要留空。
> 只写已确认事实；若信息不清楚或需要假设，先回到对话澄清，不要把未确认内容写进计划。

## 背景
- 当前状态：上游 openai/codex main（3895ddd..3cc689f，73 commits）已合并到 web 分支。但同步后遗留编译器 warnings。
- 触发原因：2026-04-15 完成上游同步后，审计发现 dead code、unused imports/variables 等问题。
- 预期影响：清理后消除编译器 warnings，确保 workspace 编译干净。

## 根因分析

### 1. tools/rewrite/ 模块 dead code（21 warnings → 4 warnings）
**根因**：上游 tool rewrite 基础设施包含多个子模块（auto_tldr, engine, decision, read_gate, tldr_routing），但本地分支只使用了 `shell_search_rewrite` + `classification` + `context` + `project_root` + `directives` 子集。

被移除的 5 个子模块（auto_tldr, decision, engine, read_gate）完全没有外部调用者。

`tldr_routing` 模块仍被 `shell_search_rewrite` 使用，但其中 4 个函数（`passthrough_reason_for_read`, `search_reason`, `extract_reason`, `non_code_reason`）只在已移除模块中被调用，所以变成了 dead code。这些函数只在 `#[cfg(test)]` 中被引用，标注为 `cfg(test)`。

**处理方式**：
- 从 `mod.rs` 移除 4 个无外部调用者的子模块
- 对 `tldr_routing` 中仅被测试使用的函数加 `#[cfg(test)]`

### 2. Session::set_thread_memory_mode 重复定义（1 warning）
**根因**：上游同步后，`codex.rs` 中 `impl Session` 块内有一个关联函数版本的 `set_thread_memory_mode(sess, sub_id, mode)`，与 `impl Codex` 块中的方法版本 `set_thread_memory_mode(&self, mode)` 功能重复。`CodexThread` 调用的是 `Codex` 方法版本，Session 版本无调用者。

**处理方式**：删除 Session 版本的重复定义。

### 3. Unused variables（4 warnings → 2 fixed in core, 2 in tui）
**根因**：上游代码变更后，某些解构出的变量被有意不使用（传了 `None` 替代）：
- `events.rs` 的 `interaction_input`：Shell variant 解构出该字段，但 `ExecCommandInput::new` 调用传 `None`
- `shell.rs` 的 `display_command`：`RunExecLikeArgs` 解构出该字段，但 `ToolEmitter::shell` 调用传 `None`
- `tui/lib.rs` 的 `err` 和 `e`：PostHog 初始化/调用中 error 被 comment out 了 warn

**处理方式**：用 `_` 前缀标注有意忽略的变量。

### 4. Unused imports（2 warnings）
**根因**：`capture_cli_startup` 和 `get_os_info` 被单独 `use` 导入到顶层，但实际通过 `posthog_events::` 完整路径调用。

**处理方式**：删除多余的 `use` 语句。

### 5. TUI dead functions（4 warnings）
**根因**：上游新增的功能模块尚未在本分支调用链中接入：
- `session_log::open` + `maybe_init`：session 日志初始化 API 已设计但未接入 app 启动流程
- `annotate_skill_reads_in_parsed_cmd`：skill 读取标注功能已实现但未接入 event 发射链路
- `discover_agents_summary`：agent 发现摘要功能已实现但未接入 status bar

**处理方式**：
- `annotate_skill_reads_in_parsed_cmd`：删除（上游未来需要时可从 git 恢复）
- `session_log::open` + `maybe_init`：保留，这些是有意设计但尚未接线的 API，上游可能在后续版本接入
- `discover_agents_summary`：同上，保留为未接入的上游功能

### 6. Buddy 相关 warnings（2 warnings — 保留）
**根因**：`BuddyReactionState` 字段和 `Session::buddy_reaction_state` 字段是 buddy 功能的基础设施，功能开发中但尚未完全接入。这些是本地独有的功能开发。

**处理方式**：保留这些 warnings，待 buddy 功能完全接入后自然消除。

## 目标
- 目标结果：`cargo check --workspace` 尽可能减少 warnings（从 30+ 降到 <10）
- 完成定义（DoD）：1) 所有已确认根因的 warnings 已处理；2) `cargo check --workspace` 无新增 error；3) 无本地独有功能丢失。
- 非目标：不消除 buddy 开发中的合理 warnings；不做全量测试。

## 范围
- 范围内：
  - `codex-rs/core/src/tools/rewrite/mod.rs` — 移除 dead 子模块
  - `codex-rs/core/src/tools/rewrite/tldr_routing.rs` — cfg(test) dead 函数
  - `codex-rs/core/src/codex.rs` — 删除重复 `set_thread_memory_mode`
  - `codex-rs/core/src/tools/events.rs` — unused variable 修复
  - `codex-rs/core/src/tools/handlers/shell.rs` — unused variable 修复
  - `codex-rs/tui/src/lib.rs` — unused imports + variables 修复
  - `codex-rs/tui/src/chatwidget/skills.rs` — 删除 dead method
  - `codex-rs/tui/src/session_log.rs` — 保留（未接入的上游 API）
  - `codex-rs/tui/src/status/helpers.rs` — 保留（未接入的上游 API）
- 范围外：
  - buddy 开发中的 warnings（合理保留）
  - 全量测试
  - 非 Rust 代码修改

## 实施策略
- 总体方案：按根因分类逐批修复，每批修复后 `cargo check` 验证。
- 关键决策：
  - 对于确定无调用者的 dead 模块：从 `mod.rs` 移除模块声明
  - 对于仅被测试使用的 dead 函数：`#[cfg(test)]` 标注
  - 对于上游已设计但未接入的 API：保留（不删除、不标注），接受 warnings
  - 对于上游重复定义：删除无调用者的版本
  - 对于有意不使用的变量：`_` 前缀标注
- 明确不采用的方案：不使用 `#[allow(dead_code)]` 掩盖问题根因

## 阶段拆分

### 阶段 1：core dead code 清理 ✅
- 移除 rewrite/ 中 4 个 dead 子模块
- cfg(test) tldr_routing 中 4 个 dead 函数
- 删除 codex.rs 重复 set_thread_memory_mode

### 阶段 2：Unused 变量/导入修复 ✅
- events.rs, shell.rs unused variables
- tui/lib.rs unused imports + variables

### 阶段 3：TUI dead function 清理 ✅
- 删除 annotate_skill_reads_in_parsed_cmd
- 保留 session_log 和 discover_agents_summary（上游未接入 API）

### 阶段 4：验证
- `cargo check --workspace` 确认 warnings 状态
- `cargo nextest run -p codex-core` 确认核心测试通过
- `cargo nextest run -p codex-tui` 确认 TUI 测试通过

## 测试与验证
- 核心验证：`cargo check --workspace` warnings 显著减少
- 回归验证：`cargo nextest run -p codex-core` + `cargo nextest run -p codex-tui`
- 手动检查：`git diff --stat` 确认改动范围合理

## 风险与缓解
- 关键风险：误删仍被间接使用的函数
- 缓解措施：每次修改后 `cargo check` 验证
- 回滚/恢复方案：`git checkout` 恢复单个文件

## 参考
- codex-rs/core/src/tools/rewrite/mod.rs
- codex-rs/core/src/tools/rewrite/tldr_routing.rs
- codex-rs/core/src/codex.rs
- codex-rs/core/src/tools/events.rs
- codex-rs/core/src/tools/handlers/shell.rs
- codex-rs/tui/src/lib.rs
- codex-rs/tui/src/chatwidget/skills.rs
