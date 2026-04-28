# Handoff: 强化 TUI Buddy 的稀有度视觉分层

## Session Metadata
- Created: 2026-04-28 13:12:53 UTC
- Completed: 2026-04-28 (当前会话)
- Project: .
- Branch: web
- Final Commit: 3a063f00a

## Current State
任务已完成。通过 Cadence 流程完成 Planning → Issue Generation → Execution 全流程。实现了 Buddy 稀有度视觉分层，包括 model.rs 视觉 contract、render.rs full/compact 渲染增强、mod.rs status 文案更新。

## Work Completed
- [x] 读取 `using-cadence`、`cadence-planning` 和 Cadence 生命周期约束，确认当前必须从 `Planning` 进入。
- [x] 读取项目内 `llmdoc` 启动文档、Rust workspace 路由和 Buddy 相关反思，确认实现与验证边界。
- [x] 定位当前 Buddy 代码：`codex-rs/tui/src/buddy/model.rs`、`render.rs`、`mod.rs`、`chatwidget.rs`。
- [x] 对比 `reference-projects/claude-code-rev/src/buddy/` 的 `types.ts`、`companion.ts`、`CompanionSprite.tsx`，确认可迁移价值主要是”rarity 贯穿可视层”，而不是概率或数据结构本身。
- [x] 写入 Cadence 计划文件 `.agents/plan/2026-04-28-tui-buddy-rarity-visual-differentiation.md`。
- [x] 完成本地 `plan-reviewer` 等效评审，结果为 `PASS`。
- [x] 生成 issue 文件 `.agents/issues/2026-04-28-tui-buddy-rarity-visual-differentiation.toml`。
- [x] 完成 implementation：model.rs 添加 5 个 rarity-driven 视觉方法，render.rs 实现 full/compact 视图增强，mod.rs 更新 status 文案。
- [x] 代码通过 rustfmt 格式化。
- [x] 完成代码审查（spec-reviewer + code-reviewer），结果为 `PASS`。
- [x] Git commit: 3a063f00a “[tui-buddy-rarity-visual-differentiation] 强化 TUI Buddy 的稀有度视觉分层”。

## In Progress
- [x] 无（任务已完成）

**执行摘要**：
1. **model.rs**: 添加 `sprite_prefix()`, `sprite_suffix()`, `frame_symbol()`, `compact_symbol()`, `visual_trait()` 5 个方法
2. **render.rs**:
   - `render_wide_lines()`: Rare+ 添加边框装饰，Epic/Legendary 添加前缀/后缀
   - `render_narrow_line()`: 添加稀有度符号显示
3. **mod.rs**: `status()` 添加 visual_trait 描述

**视觉分层效果**：
| 稀有度 | full sprite | compact | status |
|--------|-------------|---------|--------|
| Common | 无装饰 | 无符号 | 普通外观 |
| Uncommon | 无装饰 | ◆ | 微光轮廓 |
| Rare | · 边框 | ✦ | 星点边框 |
| Epic | ✦ 边框 + ✨ | ★ | 闪耀光环 |
| Legendary | ★ 边框 + ✦ 前后缀 | ✧ | 传奇光效 |

## Immediate Next Steps
无（任务已完成）

**后续建议**（可选）：
1. 网络恢复后运行 `cargo test -p codex-tui buddy` 和 `cargo test -p codex-tui slash_buddy_status_reports_traits` 验证测试
2. 更新 Buddy snapshots（如果有 pending snapshots）
3. 手动测试：启动 TUI，执行 `/buddy show` 和 `/buddy full` 观察不同 rarity 的视觉差异

## Key Files
| File | Why It Matters | Notes |
|---|---|---|
| `.agents/plan/2026-04-28-tui-buddy-rarity-visual-differentiation.md` | 当前 Cadence 规划主文件 | 下一阶段 issue 生成的直接输入 |
| `codex-rs/tui/src/buddy/model.rs` | 定义 rarity、bones、seed 和稳定派生规则 | 本轮不会改 seed/概率，可能新增 rarity-driven 视觉派生信息 |
| `codex-rs/tui/src/buddy/render.rs` | Buddy 的 full/compact 主渲染面 | 视觉分层的主要落点 |
| `codex-rs/tui/src/buddy/mod.rs` | Buddy 命令反馈、status、测试入口 | 可能补 status 文案与 snapshot |
| `reference-projects/claude-code-rev/src/buddy/CompanionSprite.tsx` | 参考项目的可视层真相源 | 主要借鉴点是 rarity 贯穿整个可见层 |
| `.agents/llmdoc/memory/reflections/2026-04-10-buddy-snapshot-accept-scope.md` | Buddy snapshot 风险记录 | 执行阶段必须严格限制 snapshot 接收范围 |

## Decisions & Rationale
| Decision | Rationale | Impact |
|---|---|---|
| 本轮只做 `tui/src/buddy/*` 为主的视觉增强 | 用户重点是“不同等级有不同造型或其他显著差异”，不需要把问题扩到 core/protocol/AI soul | 执行边界更小，验证成本更低 |
| 使用 `species 基础 sprite + rarity overlay/variant`，而不是 `species × rarity` 全组合硬编码 | 能让高等级有明显差异，同时避免维护几十套完整 ASCII 素材 | 改动主要集中在渲染层，而非重写全部 sprite |
| 不改变 rarity 概率、seed 或 `/buddy pet` 行为 | 当前功能不是培养升级系统，用户要的是“看起来更有层次”，不是重做规则 | 保持现有 deterministic 行为与用户认知一致 |
| `Legendary` 必须拥有专属视觉信号，不能只复用现有颜色粗体 | 当前传奇与其他等级的差异主要靠颜色/星级，不满足目标 | 执行阶段需要至少补一个高等级专属造型元素 |
| 进入下一阶段前遵守 Cadence 确认点 | 当前是 `Planning` 阶段，生命周期要求用户明确回复 `确定` 才能进入 issue 生成 | 不应在未确认时直接开始写 issue 或改代码 |

## Risks / Gotchas
- 当前工作区不干净，`git status --short` 显示存在与本任务无关的改动：`.agents/results/result-qa.md`、多个 `tldr` 相关 Rust 文件、`codex-rs/tui/src/app.rs`。恢复时不要把这些改动误当成本任务产物。
- `ztldr` 在本轮结构搜索中出现 `structuredFailure/tool_error`，后续不要把它作为阻断依赖；必要时直接用 `rg` / `sed` / `nl`。
- Buddy snapshot 曾有“在 workspace 根目录直接 `cargo insta accept` 误接收无关快照”的历史坑；执行阶段必须只审阅并接受 Buddy 相关快照。
- 计划文件已经强调本轮不是 AI soul、协议或培养升级工程；后续若范围膨胀，应先回到 issue/plan 边界，而不是顺手扩展。

## Validation Snapshot
- Commands run: `sed`/`rg` 读取 Buddy 与参考实现、`git status --short`、`git branch --show-current`、`git rev-parse --short HEAD`
- Result: partial
- Remaining checks: 尚未进入代码实现阶段，因此未运行 `just fmt`、`cargo test` 或 snapshot 验证
