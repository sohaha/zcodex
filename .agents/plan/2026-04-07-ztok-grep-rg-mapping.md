# 核对 ztok grep 与 Codex CLI 内部 rg 的映射关系

> 不适用项写 `无`，不要留空。
> 只写已确认事实；若信息不清楚或需要假设，先回到对话澄清，不要把未确认内容写进计划。

## 背景
- 当前状态：`shell_command` 的内嵌 ZTOK 路由会把一部分简单 `rg` 调用改写成 `ztok grep`；`ztok grep` 运行时优先执行 `rg`，失败时再回退到系统 `grep`；`codex-cli` npm 包会随平台包一起分发 `rg`，并把对应 `vendor/.../path` 注入 `PATH`。
- 触发原因：用户希望结合 `rtk-ai/rtk` issue `#169` 判断，RTK 侧提到的 `grep` 是否应理解为 Codex CLI 内部实际依赖的 `rg` 搜索能力。
- 预期影响：给出一份可追溯的结论，明确“是否对应”以及“对应到什么程度”，避免后续在同步或对接时把 `ztok grep` 当成完全独立、或当成与原生 `rg` 完全等价的能力。

## 目标
- 目标结果：基于仓库源码与 issue 上下文，确认 `ztok grep` 和 Codex CLI 内部 `rg` 的关系边界。
- 完成定义（DoD）：
  - 明确 `rg -> ztok grep` 的 shell 重写关系。
  - 明确 `ztok grep` 的运行时后端优先使用 `rg`，且存在 `grep` 回退。
  - 明确 `codex-cli` 平台包确实携带 `rg` 并通过 `PATH` 暴露给 Rust CLI。
  - 明确这不是完整 `rg` 语义透传，列出当前可见边界。
- 非目标：
  - 不修改 `rtk`、`codex-cli` 或 `codex-rs` 的任何实现。
  - 不替上游 issue 补写超出当前公开内容的设计意图。
  - 不把当前结论扩展成完整的兼容矩阵或产品文档。

## 范围
- 范围内：
  - `codex-rs/ztok/src/rewrite.rs` 中 `rg` 到 `ztok grep` 的改写规则。
  - `codex-rs/ztok/src/grep_cmd.rs` 中 `ztok grep` 的底层执行路径。
  - `codex-cli/bin/codex.js` 与 `codex-cli/scripts/build_npm_package.py` 中 `rg` 的打包与 `PATH` 注入行为。
  - `rtk-ai/rtk` issue `#169` 与指定 comment 可见上下文。
- 范围外：
  - `rtk` 内部 grep/rg 适配代码的实现细节。
  - 复杂 shell 语法、所有 `rg` flag、所有平台安装路径的穷举验证。

## 影响
- 受影响模块：
  - `codex-rs/ztok`
  - `codex-rs/core`
  - `codex-cli`
- 受影响接口/命令：
  - `shell_command`
  - `ztok grep`
  - `rg`
- 受影响数据/模式：无。
- 受影响用户界面/行为：当模型或用户发出简单 `rg` / `grep` 搜索命令时，实际执行链路与输出形态的理解会更准确。

## 约束与依赖
- 约束（时间/兼容性/安全/性能/发布窗口等）：只能依据当前仓库和 issue 页面可确认内容得出结论；不能把 comment `+1` 推断成额外设计承诺。
- 外部依赖（系统/人员/数据/权限等）：依赖 GitHub issue `#169` 当前公开内容可访问；依赖仓库当前工作树中的 `codex-cli` / `codex-rs` 源码为准。

## 实施策略
- 总体方案：
  - 同时核对三层映射：`shell_command` 的前置改写、`ztok grep` 的实际后端、`codex-cli` 对 `rg` 可执行文件的提供方式，再与 issue 可见上下文对齐。
- 关键决策：
  - 把“对应”定义为“运行链路主要依赖同一份 `rg` 能力”，而不是“命令语义完全一比一”。
  - issue comment 仅有 `+1`，因此只把它视为附和，不视为新的技术事实来源。
- 明确不采用的方案（如有）：
  - 不根据产品印象或历史经验推断 `rtk` 侧作者意图。
  - 不把 `ztok grep` 直接表述成“就是原生 rg”，因为源码明确保留了改写边界与 fallback。

## 阶段拆分
> 可按需增减阶段；简单任务可只保留一个阶段。

### 源码核对与结论收敛
- 目标：收敛 `ztok grep` 与 `rg` 的实际关系，并标出不完全等价的边界。
- 交付物：带源码引用的结论说明。
- 完成条件：能够回答“是/不是”并同时解释“为什么”和“哪些地方不完全等价”。
- 依赖：`rewrite.rs`、`grep_cmd.rs`、`codex.js`、`build_npm_package.py`、issue `#169`。

## 测试与验证
> 只写当前仓库中真正可执行、可复现的验证入口；如果没有自动化载体，就写具体的手动检查步骤，不要写含糊表述。

- 核心验证：
  - 阅读 `codex-rs/ztok/src/rewrite.rs`，确认简单 `rg` 会被改写为 `ztok grep`。
  - 阅读 `codex-rs/ztok/src/grep_cmd.rs`，确认 `ztok grep` 优先调用 `rg`。
- 必过检查：
  - 阅读 `codex-cli/bin/codex.js`，确认平台包的 `path` 目录会被注入 `PATH`。
  - 阅读 `codex-cli/scripts/build_npm_package.py`，确认各平台包原生组件包含 `rg`，且目标目录为 `path`。
- 回归验证：
  - 阅读 `codex-rs/cli/tests/ztok.rs`，确认 `ztok grep` 的基本行为与 `-r` 兼容处理已有测试覆盖。
- 手动检查：
  - 读取 `https://github.com/rtk-ai/rtk/issues/169` 与指定 comment，确认公开上下文只说明“支持/安装 Codex”，未提供更具体 grep 语义说明。
- 未执行的验证（如有）：无运行态命令验证；当前问题可通过静态源码与公开 issue 内容回答。

## 风险与缓解
- 关键风险：把“底层依赖 `rg`”误说成“对外完全等价于原生 `rg`”，导致后续对接时遗漏不支持参数与 raw passthrough 情况。
- 触发信号：有人据此假设所有 `rg` 参数、多个 path、或复杂 shell 语法都会稳定进入 `ztok grep`。
- 缓解措施：
  - 结论中同时写清“主后端是 `rg`”与“当前只覆盖简单命令形态”。
  - 明确指出 `-r/--replace` 等 flag 有专门限制，复杂 shell 语法不会进入该链路。
- 回滚/恢复方案（如需要）：无；本阶段仅产出分析结论，不改代码。

## 参考
- `codex-rs/ztok/src/rewrite.rs:164`
- `codex-rs/ztok/src/rewrite.rs:234`
- `codex-rs/ztok/src/rewrite.rs:321`
- `codex-rs/ztok/src/grep_cmd.rs:18`
- `codex-rs/ztok/src/grep_cmd.rs:37`
- `codex-rs/ztok/src/grep_cmd.rs:52`
- `codex-rs/cli/tests/ztok.rs:922`
- `codex-rs/cli/tests/ztok.rs:939`
- `codex-rs/core/README.md:103`
- `codex-cli/bin/codex.js:59`
- `codex-cli/bin/codex.js:201`
- `codex-cli/scripts/build_npm_package.py:70`
- `codex-cli/scripts/build_npm_package.py:89`
- `https://github.com/rtk-ai/rtk/issues/169`
- `https://github.com/rtk-ai/rtk/issues/169#issuecomment-4022487793`
