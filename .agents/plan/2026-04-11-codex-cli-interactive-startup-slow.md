# codex-cli 交互模式启动慢分析

> 不适用项写 `无`，不要留空。
> 只写已确认事实；若信息不清楚或需要假设，先回到对话澄清，不要把未确认内容写进计划。

## 背景
- 当前状态：用户要求深度分析 `codex-cli` 为什么启动很慢，当前已确认慢的是交互式 TUI 路径，而非普通非交互子命令路径。
- 触发原因：本地分析中，`codex --version`、`codex features list` 等非交互命令启动很快，但交互式 `codex`/`codex --no-alt-screen` 在 PTY 环境下会长时间停留在终端控制序列阶段。
- 预期影响：需要把“启动慢”的成因拆解到具体同步阻塞步骤，形成可审阅、可继续落地优化的结论基线。

## 目标
- 目标结果：形成一份基于代码与本地证据的交互模式启动慢成因分析，明确主因、次因、证据链与后续优化优先级。
- 完成定义（DoD）：
  - 已确认交互模式启动主路径及关键同步初始化步骤；
  - 已区分交互式慢启动与非交互命令快速启动的差异；
  - 已把主要可疑点落实到具体文件与代码位置；
  - 已给出按优先级排序的优化方向；
  - 计划内容可直接作为后续 issue 生成的事实基础。
- 非目标：
  - 本阶段不修改生产代码；
  - 本阶段不承诺完成性能修复；
  - 本阶段不做全平台、全终端环境的定量基准覆盖。

## 范围
- 范围内：
  - `codex-rs/cli` 交互入口；
  - `codex-rs/tui` 启动初始化链；
  - `codex-rs/core` 配置加载、project layer、project doc、state db 等启动相关路径；
  - embedded app-server 初始化链；
  - 与启动时序直接相关的日志、认证、resume/fork、buddy 影响判断。
- 范围外：
  - 与首屏启动无关的普通 turn 执行性能；
  - `ztldr` 本身的独立性能优化；
  - 未经用户要求的代码级重构与实现变更。

## 影响
- 受影响模块：`codex-rs/cli`、`codex-rs/tui`、`codex-rs/core`、`codex-rs/app-server-client`、`codex-rs/app-server`、`codex-rs/rollout`。
- 受影响接口/命令：`codex` 默认交互模式、`codex resume`、`codex fork`、TUI 启动期 embedded app-server 相关调用。
- 受影响数据/模式：配置层加载、project root/trust 判定、project doc 发现、state db 打开、TUI 终端探测。
- 受影响用户界面/行为：交互模式首屏出现前的等待时间、启动阶段是否长时间停在终端控制序列或初始化状态。

## 约束与依赖
- 约束（时间/兼容性/安全/性能/发布窗口等）：
  - 仅基于当前仓库与当前会话环境做分析；
  - 不把未验证的终端行为写成确定事实；
  - 结论必须区分“代码已确认”与“环境相关推断”。
- 外部依赖（系统/人员/数据/权限等）：
  - 依赖当前会话可访问的本地仓库代码；
  - 依赖当前 shell/PTY 环境提供有限的交互式启动观测能力；
  - 无额外人员或外部系统依赖。

## 实施策略
- 总体方案：沿交互模式真实启动链自顶向下排查，结合代码阅读与本地最小实测，把慢启动拆解为终端探测、配置加载、embedded app-server 初始化、状态与认证检查等同步阶段，并按主次排序。
- 关键决策：
  - 先区分“非交互快、交互慢”，避免把问题泛化为整个 CLI 启动慢；
  - 先确认代码级同步阻塞点，再用最小实测验证体感是否一致；
  - 将 buddy/宠物功能视为次级影响项单独排除，不混淆为首因。
- 明确不采用的方案（如有）：
  - 无基于猜测的 flame graph 结论；
  - 无未加埋点前的毫秒级精确归因承诺；
  - 无未经证据支持的“正式版不会初始化某模块”类推断。

## 阶段拆分
### 阶段一：建立启动路径与证据基线
- 目标：确认交互模式真实调用链，并建立“非交互快、交互慢”的对照证据。
- 交付物：启动路径摘要、最小本地实测结果、关键入口文件定位。
- 完成条件：已定位 `cli -> tui -> config/app-server/app` 主路径，并确认非交互命令明显更快。
- 依赖：当前仓库源码、当前 shell 可执行 `codex`。

### 阶段二：拆解同步阻塞点
- 目标：把交互模式启动前后的同步初始化步骤拆解到具体函数与模块。
- 交付物：终端探测、配置双加载、project layer、embedded app-server、state db、auth/logging 等阻塞点列表。
- 完成条件：每个主要阻塞点都有对应代码位置与行为说明。
- 依赖：`codex-rs/tui`、`codex-rs/core`、`codex-rs/app-server*`、`codex-rs/rollout` 相关代码。

### 阶段三：归纳主因、次因与优化优先级
- 目标：把已确认事实归纳成可继续推进 issue 的分析结论。
- 交付物：主因/次因排序、非主因排除项、建议优化顺序。
- 完成条件：能够明确回答“交互模式为什么慢”“不是哪些原因”“先优化哪里最值”。
- 依赖：前两阶段结论。

## 测试与验证
- 核心验证：
  - 运行非交互命令对照：`codex --version`、`codex features list`、`codex debug prompt-input hi`；
  - 运行交互命令最小观测：通过 PTY/script 包装执行 `codex --no-alt-screen`，确认启动期停留在终端控制序列阶段；
  - 对照控制序列与 TUI 初始化代码中的终端探测行为。
- 必过检查：
  - 代码路径与结论一一对应；
  - 非交互快、交互慢的对照证据成立；
  - 主因与次因区分清楚。
- 回归验证：无。
- 手动检查：
  - 逐段检查 `cli`、`tui`、`config_loader`、`app-server`、`state_db` 相关入口代码；
  - 核对抓到的控制序列是否能映射到 CPR、设备属性和颜色查询逻辑。
- 未执行的验证（如有）：
  - 未做跨终端/跨平台的系统化基准；
  - 未在代码中加入时序埋点做毫秒级分段测量；
  - 未验证所有 provider/auth 状态组合。

## 风险与缓解
- 关键风险：当前会话环境的 PTY 行为未必代表所有用户终端，可能放大或掩盖某些启动成本。
- 触发信号：同一结论只能在当前 PTY/script 环境稳定复现，无法直接推广到真实用户终端。
- 缓解措施：结论中明确区分“代码已确认的同步步骤”和“当前环境下观测到的阻塞表现”，后续建议用启动分段埋点补齐定量证据。
- 回滚/恢复方案（如需要）：无代码变更，无需回滚。

## 参考
- `codex-rs/cli/src/main.rs:697`
- `codex-rs/cli/src/main.rs:1480`
- `codex-rs/tui/src/lib.rs:654`
- `codex-rs/tui/src/lib.rs:725`
- `codex-rs/tui/src/lib.rs:813`
- `codex-rs/tui/src/lib.rs:851`
- `codex-rs/tui/src/lib.rs:921`
- `codex-rs/tui/src/lib.rs:1036`
- `codex-rs/tui/src/tui.rs:246`
- `codex-rs/tui/src/tui.rs:306`
- `codex-rs/tui/src/terminal_palette.rs:148`
- `codex-rs/tui/src/custom_terminal.rs:186`
- `codex-rs/core/src/config/mod.rs:726`
- `codex-rs/core/src/config/mod.rs:882`
- `codex-rs/core/src/config_loader/mod.rs:120`
- `codex-rs/core/src/project_doc.rs:1`
- `codex-rs/app-server-client/src/lib.rs:400`
- `codex-rs/app-server/src/in_process.rs:331`
- `codex-rs/rollout/src/state_db.rs:66`
- `codex-rs/core/src/tasks/mod.rs:661`
