# 修复 ztldr daemon 本地回退语义与 core 入口策略

## 背景
- 当前状态：
  - `ztldr` 在 `core` 入口下由标准 `TldrHandler` 处理，普通分析动作默认不在正式请求里执行 `ensure_daemon_running(...)`；仅 `Ping / Warm / Snapshot / Status / Notify` 会显式触发 daemon 启动检查。
  - 当前自然语言分析类请求在自动路由下最可能被改写为 `semantic`，而 `semantic` 在 daemon 查询结果为 `None` 或 daemon 响应缺少 `semantic` payload 时会直接回退到本地引擎。
  - `core` 会在部分结构化请求前尝试一次 `maybe_issue_first_structural_warm(...)`，且 warm 成功判定当前只依赖 `output.success`，存在把“已降级到 local 的成功返回”误记为 warm 成功的风险。
  - `AutoTldrContext.warmup_requested` 是单 turn 生命周期；同一 turn 内一旦被置位，后续请求不再获得第二次 warm 机会。
- 触发原因：
  - 用户要求分析并修复 `ztldr 守护进程当时不可用，已退回 local engine` 的实际根因。
  - 并行分析已确认：首因更像 `core` 入口策略与 warm 语义，而不是 daemon 生命周期本身持续异常。
- 预期影响：
  - 降低 `core` 下普通分析请求被静默回退到 local 的概率。
  - 让 daemon 命中、local 降级、daemon-only 失败三类结果在三条入口上更一致、更可观察。
  - 避免同一 turn 内 warm 被误记成功后放大错误心智模型。

## 目标
- 目标结果：
  - 修正 `core` 下 ztldr 的 warm 成功判定与普通分析动作的 daemon 使用策略。
  - 让 `core` 对外显式暴露 `local fallback`，不再把降级结果伪装成普通成功。
  - 为当前已识别的单 turn warm 消耗与 `semantic`/分析请求回退路径补充可执行测试。
- 完成定义（DoD）：
  - `core` 中 warm 成功只在真实命中 daemon 且未降级时成立。
  - `core` 中普通分析动作的结果能显式区分 `daemon_success` 与 `degraded_local`，至少在用户可见摘要或结构化输出上成立。
  - 至少覆盖 `semantic / search / context / structure / diagnostics / extract` 这一组高频分析动作的入口策略。
  - 增加测试覆盖：
    - 同一 turn 内 warm 机会只在真实成功后消耗。
    - 同一 turn 内前序 warm 影响后续请求的行为边界。
    - 新 turn 自动重置 `AutoTldrContext`。
  - 不破坏 CLI/MCP 现有显式回退或显式失败语义。
- 非目标：
  - 无需扩展 `native-tldr` 引擎能力或新增 action。
  - 无需在本轮重构 daemon 生命周期底层实现。
  - 无需修改与本问题无关的 shell/read/grep 自动路由逻辑。

## 范围
- 范围内：
  - `codex-rs/core/src/tools/handlers/tldr.rs`
  - `codex-rs/core/src/tools/rewrite/context.rs`
  - 必要时与 `core` 入口消费降级信号直接相关的相邻代码
  - 与上述行为直接相关的 `codex-core` 测试
  - 如需统一结果契约，允许最小范围触及 `codex-rs/native-tldr/src/tool_api.rs` 或共用输出封装层
- 范围外：
  - `cli` 与 `mcp-server` 的行为重写或文案重做
  - daemon 工件目录、锁文件、socket/pid 生命周期协议的底层重构
  - 与当前问题无关的 `ztldr` 路由优化任务

## 影响
- 受影响模块：
  - `codex-rs/core/src/tools/handlers`
  - `codex-rs/core/src/tools/rewrite`
  - 可能受影响的共享输出封装层
- 受影响接口/命令：
  - `ztldr` 工具在 `core` 入口下的普通分析动作
  - 自动 warm 路径
- 受影响数据/模式：
  - 无持久化 schema 变更
  - 可能调整工具输出中的降级可见性与内部上下文记录条件
- 受影响用户界面/行为：
  - 用户将更明确地看到“本次结果来自 local fallback”
  - 普通分析请求更少出现“看似成功但实际上未命中 daemon”的情况

## 约束与依赖
- 约束（时间/兼容性/安全/性能/发布窗口等）：
  - 必须保持 `core`、`cli`、`mcp-server` 对 daemon-only 动作的既有功能边界。
  - 不允许通过隐藏错误、静默吞掉降级状态来制造“看起来正常”。
  - 计划执行优先最小闭环修复，不做跨模块大重构。
  - 新增验证必须基于仓库中真实可执行的测试入口。
- 外部依赖（系统/人员/数据/权限等）：
  - 当前会话可用的源码分析与本地测试能力
  - 无外部系统依赖

## 实施策略
- 总体方案：
  - 先修 `core` 的 warm 成功判定与降级对外信号，再决定是否扩大普通分析动作的 auto-start 范围。
  - 优先修入口语义，保留 daemon 生命周期底层现状。
  - 通过测试把“单 turn 一次性 warm + 后续分析动作 fallback”钉死为可回归场景。
- 关键决策：
  - 不把 `output.success == true` 视为 warm 成功的充分条件；必须区分 daemon 命中与 local 降级。
  - `core` 不应再只把降级状态写入 `auto_tldr_context`；至少要对外显式暴露。
  - 若扩大 auto-start，优先覆盖当前高频结构分析动作，而不是所有动作一次性全开。
- 明确不采用的方案（如有）：
  - 不通过纯文案修补来掩盖 `core` 的语义偏差。
  - 不优先重写 daemon 生命周期层来规避 `core` 入口问题。

## 阶段拆分
### 阶段一：修正 core 的降级语义
- 目标：
  - 修正 warm 成功判定与 `core` 的降级对外可见性。
- 交付物：
  - `core` 入口代码修改
  - 对应单元测试或处理链测试
- 完成条件：
  - warm 不再把 degraded local 结果误记为成功
  - 用户可见输出或结构化结果能明确表达 `local fallback`
- 依赖：
  - 已确认的 `core` 与 `native-tldr` 行为分析结论

### 阶段二：收敛普通分析动作的 daemon 使用策略
- 目标：
  - 决定并实现 `core` 下高频普通分析动作的 auto-start 或等效语义修复。
- 交付物：
  - 高风险 action 策略修复
  - 回归测试
- 完成条件：
  - `semantic / search / context / structure / diagnostics / extract` 至少不再依赖一次性 warm 才有机会命中 daemon
- 依赖：
  - 阶段一完成

### 阶段三：补齐时序与边界测试
- 目标：
  - 覆盖同 turn warm 消耗、新 turn 重置、fallback 可见性三个关键场景。
- 交付物：
  - 新增或更新的 `codex-core` 测试
- 完成条件：
  - 缺失测试场景均有定向覆盖
- 依赖：
  - 阶段一、阶段二实现稳定

## 测试与验证
- 核心验证：
  - `cd /workspace/codex-rs && cargo test -p codex-core tldr`
  - `cd /workspace/codex-rs && cargo test -p codex-core auto_tldr`
  - `cd /workspace/codex-rs && cargo test -p codex-core shell_search_rewrite`
- 必过检查：
  - `cd /workspace/codex-rs && just fmt`
  - 与本次修改直接相关的 `codex-core` 定向测试通过
- 回归验证：
  - daemon-only 动作在 `core` 下仍保持显式失败语义
  - `cli` 的显式 local fallback 与 `mcp-server` 的显式错误行为不受影响
  - 自然语言请求仍然正确路由到 `semantic`
- 手动检查：
  - 构造同一 turn 内两个连续的结构分析请求，确认第二个请求不会因“伪 warm 成功”而误判
  - 检查用户可见输出中是否能明确识别 `local fallback`
- 未执行的验证（如有）：
  - 无

## 风险与缓解
- 关键风险：
  - 扩大 `core` 的 auto-start 范围后引入新的时序或性能回归
  - 调整输出语义导致现有测试或上层依赖失配
  - 只修 warm 判定但不修普通动作策略，仍残留相同行为缝隙
- 触发信号：
  - `codex-core` 相关测试出现大面积行为快照变化
  - 同一类请求在 `core` 与 `cli` 的 source 语义继续明显漂移
  - 仍能复现“degraded local 被当作 warm 成功”
- 缓解措施：
  - 先做阶段一，确认信号语义稳定后再扩大策略范围
  - 优先做高频 action 的定向修复，不一次性放大范围
  - 为 warm 判定与 turn 内时序增加专门测试
- 回滚/恢复方案（如需要）：
  - 分阶段提交；若阶段二引入回归，可仅保留阶段一的语义修复并回滚策略扩张

## 参考
- `codex-rs/core/src/tools/handlers/tldr.rs:106`
- `codex-rs/core/src/tools/handlers/tldr.rs:113`
- `codex-rs/core/src/tools/handlers/tldr.rs:221`
- `codex-rs/core/src/tools/rewrite/context.rs:50`
- `codex-rs/core/src/tools/rewrite/tldr_routing.rs:90`
- `codex-rs/core/src/session/turn_context.rs:572`
- `codex-rs/native-tldr/src/tool_api.rs:1209`
- `codex-rs/native-tldr/src/tool_api.rs:1222`
- `codex-rs/native-tldr/src/lifecycle.rs:63`
- `codex-rs/mcp-server/src/tldr_tool.rs:142`
- `codex-rs/cli/src/tldr_cmd.rs:869`
