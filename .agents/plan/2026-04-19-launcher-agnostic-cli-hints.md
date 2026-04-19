# Launcher 无关的 CLI hint 与子命令提示收敛

> 不适用项写 `无`，不要留空。
> 只写已确认事实；若信息不清楚或需要假设，先回到对话澄清，不要把未确认内容写进计划。

## 背景
- 当前状态：
  - `ztok` 的真实执行链路已支持通过当前运行二进制绝对路径执行 `ztok` 子命令，但 developer prompt、逻辑回显和事件文本仍把 `codex ztok` 当作固定展示值。
  - `ztldr` 的部分错误提示仍返回 `run \`codex ztldr daemon start\`` 这类硬编码命令。
  - `zmemory` 的建议命令与大量文档示例仍使用 `codex zmemory ...` 作为固定文案。
  - 当前仓库 shell 运行环境支持通过 `shell_environment_policy` 注入环境变量，但尚无内建的“当前 Codex launcher 路径”变量。
- 触发原因：
  - 用户将启动二进制从 `codex` 改名为 `z` 后，agent 仍会按提示词或 hint 原样执行/回显 `codex ztok`、`codex ztldr`、`codex zmemory`，导致命令不存在。
  - 用户明确要求优先考虑环境变量方案，以避免因路径或 launcher 名变化频繁改动提示词文本，进而影响缓存命中。
- 预期影响：
  - agent、CLI hint、建议命令和相关文档应对 launcher 名称解耦；同一套提示文本在 `codex`、`z` 或其他重命名 launcher 下都能成立。

## 目标
- 目标结果：
  - 提供一个稳定的 launcher 绝对路径环境变量，供 prompt、hint 和建议命令复用。
  - 提供一套共享的 launcher 文本渲染边界，使用户可见 hint 不再写死 `codex`，同时不直接暴露机器绝对路径。
  - 将 `ztok`、`ztldr`、`zmemory` 的高频用户可见命令提示从写死 `codex ...` 收敛为 launcher 无关的表达。
  - 保持现有子命令名、工具协议和执行主链不变，仅修复命令提示与回显耦合。
- 完成定义（DoD）：
  - shell/agent 可消费的运行环境里存在稳定的 launcher 绝对路径变量。
  - 面向用户或测试快照的展示文本改为共享的 launcher 无关表达，而不是直接内联绝对路径。
  - `ztok` 的 Embedded ZTOK developer instructions 不再要求字面执行 `codex ztok ...`。
  - `ztldr` 的 daemon retry hint 不再写死 `codex ztldr ...`。
  - `zmemory` 的 alias review suggestion command 不再写死 `codex zmemory ...`。
  - 受影响测试与快照更新到新的 launcher 无关表达，且不泄露机器相关绝对路径。
- 非目标：
  - 无意更改 `ztok`、`ztldr`、`zmemory` 子命令名。
  - 无意引入新的 PATH 搜索或 alias 自动发现机制来替代现有 `codex_self_exe`。
  - 无意在本阶段扩展 `zmemory` 工具 API 或 `native-tldr` 功能。

## 范围
- 范围内：
  - `core` 中与 Embedded ZTOK 提示、shell rewrite 逻辑展示、事件回显相关的代码与测试。
  - `cli` 中 `ztldr` retry hint 相关代码与测试。
  - `zmemory` 中 suggestion command 与相关测试。
  - 与上述行为直接对应的文档/快照更新。
- 范围外：
  - 与 launcher 名称无关的普通 `codex` 文档示例批量替换。
  - `zmemory`、`ztldr`、`ztok` 的新功能开发。
  - 不相关子命令或运行面的帮助文案重写。

## 影响
- 受影响模块：
  - `codex-rs/core`
  - `codex-rs/cli`
  - `codex-rs/zmemory`
- 受影响接口/命令：
  - Embedded ZTOK developer instructions
  - shell_command 路由后的展示命令与事件输出
  - `ztldr` daemon retry hint
  - `zmemory` alias review recommendation command
- 受影响数据/模式：
  - shell 运行环境中的内建环境变量集合
  - 测试快照与提示模板文本
- 受影响用户界面/行为：
  - agent 选择显式执行子命令时的命令文本
  - 用户在错误提示、建议命令、README 示例中看到的 launcher 表达

## 约束与依赖
- 约束（时间/兼容性/安全/性能/发布窗口等）：
  - 提示词文本应保持稳定，不应把机器相关绝对路径直接写入模板内容。
  - 用户可见输出不能泄露运行机器的绝对路径；需要保留“展示稳定、执行使用绝对路径”的边界。
  - 计划只覆盖已确认的高频故障面，不做无根据的全仓批量替换。
- 外部依赖（系统/人员/数据/权限等）：
  - 无外部人员或系统依赖。

## 实施策略
- 总体方案：
  - 在运行时统一注入一个稳定的 launcher 绝对路径环境变量，作为显式子命令提示与共享渲染逻辑的唯一运行时事实源。
  - `ztok` 相关提示与回显改为“agent/prompt 使用稳定变量或由共享 helper 提供的 launcher 无关表达，实际执行继续用 `codex_self_exe`”。
  - `ztldr` 与 `zmemory` 的错误提示、建议命令和必要文档改为复用同一套 launcher 文本渲染规则，避免各模块各自写死 `codex`。
- 关键决策：
  - 采用内建环境变量承载 launcher 绝对路径，而不是把绝对路径直接渲染进 prompt 模板。
  - 用户可见 hint、suggestion 与快照文本复用共享渲染边界，不直接输出机器绝对路径，也不再把 `codex` 当作固定 launcher 名。
  - 优先修复会被 agent 或用户直接执行的高频提示面，再同步对应测试与文档。
  - 对外展示保持稳定占位表达，不把真实绝对路径直接暴露到事件流、测试快照或用户提示中。
- 明确不采用的方案（如有）：
  - 不采用继续写死 `codex ...` 并要求用户自行保留 alias 的方案。
  - 不采用仅更新 README、不修运行时 hint 与 prompt 的方案。
  - 不采用每次根据 launcher 名动态改写整段 developer instructions 文本的方案。

## 阶段拆分
> 可按需增减阶段；简单任务可只保留一个阶段。

### 阶段一：统一 launcher 运行时事实源
- 目标：
  - 明确并落地 launcher 绝对路径环境变量的注入点、命名边界与共享文本渲染规则。
- 交付物：
  - 运行时环境变量注入实现
  - 变量命名、使用边界与共享文本渲染说明
- 完成条件：
  - shell 相关执行环境可读取稳定 launcher 绝对路径变量，且不要求提示模板跟随路径变化而改动。
  - 后续 `ztok`、`ztldr`、`zmemory` 可通过共享规则生成 launcher 无关的展示文本。
- 依赖：
  - 现有 `codex_self_exe` 与 shell environment 构造链路。

### 阶段二：收敛 `ztok` 的提示、展示与事件输出
- 目标：
  - 消除 `ztok` 高频路径中写死 `codex ztok` 的提示与展示耦合。
- 交付物：
  - Embedded ZTOK 模板更新
  - `shell.rs` / `tools/events.rs` 的展示逻辑调整
  - 对应测试与快照更新
- 完成条件：
  - agent 不再因模板建议去执行字面 `codex ztok ...`。
  - 展示层保持 launcher 无关且不泄露绝对路径。
- 依赖：
  - 阶段一提供的 launcher 运行时事实源。

### 阶段三：收敛 `ztldr`、`zmemory` 的 hint、suggestion 与相关文档
- 目标：
  - 修复 `ztldr` retry hint 和 `zmemory` suggestion command 的 launcher 写死问题，并同步直接依赖这些文案的测试/文档。
- 交付物：
  - `ztldr` retry hint 更新
  - `zmemory` suggestion command 更新
  - 直接受影响的 README、测试与快照更新
- 完成条件：
  - `ztldr` 和 `zmemory` 不再向用户或 agent 返回写死 `codex ...` 的可执行命令提示。
- 依赖：
  - 阶段一提供的 launcher 运行时事实源。

## 测试与验证
> 只写当前仓库中真正可执行、可复现的验证入口；如果没有自动化载体，就写具体的手动检查步骤，不要写含糊表述。

- 核心验证：
  - `cd /workspace/codex-rs && cargo test -p codex-core build_ztok_tool_developer_instructions_renders_embedded_template --lib`
  - `cd /workspace/codex-rs && cargo test -p codex-core shell_command_handler_routes_git_status_via_ztok_but_keeps_approval_raw --lib`
  - `cd /workspace/codex-rs && cargo test -p codex-core shell_emitter_never_exposes_absolute_ztok_exec_path --lib`
  - `cd /workspace/codex-rs && cargo test -p codex-cli tldr`
  - `cd /workspace/codex-rs && cargo test -p codex-cli zmemory`
- 必过检查：
  - `cd /workspace/codex-rs && cargo check -p codex-core --lib`
  - `cd /workspace/codex-rs && cargo check -p codex-cli --bin codex`
  - `cd /workspace/codex-rs && just fmt`
- 回归验证：
  - 检查 `pending_input` / 初始上下文相关快照中 Embedded ZTOK 文本是否已统一更新。
  - 检查 `zmemory` alias review 建议命令与 `ztldr` daemon unavailable hint 是否都使用同一套 launcher 无关表达。
- 手动检查：
  - 使用非 `codex` 名称启动 CLI，确认 agent 提示、retry hint、suggestion command 不再要求系统里必须存在 `codex` 可执行名。
  - 检查用户可见输出中不出现机器相关 launcher 绝对路径。
- 未执行的验证（如有）：
  - 规划阶段未执行任何实现验证；上述入口留待执行阶段落地。

## 风险与缓解
- 关键风险：
  - 只修 prompt 模板，不修展示事件或 suggestion command，仍会在其他用户可见链路残留 `codex ...`。
  - 直接把绝对路径渲染进提示或事件输出，导致输出不稳定、污染快照，并暴露机器路径。
  - 变更 `ztok` 展示字符串后遗漏快照与单测，造成共享层回归。
- 触发信号：
  - 仍有 agent/测试日志出现 `codex ztok`、`codex ztldr`、`codex zmemory` 的字面可执行提示。
  - 快照或事件流中出现 `/tmp/.../codex`、`/workspace/.../z` 之类绝对路径。
- 缓解措施：
  - 以“运行时环境变量承载真实路径、用户可见文本保持稳定占位表达”为统一规则。
  - 先做定向检索与目标测试，再决定是否扩大文档替换范围。
  - 把 `ztok`、`ztldr`、`zmemory` 三条链路一起审计，避免只修单点。
- 回滚/恢复方案（如需要）：
  - 若统一变量方案在共享层引入不可接受回归，先回退到当前 `codex_self_exe` 执行链路，仅撤销提示与展示变更，保持执行主链可用。

## 参考
- `/workspace/codex-rs/core/src/tools/handlers/shell.rs:245`
- `/workspace/codex-rs/core/src/tools/handlers/shell.rs:255`
- `/workspace/codex-rs/core/templates/compact/ztok.md:1`
- `/workspace/codex-rs/core/src/memories/prompts_tests.rs:165`
- `/workspace/codex-rs/core/src/tools/events.rs:600`
- `/workspace/codex-rs/cli/src/tldr_cmd.rs:3314`
- `/workspace/codex-rs/zmemory/src/system_views.rs:585`
- `/workspace/codex-rs/config/src/types.rs:873`
- `/workspace/codex-rs/config/src/shell_environment.rs:99`
