# rtk 子命令重命名为 ztok

> 不适用项写 `无`，不要留空。
> 只写已确认事实；若信息不清楚或需要假设，先回到对话澄清，不要把未确认内容写进计划。

## 背景
- 当前状态：仓库内存在广泛的 `rtk` 子命令实现、别名、测试与文档引用。
- 触发原因：用户要求将本地子命令从 `rtk` 重命名为 `ztok`，且不保留 `rtk` 兼容别名，并同步更新 `upgrade-rtk` 技能中的相关估计说明。
- 预期影响：CLI 子命令、嵌入式命令重写、提示文案、测试与文档需整体更新，涉及 Rust 多 crate 与技能文档。

## 目标
- 目标结果：`rtk` 子命令与相关引用在本仓库内全面改为 `ztok`，且不保留 `rtk` 兼容入口；`upgrade-rtk` 技能文本同步调整。
- 完成定义（DoD）：
  - CLI 入口、alias 逻辑、嵌入式命令改写、测试与文档均改为 `ztok`。
  - `codex-rtk` crate 与目录/包名按需改名为 `codex-ztok` 并可编译通过。
  - `upgrade-rtk` 技能文本反映新的命名与估计说明。
  - 通过约定的最小验证步骤。
- 非目标：
  - 不改动上游 rtk 项目或其命名。
  - 不保留 `rtk` 兼容别名或重定向。

## 范围
- 范围内：
  - Rust 代码、测试、文档与脚本中所有 `rtk` 子命令/命名引用的本地重命名。
  - `codex-rs/rtk` crate 与相关依赖路径重命名。
  - `upgrade-rtk` 技能文本中的估计/说明更新。
- 范围外：
  - 上游 `rtk-ai/rtk` 代码或命名。
  - 与 `rtk` 无关的功能改造。

## 影响
- 受影响模块：`codex-rs/cli`、`codex-rs/rtk`、`codex-rs/arg0`、`codex-rs/core`、相关测试与文档、`/workspace/.codex/skills/upgrade-rtk`。
- 受影响接口/命令：`codex rtk ...` 改为 `codex ztok ...`，以及直接 `rtk` 调用改为 `ztok` 调用。
- 受影响数据/模式：无。
- 受影响用户界面/行为：CLI 命令名称与提示输出改为 `ztok`。

## 约束与依赖
- 约束（时间/兼容性/安全/性能/发布窗口等）：不保留 `rtk` 兼容入口；本地改名不影响上游命名。
- 外部依赖（系统/人员/数据/权限等）：无。

## 实施策略
- 总体方案：全仓库范围内完成命名替换与结构改名（crate/目录/模块/测试/文档/脚本），确保命令解析、alias、重写逻辑与测试覆盖一致更新。
- 关键决策：删除 `rtk` 兼容入口并统一采用 `ztok`；同步更新技能文档中的估计说明。
- 明确不采用的方案（如有）：保留 `rtk` 作为兼容别名或重定向。

## 阶段拆分

### 1) 范围梳理与改名方案确认
- 目标：确认需要改名的模块、文件与接口。
- 交付物：改名清单与执行顺序。
- 完成条件：列出关键路径与受影响测试/文档。
- 依赖：无。

### 2) 代码与结构改名
- 目标：完成 crate/目录/命令/文案/测试的统一改名。
- 交付物：完成修改的代码与测试变更。
- 完成条件：编译路径与引用无残留 `rtk` 入口。
- 依赖：阶段 1 清单。

### 3) 文档与技能更新
- 目标：更新 `upgrade-rtk` 技能文本及相关文档描述。
- 交付物：文档与技能文件更新。
- 完成条件：文档中无冲突命名。
- 依赖：阶段 2 结果。

### 4) 验证与收尾
- 目标：完成最小可重复验证并准备提交。
- 交付物：验证结果与变更摘要。
- 完成条件：验证命令通过或记录未执行原因。
- 依赖：阶段 2、3。

## 测试与验证
- 核心验证：`cd /workspace/codex-rs && just fmt`。
- 必过检查：`cd /workspace/codex-rs && cargo test -p codex-cli`。
- 回归验证：按实际变更涉及 crate 增补相关 `cargo test -p <crate>`。
- 手动检查：检查命令帮助与文案中 `ztok` 命名一致。
- 未执行的验证（如有）：如未能执行将记录原因。

## 风险与缓解
- 关键风险：遗漏某些 `rtk` 引用导致命令解析或文档不一致。
- 触发信号：测试失败、help 输出仍含 `rtk`、构建引用路径错误。
- 缓解措施：全仓库搜索 + 关键路径手动复查；补充测试断言。
- 回滚/恢复方案（如需要）：回退到改名前的提交或逐步还原改名变更。

## 参考
- codex-rs/cli/src/main.rs
- codex-rs/rtk/Cargo.toml
- codex-rs/arg0/src/lib.rs
- codex-rs/core/src/tools/events.rs
- codex-rs/core/src/tools/rewrite/shell_search_rewrite.rs
- codex-rs/core/templates/compact/rtk_instructions.md
- codex-rs/cli/tests/rtk.rs
- .version/rtk.toml
- .codex/skills/upgrade-rtk/SKILL.md
