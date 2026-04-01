# zmemory 默认项目库路径

> 不适用项写 `无`，不要留空。
> 只写已确认事实；若信息不清楚或需要假设，先回到对话澄清，不要把未确认内容写进计划。

## 背景
- 当前状态：`zmemory` 在未显式配置 `[zmemory].path` 时，默认解析到 `$CODEX_HOME/zmemory/zmemory.db` 全局库；`system://defaults` / `system://workspace`、README、配置文档和相关测试都已围绕该全局默认合同收口。
- 触发原因：用户明确要求把 `zmemory` 的产品定位收敛为“项目知识库”，默认应按项目隔离；如果用户需要全局共享，再通过 `~/.codex/config.toml` 显式配置全局路径。
- 预期影响：需要把默认路径策略从全局根改为项目级默认，同时更新 runtime fact contract、CLI/文档说明与测试；实现时优先复用现有 session/cwd/repo-root 解析代码，避免再造一套项目识别逻辑。

## 目标
- 目标结果：`zmemory` 在未显式配置 `[zmemory].path` 时，默认落到稳定的项目级数据库路径；显式配置时仍完全尊重用户提供的路径；用户如需全局共享可通过配置显式指定全局 DB。
- 完成定义（DoD）：默认路径不再是单一全局库，而是基于当前项目稳定解析出的项目库；`system://defaults` / `system://workspace` / `stats` / `doctor` / README / `docs/config.md` / CLI 说明与测试全部对齐；原生 memory 与 `zmemory` 的 feature / prompt / 配置解耦结果保持不变。
- 非目标：重新设计 native memory；引入新的外部 memory 后端；无条件保留旧的“默认全局库”产品语义；扩展超出默认路径策略所必需的 session 元数据改造。

## 范围
- 范围内：`codex-rs/zmemory` 的默认路径解析与 project id 生成策略；`system://defaults` / `system://workspace` 的默认路径事实；`codex-rs/core` / `codex-rs/cli` 中引用这些合同的测试与说明；相关 README / config 文档。
- 范围外：显式 `[zmemory].path` 的相对/绝对路径解析规则；`native_memories` 行为；无关的 session/thread 存储结构改造；把历史所有 issue/plan 文本同步重写为新策略。

## 影响
- 受影响模块：`codex-rs/zmemory/src/path_resolution.rs`、`codex-rs/zmemory/src/config.rs`、`codex-rs/zmemory/src/system_views.rs`、`codex-rs/zmemory/src/service.rs`、`codex-rs/core/tests/suite/zmemory_e2e.rs`、`codex-rs/cli/src/zmemory_cmd.rs`、`docs/config.md`、`codex-rs/zmemory/README.md`、`codex-rs/README.md`。
- 受影响接口/命令：`codex zmemory stats`、`codex zmemory doctor`、`codex zmemory read system://defaults`、`codex zmemory read system://workspace`、默认启动时 `zmemory` 所使用的 SQLite 路径。
- 受影响数据/模式：默认 DB 路径合同、`source/reason/workspaceKey/defaultDbPath/dbPathDiffers` 等诊断字段语义，以及默认项目库的命名规则。
- 受影响用户界面/行为：同一用户在不同项目间默认不再共享一份 `zmemory` 库；用户只有显式配置全局路径时才会恢复跨项目共享。

## 约束与依赖
- 约束（时间/兼容性/安全/性能/发布窗口等）：用户已确认接受产品语义调整；应优先复用现有 repo root / cwd / workspace base 解析代码；如果仓库里没有可直接复用的稳定 `project-id` API，只允许补一个最小、可测试、基于现有项目锚点的 helper，而不是扩散成大范围 session 架构改造。
- 外部依赖（系统/人员/数据/权限等）：无额外外部系统依赖；实现依赖仓库现有 Rust 测试与文档更新流程。

## 实施策略
- 总体方案：先梳理 `zmemory` 当前默认路径、workspace base 与系统视图合同；确认可复用的项目锚点来源后，把默认路径从全局根切换到项目级目录结构；随后更新系统视图、测试与文档，最后回归验证显式路径、默认项目路径、以及用户自定义全局路径三类场景。
- 关键决策：默认路径改成项目级，而不是继续使用单一全局库；优先复用现有 `resolve_workspace_base` / repo root / cwd 解析逻辑；若当前会话栈没有可直接复用的稳定 `project-id` 值，则基于 canonical workspace base 生成稳定 project id，并把路径落到类似 `$CODEX_HOME/zmemory/projects/<project-id>/zmemory.db` 的目录结构，避免裸项目名冲突。
- 明确不采用的方案（如有）：不继续把 `$CODEX_HOME/zmemory/zmemory.db` 作为默认库；不以裸项目名直接生成 `$CODEX_HOME/zmemory/<project>.db`；不为此引入新的全局迁移器或复杂兼容桥。

## 阶段拆分
> 可按需增减阶段；简单任务可只保留一个阶段。

### 阶段一：确认并实现项目级默认路径合同
- 目标：把 `zmemory` 默认路径从全局根改为项目级，并确定可复用的项目锚点 / project id 生成方式。
- 交付物：更新后的路径解析实现、最小必要的 project id helper（若需要）、与新合同对应的 crate 级测试。
- 完成条件：未显式配置 `[zmemory].path` 时，默认路径稳定落到项目级 DB；显式路径仍保持原合同；测试能证明 repo root / worktree / 非 git cwd 下的锚点行为一致且可解释。
- 依赖：现有 `resolve_workspace_base` 与 repo root/cwd 解析逻辑。

### 阶段二：同步 system view / core / CLI 合同
- 目标：让 `system://defaults` / `system://workspace`、CLI 输出与 core e2e 对新的默认项目路径合同保持一致。
- 交付物：更新后的 `system_views/service` 输出、core/cli 断言、必要的说明文案调整。
- 完成条件：`defaultDbPath`、`dbPathDiffers`、`source/reason/workspaceKey` 等字段与新默认策略一致；core e2e 能区分默认项目库与显式全局库覆盖。
- 依赖：阶段一完成。

### 阶段三：文档与配置说明收口
- 目标：把 README / `docs/config.md` / 必要 CLI 说明同步到“默认项目库、全局需显式配置”的最终产品语义。
- 交付物：更新后的文档、示例配置与人工核对结果。
- 完成条件：文档明确说明默认项目库路径策略、显式全局配置方式，以及 `system://defaults` / `system://workspace` 的诊断用途。
- 依赖：阶段一、阶段二完成。

## 测试与验证
> 只写当前仓库中真正可执行、可复现的验证入口；如果没有自动化载体，就写具体的手动检查步骤，不要写含糊表述。

- 核心验证：`cargo test -p codex-zmemory --quiet`、`cargo test -p codex-core --test all suite::zmemory_e2e:: --quiet`、`cargo test -p codex-cli export_uri_supports_defaults_and_workspace_views --quiet`。
- 必过检查：`just fmt`；如配置 schema 或相关 config 类型变更，则运行 `just write-config-schema`；若改动触及 shared crate 且需要 lint 修复，再按仓库规则执行 `just fix -p <project>`。
- 回归验证：验证默认项目库场景；验证显式 `[zmemory].path = "./agents/memory.db"` 仍相对 repo root/cwd 正常解析；验证显式 `[zmemory].path = "$CODEX_HOME/zmemory/zmemory.db"` 或等价绝对路径时仍可恢复全局共享。
- 手动检查：核对 `docs/config.md`、`codex-rs/zmemory/README.md`、`codex-rs/README.md` 是否都明确说明“默认项目库、全局需显式配置”；核对 `system://workspace` 中 `defaultDbPath` 与当前项目对应。
- 未执行的验证（如有）：无。

## 风险与缓解
- 关键风险：项目 id 命名不稳定或冲突，导致同一项目在不同入口下映射到不同 DB，或不同项目错误共享 DB。
- 触发信号：相同 repo root / worktree 得到不同默认路径；`system://workspace.defaultDbPath` 在相同项目下不稳定；测试里 `dbPathDiffers` / `workspaceBase` 断言异常。
- 缓解措施：优先复用 canonical workspace base / repo root 解析；project id 仅建立在稳定锚点上；为 repo root、worktree、非 git cwd、显式全局路径四类场景补齐测试。
- 回滚/恢复方案（如需要）：若项目级命名策略验证不稳定，可先回退到上一版全局默认实现，再重新收敛 project id 方案；不在未验证前引入自动迁移。

## 参考
- `codex-rs/zmemory/src/path_resolution.rs`
- `codex-rs/zmemory/src/system_views.rs`
- `codex-rs/zmemory/src/config.rs`
- `codex-rs/zmemory/src/tool_api.rs`
- `codex-rs/core/tests/suite/zmemory_e2e.rs`
- `codex-rs/core/src/project_doc.rs`
- `docs/config.md`
- `codex-rs/zmemory/README.md`
