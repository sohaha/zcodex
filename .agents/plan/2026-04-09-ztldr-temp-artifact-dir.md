# ztldr 临时 artifact 目录

> 不适用项写 `无`，不要留空。
> 只写已确认事实；若信息不清楚或需要假设，先回到对话澄清，不要把未确认内容写进计划。

## 背景
- 当前状态：`native-tldr` 的 daemon socket/pid/lock 已经写入运行时目录或系统临时目录，不再落到项目根；但 semantic cache 仍固定写到 `project_root/.tldr/cache/semantic/<language>/`，因此首次建索引会在项目根生成 `.tldr` 目录。
- 触发原因：当前 `ztldr` 启动后生成 `.tldr` 会污染项目工作区，用户希望评估并推进改到系统临时目录。
- 预期影响：若语义索引缓存迁出项目根，可避免仓库目录新增 `.tldr`，但需要保持按项目隔离、并确保 daemon/semantic/文档/测试对新路径的一致性。

## 目标
- 目标结果：将 `ztldr` 生成的可重建 runtime/cache artifact 从项目根 `.tldr/` 迁移到系统临时/运行时目录下的项目隔离路径。
- 完成定义（DoD）：
  - semantic cache 默认不再写入 `project_root/.tldr/`。
  - 新路径保持按用户/项目哈希隔离，不与现有 daemon artifact 布局冲突。
  - 受影响测试更新并覆盖新默认路径。
  - `native-tldr` 相关文档更新为新默认行为。
- 非目标：
  - 不修改 `.tldrignore` 的项目级配置语义。
  - 不改变 `ztldr` 的分析/路由/embedding 行为本身。
  - 不在本轮引入新的长期持久化产品配置面，除非实现迁移所必需。

## 范围
- 范围内：
  - `native-tldr` 中 semantic cache 默认目录解析逻辑。
  - 与 cache 路径相关的测试、README 和必要的 CLI/MCP 事实说明。
  - 路径布局与目录创建的兼容处理。
- 范围外：
  - daemon artifact 现有运行时目录策略调整。
  - 其他项目级工作目录（如 `.codex/`、`.ai/`、`zmemory`）的路径策略。
  - 新增跨会话缓存管理命令或清理子命令。

## 影响
- 受影响模块：`codex-rs/native-tldr/src/semantic_cache.rs`、可能的路径辅助模块/测试、`codex-rs/native-tldr/README.md`。
- 受影响接口/命令：`codex ztldr semantic`、依赖 semantic index 的 daemon-first 查询链路、`warm` / 首次建索引路径行为。
- 受影响数据/模式：semantic manifest、units、vectors 的磁盘落点从项目根 `.tldr/cache/semantic/<language>/` 迁移到系统临时/运行时目录的项目隔离子目录。
- 受影响用户界面/行为：用户不再在项目根看到 `.tldr/`；系统临时目录中的缓存可能被系统清理，导致后续重新建索引。

## 约束与依赖
- 约束（时间/兼容性/安全/性能/发布窗口等）：
  - 必须保留按项目隔离，避免不同仓库缓存串扰。
  - 路径必须跨平台可用，并复用现有 daemon artifact 对运行时目录/临时目录的优先级习惯。
  - 不能把失败静默掩盖成成功；路径创建或读取失败仍需保留可观察错误。
  - 变更应尽量最小，优先复用已有项目哈希/运行时目录逻辑。
- 外部依赖（系统/人员/数据/权限等）：无。

## 实施策略
- 总体方案：抽取或复用一套“按用户/项目隔离的 ztldr runtime 根目录”解析逻辑，让 semantic cache 默认跟随该系统目录，而不是拼接 `project_root/.tldr`；随后同步测试与文档。
- 关键决策：
  - 继续保留 `.tldrignore` 在项目根，作为用户显式维护的项目级输入文件；仅迁移可重建 artifact。
  - 默认采用系统运行时目录/系统临时目录，而不是用户 home 下持久 cache 目录，以优先满足“避免污染项目目录”的目标，并与现有 daemon artifact 策略对齐。
  - 保持项目哈希隔离，避免直接以项目名命名目录。
- 明确不采用的方案（如有）：
  - 不继续使用 `project_root/.tldr/` 作为默认缓存目录。
  - 不在本轮改为 XDG cache/home 持久目录方案。
  - 不通过关闭 semantic 或关闭 auto-start 来规避 `.tldr` 生成。

## 阶段拆分
### 路径策略收敛
- 目标：确认并落地 semantic cache 新默认路径的统一解析方式。
- 交付物：更新后的路径解析实现与相关单元测试。
- 完成条件：semantic cache 不再默认落到项目根，且新路径复用现有隔离约束。
- 依赖：现有 daemon artifact 路径规则与 `daemon_project_hash`。

### 验证与文档同步
- 目标：确保实现、测试与文档对新默认行为一致。
- 交付物：更新后的测试、README/相关文档说明。
- 完成条件：受影响 crate 测试通过，文档不再宣称默认写入 `.tldr/cache/semantic/...`。
- 依赖：路径策略收敛阶段完成。

## 测试与验证
- 核心验证：`cargo nextest run -p codex-native-tldr` 或仓库约定的 `just native-tldr-test-fast` / `just tldr-semantic-test-fast`，覆盖 semantic cache 路径相关测试。
- 必过检查：`just fmt`。
- 回归验证：验证 daemon artifact 相关测试仍通过，确认 socket/pid/lock 路径未被回归影响。
- 手动检查：在临时项目目录执行一次 semantic 索引构建，确认项目根不生成 `.tldr/`，并能在系统临时/运行时目录下观察到对应缓存目录。
- 未执行的验证（如有）：无。

## 风险与缓解
- 关键风险：系统临时目录可能被清理，导致缓存命中率下降或首次查询更频繁重建。
- 触发信号：相同项目重复查询时总是 fresh build，或测试对磁盘复用的断言失效。
- 缓解措施：保留磁盘缓存复用测试，但将断言切换到新路径；在文档中明确该缓存是可重建 artifact。
- 回滚/恢复方案（如需要）：将 cache 路径解析恢复到项目根 `.tldr/cache/semantic/<language>/`，并回退相关测试与文档。

## 参考
- `codex-rs/native-tldr/src/semantic_cache.rs:159`
- `codex-rs/native-tldr/src/semantic.rs:351`
- `codex-rs/native-tldr/src/daemon.rs:1121`
- `codex-rs/native-tldr/src/config.rs:45`
- `codex-rs/native-tldr/README.md:10`
- `codex-rs/native-tldr/README.md:62`
- `codex-rs/native-tldr/README.md:87`
