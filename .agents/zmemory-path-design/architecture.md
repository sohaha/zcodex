---
type: architecture
outputFor: [tech-lead, scrum-master, frontend, backend, devops]
dependencies: [prd]
---

# 系统架构文档

## 文档信息
- **功能名称**：zmemory-path-design
- **版本**：1.0
- **创建日期**：2026-03-30
- **作者**：Architect Agent

## 摘要

> 下游 Agent 请优先阅读本节，需要细节时再查阅完整文档。

- **架构模式**：单体 Rust 模块 (`codex-rs/zmemory`) + 共用路径策略，确保每个主仓库/cwd 自动隔离 sqlite 文件。
- **技术栈**：Rust + rusqlite/FTS5；依赖 `codex-git-utils` 的主仓库根检测、`codex-cli` 的配置层和 TUI 会话 Context。
- **核心设计决策**：新增 `[zmemory].path` 可配置项；绝对路径直用、相对路径按主仓库根/cwd 解析；默认路径改为 `$CODEX_HOME/zmemory/projects/<project-key>/zmemory.db`，`project-key` 来源于 canonical repo/cwd 的 hash，而不是原始路径片段；首版把可观测性放进现有 `doctor/stats` 输出，不额外新增命令。
- **主要风险**：仓库与工作目录的符号链接差异、git worktree 与主仓库根语义、旧全局数据库迁移、CLI/TUI 解析不一致。
- **项目结构**：关注模块为 `codex-rs/zmemory/src/config.rs`、`repository.rs`、`codex-rs/zmemory/README.md`、codex-cli/core 提供的 repo_root 辅助及 TUI/会话启动代码。

---
---

## 1. 架构概述

### 1.1 设计目标

- 让 zmemory 无需 scope、软编码路径，相同主仓库/cwd 保证使用相同 sqlite 文件，不同主仓库/cwd 自动隔离。只对 `[zmemory].path` 暴露配置。
- 兼顾 CLI、core tool、TUI/会话三方入口，路径判定逻辑共享一套，避免不同客户端产生不同文件或锁冲突。
- 预留可控迁移：旧的 `$CODEX_HOME/zmemory/zmemory.db` 依旧可访问，但首版不做自动迁移；需要复用旧库时由用户显式通过 `[zmemory].path` 指定。

### 1.2 路径解析优先级

1. **显式配置（`[zmemory].path`）**
   - 绝对路径：直接使用，不做哈希。
   - 相对路径：若解析上下文处在 git 仓库，主仓库根为基准；否则以 `cwd` 为基准，最终仍会 canonicalize。
2. **默认策略（未配置）**
   - 先尝试从 `cwd` 解析主仓库根：复用现有 root-git-project 语义，与 trust/project 识别保持一致；同一主仓库下的 worktree 默认共享同一个 project identity。
   - 以结果路径（主仓库根优先；没有主仓库根则 fallback 为 `cwd`）的 canonical 表示计算 `project-key`：`sha256(canonical_path.to_string_lossy())`，取固定前缀（例如前 12 个 hex），再拼接到 `$CODEX_HOME/zmemory/projects/<project-key>/zmemory.db`。
   - 该 key 作为稳定目录名，避免直接拼接原始 path 引入 `/`、空格或大小写差异所带来的目录冲突。
3. **回退保障**：如果主仓库根无法确定，则用 `cwd`；如果 base 路径 canonicalize 失败，直接报错并提示用户显式配置 `[zmemory].path`，避免静默切换到意外数据库。

### 1.3 repo_root/cwd 检测共享

- 新增公共 helper，建议放在 `codex-rs/zmemory/src/path_resolution.rs`，由该 crate 直接依赖 `codex-git-utils`；这样 `codex-core` 和 `codex-cli` 都可以复用同一实现，避免 `core <-> zmemory` 循环依赖。
- 该 helper 接收 `cwd`、`codex_home`、可选 `[zmemory].path` 原始配置值，输出最终路径与日志理由；内部依赖现有 root-git-project 语义和路径解析工具，确保与 `core` 中的 project/trust 识别一致。
- `ZmemoryConfig::new` 不再自行推导路径，只接受已解析好的 `ZmemoryPathResolution`；这样 CLI、core handler、tool API 都必须显式调用同一个 resolver，杜绝多套逻辑。

## 2. 技术栈

### 2.1 关键模块与改造点

| 模块 | 说明 | 改造要点 |
|------|------|----------|
| `codex-rs/zmemory/src/config.rs` | `ZmemoryConfig` 暴露 sqlite 路径；当前只依赖 `codex_home` | - 接收新的 `ZmemoryPathResolution`；<br/>- 暴露 `path_resolution()` 供 repository/service 读取；<br/>- 不在 config 内部重复做路径解析。 |
| `codex-rs/zmemory/src/path_resolution.rs` | 新增 resolver 模块 | - 解析 `[zmemory].path` 的绝对/相对/默认策略；<br/>- 调用 git-utils 获取主仓库根；<br/>- 生成稳定项目 key（实现字段名仍为 `workspace_key`）与 `reason`。 |
| `codex-rs/zmemory/src/repository.rs` | `ZmemoryRepository::connect` 负责创建目录和打开 sqlite | - 保持目录创建逻辑；<br/>- 在 `connect` 之前确保 helper 已 canonicalize，并将 `project-key` 目录权限调整为 `0o755`；<br/>- 额外提供 `db_path_reason()` 方法供日志或 TUI 读取。 |
| `codex-rs/zmemory/README.md` | 文档说明 database 位置和配置 | - 更新章节解释 `[zmemory].path` 语义、默认隔离逻辑、手动迁移旧文件；<br/>- 指出 CLI/TUI 如何通过 `doctor`/`stats` 校验当前路径。 |
| CLI/core/TUI 启动 | `codex-cli`, `codex-app-server`/TUI/agent session 等 | - 在构建 `Session` 时调用统一 helper，传入 `cwd`、`project_config`、`codex_home`、`[zmemory].path` config；<br/>- 统一将最终 `db_path` 写入运行时 config，避免多个模块独立判断；<br/>- TUI/会话启动日志 (via `tracing::info!`) 输出 `[zmemory].path` 及决定原因；<br/>- 首版通过 `doctor`/`stats` 输出 `pathResolution` 供调查使用。 |

### 2.2 CLI/Core/TUI 共享入口

- `codex-cli` 读取统一配置链中的 `[zmemory].path`，将 raw 字符串传给 helper；
- `codex-rs/core` 在 `Session` 建立时 sequence: `resolve_cwd -> active_project -> helper -> ZmemoryConfig::new_with_settings`; `codex-rs/app-server` 复用同一 helper。
- TUI/agent 端通过现有 session/config 传递解析结果，以便 UI/agent 侧用于诊断或 `zmemory doctor` 输出。这样 CLI、core 工具、TUI 只要依赖同一套 helper，就能保持路径一致。

## 3. 目录结构

```
$CODEX_HOME/
└── zmemory/
    ├── zmemory.db                         # 旧全局文件，升级期间可手动映射
    └── projects/<project-key>/
        └── zmemory.db                     # 按 repo_root/cwd 哈希隔离的数据库
```

- `projects/<project-key>` 文件夹名建议固定前缀+定长 hex（例如 12 字符），避免直接从路径中截取目录名。
- 当 `[zmemory].path` 显式配置为 `workspace/custom.db`（相对路径）时，解析结果位于 repo_root 或 cwd 的 `workspace` 子目录；log 中记录实际绝对路径。
- 各稳定项目 key 会通过 `codex zmemory doctor --json` 或 `codex zmemory stats --json` 的 `workspaceKey` 字段暴露，便于排查锁和权限问题。

## 4. 数据模型

### 4.1 `[zmemory].path` 语义

- 配置项只接受单个值（不再增 scope）。
- 绝对路径直接信任；相对路径的 base 由 helper 决定（优先 repo_root，否则是 cwd）。
- 解析后统一 canonicalize（消除符号链接与 `..` 路径差异）。如果 canonicalize 失败，直接报错并提示用户显式配置路径。

### 4.2 默认路径与迁移策略

1. 新安装：`[zmemory].path` 未配置，则在 `$CODEX_HOME/zmemory/projects/<project-key>/zmemory.db` 创建数据库。
2. 老用户升级：首版不尝试自动复制 `$CODEX_HOME/zmemory/zmemory.db`；若用户需要沿用旧库，应显式在配置文件中填入旧路径。
3. 避免重复：若 repo/cwd 之间仅仅通过符号链接造成 canonical 结果相同，helper 会统一到同一个稳定项目 key（运行时字段名仍为 `workspaceKey`），避免多个 path 指向不同数据库。
4. 首版不增加额外命令行 flag 或环境变量入口，避免外部入口膨胀；共享单库的需求仍通过配置文件中的 `[zmemory].path` 表达。

### 4.3 配置兼容策略

- 新 helper 读取统一配置链中的 `[zmemory].path`。
- `ZmemoryConfig` 继续接受 `ZmemorySettings`（valid_domains/boot URIs），但 `[zmemory].path` 永远由 helper 输出；如果 helper 失败，`ZmemoryConfig::new` 返回 `Err`，避免静默使用错误路径。
- README 说明通过 `[zmemory]` 配置块设置 `path`，以及如何回退到旧的全局 DB；在 CLI 输出中也建议用户确认 `codex zmemory doctor --json`/`stats --json` 当前 `db_path` 与 `reason`。

## 5. API 设计

- 新增内部结构 `ZmemoryPathResolution`（包含 `db_path`、`workspace_key`、`reason`；其中 `workspace_key` 承载稳定项目 key），供 CLI/TUI/测试调用。该结构也可在日志/诊断输出中序列化。
- 在命令行 `codex zmemory` 子命令中，仅通过统一配置读取 `[zmemory].path`；首版不为每个子命令增加独立 flag，避免接口膨胀。
- `SessionConfig` 或 `Re`,  目前 `codex_state` 通过 `Session::get_config()` 读取 `cwd`，因此 helper 应接受 `AbsolutePathBuf`，以保持 `core` 层对于 `cwd` 的 `resolved` 版本一致。
- 保持 `codex-rs/zmemory` crate 对外 API 简洁：只提供 `ZmemoryConfig::new_with_settings(codex_home, settings, path_resolution)`，不暴露 `project-key` 细节。

## 6. 安全设计

### 6.1 canonicalize + 符号链接/大小写

- 所有路径都会调用 `std::fs::canonicalize`（或等价解析工具），确保符号链接与 `..` 结果一致；哈希输入使用 canonical 后的原始路径字符串，不额外 lowercase/casefold。
- 对 `canonicalize` 失败的情况直接返回错误并提示用户手动检查路径或显式配置 `[zmemory].path`。

### 6.2 git worktree 与 repo_root

- 依赖现有 root-git-project 语义，其对 git worktree 会归并到主仓库根；因此首版默认让同一主仓库下的 worktree 共享一个稳定项目 key（字段名仍为 `workspaceKey`），与现有 trust/project 识别保持一致。
- `core` 中的 project/trust 只作为语义对齐参考，不再要求 helper 驻留在 `core` crate。

### 6.3 可观测性

- 每次解析都会 log（`tracing::info!`）: `zmemory path resolved to {db_path} (reason: {reason})`，reason 包括 `[zmemory].path` 来源、hash key 组成，便于排查跨 repo 重用与锁竞争。
- 若目录创建失败，捕获 `std::io::Error` 并以 `tracing::warn!` 记录 `db_path` 与 `source`；`codex zmemory doctor`/`stats` 可将 diagnostics 字段拆出 `path_resolution` 供用户检查。

## 7. 部署架构

- 具体改动顺序：编写 helper → 更新 `ZmemoryConfig`/`repository` → 修改 CLI/TUI 会话调用 → README/文档 → 不需要 deploy 环境变化。
- CLI 端在现有 `codex-rs/cli/src/zmemory_cmd.rs` 接入 helper，并将 `path_resolution` 监控信息写入 stdout/stderr。
- TUI/agent 端在现有 session/config 组装链路调用同一 helper，确保 app-server 侧与 CLI 看到同一个 `db_path`。

## 8. 性能考虑

- 每次启动运行 `sha256` 与 `canonicalize` 开销非常低，主要影响在 `tracing` 与 `fs::create_dir_all`；只会在工作目录变更或 `[zmemory].path` 调整时执行。
- 目录结构使用哈希前缀可避免因为工作目录变更造成的大量目录重建；但要注意 `codex_home/zmemory` 下的子目录可能随 repo 数量线性增长，清理能力留作后续增量需求。

## 9. 风险与实施次序

### 9.1 风险清单

1. **路径冲突**：不同 repo/cwd canonical 之后仍旧相同，导致互相污染；哈希的 length 和 collision 检测需明确。
2. **主仓库根语义**：若实现方误把 worktree 当作独立 identity，会与现有 project/trust 识别打架。
3. **多端判定不一致**：CLI/core/TUI 其中之一未使用 helper，导致文件锁或重复数据。
4. **符号链接差异**：Windows/Linux 上 canonicalize 行为不同，可能生成新 `project-key`；需用 targeted test 覆盖，而不是通过 casefold 粗暴折叠。
5. **用户观察能力弱**：无法确定当前路径时难以排查，需要 `zmemory doctor`/`stats` 输出 `path_reason`。

### 9.2 推荐实施次序

1. 在 `codex-rs/zmemory` 内实现共享 helper，引入 `ZmemoryPathResolution`，编写单元测试覆盖 `absolute/relative/default` 情形。
2. 扩展 `ZmemoryConfig`/`ZmemoryRepository` 接口接收 `ZmemoryPathResolution`，禁止内部自行 fallback 到旧全局路径。
3. 在 CLI、core 工具与 TUI 会话启动链路中统一调用 helper，打印解析原因并用 `tracing::info!` 记录 `db_path`。
4. 在 README 与 `codex config` 文档中说明 `[zmemory].path` 语义、迁移建议（例如手动指定旧路径）、以及通过 `doctor`/`stats` 检查当前解析结果。
5. 若后续确认 `doctor`/`stats` 的可观测性不足，再单独评估是否新增诊断命令，首版不纳入实现。

## 10. 明确不做

- 不引入新的 `scope` 或额外命名空间；所有隔离行为由 `[zmemory].path` 与 hashed 目录决定。
- 不支持远端数据库/网络存储；仍使用本地 sqlite。
- 不为每个 CLI 命令传递额外参数去决定路径，只通过统一 helper 与配置层。
- 不将新的隔离机制要求用户立即迁移；用户可手动设置 `[zmemory].path=$CODEX_HOME/zmemory/zmemory.db` 继续使用旧行为。
