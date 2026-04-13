# ztok find 自动重写回退优化

> 不适用项写 `无`，不要留空。
> 只写已确认事实；若信息不清楚或需要假设，先回到对话澄清，不要把未确认内容写进计划。

## 背景
- 当前状态：shell 命令重写层会把普通 `find ...` 直接改写为 `ztok find ...`；而 `ztok find` 运行时会拒绝 `-o`、`-or`、`-a`、`-not`、`-exec` 等不支持参数并报错。
- 触发原因：用户执行带复合谓词的 `find` 时，被自动改写为 `ztok find` 后失败，改变了原生命令本可成功执行的语义。
- 预期影响：仅在 `ztok find` 已支持的参数子集内才自动重写；遇到已知不支持参数时直接透传到系统 `find`。

## 目标
- 目标结果：让 shell 自动重写对 `find` 采用“支持才改写，不支持即 passthrough”的策略。
- 完成定义（DoD）：
  - `find` 不再走无条件直通重写。
  - rewrite 层能基于现有 `UNSUPPORTED_FIND_FLAGS` 判定是否放弃改写。
  - 为“命中不支持参数不改写”和“简单 `find` 仍改写”补充测试。
  - 相关 Rust 格式化与受影响 crate 的局部测试通过。
- 非目标：
  - 扩展 `ztok find` 对复合谓词、动作参数或原生 `find` 全语法的支持。
  - 修改其他命令（如 `rg`、`grep`）的重写策略。

## 范围
- 范围内：
  - `codex-rs/ztok` 内 shell rewrite 层对 `find` 的路由判定。
  - `ztok find` 不支持参数集合的复用方式。
  - 对应单元测试或重写测试。
- 范围外：
  - `ztok find` 输出格式、搜索结果展示或能力扩展。
  - shell 元字符检测策略整体重构。
  - 非 `find` 命令的自动重写规则调整。

## 影响
- 受影响模块：`codex-rs/ztok/src/rewrite.rs`、`codex-rs/ztok/src/find_cmd.rs`，以及对应测试。
- 受影响接口/命令：shell 自动重写入口对 `find` 的处理；`ztok find` 子命令本身的 CLI 行为预期不变。
- 受影响数据/模式：无。
- 受影响用户界面/行为：用户执行复杂 `find` 时将保留系统 `find` 原生行为，不再被错误拦截改写。

## 约束与依赖
- 约束（时间/兼容性/安全/性能/发布窗口等）：
  - 必须保持显式 `codex ztok find ...` / `ztok find ...` 的现有严格报错行为，不把运行时错误静默吞掉。
  - 优先复用已有 `UNSUPPORTED_FIND_FLAGS`，避免 rewrite 层和执行层维护两套能力边界。
  - 仅做最小必要改动，不扩大到 `find` 语法增强。
- 外部依赖（系统/人员/数据/权限等）：无。

## 实施策略
- 总体方案：为 `find` 增加专用 `rewrite_find(...)` 判定路径；rewrite 阶段若发现参数命中现有不支持集合，则返回 `None` 走 passthrough，否则才改写为 `ztok find ...`。
- 关键决策：
  - 不再让 `find` 走 `DIRECT_PREFIXES` 的无条件改写。
  - 复用 `find_cmd.rs` 中的不支持参数事实源，而不是复制常量。
  - 保留 shell 元字符的既有提前 passthrough 逻辑，不额外扩展本轮判断面。
- 明确不采用的方案（如有）：
  - 继续维持“先改写、运行时报错”的现状。
  - 为复合谓词临时加半套解析逻辑掩盖能力缺口。

## 阶段拆分
### 阶段一：能力边界收敛
- 目标：把 rewrite 层的 `find` 判定与 `ztok find` 运行时能力边界对齐。
- 交付物：`find` 专用 rewrite 判定实现，以及可复用的不支持参数检测入口。
- 完成条件：复杂 `find` 不再被自动改写；简单 `find` 仍保持改写能力。
- 依赖：现有 `UNSUPPORTED_FIND_FLAGS` 与 rewrite 分析入口。

### 阶段二：验证与回归
- 目标：证明新策略不会回归简单 `find` 路由，同时修复复杂 `find` 误拦截。
- 交付物：新增或更新的单元测试、格式化结果、受影响 crate 的局部测试记录。
- 完成条件：对应测试通过，且验证结果可复现。
- 依赖：阶段一代码改动完成。

## 测试与验证
- 核心验证：覆盖 rewrite 层对 `find . -name '*.rs' -o -name '*.ts'` 不改写、对简单 `find apps -name '*.test.ts' -type f` 仍改写。
- 必过检查：`just fmt`；`cargo nextest run -p codex-ztok`（若本地不可用则退回 `cargo test -p codex-ztok`）。
- 回归验证：检查 `ztok find` 显式调用命中不支持参数时仍报错。
- 手动检查：无。
- 未执行的验证（如有）：无。

## 风险与缓解
- 关键风险：rewrite 层与执行层若未共用同一事实源，后续新增不支持参数时仍可能漂移。
- 触发信号：测试只在一层更新，另一层行为不一致；复杂 `find` 继续被改写或简单 `find` 意外不再改写。
- 缓解措施：把不支持参数检测收敛为同一入口，并为“支持/不支持”两侧都补测试。
- 回滚/恢复方案（如需要）：若新判定引入回归，可回滚本次 rewrite 层改动，恢复原路由，再重新设计更细粒度解析。

## 参考
- `codex-rs/ztok/src/find_cmd.rs:58`
- `codex-rs/ztok/src/rewrite.rs:8`
- `codex-rs/ztok/src/lib.rs:1354`
