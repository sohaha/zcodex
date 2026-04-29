# Codex CLI Mission 系统实现

> 不适用项写 `无`，不要留空。
> 只写已确认事实；若信息不清楚或需要假设，先回到对话澄清，不要把未确认内容写进计划。

## 背景

- **当前状态**：
  - `codex-cli` 是一个轻量级 JavaScript 包装器，负责调用平台特定的 Rust 二进制文件。[codex-cli/bin/codex.js](/workspace/codex-cli/bin/codex.js)
  - `codex-rs/cli` 是 Rust CLI 的主要实现，使用 clap 进行命令行解析，包含多个子命令如 `Exec`、`Login`、`Mcp`、`Ztok`、`Zfeder`、`Zinit`、`Zmemory`、`Zoffsec` 等。[codex-rs/cli/src/main.rs](/workspace/codex-rs/cli/src/main.rs)
  - 现有 `ZTeam` 功能在 `codex-rs/tui` 中实现，提供本地双 worker 协作能力，但其产品语义与 Mission 系统不同。[codex-rs/tui/src/zteam.rs](/workspace/codex-rs/tui/src/zteam.rs)
  - 当前 `codex-cli` 不提供独立的 `mission` 命令，Mission 相关功能需要通过 TUI 中的 `/zteam` 命令访问。

- **触发原因**：
  - 用户希望在 `codex-cli` 中实现一个内置的 `Mission` 命令，作为一套完整的工程系统。
  - 该系统应包含 5 个核心 skill 文件：规划、Worker 定义、Worker 基础、代码审查验证、用户测试验证。
  - 用户提供了详细的 Mission 系统设计文档，描述了 7 个规划阶段、独立 session Worker、双层验证、交接机制和知识沉淀等核心特性。

- **预期影响**：
  - 新增 `codex mission` CLI 命令，提供从规划到验证的完整工程流程。
  - 与现有 `ZTeam` 功能保持独立，避免产品语义混淆。
  - 为后续在 TUI 中集成 Mission 系统奠定基础。

## 目标

- **目标结果**：
  - 在 `codex-cli` 中实现 `Mission` 子命令，提供完整的 Mission 工程流程。
  - 实现 Mission 的 5 个核心 skill 文件系统。
  - 支持 Mission 的 7 个规划阶段、Worker 管理和双层验证机制。

- **完成定义（DoD）**：
  1. CLI 层面新增 `Mission` 子命令，支持 `start`、`status`、`continue` 等基本操作。
  2. 实现 5 个核心 skill 文件的结构和内容模板。
  3. Mission 状态管理机制，支持规划、执行、验证等阶段的状态跟踪。
  4. Worker session 管理，支持创建、恢复和监控 Worker。
  5. 验证器集成，支持代码审查和用户测试验证。
  6. 交接（Handoff）机制，支持 Worker 之间的结构化信息传递。
  7. 知识沉淀系统，支持 `.factory/` 目录的服务、库和规范管理。
  8. 文档更新，包括用户指南和 CLI 帮助文档。

- **非目标**：
  - 不在本阶段实现完整的 TUI Mission 界面（保留给后续阶段）。
  - 不修改现有 `ZTeam` 功能的实现。
  - 不引入新的服务端组件或数据库依赖。
  - 不实现跨会话的 Mission 持久化和同步。

## 范围

- **范围内**：
  - `codex-rs/cli/src/mission_cmd.rs`：新增 Mission 子命令模块。
  - `codex-rs/cli/src/main.rs`：集成 Mission 子命令到 CLI。
  - `codex-rs/core/src/mission/`：新增 Mission 核心逻辑模块（状态管理、skill 管理、Worker 管理）。
  - `codex-rs/skills/src/assets/mission/`：新增 Mission skill 文件模板。
  - `docs/mission.md`：新增 Mission 系统用户文档。
  - 相关测试文件和 snapshot。

- **范围外**：
  - TUI 界面的 Mission 集成。
  - Mission 的远端协作和分布式执行。
  - 与现有 `ZTeam` 功能的深度集成。
  - Mission 历史记录和跨会话持久化。

## 影响

- **受影响模块**：
  - `codex-rs/cli/src/main.rs`
  - `codex-rs/cli/src/Cargo.toml`
  - `codex-rs/core/src/Cargo.toml`
  - `codex-rs/skills/src/Cargo.toml`
  - `docs/slash_commands.md`

- **受影响接口/命令**：
  - 新增 `codex mission <subcommand>` 命令族。
  - `codex mission start <goal>`：启动新的 Mission。
  - `codex mission status`：查看 Mission 状态。
  - `codex mission continue`：继续执行中的 Mission。
  - `codex mission validate`：运行验证器。

- **受影响数据/模式**：
  - 新增 `.factory/` 目录结构（services.yaml、library/、AGENTS.md）。
  - 新增 `.mission/` 状态目录（mission_state.json、worker_sessions/、handoffs/）。

- **受影响用户界面/行为**：
  - 用户可以通过 CLI 直接使用 Mission 功能，无需启动 TUI。
  - Mission 的 7 个规划阶段将在 CLI 中交互式进行。

## 约束与依赖

- **约束**：
  - 必须保持与现有 CLI 架构的一致性，使用 clap 进行命令解析。
  - 新增代码必须符合 Rust 代码规范和项目约定。
  - Mission 状态管理应基于本地文件，避免引入复杂依赖。
  - 与现有 `ZTeam` 功能保持独立，避免命名冲突。

- **外部依赖**：
  - 无外部人员依赖。
  - 依赖现有的 clap、tokio、serde 等 Rust 生态库。
  - 依赖现有的 Codex core 和 skills 基础设施。

- **当前会话可用能力适配检查**：
  - 已使用 `llmdoc` 提取稳定文档与反思事实。
  - 已使用 `think` 完成前置设计分歧收敛。
  - 已使用 `using-cadence` / `cadence-planning` 进入当前流程。
  - 已使用 `Glob`、`Grep`、`Read` 工具提取代码结构信息。
  - 其余当前会话可用 skill/MCP 对本轮计划起草没有更直接收益，记为 `none-applicable`。

## 实施策略

- **总体方案**：
  - 采用分层架构，将 Mission 系统分为 CLI 层、Core 层和 Skills 层。
  - CLI 层负责命令解析和用户交互，Core 层负责业务逻辑，Skills 层负责 skill 文件管理。
  - 使用状态机模式管理 Mission 生命周期，支持阶段转换和状态持久化。
  - 采用 Worker pool 模式管理 Worker session，支持创建、恢复和销毁。
  - 使用验证器模式实现双层验证机制。

- **关键决策**：
  - **命令设计**：采用 `codex mission <subcommand>` 形式，子命令包括 `start`、`status`、`continue`、`validate`。
  - **状态存储**：使用本地 JSON 文件存储 Mission 状态，位于 `.mission/mission_state.json`。
  - **Skill 管理**：将 5 个核心 skill 文件作为模板嵌入到 `codex-rs/skills/src/assets/mission/`，运行时复制到工作目录。
  - **Worker 管理**：使用现有的 Codex agent 机制，通过 session ID 管理 Worker 生命周期。
  - **验证机制**：实现 Scrutiny Validator 和 User Testing Validator，作为独立的验证模块。
  - **交接格式**：定义标准化的 Handoff JSON schema，包含 salientSummary、whatWasImplemented、verification 等字段。
  - **知识管理**：实现 `.factory/` 目录的自动创建和更新，支持 services.yaml、library/ 和 AGENTS.md 的管理。

- **明确不采用的方案**：
  - 不使用数据库存储 Mission 状态，避免引入额外依赖。
  - 不实现复杂的权限管理，依赖现有的文件系统权限。
  - 不实现跨主机的 Mission 分布式执行。
  - 不修改现有的 `ZTeam` 实现，保持功能独立。

## 阶段拆分

### 阶段一：CLI 命令与基础架构

- **目标**：
  - 实现 Mission CLI 命令框架和基础数据结构。

- **交付物**：
  - `codex-rs/cli/src/mission_cmd.rs`：Mission 子命令实现。
  - `codex-rs/core/src/mission/mod.rs`：Mission 核心模块。
  - `codex-rs/core/src/mission/state.rs`：Mission 状态管理。
  - `codex-rs/core/src/mission/error.rs`：Mission 错误类型。
  - 更新 `codex-rs/cli/src/main.rs` 集成 Mission 子命令。
  - 更新相关 Cargo.toml 文件。

- **完成条件**：
  - `codex mission --help` 可以正常显示帮助信息。
  - `codex mission status` 可以显示 Mission 状态（即使为空）。
  - 所有基础数据结构定义完整，编译通过。

- **依赖**：
  - 现有 CLI 架构和 Codex core 基础设施。

### 阶段二：Mission 状态机与规划流程

- **目标**：
  - 实现 Mission 状态机和 7 个规划阶段的交互流程。

- **交付物**：
  - `codex-rs/core/src/mission/planner.rs`：Mission 规划器。
  - `codex-rs/core/src/mission/phases.rs`：7 个规划阶段的实现。
  - Mission 状态机的状态转换逻辑。
  - 交互式用户输入处理。

- **完成条件**：
  - `codex mission start <goal>` 可以启动 Mission 并进入规划阶段。
  - 7 个规划阶段可以按顺序执行，每个阶段有明确的出口条件。
  - 规划阶段的结果可以持久化到 `.mission/mission_state.json`。

- **依赖**：
  - 阶段一的 CLI 命令与基础架构。

### 阶段三：Skill 系统与 Worker 管理

- **目标**：
  - 实现 5 个核心 skill 文件和 Worker session 管理。

- **交付物**：
  - `codex-rs/skills/src/assets/mission/mission-planning.md`：规划阶段 skill。
  - `codex-rs/skills/src/assets/mission/define-mission-skills.md`：Worker 定义 skill。
  - `codex-rs/skills/src/assets/mission/mission-worker-base.md`：Worker 基础 skill。
  - `codex-rs/skills/src/assets/mission/scrutiny-validator.md`：代码审查验证器 skill。
  - `codex-rs/skills/src/assets/mission/user-testing-validator.md`：用户测试验证器 skill。
  - `codex-rs/core/src/mission/skill_manager.rs`：Skill 管理器。
  - `codex-rs/core/src/mission/worker_manager.rs`：Worker 管理器。
  - Worker session 的创建、恢复和监控逻辑。

- **完成条件**：
  - 5 个核心 skill 文件可以作为模板加载和使用。
  - Worker 可以基于 skill 文件创建并执行任务。
  - Worker session 的状态可以被跟踪和恢复。

- **依赖**：
  - 阶段二的 Mission 状态机与规划流程。

### 阶段四：验证器与交接机制

- **目标**：
  - 实现双层验证器和交接（Handoff）机制。

- **交付物**：
  - `codex-rs/core/src/mission/validators.rs`：验证器模块。
  - `codex-rs/core/src/mission/validators/scrutiny.rs`：Scrutiny Validator 实现。
  - `codex-rs/core/src/mission/validators/user_testing.rs`：User Testing Validator 实现。
  - `codex-rs/core/src/mission/handoff.rs`：交接机制实现。
  - Handoff JSON schema 定义和序列化/反序列化。

- **完成条件**：
  - `codex mission validate` 可以运行验证器并报告结果。
  - Worker 可以生成标准化的 Handoff JSON。
  - 验证器可以基于 Handoff 内容进行验证并生成报告。

- **依赖**：
  - 阶段三的 Skill 系统与 Worker 管理。

### 阶段五：知识沉淀与文档

- **目标**：
  - 实现知识沉淀系统和完整的用户文档。

- **交付物**：
  - `codex-rs/core/src/mission/knowledge.rs`：知识管理模块。
  - `.factory/` 目录的自动创建和更新逻辑。
  - `docs/mission.md`：Mission 系统用户文档。
  - CLI 帮助文档和示例。
  - 相关测试和 snapshot。

- **完成条件**：
  - `.factory/` 目录可以在 Mission 启动时自动创建。
  - services.yaml、library/ 和 AGENTS.md 可以被自动更新。
  - 用户文档完整，包含快速开始、命令参考和最佳实践。
  - 所有测试通过，snapshot 覆盖充分。

- **依赖**：
  - 阶段四的验证器与交接机制。

## 测试与验证

- **核心验证**：
  - 每个阶段的交付物都需要通过单元测试和集成测试。
  - CLI 命令需要通过端到端测试验证用户交互流程。
  - Mission 状态机需要通过状态转换测试验证所有可能的路径。

- **必过检查**：
  - 所有代码必须通过 `cargo test` 和 `cargo clippy`。
  - 所有新增模块必须有对应的测试文件。
  - CLI 命令的帮助文档必须完整且准确。
  - Mission 状态文件必须有 schema 验证。

- **回归验证**：
  - 确保现有的 CLI 命令不受新增 Mission 命令影响。
  - 确保 `ZTeam` 功能继续正常工作。

- **手动检查**：
  1. 验证 `codex mission start` 可以完整走完 7 个规划阶段。
  2. 验证 Worker 可以基于 skill 文件正确执行任务。
  3. 验证验证器可以正确检测问题并生成报告。
  4. 验证交接机制可以正确传递信息。
  5. 验证知识沉淀系统可以正确更新 `.factory/` 目录。

- **未执行的验证**（如有）：
  - TUI 界面的 Mission 集成测试（留待后续阶段）。
  - 跨会话的 Mission 持久化和恢复测试（留待后续阶段）。

## 风险与缓解

- **关键风险**：
  - **复杂度过高**：Mission 系统包含多个复杂模块，可能导致实现周期过长。
  - **状态管理复杂性**：Mission 状态机的状态转换逻辑复杂，容易出现边界情况处理不当。
  - **Worker session 管理**：Worker 的创建、恢复和销毁涉及多线程和进程管理，容易出现资源泄漏。
  - **验证器准确性**：验证器的准确性直接影响 Mission 质量，容易出现误报或漏报。
  - **与现有功能的冲突**：新增 Mission 功能可能与现有 `ZTeam` 功能产生冲突。

- **触发信号**：
  - 单个阶段的实现时间超过预期 50% 以上。
  - 状态机测试中发现的边界情况超过预期。
  - Worker session 的资源泄漏测试失败。
  - 验证器的误报或漏报率超过 10%。
  - `ZTeam` 功能测试开始出现失败。

- **缓解措施**：
  - **分阶段交付**：严格按照阶段拆分交付，每个阶段都有明确的完成条件和验收标准。
  - **状态机简化**：使用成熟的状态机模式，明确所有可能的状态和转换条件。
  - **资源管理**：使用 RAII 模式管理 Worker session 资源，确保异常情况下也能正确释放。
  - **验证器测试**：为验证器建立完整的测试用例库，覆盖各种正常和异常情况。
  - **功能隔离**：确保 Mission 功能与 `ZTeam` 功能在代码和命令层面完全隔离。

- **回滚/恢复方案**：
  - 由于 Mission 功能是新增的独立模块，如果实现出现问题，可以通过移除 `mission_cmd.rs` 和相关模块快速回滚。
  - Mission 状态文件存储在 `.mission/` 目录，删除该目录即可清除所有 Mission 状态。
  - `.factory/` 目录是独立的知识管理系统，不影响其他功能。

## 参考

- [codex-cli/bin/codex.js](/workspace/codex-cli/bin/codex.js)
- [codex-rs/cli/src/main.rs](/workspace/codex-rs/cli/src/main.rs)
- [codex-rs/cli/src/zinit_cmd.rs](/workspace/codex-rs/cli/src/zinit_cmd.rs)
- [codex-rs/cli/src/zmemory_cmd.rs](/workspace/codex-rs/cli/src/zmemory_cmd.rs)
- [codex-rs/tui/src/zteam.rs](/workspace/codex-rs/tui/src/zteam.rs)
- [AGENTS.md](/workspace/AGENTS.md)
- [docs/slash_commands.md](/workspace/docs/slash_commands.md)
- [.agents/plan/2026-04-24-zteam-mission-v2.md](/workspace/.agents/plan/2026-04-24-zteam-mission-v2.md)
- [.agents/llmdoc/memory/decisions/zteam-mission-v2-contract.md](/workspace/.agents/llmdoc/memory/decisions/zteam-mission-v2-contract.md)
