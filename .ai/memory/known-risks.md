# 已知风险

## 跨模块风险

- `SessionConfiguration.cwd` 的 turn 级覆盖不会自动重算整份 project-layer config；任何直接消费 `turn.config.*` 中项目作用域配置的链路，都可能沿用线程初始项目配置。
- `zmemory` 已确认受这类耦合影响：function handler、稳定偏好主动写入、以及子线程 `spawn` / `resume_agent` / `agent_jobs` 在 turn cwd 改变后，都可能读到过期的 project-scoped `zmemory.path`。
- 2026-04-02 已对齐上述 `zmemory` 链路，但尚未对所有 project-scoped config 做通用重载；未来若其它 handler 直接消费 project-scoped config，仍可能复现同类问题。
- 上游同步技能或手工 merge/cherry-pick 若只按冲突解决通过编译，不额外审计分叉版自有功能，容易把本地保留特性一并覆盖掉；已验证高风险区包括 `core` 的上下文重注入/turn cwd 解析链路，以及 `tui` 的 buddy、plugins、approval、history、`request_user_input` 汉化与分叉交互细节。

## 回归高危点

- app-server / core 需要持续覆盖“线程初始 cwd 与 turn cwd 不同”的场景，确认 `system://workspace` 的 `dbPath`、`source`、`reason` 与目标项目 `.codex/config.toml` 一致。
- 新增或改动直接读取 `turn.config.*` 的 handler 时，需要显式验证 turn cwd override 后是否重新解析 project-scoped 配置，而不是复用线程初始配置。
- 子线程相关链路继续重点回归：`spawn_agent`、`resume_agent`、`agent_jobs` 必须继承当前 turn cwd 下解析出的 `zmemory` 配置，而不是父线程启动时的项目路径。
- 以后使用 `sync-openai-codex-pr` 或其他上游同步流程前，必须先列出“本分叉独有功能/文案保留清单”，同步后逐项核对；至少覆盖 `auto_tldr_routing`、`reference_context_item`/完整上下文重注入、`AGENTS.md` 重新解析、TUI 中文文案、buddy 可见性/状态展示与插件弹窗。
- 2026-04-08 已验证 upstream web 的 browse/review/maintenance 主链路可复用，但 memory browser 的 keyword manager 仍依赖 `/browse/glossary` POST/DELETE；当前 compat adapter 只实现 GET，不能把上游 web 记为“全量可写”，后续若开放 glossary 写入需补齐显式接口与回归。
