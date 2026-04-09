# 2026-04-09 ztldr semantic cache 迁到 runtime 目录反思

## 背景
- 用户反馈 `ztldr` 启动后会在项目根生成 `.tldr`，污染工作区。
- 初看容易误以为是 daemon socket/pid/lock 还在项目目录，但当前实现里 daemon artifact 早已迁到运行时目录或系统临时目录。
- 真正还落在项目根的是 semantic cache：`native-tldr/src/semantic_cache.rs` 直接拼 `project_root/.tldr/cache/semantic/<language>/`。

## 本轮有效做法
- 先分别核对 daemon artifact 和 semantic cache 的真实落点，避免把问题误判成 daemon 路径回归。
- 复用现有 `daemon_project_hash` + runtime/tmp scope 规则，把 semantic cache 收敛到同一个项目级 runtime artifact 根目录，而不是再造一套独立命名方案。
- 额外补一条单元测试，明确 semantic cache 默认路径不应位于项目根下；这样后续即使 README 或断言回归，也更容易第一时间发现。

## 关键收益
- 用户工作区不再因为首次 semantic 建索引出现 `.tldr/`。
- daemon 与 semantic 的“可重建 runtime artifact”口径统一，后续调整目录策略时只需围绕同一套项目隔离规则思考。
- `.tldrignore` 继续保留在项目根作为用户输入文件，避免把输入配置与可重建产物混在一起迁移。

## 踩坑
- 仓库环境里 `rg` 会默认读取缺失的 `/root/.config/ripgreprc`，在“无匹配”时会夹带噪声；做无匹配扫描时最好显式加 `RIPGREP_CONFIG_PATH=/dev/null`。
- Cadence 执行期若要把验证证据写回 issue notes，尽量直接用已执行的稳定命令结果，避免临时脚本依赖 `python` 这类当前环境未必存在的解释器。
- 深度审查后又补出一个真实回归点：仅凭“`XDG_RUNTIME_DIR` 是绝对路径”并不足以安全采用；若该路径存在但不可写/不是目录，semantic cache 会从“以前还能工作”退化成启动即失败。最终应在选择 runtime 目录时先验证可创建，再回退到 `temp_dir`。

## 后续建议
- 以后再遇到“项目根被 ztldr 污染”的反馈，先区分是输入文件（如 `.tldrignore`）还是 runtime artifact，不要一概按“全部迁走”处理。
- 如果后续需要给 semantic cache 增加显式配置项，优先在统一的 runtime artifact 路径模型上扩展，而不是重新引入 `project_root/.tldr` 默认值。

## 深度审查后的修正
- 仅把 semantic cache 迁到 runtime/tmp 目录还不够；如果把“运行时目录不可写时回退”放在 daemon artifact 根目录选择层，会让 socket、pid、lock 也跟着静默漂移到临时目录，破坏 daemon IPC 路径稳定性。
- 本次修正把职责重新拆开：`native-tldr/src/daemon.rs` 只做路径解析，不再因为目录不可写而换根；`native-tldr/src/semantic_cache.rs` 自己在写盘时尝试 runtime 目录，失败后才回退到 temp 目录。
- semantic cache 读取端也同步改为按 primary/fallback 两个候选目录顺序查找，这样此前因不可写而落到 temp 的缓存仍可复用，不会每次都重新建索引。

## 额外经验
- 测试里不要再通过修改 `XDG_RUNTIME_DIR` 进程环境去验证这类路径策略；直接向 helper 注入 primary/fallback artifact dir，更容易稳定复现“主目录不可写但 fallback 可用”的场景，也避免并行测试互相污染。
- 对 runtime artifact 相关问题做审查时，要先分清“路径选择层”和“写盘回退层”；前者决定 daemon 对外可观测路径，后者才允许围绕可写性做容错，两者不能混用。
