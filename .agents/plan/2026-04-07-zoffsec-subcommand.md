# 为 Codex 添加 zoffsec 子命令

> 不适用项写 `无`，不要留空。
> 只写已确认事实；若信息不清楚或需要假设，先回到对话澄清，不要把未确认内容写进计划。

## 背景
- 当前状态：`codex` 现有顶层子命令集中在 `codex-rs/cli/src/main.rs` 维护；配置 profile 已支持 `model_instructions_file`；resume 会继续写回同一份 rollout 会话文件。
- 触发原因：用户希望参考 `ryfineZ/codex-session-patcher` 的工作流，在本仓库内新增 `zoffsec` 启动入口，把 offsec 指令注入与拒绝后会话清理能力纳入原生 CLI，但不希望引入单独的 install 步骤或 Web UI。
- 预期影响：用户可直接通过 `codex zoffsec` 启动带 offsec 上下文的 Codex 会话，并可通过 CLI 参数切换多套内置模板；若某个会话被标记为 zoffsec 会话，恢复时可触发针对该会话的清理流程，再继续使用现有 resume 路径。

## 目标
- 目标结果：提供一个原生 `codex zoffsec` 命令族，直接以可切换的内置 offsec 指令模板启动 Codex，并提供面向 Codex rollout 的拒绝清理与 zoffsec 专属恢复能力。
- 完成定义（DoD）：`codex` 帮助输出出现 `zoffsec` 命令族；`codex zoffsec` 能在不要求用户预先 install 的前提下注入所选内置模板对应的指令启动会话；CLI 可通过参数切换多套受控内置模板；zoffsec 会话可被识别；能扫描并清理 Codex rollout 中的拒绝消息与相关推理/冗余副本；`codex zoffsec resume` / `codex zoffsec r` 可复用现有会话选择/恢复体验，并在命中 zoffsec 会话时执行显式的 clean-then-resume 流程；新增命令有对应测试与文档。
- 非目标：无 Web UI；无 Claude Code/OpenCode 兼容层；不实现外部仓库中的 AI 改写、用户自定义模板系统与多平台安装流程；不做静默、不可观察的自动历史篡改。

## 范围
- 范围内：`codex zoffsec` CLI 入口设计；多套内置 offsec prompt 资产与模板选择参数；会话启动时的指令注入；zoffsec 会话标记方案；Codex rollout JSONL 拒绝检测与清理；`codex zoffsec resume` / `codex zoffsec r` 的恢复与会话选择集成；帮助文档与测试。
- 范围外：外部仓库的 Web 前后端；非 Codex 平台支持；远程服务端接口变更；把 zoffsec 模式自动注入所有普通 `codex` 会话；首批单独新增交互式会话选择器。

## 影响
- 受影响模块：`codex-rs/cli`、rollout 会话读取/写回逻辑、会话元数据/启动参数链路、相关测试与文档。
- 受影响接口/命令：`codex` 顶层子命令列表；新增 `codex zoffsec`、`codex zoffsec resume`、`codex zoffsec r`、`codex zoffsec clean`；普通 `codex resume` 不承载 zoffsec 专属行为。
- 受影响数据/模式：offsec prompt 资产文件与模板标识；`$CODEX_HOME/sessions/**/*.jsonl` rollout 文件内容；zoffsec 会话标记所依赖的 session metadata 或等效会话上下文。
- 受影响用户界面/行为：CLI 帮助、`codex zoffsec --template <name>` 启动行为、会话清理前后的 resume 体验。

## 约束与依赖
- 约束（时间/兼容性/安全/性能/发布窗口等）：必须遵守仓库规则，避免无必要地向 `codex-core` 继续堆逻辑；任何“恢复时清理”都必须显式、可观察、可关闭，不能做隐藏 fallback；当前工作区存在无关快照改动，后续实现与提交必须避开这些现有变更。
- 外部依赖（系统/人员/数据/权限等）：依赖现有 CLI 启动链路、rollout 文件格式与 resume 行为；参考外部仓库 `ryfineZ/codex-session-patcher` 的工作流与会话清理策略，但不直接引入其 Python/Web 运行时依赖。

## 实施策略
- 总体方案：在 `cli` 增加 `zoffsec` 入口，作为启动 Codex 的 wrapper，在启动时按所选内置模板直接注入 offsec 指令而不是要求用户先 install profile；将 Codex rollout 的拒绝检测与清理逻辑放在比 `codex-core` 更合适的边界（优先评估 `rollout` 或新的轻量 crate / 模块），供 `zoffsec clean` 与 zoffsec 会话恢复路径调用；继续复用现有 `codex resume`，不额外发明新的恢复命令。
- 关键决策：
  - 优先把 `zoffsec` 设计成“启动入口 + 清理能力”，而不是依赖 `install` 的配置命令族。
  - `codex zoffsec` 启动时直接注入所选内置模板对应的 offsec 指令；具体实现优先评估“进程内 override / 临时文件 + override”，避免要求用户预先写入全局 `config.toml`。
  - 首批采用受控内置模板集，并通过明确 CLI 参数切换；不把模板管理扩展成用户可写系统。
  - 必须为 zoffsec 会话设计稳定标记，供后续 resume 路径识别“这是 zoffsec 会话”；标记优先落在现有 session metadata 可承载的字段或等效会话上下文中。
  - `clean` 仍由 CLI 触发，不依赖 Web UI；保留显式入口（如 `codex zoffsec clean --last`）。
  - zoffsec 专属恢复能力收敛在 `codex zoffsec resume` / `codex zoffsec r`，优先复用现有 resume 的会话选择/恢复体验；普通 `codex resume` 不承载 zoffsec 专属逻辑。
  - `codex zoffsec resume` 选中目标会话后，若命中 zoffsec 会话，则执行显式、可见、可关闭的 clean-then-resume 流程；不做静默自动修改。
  - `preview` 能力优先折叠为 `clean --dry-run`，不单独增加 `preview` 子命令。
  - `clean` 首批 flags 以最小够用为主：`--dry-run`、`--no-backup`、`--keep-reasoning`、`--replacement <text>`；不默认自动 resume。
  - 参考外部工具的“改主消息 + 改 resume 冗余副本 + 清推理”思路，但实现要对齐本仓库真实 rollout 格式与测试夹具。
  - `codex zoffsec clean` 保持“只清理不恢复”的单一职责。
- 推荐目录/模块边界：
  - `codex-rs/cli/src/zoffsec_cmd.rs`：`zoffsec` 入口与参数解析
  - `codex-rs/cli/src/zoffsec_config.rs`：内置模板清单、模板解析与少量 prompt/临时资源管理；不以前置 install 为中心
  - `codex-rs/rollout/src/patch.rs`（或同等新模块）：rollout 查找、预览、备份、清理与原子写回
- rollout 清理实现优先采用“逐行 JSONL + `serde_json::Value` 定点修改”，避免整文件强类型回写导致未知字段丢失。
- 明确不采用的方案（如有）：不直接内嵌外部工具的 Python 实现；不先做 Web UI 再回填 CLI；不把整套清理逻辑塞进 `codex-core` 顶层会话主流程里；不做静默自动 clean；不把 zoffsec 专属恢复逻辑混入普通 `codex resume`。

## 阶段拆分
### 阶段一：zoffsec 启动入口与会话标记
- 目标：设计并接入 `codex zoffsec` 启动入口，打通启动时指令注入与 zoffsec 会话标记。
- 交付物：CLI 入口解析与帮助文本；多套内置 offsec prompt 模板资产；模板选择参数；启动注入实现；zoffsec 会话标记方案；命令级测试。
- 完成条件：`codex zoffsec` 可在不要求 install 的前提下，按所选模板启动带 offsec 指令的会话，且新会话能被后续恢复链路识别为 zoffsec 会话。
- 依赖：现有 CLI 启动链路、base instructions / model instructions override 能力、session metadata 持久化行为。

### 阶段二：会话清理能力
- 目标：为 Codex rollout 提供拒绝检测、替换与 reasoning/event 副本清理。
- 交付物：rollout 扫描/清理实现；`zoffsec clean` 命令；基于 `--dry-run` 的预览能力；必要的备份策略；对应测试。
- 完成条件：可针对目标 rollout 文件完成拒绝替换与推理删除，并保持 resume 可继续使用同一会话。
- 依赖：对现有 rollout JSONL 结构、`response_item` / `event_msg` / `reasoning` 的确认。

### 阶段三：zoffsec 专属恢复链路、验证与文档
- 目标：补齐 `codex zoffsec resume` / `codex zoffsec r` 的恢复链路、会话选择集成、用户文档与回归验证。
- 交付物：zoffsec 专属恢复实现；更新后的 CLI/README 文档；命令与集成测试；必要的快照或文本断言。
- 完成条件：用户能按文档完成 `codex zoffsec -> 遇拒绝 -> codex zoffsec clean` 或 `codex zoffsec resume` 工作流，且恢复前的 clean 行为对用户可见。
- 依赖：前两阶段命令面、会话标记与清理能力稳定。

## issue 拆分建议
- issue A：`codex zoffsec` 启动入口与会话标记
  - 聚焦启动时指令注入、多套内置模板切换、offsec prompt 资产与会话标记
  - 风险面主要是 CLI 启动链路与会话身份识别
- issue B：`zoffsec clean`
  - 聚焦 rollout 目标选择、拒绝检测、备份、预览、写回
  - 风险面主要是会话历史修改与 resume 兼容
- issue C：`codex zoffsec resume` / `codex zoffsec r`
  - 聚焦复用现有 resume 选择器/恢复体验，以及 zoffsec 会话的 clean-then-resume 集成
  - 风险面主要是恢复行为改变与“不能静默修改历史”的规则约束
- 拆分原则：启动链路、会话历史修改、zoffsec 专属恢复集成分开评审、分开验证，避免高风险逻辑耦合在同一实现 issue 中。

## 测试与验证
- 核心验证：命令测试覆盖 `codex zoffsec` 启动、模板切换、zoffsec 会话标记、`clean`、`zoffsec resume` / `zoffsec r`；验证不同模板启动时注入内容与模板标识正确；验证启动注入后的会话确实携带 zoffsec 标记；验证 rollout 被清理后仍可配合恢复流程；验证 `zoffsec resume` 的 clean-then-resume 行为是显式且可关闭的。
- 必过检查：`just fmt`；受影响 crate 的针对性测试；新增 CLI/配置/rollout 测试全部通过。
- 回归验证：现有指令注入/`model_instructions_file` 生效测试、已有 resume / rollout 测试不回退。
- 手动检查：在隔离 `CODEX_HOME` 下分别执行 `codex zoffsec --template <name>`，确认不同模板对应的指令与模板标识正确写入；准备带拒绝内容的 rollout 样本执行 `codex zoffsec clean --dry-run` 预览，再执行真实清理；使用 `codex zoffsec resume` / `codex zoffsec r` 验证会话选择后可按预期执行 clean-then-resume。
- 未执行的验证（如有）：无。

## 风险与缓解
- 关键风险：实际 rollout 结构比当前测试认知更复杂，导致只改主消息不足以支撑 resume；zoffsec 会话标记若设计不稳，恢复链路无法可靠识别；模板切换若与既有 instructions override 冲突，可能注入错误模板或污染普通会话；`zoffsec resume` 若无法平滑复用现有选择器体验，用户心智会割裂；恢复时自动 clean 若做成隐式行为，会违反当前仓库的显式/可观察约束；若默认目标选择过于隐式，容易误改错误会话；工作区现有无关改动导致误提交。
- 触发信号：清理后恢复仍显示旧拒绝；恢复路径无法区分普通会话与 zoffsec 会话；指定模板后实际注入内容不匹配；`zoffsec resume` 和现有 resume 表现明显分叉；恢复时历史被静默修改；`clean` 在未显式指定目标时误操作非预期会话；测试或提交包含无关快照文件。
- 缓解措施：实现前先以当前仓库 rollout 夹具与测试为准补齐结构认知；优先选择稳定、可测试的 zoffsec 会话标记；把模板枚举与模板内容集中在受控位置并补齐命令测试；`zoffsec resume` 尽量复用现有 resume 选择/恢复链路；恢复时清理触发必须打印检测结果并提供显式确认或显式开关；`clean` 默认显式等价于 `--last` 并打印目标/备份信息，保留 `--dry-run` 与备份；开发中持续检查 `git status --short`，仅 stage 当前任务相关文件。
- 回滚/恢复方案（如需要）：模板注入方案应支持直接移除新增 `zoffsec` 入口实现；会话清理命令需保留可恢复策略或至少在计划中要求先备份原 rollout。

## 参考
- `codex-rs/cli/src/main.rs`
- `codex-rs/core/src/config/profile.rs:40`
- `codex-rs/core/src/config/mod.rs:2162`
- `codex-rs/core/src/config/mod.rs:2597`
- `codex-rs/core/src/config/edit.rs:843`
- `codex-rs/core/tests/suite/cli_stream.rs:235`
- `codex-rs/core/tests/suite/cli_stream.rs:469`
- `codex-rs/rollout/src/list.rs:1130`
- `https://github.com/ryfineZ/codex-session-patcher/blob/main/README.md`
- `https://github.com/ryfineZ/codex-session-patcher/blob/main/codex_session_patcher/core/formats.py`
