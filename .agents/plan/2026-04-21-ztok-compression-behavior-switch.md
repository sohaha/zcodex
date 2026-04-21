# ztok 压缩行为模式规划

## 背景
- 当前状态：
  - `ztok` 当前默认行为已经叠加了共享压缩、会话级 dedup 与 near-diff 能力。
  - `read`、`json`、`log`、`summary` 当前都通过共享压缩或共享 dedup 路径工作，其中 `session_dedup` 统一承接 exact dedup、near-diff 和无会话标识时的显式回退。
  - `codex-rs/cli/src/main.rs` 当前只在进入 `ztok` 前桥接 `CODEX_THREAD_ID -> CODEX_ZTOK_SESSION_ID`，尚未向 `codex-ztok` 注入任何压缩模式配置。
  - `summary` 当前仍存在两类已知风险：摘要身份过粗，且会把完整原始输出写入会话缓存。
- 触发原因：
  - 用户要求在保持当前默认能力的同时，新增一个可配置开关，让不需要增强压缩逻辑的场景可以切换到更基础的行为模式。
  - 用户明确要求先做并行研究与子代理讨论，再形成规划。
- 预期影响：
  - 为 `ztok` 增加一个可审计、可文档化、可测试的运行模式开关。
  - 默认行为继续保留当前增强模式；显式开启基础模式时，禁用增强压缩路径，避免不需要的会话缓存与近重复压缩介入。

## 目标
- 目标结果：
  - 为 `ztok` 新增独立配置块 `[ztok]`，提供 `behavior = "enhanced" | "basic"` 的行为开关。
  - 当 `behavior = "enhanced"` 时，保持当前共享压缩、session dedup、near-diff 的默认行为不变。
  - 当 `behavior = "basic"` 时，禁用增强压缩逻辑，并让各命令退回到预先定义的基础行为矩阵，而不是半关闭或静默混用。
  - 同时完成 `summary` 的收口与优化，消除当前摘要身份与会话落库边界上的已知问题。
- 完成定义（DoD）：
  - `config.toml` 可以稳定解析 `[ztok]` 配置，schema 与文档同步更新。
  - `codex`/`ztok` 两个入口在同一配置下行为一致，不会出现 alias 与子命令分叉。
  - `basic` 模式下不会再出现 session dedup、near-diff 或其相关 fallback 语义。
  - `read`、`json`、`log`、`summary` 的双模式行为都有明确测试覆盖，且默认模式仍保持现有行为。
  - 计划中定义的每个命令退路都是显式、可解释、可验证的，不依赖“关闭后自然应该怎样”的隐含假设。
  - `summary` 在默认增强模式下也必须完成收口：不再用过粗摘要身份参与会话复用，且会话缓存边界与持久化内容受到明确约束。
- 非目标：
  - 不宣称本轮会把 `ztok` 做成外部参考实现的逐字节完全对齐版本。
  - 不改动现有参考基线记录与同步工作流。
  - 不同时扩展新的外部产品面能力，如 `resume`、`stats`、`dashboard`、hook、插件或代理。
  - 不在本轮把 `ztok` 的所有行为都重新设计为独立产品体系。

## 范围
- 范围内：
  - `config.toml` 中新增 `ztok` 独立配置块与行为枚举
  - `codex-rs/cli` 到 `codex-rs/ztok` 的配置桥接
  - `session_dedup`、`near_dedup`、共享压缩 dispatcher 的模式 gate
  - `read`、`json`、`log`、`summary` 在 `basic` 模式下的显式退路定义与实现
  - `summary` 在默认增强模式下的身份、落库边界与测试收口
  - 对应 schema、文档、CLI 测试与 crate 内单元测试
- 范围外：
  - 新增独立 CLI flag 作为唯一配置入口
  - 把该开关塞进 `[features]`、`[tools]` 或其他现有无关配置块
  - 修改 app-server protocol、MCP 协议或远端 RPC 契约
  - 重写 `ztok` 的全部命令帮助或重新命名现有命令面

## 影响
- 受影响模块：
  - `codex-rs/config`
  - `codex-rs/core` 配置加载与 schema 生成链路
  - `codex-rs/cli`
  - `codex-rs/ztok`
  - `docs/` 与 `codex-rs/README.md`
- 受影响接口/命令：
  - `codex ztok`
  - `ztok`
  - `ztok read`
  - `ztok json`
  - `ztok log`
  - `ztok summary`
- 受影响数据/模式：
  - `config.toml` 新增 `[ztok]` 配置块
  - `core/config.schema.json` 需要更新
  - `CODEX_ZTOK_SESSION_ID` 继续保留；另需新增行为模式桥接方式
- 受影响用户界面/行为：
  - 默认用户无行为变化
  - 显式启用 `basic` 后，`[ztok dedup ...]`、`[ztok diff ...]` 及相关近重复压缩输出将消失
  - `json` / `log` / `summary` 在基础模式下的输出会比默认模式更保守
  - 默认增强模式下，`summary` 的会话复用与缓存边界会更严格，减少误命中与不必要持久化

## 约束与依赖
- 约束（时间/兼容性/安全/性能/发布窗口等）：
  - 默认行为必须保持当前 `codex` 模式，不得因新增开关破坏现有用户与测试合同。
  - 新配置不能使用布尔值表达“是否禁用增强压缩”，避免双重否定和后续扩展受限；应采用枚举模式。
  - 新配置不能放进 `[features]` 或 `[tools]`，避免污染能力开关语义或模型工具配置语义。
  - `basic` 必须是完整行为模式，而不是只切掉一半实现层后留下混合态。
  - 对 `json` / `log` 这类当前主要依赖共享压缩的命令，必须先定义明确退路，再落实现；不能先关掉压缩再临时决定返回什么。
  - `ztok json --schema-only` 在 `basic` 模式下必须有显式合同，不能留给实现阶段自由解释。
  - `summary` 的优化不能只停留在“关闭基础模式下的复用”；默认增强模式也必须修掉当前身份过粗和原始输出落库过宽的问题。
- 外部依赖（系统/人员/数据/权限等）：
  - 需要依赖当前仓库已有配置加载、schema 生成与文档更新工作流。
  - 如需再次核对基础模式的外部行为参考，应基于当前记录的外部资料单独审查，但本轮不以“全量外部对齐”作为完成标准。

## 实施策略
- 总体方案：
  - 采用独立配置块 `[ztok]`，以 `behavior` 枚举表达运行模式。
  - 由 `codex-rs/cli` 在进入 `codex_ztok::run_from_os_args(...)` 前读取解析后的配置，并桥接给 `codex-rs/ztok`；不要让 `codex-rs/ztok` 直接承担 Codex 全局配置解析职责。
  - 运行时优先切断“会话复用与持久化风险”而不是先做大拆分：`basic` 首先要让 `read/json/log/summary` 四条用户可见链路整体绕过 `session_dedup`，并使 SQLite 写入与 near-diff 输出不可达。
  - 在确认基础模式的会话路径已经整体关闭后，再按命令收口共享压缩 facade 与显式退路矩阵，而不是一开始就重拆全部 `compression.rs` 结构。
  - 基础模式下对各命令采用明确退路矩阵，而不是仅通过 early return 让行为“自然退化”。
  - `summary` 单独按“默认增强模式收口 + 基础模式退路”两条线处理，不把默认模式下的已知风险推迟到后续。
- 关键决策：
  - 配置键采用：
    - `[ztok]`
    - `behavior = "enhanced" | "basic"`
  - 默认值为 `enhanced`，保持当前增强行为不变。
  - `basic` 的第一原则是“禁用增强压缩路径”，不是“追求绝对外部对齐”。
  - `basic` 的第一落点是整体绕过 session dedup / near-diff / snapshot persistence；若这三层仍可达，则该模式不算完成。
  - 命令退路矩阵按当前本地实现能力定义：
    - `read`：保留文件读取、语言识别、过滤/窗口/行号等本地基础能力，但关闭 session dedup 与 near-diff
    - `json`：普通文件或 stdin 输入在 `basic` 下返回原始 JSON 文本；`--schema-only` 在 `basic` 下显式报不支持；无效 JSON 继续走显式解析错误，不做静默透传
    - `log`：关闭共享日志压缩后，退回原始日志输出
    - `summary`：保留本地测试/构建/列表/通用文本摘要框架，但关闭 session dedup；输入为 JSON 或日志时，不再调用专用压缩摘要，而是统一退回通用文本摘要分支
  - `summary` 在增强模式下必须额外满足两条收口要求：
    - 参与会话复用时，身份不能只由过粗摘要签名决定，避免不同命令因摘要文本接近而误命中
    - 会话缓存中的持久化内容必须受边界约束，不能继续无条件写入完整原始输出
  - rollout 需要分期，先稳定配置合同和行为桥，再整体切断会话复用/落库路径，最后视行为差距决定是否继续拆解共享压缩 facade。
- 明确不采用的方案（如有）：
  - 不使用 `disable_enhanced = true`、`use_basic_mode = false` 这类布尔开关
  - 不拆成多个独立开关，例如分别控制 compression / dedup / near-diff
  - 不先上 CLI flag、后补 config.toml
  - 不把本轮开关命名成 `summary` 或其他无法覆盖全局行为语义的局部名称

## 阶段拆分
### 配置合同与桥接
- 目标：
  - 建立 `[ztok]` 配置块、行为枚举及 CLI 到 ztok 的单一桥接链路。
- 交付物：
  - `ConfigToml` / 运行时配置类型新增 `ztok` 配置
    - `behavior = "enhanced" | "basic"` schema
  - `codex-rs/cli` 到 `codex-rs/ztok` 的模式桥接
- 完成条件：
  - 配置可解析、可序列化、可出现在 schema 中，且 `codex ztok` 与 alias `ztok` 行为一致。
- 依赖：
  - 当前配置系统与 `run_ztok_subcommand` 入口

### 会话路径切断
- 目标：
  - 让 `basic` 模式先整体绕过 session dedup、near-diff 和相关 SQLite 持久化。
- 交付物：
  - `session_dedup.rs` / `near_dedup.rs` gate
  - `read` / `json` / `log` / `summary` 四条入口对共享 dedup 的统一绕过
  - `.ztok-cache/*.sqlite` 在基础模式下不可写的行为保证
- 完成条件：
  - `basic` 下不再出现 `[ztok dedup ...]`、`[ztok diff ...]` 或 near-diff fallback 语义，且不会写入基础模式下不应存在的会话缓存。
- 依赖：
  - 配置合同与桥接

### 兼容行为收口
- 目标：
  - 为四个命令补齐基础模式下的显式退路矩阵，并视行为差距决定是否需要进一步拆解共享 `compression.rs` facade。
- 交付物：
  - `read` / `json` / `log` / `summary` 的 `basic` 行为定义与实现
  - 如有必要，对共享压缩 facade 的最小收口调整
- 完成条件：
  - 四个命令在默认模式与基础模式下都具备明确、可验证的输出行为，不存在“同一模式、多种语义”。
- 依赖：
  - 前两阶段完成

### summary 收口与优化
- 目标：
  - 完成 `summary` 在默认增强模式下的身份修正、缓存边界收紧与行为验证。
- 交付物：
  - `summary` 的会话复用身份合同
  - `summary` 与会话缓存之间的持久化边界调整
  - 针对误命中与原始输出落库问题的回归测试
- 完成条件：
  - 默认增强模式下，`summary` 不再因摘要身份过粗产生错误 exact-dedup，且不会继续无条件把完整原始输出写入会话缓存。
- 依赖：
  - 前两阶段完成

### 测试、文档与回归锁定
- 目标：
  - 用双模式测试与文档说明锁定默认模式和兼容模式的边界。
- 交付物：
  - 配置解析与默认值测试
  - `cli/tests/ztok.rs` 双模式回归测试
  - `codex-rs/ztok` 单元测试
  - `docs/config.md`、`codex-rs/README.md`、必要说明更新
- 完成条件：
  - 文档、schema、测试与实现一致；默认模式回归不破坏现有合同，兼容模式行为可复现。
- 依赖：
  - 前三阶段完成

## 测试与验证
- 核心验证：
  - `cd /workspace/codex-rs && env -u RUSTC_WRAPPER cargo test -p codex-cli --test ztok`
  - `cd /workspace/codex-rs && env -u RUSTC_WRAPPER cargo test -p codex-ztok`
  - `cd /workspace/codex-rs && env -u RUSTC_WRAPPER cargo test -p codex-core --lib`
- 必过检查：
  - `cd /workspace/codex-rs && just fmt`
  - 若修改 `ConfigToml` 或嵌套配置类型：`cd /workspace/codex-rs && just write-config-schema`
- 回归验证：
  - 默认 `enhanced` 模式下，现有有 session 的 dedup 命中测试仍成立
  - `basic` 模式下，`read/json/log/summary` 均不再产生 dedup / diff 输出
  - alias `ztok` 与 `codex ztok` 在同一配置下行为一致
  - `json` / `log` 在基础模式下按计划的退路矩阵输出，不会进入共享压缩路径
  - `ztok json --schema-only` 在基础模式下稳定返回预期的不支持错误
  - `summary` 在增强模式下不会因不同命令但相近摘要文本而误命中复用
  - `summary` 的会话缓存不再无条件落入完整原始输出
- 手动检查：
  - 在临时 `CODEX_HOME` 下分别执行默认模式与 `basic` 模式，核对 `ztok read`、`ztok json`、`ztok log`、`ztok summary` 输出差异
  - 重点核对 `summary` 在 JSON/日志输入时不再走专用压缩摘要，而会稳定落到通用文本摘要分支
  - 在默认增强模式下，用两条不同但摘要文本接近的 `summary` 命令验证不会被错误折叠成同一会话复用结果
  - 核对 `docs/config.md` 与 `codex-rs/README.md` 对新配置的说明与实际默认值一致
- 未执行的验证（如有）：
  - 无

## 风险与缓解
- 关键风险：
  - `json` / `log` 在基础模式下退回原文后 token 开销显著上升。
  - 只切掉 dedup 而未切掉共享压缩，或反之，会制造混合态行为。
  - alias `ztok` 与 `codex ztok` 若桥接链路不一致，会出现入口分叉。
  - `summary` 若只做基础模式退路、不修默认增强模式的身份和落库边界，已知风险会继续保留。
  - 若文档把 `basic` 描述成“完全等同外部参考实现”，会造成错误承诺。
  - 默认模式测试若未做双模式覆盖，新增基础模式可能反向打坏当前默认行为。
- 触发信号：
  - 基础模式下仍出现 `[ztok dedup ...]`、`[ztok diff ...]` 或 near-diff fallback reason
  - `json` / `log` 输出在基础模式下既不像当前默认模式，也不是明确定义的原文退路
  - `ztok json --schema-only` 在基础模式下仍然进入共享 schema 渲染，或出现未定义的静默降级
  - 同一配置下 `ztok` alias 与 `codex ztok` 输出不一致
  - 默认增强模式下，`summary` 仍会把完整原始输出直接写入会话缓存，或不同命令被错误折叠
  - 文档、schema、测试与默认值不一致
- 缓解措施：
  - 先定义行为矩阵，再写 gate；禁止“先关后看”
  - 以双模式测试锁住默认行为与兼容行为
  - 通过单一 CLI 桥接点统一 alias 与子命令入口
  - 文档中只使用“增强模式/基础模式”这类中性名称，不暴露实现来源或外部参考名称
- 回滚/恢复方案（如需要）：
  - 若 `basic` 模式在实现或验证阶段出现不可接受回归，回滚单位应是“整个基础模式”，而不是发布一个只关闭部分增强路径的半实现模式。
  - 允许保留 `[ztok]` 配置合同与默认 `enhanced` 行为，但在未完成完整命令退路矩阵之前，不启用或不发布 `basic`。
  - 已启用基础模式后若发现严重回归，恢复方案应为整体回退到默认 `enhanced` 行为，并同步撤回该模式对应文档与测试预期。

## 参考
- `/workspace/codex-rs/cli/src/main.rs:1276`
- `/workspace/codex-rs/config/src/config_toml.rs:82`
- `/workspace/codex-rs/config/src/types.rs:971`
- `/workspace/codex-rs/core/src/config/mod.rs:2344`
- `/workspace/codex-rs/core/src/config/config_tests.rs`
- `/workspace/codex-rs/ztok/src/compression.rs:144`
- `/workspace/codex-rs/ztok/src/compression_json.rs`
- `/workspace/codex-rs/ztok/src/compression_log.rs`
- `/workspace/codex-rs/ztok/src/session_dedup.rs:31`
- `/workspace/codex-rs/ztok/src/near_dedup.rs:87`
- `/workspace/codex-rs/ztok/src/read.rs`
- `/workspace/codex-rs/ztok/src/json_cmd.rs`
- `/workspace/codex-rs/ztok/src/log_cmd.rs`
- `/workspace/codex-rs/ztok/src/summary.rs:43`
- `/workspace/codex-rs/cli/tests/ztok.rs`
- `/workspace/docs/config.md`
- `/workspace/codex-rs/README.md`
