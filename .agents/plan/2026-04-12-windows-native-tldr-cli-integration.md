# Windows 上 native-tldr 直集成到 codex-cli 的规划

## 背景
- 当前状态：`codex-cli` 已直接依赖 `codex-native-tldr`，Unix 路径支持 `ztldr internal-daemon` auto-start 与 daemon-first 查询；非 Unix 路径中，`query_daemon()` 固定返回 `None`，`ensure_daemon_running_detailed()` 在非 Unix 直接返回 not ready，CLI 因此回退到本地引擎。
- 触发原因：用户明确质疑“macOS 只需要一个 codex-cli，而 Windows 为什么还需要独立 native-tldr”，并进一步确认 Windows 是否也能直接集成。
- 预期影响：明确 Windows 侧的真实缺口、收敛后续实现边界，并为后续 issue 生成提供可执行拆分。

## 目标
- 目标结果：形成一份可执行计划，说明如何让 Windows 保持“只交付 codex-cli”前提下接入 native-tldr 的后台复用能力。
- 完成定义（DoD）：计划明确当前事实、目标边界、实施阶段、验证入口、主要风险与不采用方案，足以直接进入 `cadence-issue-generation`。
- 非目标：
  - 无

## 范围
- 范围内：
  - `codex-rs/cli/src/tldr_cmd.rs` 的 Windows daemon 查询、拉起、停止与状态面规划
  - `codex-rs/native-tldr/src/daemon.rs` 的非 Unix daemon 通信与 artifact 策略规划
  - Windows 与 Unix 交付形态差异的收口方案
  - 文档/说明层面对“独立 native-tldr”认知的澄清
- 范围外：
  - 立即修改实现代码
  - 变更 Unix 已工作的 daemon-first 主路径
  - 引入新的 HTTP 服务、系统服务安装器或独立 GUI

## 影响
- 受影响模块：
  - `codex-rs/cli`
  - `codex-rs/native-tldr`
  - 相关文档与架构说明
- 受影响接口/命令：
  - `codex ztldr ...`
  - hidden `codex ztldr internal-daemon --project ...`
  - `codex ztldr daemon start|stop|ping|warm|snapshot|status|notify`
- 受影响数据/模式：
  - native-tldr daemon artifact 布局
  - 非 Unix daemon 通信寻址方式
  - daemon health/status 诊断字段
- 受影响用户界面/行为：
  - Windows 上 `codex ztldr` 从“本地 fallback”升级为“daemon-first + fallback”时的可观测行为

## 约束与依赖
- 约束（时间/兼容性/安全/性能/发布窗口等）：
  - 必须保持“用户侧只安装 codex-cli 即可使用”这一交付形态，不额外要求独立 native-tldr 安装。
  - 不扩大为新的网络服务面；应延续当前 CLI 自拉起内部 daemon 的模型。
  - 规划内容必须基于仓库已确认事实：当前非 Unix daemon server 分支存在，但客户端查询与 auto-start 未接通。
- 外部依赖（系统/人员/数据/权限等）：
  - Windows 本地进程管理、文件锁与通信通道的具体实现能力
  - 后续真实开发阶段对 Windows 环境的验证资源

## 实施策略
- 总体方案：在保持 `codex-cli` 单一交付物不变的前提下，补齐 Windows daemon-first 生命周期：统一由 `codex` 自身拉起 hidden internal-daemon，补上非 Unix 查询链路、寻址协议、停止/状态管理与验证覆盖。
- 关键决策：
  - 继续沿用“`codex-cli` 集成 `codex-native-tldr` 库 + `codex` 自拉起内部 daemon”架构，不设计单独安装物。
  - 把当前非 Unix `run_tcp()` 视为过渡能力；在 issue 阶段明确评估继续用 TCP 还是改为更适合 Windows 的本地 IPC，但本规划不预先假设最终通道。
  - 保留 fallback 本地引擎，作为 daemon 不可用时的降级路径，而不是强制依赖后台进程。
- 明确不采用的方案（如有）：
  - 不采用“要求用户单独安装 native-tldr 可执行文件”的交付方案。
  - 不采用“为 Windows 单独引入 HTTP server/常驻系统服务”的扩张方案。

## 阶段拆分

### 阶段一：收口 Windows 当前差异与目标契约
- 目标：把“已集成能力”和“未集成的 daemon 生命周期”边界写清楚。
- 交付物：面向 issue 生成的差异清单与目标契约。
- 完成条件：明确列出当前非 Unix 的查询、拉起、停止、状态缺口，以及未来完成后的用户侧行为。
- 依赖：现有 `cli/native-tldr` 代码与架构文档。

### 阶段二：设计 Windows daemon 生命周期接线方案
- 目标：确定 Windows 端 daemon 的通信寻址、健康检查、artifact 与 auto-start 闭环。
- 交付物：实现任务拆分依据，包括 CLI 端与 native-tldr 端的改动面。
- 完成条件：能够把任务拆到 issue 级别，并为每项给出明确验证入口。
- 依赖：阶段一产物。

### 阶段三：规划验证与文档收口
- 目标：定义完成后必须补齐的测试、文档与用户可观测说明。
- 交付物：验证矩阵与文档更新范围。
- 完成条件：issue 进入执行后不会因验证入口不清或文档缺位而阻塞。
- 依赖：阶段二产物。

## 测试与验证
- 核心验证：
  - 规划文件内容与仓库现状一致，可直接支撑后续 issue 拆分。
- 必过检查：
  - 本地 `plan-reviewer` 评审通过。
- 回归验证：
  - 无
- 手动检查：
  - 核对计划中的事实是否均可追溯到现有代码或已存在架构文档。
- 未执行的验证（如有）：
  - 尚未做 Windows 环境实机验证；本阶段仅进行规划，不做代码执行验证。

## 风险与缓解
- 关键风险：当前非 Unix 分支只有 server 侧雏形，客户端接线方案若过早绑定某种 IPC 实现，后续可能返工。
- 触发信号：进入 issue 拆分时无法就通信寻址、停止机制或健康检查形成稳定 contract。
- 缓解措施：在 issue 阶段优先拆出“通信通道与 artifact contract 决策”子项，再安排 CLI/native-tldr 接线。
- 回滚/恢复方案（如需要）：若 Windows daemon-first 无法在可接受成本内收口，保留当前本地 fallback 路径并将 daemon-first 标记为后续增强项。

## 参考
- `/workspace/codex-rs/cli/Cargo.toml`
- `/workspace/codex-rs/native-tldr/README.md`
- `/workspace/codex-rs/cli/src/tldr_cmd.rs`
- `/workspace/codex-rs/native-tldr/src/daemon.rs`
- `/workspace/.agents/codex-cli-native-tldr/architecture.md`
