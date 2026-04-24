# 2026-04-24 ZTeam 架构深审

CHARTER_CHECK:
- Clarification level: LOW
- Task domain: architecture
- Must NOT do: 不改源码；不把 UI 细节误判为协议问题；不建议继续膨胀 `codex-core`
- Success criteria: 明确当前 zteam/tui/federation 的架构问题；比较至少两种不同方案；给出推荐、风险、验证步骤；结果写入规定产物
- Assumptions: 当前任务是只读架构审查与结果落盘，不做实现与测试扩张

## Status

recommended

## Scope

本次审查聚焦三个边界：

- `codex-rs/tui` 内的 ZTeam 生命周期与恢复控制面
- TUI 与 app/thread attach seam 的职责分布
- federation 预留能力是否已经形成可扩展边界

不在本次范围内：

- UI 视觉/交互细节评审
- app-server v2 federation 协议重设计
- 任何源码实现或测试扩张

## Architecture Problem

当前实现已经证明 ZTeam 可以在本地 TUI 模式里工作，并且恢复链路明显吸收了前一轮反思的正确方向。但系统仍存在一个更深的架构缺口：

**团队协作的控制面尚未从模型行为和分散状态里抽离出来。**

这导致三个后果同时存在：

1. 启动端依赖模型遵守 prompt。
2. 恢复端依赖启发式推断 worker 身份。
3. federation 端只有 UI 摘要位，没有真正可执行的 source/transport seam。

## Evidence

### A. 启动仍是 prompt 驱动

- `codex-rs/tui/src/app.rs:2057`
- `codex-rs/tui/src/zteam.rs:604`

`/zteam start` 当前不是一条确定性控制命令，而是向当前线程提交一段自然语言任务描述，让模型自行调用 `spawn_agent` 产出固定角色 worker。  
这意味着 worker roster 的建立并不由代码直接保证，而是由模型行为间接决定。

### B. 恢复链路本身方向对，但身份真相源不够强

- `codex-rs/tui/src/app.rs:3957`
- `codex-rs/tui/src/app.rs:3982`
- `codex-rs/tui/src/app.rs:3987`
- `codex-rs/tui/src/app.rs:4043`
- `codex-rs/tui/src/zteam/recovery.rs:51`
- `codex-rs/tui/src/zteam/recovery.rs:78`

这里的好消息是：恢复并没有走“扫全量 thread 然后逐条 attach”的老路，而是通过 `latest_local_threads_for_primary(...)` 做候选筛选，再走现有 live attach seam。  
这说明恢复架构主方向是对的，问题主要不在 attach 机制，而在 attach 前的身份判定和启动真相源。

### C. worker 身份识别仍含昵称/角色启发式

- `codex-rs/tui/src/zteam/worker_source.rs:55`
- `codex-rs/tui/src/zteam/worker_source.rs:74`

当前逻辑先匹配 canonical `agent_path`，失败后仍会退回 `agent_role + nickname` 模糊识别。  
这会把“历史兼容”与“运行期主判定”混在一起，给恢复和 attach 带来误吸附空间。

### D. federation adapter 还只是摘要态

- `codex-rs/tui/src/zteam.rs:190`
- `codex-rs/tui/src/zteam.rs:302`
- `codex-rs/tui/src/zteam/worker_source.rs:38`

`SharedState` 目前只维护 frontend/backend/activity/results/federation_adapter 等本地槽位。  
`configure_federation_adapter` 更像设置 UI/状态摘要，而 `WorkerSource::FederationBridge(...)` 仍带 `#[allow(dead_code)]`，说明真正的 transport/source seam 还没有进入运行期。

### E. 领域逻辑仍然分散

- `codex-rs/tui/src/zteam.rs:184`
- `codex-rs/tui/src/chatwidget.rs:9685`

`zteam.rs` 承担状态盒子职责，`app.rs` 处理恢复和 attach 编排，`chatwidget.rs` 还承担 UI 包装与入口。  
这说明 ZTeam 当前边界更接近“零散 capability + 共享状态”，而不是完整 coordinator。

## Findings

### 高: 启动控制面不确定

只要 worker 创建仍依赖 prompt 和模型工具选择，代码层就没有稳定的团队生命周期入口。  
恢复越完善，越会暴露这个缺口，因为恢复依赖“启动时曾经严格落盘过什么”，而现在启动并不完全受代码控制。

### 中: 身份判定不是单一真相源

当 canonical metadata 与启发式 fallback 并存且都参与运行期主判定时，系统最终会把“识别”变成概率事件。  
这对 attach/recovery/workbench 这种长期状态尤其危险。

### 中: federation seam 还不可执行

如果未来真要支持 federation-backed worker，当前 adapter 摘要态不能直接承接 worker lifecycle。  
到时会出现“UI 已有 adapter，运行期却没有 source”的结构断层。

### 中: coordinator 缺位导致 orchestration 向 TUI 中心文件回流

只要生命周期逻辑继续散落，新增 team 能力时就会优先往 `app.rs` 和 `chatwidget.rs` 填逻辑。  
这会让当前正确的恢复思路逐渐再次埋进大文件 orchestration 噪音里。

## Options

### Option A: 维持现状，继续渐进加补丁

做法：

- 保留 prompt 驱动启动。
- 保留启发式 fallback 作为常态路径。
- federation 先只放 adapter 摘要，未来再说。

比较：

- 实现成本：低
- 运维成本：中到高
- 团队复杂度：中
- 未来变更成本：高

适用前提：

- 只把 ZTeam 视为轻量体验能力，不要求强恢复和稳定扩展。

主要问题：

- 架构债会持续累积，尤其是 federation 真接线时会回头重整控制面。

### Option B: 提炼 ZTeam coordinator，并把 worker source 抽象补成真接口

做法：

- 在 `tui/src/zteam/` 下新增明确 coordinator。
- 由代码负责 worker 启动、注册、恢复、attach 的状态机。
- canonical metadata 成为 worker 身份主真相源。
- federation 只在 source seam 上接入，不直接把适配逻辑散落进 `app.rs`。

比较：

- 实现成本：中
- 运维成本：中
- 团队复杂度：中
- 未来变更成本：低到中

适用前提：

- 接受一次边界重整，以换取后续本地与 federation 两条路径共用控制面。

主要收益：

- 本地模式先稳定下来，federation 才有可靠落点。

## Recommendation Summary

推荐 **Option B**，并按下面顺序推进：

1. 先把本地 ZTeam lifecycle 收敛成确定性 coordinator。
2. 再把 worker identity 从启发式切到 canonical metadata 主判定。
3. 最后让 federation 以真正 `WorkerSource` 的形式接入，而不是继续停留在 adapter 摘要层。

这条路径最轻，因为它优先修正已经暴露的控制面问题，不额外扩大到 `codex-core` 或 app-server 公共协议层。

## Tradeoffs

- 短期会新增一个内部抽象层，但这是为了解决当前职责扩散，而不是抽象过度。
- 启发式识别降级后，可能需要显式处理少量历史线程兼容。
- federation 接入会稍晚于“先把 adapter 接上 UI”的表面速度，但返工更少。

## Risks

- 如果 coordinator 设计过宽，可能重新做成一个新的大状态盒子。
- 如果 migration 方案不清楚，历史 worker 线程可能短期无法自动归位。
- 如果 federation source 抽象没有先从本地模式验证，容易做成空泛接口。

## Validation Steps

1. 明确 `start -> spawn -> register -> recover -> attach` 状态机及真相源。
2. 证明仅靠 canonical metadata 就能正确恢复本地 worker。
3. 验证 thread-list attach 与 loaded auto-recovery 都经过同一 coordinator 入口。
4. 在 source seam 上做一次本地 source 与 federation source 的接口对照，确认最小共同面只包含：
   - start
   - enumerate/recover
   - attach
   - status/result

## Artifacts Created

- `.agents/results/result-architecture.md`
- `.agents/results/architecture/2026-04-24-zteam-architecture-deep-review.md`
