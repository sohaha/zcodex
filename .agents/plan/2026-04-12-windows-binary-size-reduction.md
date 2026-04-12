# Windows 主二进制瘦身

## 背景
- 当前状态：已完成 Windows x64 构建与体积归因；本地产物 `dist/codex-x86_64-pc-windows-msvc.exe` 为 `190.04 MiB`，官方稳定版 `rust-v0.120.0` 为 `175.29 MiB`，差额主要落在 `.text` `+11.22 MiB` 与 `.rdata` `+3.27 MiB`。
- 触发原因：用户要求解释 Windows 版本为何比官方大，并基于当前源码继续做符号级体积归因与后续瘦身规划。
- 预期影响：明确 Windows 主 `codex.exe` 的瘦身优先级、实施边界与验证方式，避免后续把时间浪费在低收益的链接参数微调上。

## 目标
- 目标结果：形成一份可直接进入 issue 拆分与执行的 Windows 主二进制瘦身计划，优先减少主 `codex.exe` 的 `.text` 与 `.rdata` 体积。
- 完成定义（DoD）：计划明确根因、优先级、阶段拆分、验证入口、风险与非目标；后续工程师无需再重新做一轮归因就能进入 `cadence-issue-generation`。
- 非目标：
  - 不在本计划阶段直接修改 Rust 源码或 Cargo 配置；
  - 不把目标扩大为“总安装包体积最小化”；
  - 不以单个函数或单个 tree-sitter parser 微优化作为主路径。

## 范围
- 范围内：
  - `codex-rs/cli` 的主二进制聚合边界；
  - `codex-rs/core` 的重依赖边界；
  - `codex-rs/app-server-protocol` 运行时代码与代码生成逻辑的拆分方案；
  - `codex-rs/code-mode` / `v8` 与 `codex-rs/native-tldr` 的主二进制脱钩策略；
  - Windows 体积回归的构建与节区验证方法。
- 范围外：
  - 与本任务无关的 TUI 功能改动；
  - 官方发布流程、安装脚本或 CDN 分发策略调整；
  - 纯粹为了“看起来更小”而牺牲现有功能可用性的删功能方案。

## 影响
- 受影响模块：
  - `codex-rs/cli`
  - `codex-rs/core`
  - `codex-rs/app-server`
  - `codex-rs/app-server-protocol`
  - `codex-rs/code-mode`
  - `codex-rs/native-tldr`
  - `codex-rs/mcp-server`
  - `codex-rs/zmemory`
- 受影响接口/命令：
  - `codex` 默认入口
  - `codex app-server`
  - `codex mcp-server`
  - `codex ztldr`
  - `codex zmemory`
  - 相关代码生成命令（`generate-ts` / `generate-json-schema`）
- 受影响数据/模式：Windows release 构建产物、分析构建产物、`.text` / `.rdata` 节区归因报告。
- 受影响用户界面/行为：优先影响二进制组织方式与子命令装配方式；若采用 launcher + 子二进制，用户可见命令名保持不变。

## 约束与依赖
- 约束（时间/兼容性/安全/性能/发布窗口等）：
  - 当前已确认主因是架构级静态聚合，而非 debug 符号残留；后续应优先做依赖边界拆分，而不是先抠链接参数。
  - 必须保持现有 CLI 子命令语义稳定，不能因瘦身破坏 `codex` 现有命令入口。
  - 当前工作区存在无关脏改动 `codex-rs/tui/src/app.rs` 与 `codex-rs/tui/src/lib.rs`，执行阶段需避免误改或误提交流程外文件。
  - Windows 体积验收必须以实际 `win amd64` 构建产物与节区变化为准，不能只看 Cargo 依赖树。
- 外部依赖（系统/人员/数据/权限等）：
  - `mise` / `cargo xwin` Windows 交叉编译链路可用；
  - `llvm-objdump` 可用于 PE/COFF 节区检查；
  - 既有体积归因中间产物位于 `/tmp/codex-analysis-attribution.txt` 等临时文件，可作为 issue 生成与执行阶段的事实基线。

## 实施策略
- 总体方案：先冻结当前 Windows 体积基线与热点分桶，再按“主二进制拆分收益 > 重依赖脱钩收益 > 运行时代码与生成时代码分层收益 > 局部微优化收益”的顺序推进；优先把不需要常驻主 `codex.exe` 的运行面与重能力从主程序里剥离。
- 关键决策：
  - 以“主 `codex.exe` 瘦身”作为首要成功指标，而不是先追求总安装包体积同步下降；
  - 先拆二进制/运行面边界，再处理 crate 内 feature 或 derive gate；
  - `v8`、`native-tldr`、app-server schema/export 逻辑应视为高价值脱钩对象；
  - `codex-core` 不再继续承担新的重聚合职责，后续执行阶段要优先把运行面专属能力移出。
- 明确不采用的方案（如有）：
  - 不把本轮主路径放在 `strip`、LTO、链接器 flags 微调；
  - 不先做零散函数级优化；
  - 不以删除现有对外命令为代价换取表面体积下降。

## 阶段拆分
### 阶段一：冻结基线并落定拆分目标
- 目标：把当前已确认的体积数据、依赖链与优先级沉淀为可执行事实，确定第一批要从主 `codex.exe` 脱钩的对象。
- 交付物：
  - Windows 体积基线说明；
  - 第一批拆分对象列表（至少包含 app-server codegen、`v8`/code-mode、`native-tldr`）；
  - 主二进制成功指标与验收方式。
- 完成条件：后续 issue 可以直接围绕明确对象立项，不再需要重复归因。
- 依赖：现有构建与归因产物。

### 阶段二：主入口与运行面解耦设计
- 目标：给出 `codex` 主入口与 `app-server` / `mcp-server` / `ztldr` / `zmemory` 等运行面的装配重构方案，优先考虑 launcher + 子二进制或等价拆分。
- 交付物：
  - 运行面拆分设计；
  - 兼容性约束；
  - 最小迁移顺序。
- 完成条件：能明确哪些子命令不再需要静态链接进主 `codex.exe`。
- 依赖：阶段一冻结的热点优先级。

### 阶段三：重依赖脱钩设计
- 目标：为 `v8`/`codex-code-mode`、`native-tldr`、`app-server-protocol` 代码生成逻辑制定与主运行时脱钩的实现路线。
- 交付物：
  - `v8` 脱钩方案；
  - `native-tldr` 脱钩方案；
  - `app-server-protocol` 运行时/生成时拆层方案。
- 完成条件：每个热点都有清晰的 crate / binary / feature 级落点与依赖迁移方向。
- 依赖：阶段二的二进制边界决策。

### 阶段四：Windows 体积回归验证
- 目标：为后续每个 issue 建立统一的构建、节区、crate 归因复测方法。
- 交付物：
  - release 构建复测命令；
  - 分析构建复测命令；
  - 通过/失败判定标准。
- 完成条件：执行阶段能逐项验证每次拆分是否真正降低主 `codex.exe` 的 `.text` / `.rdata`。
- 依赖：前述设计阶段输出的目标边界。

## 测试与验证
- 核心验证：
  - `CODEX_CARGO_TARGET_DISABLE=1 RUSTC_WRAPPER= mise run build ubuntu-win-amd64`
  - `llvm-objdump -h dist/codex-x86_64-pc-windows-msvc.exe`
- 必过检查：
  - 主 `codex.exe` 体积下降；
  - `.text` 与 `.rdata` 节区变化与拆分目标一致；
  - 受影响子命令仍可启动。
- 回归验证：
  - `RUSTC_WRAPPER= CARGO_TARGET_DIR=/workspace/.cargo-target/analysis-win-no-lto CARGO_PROFILE_RELEASE_LTO=off CARGO_PROFILE_RELEASE_STRIP=none cargo xwin build -p codex-cli --bin codex --release --target x86_64-pc-windows-msvc -j 25`
  - 对比 `/tmp/codex-analysis-attribution.txt` 的 crate 级热点变化；
  - 必要时重新检查 `cargo tree -p codex-cli -i <crate> --target x86_64-pc-windows-msvc -e normal`
- 手动检查：
  - 启动 `codex` 默认交互模式；
  - 启动 `codex app-server`；
  - 启动 `codex ztldr` / `codex zmemory` 相关子命令；
  - 确认命令入口与帮助文本未因拆分破坏。
- 未执行的验证（如有）：
  - 本计划阶段不执行代码修改后的回归构建；
  - 官方安装包总大小与分发成本变化不在本计划阶段验证。

## 风险与缓解
- 关键风险：
  - 仅做 crate feature 化但仍保留同一主二进制，最终对主 `codex.exe` 体积收益不足；
  - `codex-core` 与各运行面的耦合深，执行阶段可能出现迁移面超出预期；
  - 子二进制拆分若处理不当，可能破坏现有命令兼容性或安装分发语义。
- 触发信号：
  - 主 exe 体积基本不降，但 crate 结构复杂度显著上升；
  - `cargo tree` 显示 `v8`、`native-tldr`、schema/export 逻辑仍经 `codex-cli` 主路径硬引入；
  - 子命令启动方式或帮助输出出现不兼容变化。
- 缓解措施：
  - 每个 issue 都要求“构建后再量体积”，不接受只改依赖图不复测；
  - 先拆边界最清晰的对象，再处理 `codex-core` 内部深耦合；
  - 保持命令名与用户入口不变，把兼容层留在 launcher 层。
- 回滚/恢复方案（如需要）：
  - 若某次拆分导致命令兼容性回归，则回退该 issue 的二进制装配改动，保留已验证的体积基线与归因数据，重新调整边界后再推进。

## 参考
- `codex-rs/cli/Cargo.toml`
- `codex-rs/core/Cargo.toml`
- `codex-rs/app-server-protocol/Cargo.toml`
- `codex-rs/code-mode/Cargo.toml`
- `codex-rs/native-tldr/Cargo.toml`
- `codex-rs/cli/src/main.rs`
- `/tmp/codex-analysis-attribution.txt`
- `/tmp/codex-analysis-section-contribs.txt`
