# 将 `tldr` 子命令重构为 `ztldr`

> 不适用项写 `无`，不要留空。
> 只写已确认事实；若信息不清楚或需要假设，先回到对话澄清，不要把未确认内容写进计划。

## 背景
- 当前状态：CLI 仍以 `codex tldr` 暴露原生代码上下文分析命令，入口定义在 `codex-rs/cli/src/main.rs:122` 与 `codex-rs/cli/src/tldr_cmd.rs:56`；仓库内测试、帮助文案、README、MCP 接口文档、安装文档、错误提示与 native-tldr README 仍广泛引用 `codex tldr ...`。
- 触发原因：用户要求把当前 `tldr` 子命令重构为 `ztldr` 名称，并同步修改仓库内其他会使用该子命令名的地方；明确排除“上游参考仓库”。
- 预期影响：会波及 `codex-cli` 的 clap 子命令接线、解析测试、帮助/错误提示、用户文档，以及任何拼接或建议用户执行 `codex tldr ...` 的文本；同时需要避免误改 upstream/reference 资产。

## 目标
- 目标结果：用户在当前仓库内统一通过 `codex ztldr ...` 调用现有 native-tldr CLI 能力，且 MCP tool 中原名为 `tldr` 的工具也同步改为 `ztldr`；仓库内所有直接依赖这些名称的实现、测试与文档同步收敛到 `ztldr`。
- 完成定义（DoD）：
  - `codex` CLI 顶层子命令从 `tldr` 改为 `ztldr`，对应解析、帮助输出与内部 dispatch 正常工作。
  - 仓库内所有面向当前 CLI 的命令示例、提示文本、回归测试改为 `codex ztldr ...`。
  - `codex` CLI 子命令与 MCP tool 名都改为 `ztldr`；除此之外，与命令入口无关但名称相近的概念（如 `codex-native-tldr` crate、`.codex/tldr.toml`、native-tldr 作为能力名）不做无依据扩散重命名。
  - 明确排除上游参考仓库/同步参考资产，不在本轮修改范围内。
  - 受影响 crate 的局部测试与必要文档检查完成。
- 非目标：
  - 重命名 `codex-native-tldr` crate、目录名、配置文件名或 `.tldr`/`.tldrignore` 等现有 artifact 名称。
  - 进入上游参考仓库、同步基线或外部 vendored 目录做同名替换。

## 范围
- 范围内：
  - `codex-rs/cli/src/main.rs` 中的子命令声明、dispatch、相关解析测试。
  - `codex-rs/cli/src/tldr_cmd.rs` 与 `codex-rs/cli/tests/tldr.rs` 中所有 CLI 路径、用法文案与断言。
  - `codex-rs/mcp-server/src/tldr_tool.rs`、`codex-rs/native-tldr/src/mcp.rs`、`codex-rs/tools/src/tool_spec.rs`、`codex-rs/core/src/tools/rewrite/`、`codex-rs/core/src/tools/handlers/tldr.rs` 等直接声明、分发、建议或重写 MCP `tldr` tool 的实现与测试。
  - `codex-rs/native-tldr/src/daemon.rs`、`codex-rs/native-tldr/src/tool_api.rs`、`codex-rs/mcp-server/tests/suite/codex_tool.rs` 等直接向用户建议 `codex tldr ...` 或断言 MCP tool 名称的错误/提示文本与测试。
  - `codex-rs/README.md`、`codex-rs/docs/codex_mcp_interface.md`、`codex-rs/native-tldr/README.md`、`docs/install.md` 等仓库内命令示例与 MCP tool 说明文档。
- 范围外：
  - 仅描述 native-tldr 能力、crate 名或配置 schema 的文本，但未绑定 CLI 子命令路径者。
  - `.agents/plan/*`、`.agents/issues/*` 这类历史 Cadence 产物的批量回写。
  - 用户明确排除的上游参考仓库/参考实现同步目录。

## 影响
- 受影响模块：`codex-cli`、`codex-mcp-server` 为主；`codex-native-tldr`、`codex-core`、`codex-tools` 中与 CLI/MCP 名称绑定的实现、提示文本和测试会被连带更新；仓库文档同步受影响。
- 受影响接口/命令：`codex tldr ...` 将迁移为 `codex ztldr ...`；MCP tool 名 `tldr` 也将迁移为 `ztldr`；隐藏子命令 `internal-daemon` 仍由新的 `ztldr` 命名空间承载。
- 受影响数据/模式：无结构化数据 schema 变化；主要是 CLI 表面与文本契约变化。
- 受影响用户界面/行为：帮助输出、错误提示、README 示例、测试命令及任何自动建议的 CLI 调用路径统一改成 `codex ztldr ...`。

## 约束与依赖
- 约束（时间/兼容性/安全/性能/发布窗口等）：
  - 本轮只做当前仓库内的命令面重命名与配套引用修正，避免把内部 crate/API/配置名一并重命名成更大范围改动。
  - 需要显式排除 upstream/reference 资产，避免污染后续同步基线。
  - Rust 改动需遵守仓库要求：`just fmt`，并对受影响 crate 运行局部测试。
  - 若触及共享 crate（如 `core`）仅做提示文本/测试修正，局部验证通过后是否扩大到全量测试需按仓库规则再决定。
- 外部依赖（系统/人员/数据/权限等）：无。

## 实施策略
- 总体方案：先完成仓库内 `codex tldr` 与 MCP `tldr` tool 引用盘点，并区分“需要一起收敛到 `ztldr` 的 CLI/MCP surface”与“应保持原名的 native-tldr/config/artifact 名称”两类；随后在 `codex-cli` 与 MCP tool 注册/重写入口中把外部可见命名统一切换为 `ztldr`，保持底层 native-tldr 执行逻辑复用现有实现；再同步修正错误提示、测试与文档中的命令路径和工具名，最后做定向验证。
- 关键决策：
  - 优先只重命名 CLI-facing 子命令，不额外扩大到 crate 名、模块文件名或配置文件名，以控制范围。
  - `codex ztldr ...` 与 MCP `ztldr` tool 视为同一轮对外命名收敛，任何包含旧 CLI 路径或旧 MCP tool 名的提示/文档/断言都视为本轮必须同步的契约。
  - 仅收敛对外命令/工具 surface；`native-tldr` 能力名、crate 名、配置名与落盘 artifact 名保持现状。
- 明确不采用的方案（如有）：
  - 机械全仓字符串替换 `tldr -> ztldr`，导致误改 `native-tldr`、crate 名、配置键或历史参考资料。
  - 为兼容保留 `tldr` 与 `ztldr` 双入口；用户要求是“重构成 ztldr 名称”，默认以切换主入口为准。

## 阶段拆分

### 阶段一：盘点并分类受影响引用
- 目标：把仓库内与 `codex tldr` 子命令直接耦合的实现、测试、提示文本和文档分组，并标记要排除的参考资产。
- 交付物：可执行的修改清单与排除清单。
- 完成条件：后续改动范围清晰，不会误伤 `native-tldr` 等非 CLI 名称。
- 依赖：当前代码与文档检索结果。

### 阶段二：切换 CLI 子命令入口到 `ztldr`
- 目标：完成 `codex-cli` 顶层子命令声明、dispatch 与解析测试迁移，保持现有 handler 逻辑可复用。
- 交付物：更新后的 clap 子命令接线、内部 hidden daemon 调用路径与 CLI 测试。
- 完成条件：`codex ztldr ...` 解析通过，CLI 自身不再要求 `tldr` 作为顶层子命令名。
- 依赖：阶段一的引用分类。

### 阶段三：同步提示文本、文档与跨 crate 回归
- 目标：修正所有直接展示 `codex tldr ...` 或 MCP `tldr` tool 的提示、README、接口文档与跨 crate 测试断言。
- 交付物：更新后的文案、README/文档、MCP tool 名称接线与相关测试断言。
- 完成条件：仓库内面向用户的 CLI 路径一致指向 `codex ztldr ...`，MCP tool 对外名称一致为 `ztldr`，且排除项未被修改。
- 依赖：阶段二完成后的最终命令路径。

### 阶段四：格式化与定向验证
- 目标：对受影响 crate 做最小可靠验证，确认命令面重命名没有破坏解析与提示契约。
- 交付物：格式化结果与测试记录。
- 完成条件：至少完成 `codex-cli` 定向测试；若同步改到 `codex-native-tldr` / `codex-mcp-server` / `codex-core` 的测试断言，则分别跑对应最小验证入口。
- 依赖：阶段三完成。

## 测试与验证
- 核心验证：
  - 运行 `cargo nextest run -p codex-cli --test tldr`（若 nextest 可用）或等价 `cargo test -p codex-cli --test tldr`，验证 CLI 路径与 JSON/帮助契约。
  - 如修改到 `codex-native-tldr` 的提示或测试，运行 `cargo nextest run -p codex-native-tldr` 或仓库推荐的 `just native-tldr-test-fast`。
  - 如修改到 `codex-mcp-server`、`codex-tools` 或 `codex-core` 的 MCP tool 名称接线与测试断言，运行对应受影响测试，至少覆盖 `tldr_tool` / `suite::codex_tool` / tool rewrite 相关用例。
- 必过检查：
  - `just fmt`
  - 受影响 crate 的局部测试
- 回归验证：
  - 检查 `codex-rs/README.md`、`codex-rs/docs/codex_mcp_interface.md`、`codex-rs/native-tldr/README.md`、`docs/install.md` 中的命令示例已统一。
  - 抽查错误提示中的 retry hint 已改为 `codex ztldr ...`。
  - 抽查 MCP tool 列表、tool 分发与自动重写提示中的工具名已统一为 `ztldr`。
- 手动检查：
  - 本地执行 `codex ztldr --help` 与至少一个代表性子命令帮助/解析路径，确认顶层命令名显示正确。
- 未执行的验证（如有）：
  - 若仅修改到 `core` 的提示文本而未改行为，默认不主动请求全量 workspace 测试；若后续发现共享行为改动，再单独升级验证范围。

## 风险与缓解
- 关键风险：
  - 漏改隐藏在提示文本、测试字符串或 README 中的 `codex tldr ...`，导致命令面与文档不一致。
  - 漏改 MCP tool 注册名、tool rewrite 配置或测试断言，导致 CLI 已切换但 MCP 仍暴露旧名。
  - 误把 `native-tldr`、`.codex/tldr.toml` 等非命令/工具 surface 一并重命名，扩大范围并引入不必要兼容风险。
  - 解析测试更新不完整，导致 hidden `internal-daemon` 或 daemon 子命令路径断裂。
- 触发信号：
  - `rg` 仍能在非排除范围内搜索到 `codex tldr ...`。
  - CLI 测试仍以 `tldr` 作为顶层子命令才能通过。
  - MCP tools/list 或相关测试仍暴露/断言 `tldr` 作为工具名。
  - MCP/Core 错误断言仍要求 `run \`codex tldr ...\``。
- 缓解措施：
  - 先做分类检索，再只对“CLI 路径文本”实施精确替换。
  - 保持 `tldr_cmd.rs` 等内部文件名不变，减少无收益重命名。
  - 以 `rg` 复查剩余命中，并用定向测试锁住解析与提示路径。
- 回滚/恢复方案（如需要）：若 `ztldr` 切换后发现关键调用链断裂，可仅回退 CLI 顶层命令声明与相关字符串改动，再重新缩小范围推进。

## 参考
- `codex-rs/cli/src/main.rs:122`
- `codex-rs/cli/src/main.rs:757`
- `codex-rs/cli/src/main.rs:2364`
- `codex-rs/cli/src/tldr_cmd.rs:56`
- `codex-rs/cli/src/tldr_cmd.rs:3219`
- `codex-rs/cli/tests/tldr.rs:41`
- `codex-rs/mcp-server/src/tldr_tool.rs:48`
- `codex-rs/native-tldr/src/mcp.rs:13`
- `codex-rs/tools/src/tool_spec.rs:129`
- `codex-rs/core/src/tools/rewrite/auto_tldr.rs:159`
- `codex-rs/README.md:95`
- `codex-rs/docs/codex_mcp_interface.md:71`
- `codex-rs/native-tldr/README.md:27`
- `codex-rs/native-tldr/src/daemon.rs:1302`
- `codex-rs/native-tldr/src/tool_api.rs:2352`
- `codex-rs/core/src/tools/handlers/tldr.rs:2021`
- `codex-rs/mcp-server/src/tldr_tool.rs:1946`
- `docs/install.md:84`
