# ztok 与 sqz 差异分析

## 背景
- 当前状态：仓库内已存在 `codex-rs/ztok`，且 `codex-rs/cli` / `codex-rs/arg0` 已把它作为 `codex ztok` 子命令与 `ztok` 别名接入。
- 触发原因：用户要求在 `Cadence` 流程内分析当前 `ztok` 与外部项目 `https://github.com/ojuschugh1/sqz` 的差异。
- 预期影响：产出一份基于源码与公开文档的差异结论，帮助后续判断是否需要同步、借鉴或保持分叉。

## 目标
- 目标结果：给出 `ztok` 与 `sqz` 在定位、架构、能力、接入方式、状态保持与适用场景上的明确差异。
- 完成定义（DoD）：结论同时引用本仓库实现证据与 `sqz` 公开源码/README 证据，并明确哪些判断是直接事实，哪些是基于事实的推论。
- 非目标：无代码修改；无上游同步；不输出迁移方案、实施计划或功能设计。

## 范围
- 范围内：
  - `codex-rs/ztok` 的命令面、接入方式与实现边界
  - `codex-rs/cli`、`codex-rs/arg0` 对 `ztok` 的集成方式
  - `sqz` 的 README、workspace 结构与核心库公开导出能力
- 范围外：
  - 对 `sqz` 做本地构建、安装或运行验证
  - 对 `ztok` 或 `sqz` 的性能复测
  - 直接修改任何 Rust 代码或提示词

## 影响
- 受影响模块：
  - `codex-rs/ztok`
  - `codex-rs/cli`
  - `codex-rs/arg0`
- 受影响接口/命令：
  - `codex ztok`
  - `ztok`
- 受影响数据/模式：无
- 受影响用户界面/行为：无

## 约束与依赖
- 约束（时间/兼容性/安全/性能/发布窗口等）：只写已确认事实；外部项目结论以 2026-04-20 拉取到的公开仓库内容为准。
- 外部依赖（系统/人员/数据/权限等）：需要访问 `sqz` GitHub 仓库公开文件。

## 实施策略
- 总体方案：先读取本仓库 `ztok` 的入口、命令面、重写/执行/跟踪实现与测试，再读取 `sqz` 的 README、workspace 清单与核心库导出，最后按“产品定位、系统形态、压缩机制、集成面、状态保持、功能宽度”整理差异。
- 关键决策：
  - 以源码和测试为本仓库事实源，不以二手描述替代
  - 以 `sqz` 仓库公开文件为外部事实源，不依赖第三方转述
  - 明确区分“事实”与“基于事实的推论”
- 明确不采用的方案（如有）：不做性能跑分复现；不基于命令名称相似度做类比结论。

## 阶段拆分
### 证据收集
- 目标：确认 `ztok` 与 `sqz` 各自的实现边界与公开能力。
- 交付物：本地文件证据清单与外部链接证据清单。
- 完成条件：能够回答两者是否属于同一层级产品，以及各自是否具备会话状态、hook、代理、插件或持久化能力。
- 依赖：本仓库源码；`sqz` GitHub 仓库公开文件。

### 差异归纳
- 目标：把证据压缩成可审阅的差异分析。
- 交付物：面向用户的中文结论，包含差异项、推论与适用场景判断。
- 完成条件：每个核心差异都有对应证据来源。
- 依赖：证据收集结果。

## 测试与验证
- 核心验证：
  - 读取本仓库实现与测试文件，确认 `ztok` 的实际命令面和行为边界
  - 拉取并阅读 `sqz` 的 README、`Cargo.toml`、`sqz_engine/src/lib.rs`
- 必过检查：
  - `codex ztok --help`
  - `git status --short`
- 回归验证：无
- 手动检查：
  - 交叉比对 `ztok` 帮助输出、测试断言与 `upgrade-rtk` skill 中的集成边界描述
  - 交叉比对 `sqz` README 中的 CLI/Hook/插件说明与其 workspace / engine 暴露模块
- 未执行的验证（如有）：未对 `sqz` 做本地安装、运行或基准复现。

## 风险与缓解
- 关键风险：`sqz` 的 `main` 分支后续可能变化，导致分析与未来状态不一致。
- 触发信号：README、Cargo workspace 或 `sqz_engine` 导出能力与当前引用内容不一致。
- 缓解措施：在结论中注明取证日期，并直接附上引用链接。
- 回滚/恢复方案（如需要）：无

## 参考
- `/workspace/codex-rs/cli/src/main.rs`
- `/workspace/codex-rs/arg0/src/lib.rs`
- `/workspace/codex-rs/cli/tests/ztok.rs`
- `/workspace/codex-rs/ztok/src/lib.rs`
- `/workspace/codex-rs/ztok/src/rewrite.rs`
- `/workspace/codex-rs/ztok/src/runner.rs`
- `/workspace/codex-rs/ztok/src/tracking.rs`
- `/workspace/.codex/skills/upgrade-rtk/SKILL.md`
- `/workspace/.version/rtk.toml`
- `https://raw.githubusercontent.com/ojuschugh1/sqz/main/README.md`
- `https://raw.githubusercontent.com/ojuschugh1/sqz/main/docs/benchmark-vs-rtk.md`
- `https://raw.githubusercontent.com/ojuschugh1/sqz/main/Cargo.toml`
- `https://raw.githubusercontent.com/ojuschugh1/sqz/main/sqz_engine/src/lib.rs`
