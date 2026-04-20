# ztok 通用内容压缩内核规划

## 背景
- 当前状态：
  - `ztok` 是内嵌在 Codex CLI 中的 curated integration，而不是独立产品。
  - `codex-rs/ztok/src/read.rs` 当前每次调用都会完整读取文件、过滤并直接输出，没有会话级缓存或重复读取复用能力。
  - `codex-rs/ztok/src/tracking.rs` 当前仅保留 no-op 运行期适配层，明确不包含上游分析、持久化或遥测能力。
  - `codex-rs/ztok/Cargo.toml` 当前没有现成的数据库或会话缓存依赖。
  - `codex-rs/ztok/src/filter.rs`、`summary.rs`、`json_cmd.rs`、`log_cmd.rs` 已存在按内容类型或命令类型工作的启发式压缩/摘要逻辑，但当前仍是分散实现，不是统一的通用压缩内核。
  - 仓库整体存在 `session_id` / `thread_id` 概念，但目前尚未确认 `ztok` 路径已直接消费稳定会话标识；因此第一阶段需要显式补一条仅服务 `ztok` 的会话标识注入链路。
- 触发原因：
  - 用户要求基于 `sqz` 与 `ztok` 的差异分析，规划“最值得移植”的能力。
  - 用户进一步明确：需要像 `sqz` 一样强调通用内容压缩，并且希望引入完整的 SimHash + LCS 近重复识别/差分能力，而不只是窄范围的 `read + delta-lite`。
- 预期影响：
  - 在不改变 `ztok` 产品边界的前提下，为代码、JSON、日志、文本等内容建立统一的压缩入口与共享能力底座。
  - 让重复/近重复内容在同一会话中不再被当成全新正文反复发送。
  - 把当前分散在多个命令里的启发式压缩能力收拢到可扩展、可测试的共享路径上。
  - 为后续继续对齐 `RTK + sqz` 双上游参考实现保留可审计的上游基线记录，并把同步口径收敛到现有 `upgrade-rtk` skill。

## 目标
- 目标结果：
  - 为 `ztok` 建立最小可用的通用内容压缩内核，并在第一轮接入 `read` 及现有高价值内容路径。
- 完成定义（DoD）：
  - 共享压缩入口能够先做内容路由，再调用对应的压缩策略，而不是继续把通用逻辑散落在各命令内部。
  - 同一会话内对相同内容的重复读取可返回短引用，而不是重复输出完整内容。
  - 同一会话内对近重复内容可通过 SimHash 命中候选，再用 LCS 生成受控差分输出；低置信度时显式回退到完整输出。
  - 第一轮至少覆盖代码/文本读取、JSON、日志这三类内容路径的共享压缩能力复用，不要求三者一开始都具备相同深度的 dedup/delta。
  - 相关单元测试/集成测试覆盖内容分类、缓存命中、近重复命中、LCS 差分、以及回退到完整输出的边界。
  - 第一阶段明确落地：`codex-rs/cli` 向 `ztok` 注入稳定会话标识；`ztok` 在 `CODEX_HOME/.ztok-cache/<session-id>.sqlite` 持久化会话缓存；当会话标识缺失时，禁用会话 dedup，仅保留共享内容压缩，不使用粗粒度缓存替代。
  - 仓库内新增 `sqz` 上游基线记录文件，写明 source / ref / commit hash / integration mode。
  - 现有 `.codex/skills/upgrade-rtk` skill 被扩展为 `ztok` 的统一双上游同步入口，明确区分“RTK 命令面基线”和“sqz 通用压缩参考基线”。
- 非目标：
  - 不移植 `sqz` 的 hook 安装、proxy、gain/stats、浏览器扩展、IDE 插件、WASM 或跨工具产品面。
  - 不直接引入完整 `sqz_engine` 或把 `ztok` 改造成独立压缩平台。
  - 不在本轮同时实现 `sqz` 全量产品面，例如 resume、dashboard、跨工具 hook 管理、插件市场能力。

## 范围
- 范围内：
  - 通用内容路由与共享压缩入口
  - 会话级 dedup 缓存
  - SimHash 候选匹配与 LCS 差分输出
  - `read` 的第一轮接入
  - `json` / `log` / 现有摘要路径对共享压缩底座的最小复用改造
  - `sqz` 上游 source/ref/commit hash 的仓库内记录
  - `.codex/skills/upgrade-rtk` 对双上游同步口径的收敛改造
  - 与该功能直接相关的测试与必要说明
- 范围外：
  - 跨工具 hook 安装与外部平台接入
  - 跨会话 resume / summary / gain
  - 独立 CLI 产品化能力

## 影响
- 受影响模块：
  - `codex-rs/ztok`
  - `codex-rs/cli`（仅当需要向 `ztok` 暴露稳定会话标识时）
  - `.version`
  - `.codex/skills/upgrade-rtk`
- 受影响接口/命令：
  - `codex ztok read`
  - `ztok read`
  - `ztok json`
  - `ztok log`
  - `ztok summary`
- 受影响数据/模式：
  - `CODEX_HOME/.ztok-cache/<session-id>.sqlite` 会话缓存
  - `sqz` 上游基线记录文件
- 受影响用户界面/行为：
  - 压缩输出将逐步从“命令各自处理”转为“共享压缩内核 + 按内容类型定制展示”
  - 重复/近重复内容不再总是完整正文，可能改为短引用或差分输出
  - 后续同步 `ztok` 上游参考时将继续通过统一的 `upgrade-rtk` skill 处理，而不是新增并行 skill

## 约束与依赖
- 约束（时间/兼容性/安全/性能/发布窗口等）：
  - 必须保持 `ztok` 作为 Codex 内嵌命令层的边界，不把本轮实现扩张成独立压缩平台。
  - 当前 `ztok` 是单次命令调用进程，进程内缓存无法跨调用复用；若要实现“会话 dedup”，必须有跨调用状态载体。
  - 当前仓库虽存在 `session_id` / `thread_id` 概念，但尚未确认 `ztok` 已直接拿到稳定会话标识；本计划默认补一条最小传递链路，由 `codex-rs/cli` 通过专用环境变量向 `ztok` 注入会话标识。
  - 当前通用压缩逻辑分散在多个命令模块内；若直接在原地继续叠加，会放大重复实现与后续维护成本。
  - 新能力必须保持显式、可观察、可回退；不允许为了命中去重而隐藏真实内容缺失风险。
- 外部依赖（系统/人员/数据/权限等）：
  - 需要访问 `sqz` 上游仓库以固定 source / ref / commit hash。

## 实施策略
- 总体方案：
  - 先建立一个共享的“内容分类 + 压缩入口 + 会话缓存 + 近重复比较”底座，再让 `read`、`json`、`log`、`summary` 逐步改为调用该底座。
  - 重复内容走“稳定内容指纹 + 会话 dedup 短引用”路径，近重复内容走“SimHash 候选筛选 + LCS 差分”路径，其他内容按内容类型走现有或增强后的压缩策略。
  - 第一阶段默认实现一条窄范围、仅服务 `ztok` 的会话标识暴露链路：由 `codex-rs/cli` 在调用 `ztok` 时注入专用环境变量，`ztok` 读取该值作为会话作用域，并在 `CODEX_HOME/.ztok-cache/<session-id>.sqlite` 中读写缓存。
  - 若运行时拿不到该会话标识，则显式禁用会话 dedup，不退化为按 `CODEX_HOME` 或 cwd 的粗粒度缓存；此时只保留共享内容压缩与单次调用内逻辑。
- 关键决策：
  - 第一轮优先建设共享底座，再让 `read` 作为最深接入点，`json` / `log` / `summary` 作为次级接入点。
  - 缓存应为轻量、可清理、以会话为边界的磁盘状态，而不是完整移植 `sqz` 会话系统。
  - SimHash 只用于近重复候选筛选，不直接决定最终输出；最终差分以 LCS 或等效可解释算法为准。
  - 短引用与差分输出必须是用户可见、可理解的，不做隐式替换。
  - 上游 `sqz` 基线记录应采用与 `.version/rtk.toml` 同类的单文件模式，避免把参考 hash 散落在计划或 issue notes 中。
  - `ztok` 既然同时参考 `RTK` 与 `sqz`，后续同步工作流应收敛到现有 `upgrade-rtk` skill，而不是再新增一个平行的 `sqz` sync skill；否则双上游口径会分裂。
  - 会话标识缺失时的默认行为不是“粗粒度缓存降级”，而是“禁用会话 dedup”；这是本计划的强约束，避免不可审计的跨会话误命中。
- 明确不采用的方案（如有）：
  - 不直接引入 `sqz_engine`、SQLite 会话系统、proxy、resume 或通用文本压缩算法。
  - 不在第一轮实现跨工具 hook 管理、浏览器/IDE 集成、dashboard 或 gain/stats。

## 阶段拆分
### 共享压缩合同
- 目标：
  - 确认并实现共享内容压缩入口、内容分类与压缩结果合同。
- 交付物：
  - 内容类型枚举与路由规则
  - 共享压缩结果结构
  - 缓存/差分不可用时的显式回退路径
- 完成条件：
  - 各命令能够通过共享入口接入统一压缩合同，而不是继续各自维护通用逻辑分支。
- 依赖：
  - `codex-rs/ztok`
  - 如有必要，`codex-rs/cli` 的窄范围会话标识暴露

### 会话 dedup
- 目标：
  - 让同一会话内重复内容不再重复输出完整正文。
- 交付物：
  - 稳定内容指纹
  - 会话作用域来源：`codex-rs/cli` 注入的专用环境变量
  - 缓存文件格式与清理策略：`CODEX_HOME/.ztok-cache/<session-id>.sqlite`
  - 短引用输出格式
  - 命中/未命中缓存的测试
- 完成条件：
  - 相同内容在多次独立调用之间能稳定命中短引用；误判时不会丢失正文。
  - 会话标识缺失时，`ztok` 会显式跳过 dedup 路径并走完整压缩输出，不会命中粗粒度缓存。
- 依赖：
  - 共享压缩合同

### SimHash + LCS 近重复压缩
- 目标：
  - 让近重复内容在同一会话中优先输出差分，而不是完整正文。
- 交付物：
  - SimHash 候选筛选
  - 基于 LCS 的差分生成
  - 低置信度时回退完整输出的规则
  - 相关测试
- 完成条件：
  - 同路径小改动以及高相似度近重复内容场景输出可读、可验证、可回退。
- 依赖：
  - 会话 dedup

### 命令接入与收口
- 目标：
  - 把共享底座接入现有高价值命令，并固定行为边界。
- 交付物：
  - `read` 的深接入
  - `json` / `log` / `summary` 的最小复用接入
  - `codex-rs/cli/tests/ztok.rs` 集成覆盖
  - `codex-rs/ztok` 单元测试
  - 如涉及用户可见语义变化，则补充对应帮助或说明
- 完成条件：
  - 核心内容路径已接入共享压缩入口，且命中缓存/近重复/回退场景均有测试锁定。
- 依赖：
  - 前三阶段实现完成

### 双上游基线与 `upgrade-rtk` 收口
- 目标：
  - 为 `ztok` 建立 `RTK + sqz` 双上游的可审计基线记录，并把后续同步入口收敛到 `upgrade-rtk`。
- 交付物：
  - `.version/sqz.toml` 或等效基线文件
  - `.codex/skills/upgrade-rtk/SKILL.md` 的双上游扩展说明
  - 如有必要，`upgrade-rtk` 对应的最小状态/检查清单补充
- 完成条件：
  - 仓库内可以直接查到当前参考的 `sqz` source / ref / commit hash
  - 后续若继续对齐 `ztok` 上游参考，仍通过统一的 `upgrade-rtk` skill 执行，而不是重新分裂出第二个专用 skill
- 依赖：
  - 上游 `sqz` 参考范围与本地目标边界已在本轮实现中明确

## 测试与验证
- 核心验证：
  - `cd /workspace/codex-rs && env -u RUSTC_WRAPPER cargo test -p codex-cli --test ztok`
  - `cd /workspace/codex-rs && env -u RUSTC_WRAPPER cargo test -p codex-ztok`
- 必过检查：
  - `cd /workspace/codex-rs && just fmt`
- 回归验证：
  - 共享内容路由能把代码/JSON/日志分发到正确压缩路径
  - 重复内容命中短引用
  - 近重复内容命中 SimHash 候选并生成 LCS 差分
  - 缺少会话标识或缓存状态异常时回退到完整输出
  - `codex-rs/cli` 注入会话标识时，`ztok` 能命中 `CODEX_HOME/.ztok-cache/<session-id>.sqlite`
- 手动检查：
  - 同一 `CODEX_HOME` / 同一会话范围内，连续执行 `codex ztok read <file>` 验证短引用与差分输出
  - 选取 JSON 与日志样本，验证其仍能走正确的内容压缩路径
  - 清空或破坏缓存后验证显式回退
  - 核对 `.version/sqz.toml` 中的 source / ref / commit hash 与本轮实际参考上游一致
  - 核对 `upgrade-rtk` 中新增的双上游说明、状态记录与校验清单与当前任务边界一致
  - 清除会话环境变量后再次执行 `codex ztok read <file>`，验证 dedup 被禁用且不使用粗粒度缓存
- 未执行的验证（如有）：
  - 无

## 风险与缓解
- 关键风险：
  - 当前 `ztok` 路径拿不到稳定会话标识，导致会话 dedup 无法落地。
  - SimHash 候选误命中或 LCS 差分错误会让模型看到不完整或误导性的内容。
  - 共享压缩底座改造会与现有分散命令逻辑发生职责重叠，导致回归面扩大。
  - 缓存膨胀或脏状态可能引发重复命中错误。
  - 如果 `RTK` 与 `sqz` 的同步口径分散在多个 skill 中，会让 `ztok` 的双上游参考边界失去审计一致性。
- 触发信号：
  - 不同会话之间互相命中缓存
  - 近重复输出无法独立理解，或不同内容被错误识别为相似
  - 原有 `json` / `log` / `summary` 行为在接入共享底座后出现明显退化
  - 缓存损坏后 `read` 行为异常而未回退
- 缓解措施：
  - 第一阶段直接落地固定方案：CLI 注入会话标识，缓存落到 `CODEX_HOME/.ztok-cache/<session-id>.sqlite`；若环境中拿不到会话标识，则禁用 dedup 而不是退化为粗粒度缓存。
  - SimHash 只做候选筛选，最终是否输出差分必须经过更严格的相似度/可读性判断；否则一律回退完整输出。
  - 先让 `read` 做最深接入，再按最小改动原则接入 `json` / `log` / `summary`，避免一轮内大面积替换旧逻辑。
  - 为缓存读写异常、指纹冲突、缺失快照添加显式回退路径和测试。
  - 将 `sqz` 基线记录集中到单一版本文件，并让 `upgrade-rtk` 成为唯一的双上游同步入口，避免 hash 漂移或技能口径不一致。
- 回滚/恢复方案（如需要）：
  - 通过关闭共享压缩入口与会话缓存路径，恢复当前各命令各自压缩/过滤的实现。
  - 若上游基线记录或同步 skill 不成熟，可先保留本地实现与测试，不让其阻塞压缩核心能力落地。

## 参考
- `/workspace/codex-rs/ztok/src/read.rs`
- `/workspace/codex-rs/ztok/src/tracking.rs`
- `/workspace/codex-rs/ztok/Cargo.toml`
- `/workspace/codex-rs/ztok/src/filter.rs`
- `/workspace/codex-rs/ztok/src/summary.rs`
- `/workspace/codex-rs/ztok/src/json_cmd.rs`
- `/workspace/codex-rs/ztok/src/log_cmd.rs`
- `/workspace/codex-rs/ztok/src/lib.rs`
- `/workspace/codex-rs/cli/src/main.rs`
- `/workspace/codex-rs/cli/tests/ztok.rs`
- `/workspace/.codex/skills/upgrade-rtk/SKILL.md`
- `/workspace/.version/rtk.toml`
- `/workspace/.agents/plan/2026-04-20-ztok-vs-sqz-analysis.md`
