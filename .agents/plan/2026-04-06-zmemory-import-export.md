# zmemory import/export

## 背景
- 当前状态：zmemory 已完成真实 `export` / `import` 主体实现，CLI 新增 `export-memory` / `import-memory`，tool schema 与 README 也已同步主体契约；旧 `codex zmemory export` 仍保留为 system view 薄封装。
- 触发原因：需要核实最初 issue 的真实性并在确认后落实优化，其中本轮已将“真实导入导出”作为明确交付项推进到接近完成，只剩最后测试修正、Cadence 记录和提交收尾。
- 预期影响：zmemory 可以导出真实记忆项并重新导入，CLI/工具层语义更清晰，shared-edge alias metadata 冲突不再被静默吞掉。

## 目标
- 目标结果：交付可用的 zmemory 真实 import/export 能力，并让 service、CLI、tool schema、README 与测试覆盖保持一致。
- 完成定义（DoD）：`codex-zmemory`、`codex-cli`、`codex-tools` 的相关定向测试通过；Cadence 计划/issue 文件存在；本任务相关文件完成 review、commit、push。
- 非目标：不扩展 batch/history 之外的新产品能力；不改旧 `codex zmemory export` 的既有 discoverability 语义；不做 workspace 级全量测试。

## 范围
- 范围内：
  - 新增 `action: "export"` / `action: "import"` 及 typed params。
  - 新增 `service/export.rs`、`service/import.rs` 并接入路由。
  - CLI 新增 `export-memory` / `import-memory`。
  - tools schema、README 与 service/tests 对齐。
  - 修正 shared-edge alias metadata conflict 相关测试与契约。
- 范围外：
  - 不新增额外 UI 或 app-server 接口。
  - 不处理无关 core 脏改动。
  - 不做全仓回归，只做本任务相关定向验证。

## 影响
- 受影响模块：
  - `codex-rs/zmemory`
  - `codex-rs/cli`
  - `codex-rs/tools`
- 受影响接口/命令：
  - `codex zmemory export-memory`
  - `codex zmemory import-memory`
  - zmemory tool `action: "export" | "import"`
- 受影响数据/模式：
  - export payload：`scope / count / items`
  - import payload：单事务回放 `create -> aliases -> triggers`
- 受影响用户界面/行为：CLI 帮助文本与 README 文档更新；旧 `export` 命令语义保持不变。

## 约束与依赖
- 约束（时间/兼容性/安全/性能/发布窗口等）：
  - 保持旧 `codex zmemory export` 兼容。
  - 只提交本任务相关文件。
  - 并行 Rust 测试需隔离 `CARGO_HOME` 与 `CARGO_TARGET_DIR`。
- 外部依赖（系统/人员/数据/权限等）：
  - 依赖本地 Rust 工具链与 `cargo nextest`。
  - 依赖 git 可提交、可推送当前分支。

## 实施策略
- 总体方案：在现有 tagged-union API 基础上接入真实 import/export；保持旧命令不变；对 shared-edge alias metadata 冲突采取显式报错，避免 round-trip 静默失真；最后以最小测试修正完成收尾。
- 关键决策：
  - 真实导入导出走新 action 与新 CLI 子命令。
  - 导出按请求 URI 保留主项，而不是强制 canonicalize。
  - 导入采用单事务 fail-fast。
  - shared-edge 元数据冲突显式报错，而不是静默覆盖或吞掉。
- 明确不采用的方案（如有）：
  - 不把旧 `export` 改造成真实 memory export。
  - 不在本轮引入 path 级独立 alias metadata 存储模型。

## 阶段拆分
### 阶段一：主体实现对齐
- 目标：完成 service / CLI / tool schema / README 主体实现。
- 交付物：新增 export/import action、CLI 命令、schema 与 README 更新。
- 完成条件：主体代码与对应测试就位。
- 依赖：无。

### 阶段二：shared-edge 语义与测试收尾
- 目标：确认 alias shared-edge 冲突语义并修正受影响测试。
- 交付物：冲突显式报错、测试数据修正、定向测试通过。
- 完成条件：`codex-zmemory` 定向测试通过。
- 依赖：阶段一。

### 阶段三：Cadence / review / 提交
- 目标：补齐 Cadence 文件、做最终自审并提交推送。
- 交付物：计划文件、issue 文件、git commit、git push、handoff 更新。
- 完成条件：相关文件已提交推送，交接信息更新完成。
- 依赖：阶段二。

## 测试与验证
- 核心验证：
  - `cd /workspace/codex-rs && CARGO_HOME=/tmp/codex-zmemory-home-exec3 CARGO_TARGET_DIR=/tmp/codex-zmemory-target-exec3 cargo nextest run -p codex-zmemory`
- 必过检查：
  - `cd /workspace/codex-rs && CARGO_HOME=/tmp/codex-cli-home-exec2 CARGO_TARGET_DIR=/tmp/codex-cli-target-exec2 cargo nextest run -p codex-cli zmemory_`
  - `cd /workspace/codex-rs && CARGO_HOME=/tmp/codex-tools-home-exec2 CARGO_TARGET_DIR=/tmp/codex-tools-target-exec2 cargo nextest run -p codex-tools zmemory_tool`
  - `cd /workspace/codex-rs && just fmt`
- 回归验证：
  - `node /root/.config/lnk/.agents/skills/using-cadence/scripts/cadence_validate.js .agents/issues/2026-04-06-zmemory-import-export.toml`
- 手动检查：
  - 确认仅 stage 本任务相关文件，排除 `.ai/`、`.worktrees/`、`nmtx/` 与无关 core 变更。
- 未执行的验证（如有）：
  - 全仓 `just test` 未执行，因为本轮仅改动 zmemory/cli/tools 且无需 workspace 级回归。

## 风险与缓解
- 关键风险：
  - shared-edge alias 元数据语义容易与导入导出契约不一致。
  - 提交时混入无关脏文件。
- 触发信号：
  - `codex-zmemory` import/export 相关测试失败。
  - `git status --short` 出现 `.ai`、`.worktrees/`、core 无关改动被 stage。
- 缓解措施：
  - 用定向测试锁定语义，并仅暂存明确文件列表。
  - 提交前执行 `git diff --cached --name-only` 复核。
- 回滚/恢复方案（如需要）：
  - 若发现提交范围错误，先 reset staged 文件再重新 stage。
  - 若 push 后发现语义问题，基于本次 commit 做针对性修复提交。

## 参考
- `codex-rs/zmemory/src/tool_api.rs`
- `codex-rs/zmemory/src/service/export.rs`
- `codex-rs/zmemory/src/service/import.rs`
- `codex-rs/zmemory/src/service/alias.rs`
- `codex-rs/zmemory/src/service/tests.rs`
- `codex-rs/cli/src/zmemory_cmd.rs`
- `codex-rs/cli/tests/zmemory.rs`
- `codex-rs/tools/src/zmemory_tool.rs`
- `codex-rs/zmemory/README.md`
