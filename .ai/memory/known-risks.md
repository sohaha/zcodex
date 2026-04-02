# 已知风险

## 跨模块风险

- `SessionConfiguration.cwd` 的 turn 级覆盖不会自动重算整份 project-layer config；任何直接消费 `turn.config.*` 中项目作用域配置的链路，都可能沿用线程初始项目配置。
- `zmemory` 已确认受这类耦合影响：function handler、稳定偏好主动写入、以及子线程 `spawn` / `resume_agent` / `agent_jobs` 在 turn cwd 改变后，都可能读到过期的 project-scoped `zmemory.path`。
- 2026-04-02 已对齐上述 `zmemory` 链路，但尚未对所有 project-scoped config 做通用重载；未来若其它 handler 直接消费 project-scoped config，仍可能复现同类问题。

## 回归高危点

- app-server / core 需要持续覆盖“线程初始 cwd 与 turn cwd 不同”的场景，确认 `system://workspace` 的 `dbPath`、`source`、`reason` 与目标项目 `.codex/config.toml` 一致。
- 新增或改动直接读取 `turn.config.*` 的 handler 时，需要显式验证 turn cwd override 后是否重新解析 project-scoped 配置，而不是复用线程初始配置。
- 子线程相关链路继续重点回归：`spawn_agent`、`resume_agent`、`agent_jobs` 必须继承当前 turn cwd 下解析出的 `zmemory` 配置，而不是父线程启动时的项目路径。
