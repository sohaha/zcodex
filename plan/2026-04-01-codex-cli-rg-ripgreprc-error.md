# 修复 Codex CLI 继承缺失 ripgreprc 导致 rg 报错

> 不适用项写 `无`，不要留空。
> 只写已确认事实；若信息不清楚或需要假设，先回到对话澄清，不要把未确认内容写进计划。

## 背景
- 当前状态：当前会话环境存在 `RIPGREP_CONFIG_PATH=/root/.config/ripgreprc`，但该文件不存在；直接执行 `rg` 会输出 `/root/.config/ripgreprc: No such file or directory (os error 2)`。
- 触发原因：Codex CLI 运行时会把父进程环境透传给原生 `codex` 子进程，导致内部 shell 命令继承该无效配置。
- 预期影响：修复后，Codex CLI 内执行 `rg` 不再因为缺失的 ripgrep 配置文件产生噪音或失败。

## 目标
- 目标结果：在不影响用户显式有效 ripgrep 配置的前提下，避免 Codex CLI 继承无效 `RIPGREP_CONFIG_PATH`。
- 完成定义（DoD）：
  - 代码中对无效 `RIPGREP_CONFIG_PATH` 做显式处理；
  - 能通过可复现命令验证修复行为；
  - 相关变更经最小范围检查确认无额外回归。
- 非目标：
  - 不修改 ripgrep 上游行为；
  - 不处理与 `RIPGREP_CONFIG_PATH` 无关的其他 shell 环境问题；
  - 不引入新的隐式 fallback 逻辑到 Rust 核心层。

## 范围
- 范围内：`codex-cli` 启动入口中的环境整理逻辑及其验证。
- 范围外：`codex-rs` 其他 crate、npm 发布流程、系统级 shell 配置文件。

## 影响
- 受影响模块：`codex-cli/bin/codex.js`
- 受影响接口/命令：`codex` 启动后的子进程环境、内部执行的 `rg` 命令
- 受影响数据/模式：进程环境变量 `RIPGREP_CONFIG_PATH`
- 受影响用户界面/行为：用户在 Codex CLI 中触发 `rg` 时不再看到缺失 ripgreprc 的错误

## 约束与依赖
- 约束（时间/兼容性/安全/性能/发布窗口等）：保持改动最小；不能破坏用户提供的有效 `RIPGREP_CONFIG_PATH`；行为需显式可读。
- 外部依赖（系统/人员/数据/权限等）：无。

## 实施策略
- 总体方案：在 `codex-cli/bin/codex.js` 构造子进程环境时检查 `RIPGREP_CONFIG_PATH`；若变量已设置但目标文件不存在，则移除该环境变量并保留其余环境不变。
- 关键决策：在 CLI 入口层修复，而不是依赖每次 shell 调用手动追加 `--no-config`，以减少散落修补点。
- 明确不采用的方案（如有）：
  - 不要求用户手动创建空的 `/root/.config/ripgreprc`；
  - 不全局强制为所有 ripgrep 调用追加 `--no-config`；
  - 不修改仓库内所有 `rg` 命令调用点。

## 阶段拆分
### 计划与实现
- 目标：完成最小修复并验证。
- 交付物：代码变更、验证结果。
- 完成条件：缺失配置文件时 `rg` 不再报错，且有效配置场景不被破坏。
- 依赖：无。

## 测试与验证
- 核心验证：在设置无效 `RIPGREP_CONFIG_PATH` 的环境下，通过 CLI 启动链验证传递给子进程的环境已清理。
- 必过检查：针对改动文件的最小自检与脚本/命令验证。
- 回归验证：确认未设置该变量或变量指向存在文件时，CLI 仍保留原行为。
- 手动检查：必要时用 Node 片段模拟 `codex.js` 的环境构造逻辑。
- 未执行的验证（如有）：无。

## 风险与缓解
- 关键风险：误删用户本来可用的 ripgrep 配置，导致行为变化。
- 触发信号：当 `RIPGREP_CONFIG_PATH` 指向存在文件时被错误移除。
- 缓解措施：仅在变量非空且目标文件不存在时删除。
- 回滚/恢复方案（如需要）：回退 `codex-cli/bin/codex.js` 的相关环境清理逻辑。

## 参考
- `codex-cli/bin/codex.js`
- `codex-cli/bin/rg`
