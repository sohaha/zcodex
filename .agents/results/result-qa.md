Status: completed

Summary:
- 对 `/workspace/.agents/plan/2026-04-20-ztok-general-content-compression.md`、`/workspace/codex-rs/cli/tests/ztok.rs`、`/workspace/codex-rs/ztok/src/` 做了面向“下一阶段 ztok 验证策略”的审查。
- 已执行最小自动化取证：
  - `cd /workspace/codex-rs && env -u RUSTC_WRAPPER cargo test -p codex-ztok`
  - `cd /workspace/codex-rs && env -u RUSTC_WRAPPER cargo test -p codex-cli --test ztok`
- 结论：当前 exact dedup、basic/enhanced 行为切换、`read/json/log/summary` 的共享入口都已有基础验证，但候选能力 1/2/3/4/5 仍存在中等级别的验证与架构缺口，适合拆成独立 issue 执行。

Files changed:
- `.agents/results/result-qa.md`

Acceptance criteria checklist:
- [x] 审阅计划文档、现有实现和 CLI/库测试
- [x] 运行 ztok 相关自动化测试确认当前覆盖边界
- [x] 为 5 个候选方向给出自动化入口、测试类型、手动检查、风险、触发信号、回滚策略
- [x] 指出应拆分为独立 issue 的范围
- [x] 仅报告已由源码或测试结果证实的结论

## Review Result: WARNING

### CRITICAL
- None

### HIGH
- None

### MEDIUM
- `codex-rs/ztok/src/session_dedup.rs:38` — 生产路径始终把 `NearDuplicateConfig::default()` 硬编码进 dedup 入口，CLI 只桥接了行为模式，没有 near-diff/dedup 策略配置 seam；候选 1 若不先拆独立 issue，就无法稳定验证“配置变更真的影响阈值、候选数、回退策略”。 — remediation code:
```rust
pub(crate) struct SessionDedupSettings {
    pub near_duplicate: NearDuplicateConfig,
    pub cache_policy: CachePolicy,
}

pub(crate) fn dedup_output_with_settings(
    source_name: &str,
    raw_content: &str,
    output_signature: &str,
    result: CompressionResult,
    settings: &SessionDedupSettings,
) -> CompressionResult
```

- `codex-rs/ztok/src/session_dedup.rs:215` — 会话缓存 schema 只负责建表和补列，未定义 TTL、行数上限、清理时机或 schema 演进验证；候选 2 若不先补治理策略，SQLite 会长期累积快照，验证也无法覆盖“缓存膨胀、锁冲突、损坏后恢复”。 — remediation code:
```rust
connection.execute(
    "DELETE FROM session_cache WHERE created_at < ?1",
    params![expires_before],
)?;
connection.execute(
    "DELETE FROM session_cache
     WHERE rowid NOT IN (
       SELECT rowid FROM session_cache
       ORDER BY created_at DESC
       LIMIT ?1
     )",
    params![max_rows],
)?;
```

- `codex-rs/ztok/src/near_dedup.rs:47` — 近重复分析仍是通用 token simhash + 行级 LCS；`json` 与 `log` 只是先被渲染成文本摘要，没有结构化 candidate/snapshot 语义，候选 3 不能和通用 near-diff 混做一个 issue。 — remediation code:
```rust
enum SnapshotKind {
    Text(String),
    Json(serde_json::Value),
    NormalizedLog(Vec<LogEvent>),
}

struct DedupSnapshot {
    kind: SnapshotKind,
    rendered_output: String,
}
```

- `codex-rs/ztok/src/summary.rs:48` — dedup/fallback 决策只体现在最终 `output` 文本，CLI 没有可观察的调试视图；候选 4 若直接混进其它实现，会继续靠读 SQLite 或猜测输出定位问题。 — remediation code:
```rust
#[derive(Serialize)]
struct CompressionDecisionView {
    source: String,
    output_kind: String,
    fallback: Option<String>,
    signature: String,
}

if debug_enabled {
    eprintln!("{}", serde_json::to_string(&decision_view)?);
}
```

- `codex-rs/ztok/src/container.rs:189` — `docker logs` / `kubectl logs` / `docker compose logs` 仍直接调用 `log_cmd::run_stdin_str`，`aws_cmd` / `curl_cmd` 也各自走局部 JSON 压缩逻辑，没有接入会话 dedup 或共享行为模式；候选 5 需要按入口家族拆 issue，否则回归面会过大。 — remediation code:
```rust
let compressed = compression::compress_for_behavior(request, behavior)?;
let output = session_dedup::dedup_output(
    source_name,
    raw_content,
    output_signature,
    compressed,
);
```

### LOW
- `codex-rs/cli/tests/ztok.rs:186` — CLI 集成测试已覆盖 exact dedup 和 basic 模式，但没有任何 `[ztok diff ...]` 命中断言，也没有缓存治理/清理用例；下一阶段若引入 near-diff 配置或缓存治理，现有 CLI 入口不足以证明跨命令行为稳定。 — remediation code:
```rust
#[test]
fn ztok_json_emits_diff_for_near_duplicate_payloads() -> Result<()> {
    // 1. 首次输出完整内容
    // 2. 第二次输出结构化 diff
    // 3. 断言 stdout 包含 "[ztok diff"
}
```

## 候选能力验证策略

### 1. dedup / near-diff 策略配置化
- 自动化验证入口：
  - `cd /workspace/codex-rs && env -u RUSTC_WRAPPER cargo test -p codex-ztok`
  - `cd /workspace/codex-rs && env -u RUSTC_WRAPPER cargo test -p codex-cli --test ztok`
  - 新增一个只验证配置桥接的 CLI 用例，覆盖 `codex ztok ...` 与 alias `ztok ...` 两条入口。
- 应新增的测试类型：
  - `session_dedup` 单元测试：不同 `NearDuplicateConfig` 下是否从 `Full` 切到 `Diff` / `Fallback`。
  - `cli/tests/ztok.rs` 集成测试：写入 `[ztok]` 配置后，`basic/enhanced` 之外的 near-diff 阈值或开关也确实生效。
  - 配置优先级测试：CLI override > profile > config file > default。
- 手动检查：
  - 用两组只差 1 行、3 行、10 行的样本文件跑 `ztok read/json/log`，确认阈值变化会影响 `Full` / `Diff` 分界。
  - 同时验证 `codex ztok` 与 alias `ztok` 输出完全一致。
- 主要风险：
  - CLI 配置与库内默认值分叉。
  - 阈值过多导致行为不可预测。
  - basic 模式意外重新碰到 near-diff。
- 触发信号：
  - 同样输入在两次运行中一会儿 `Full`、一会儿 `Diff`。
  - `codex ztok` 与 alias `ztok` 对同一配置给出不同结果。
  - basic 模式仍出现 `[ztok diff]` 或 `[ztok dedup]`。
- 回滚策略：
  - 保留现有默认值路径，先关闭新增配置桥接，只回退到 default-only。
- 是否应拆独立 issue：
  - 应拆。
  - 理由：同时涉及 `cli/src/main.rs` 配置桥接与 `ztok/src/session_dedup.rs` 运行时合同，是单独的接口变更面。

### 2. 会话缓存治理
- 自动化验证入口：
  - `cargo test -p codex-ztok` 作为主入口。
  - 新增面向 SQLite 的临时目录测试，不需要放大到全仓。
- 应新增的测试类型：
  - schema migration 测试：旧库缺列时自动补列后仍可读写。
  - retention/cleanup 测试：达到上限后旧行被清理。
  - corruption/permission 测试：缓存损坏、目录不可写时显式 fallback。
  - CLI smoke：二次运行命中 dedup 后，治理逻辑不会误删当前 session 记录。
- 手动检查：
  - 连续运行同一命令 100+ 次，观察 `.ztok-cache/<session>.sqlite` 大小是否受控。
  - 人工损坏 sqlite 文件、改成目录、改权限只读，确认输出回退明确。
- 主要风险：
  - 快照无上限增长。
  - sqlite 锁冲突导致命令抖动。
  - schema 升级失败让整个 dedup 长期不可用。
- 触发信号：
  - 同一 session 运行越多越慢。
  - 磁盘上 `.ztok-cache` 持续增长。
  - `DedupCacheUnavailable` 回退频繁出现。
- 回滚策略：
  - 临时关闭持久化写入，只保留共享压缩和无缓存路径。
- 是否应拆独立 issue：
  - 应拆。
  - 如果范围扩大到“清理策略 + 损坏恢复 + 诊断统计”，建议再拆 2 个子 issue。

### 3. JSON / 日志类型感知 near-diff
- 自动化验证入口：
  - `cargo test -p codex-ztok`
  - `cargo test -p codex-cli --test ztok`
- 应新增的测试类型：
  - JSON:
    - 字段新增/删除/重排的结构化 diff 测试。
    - 数值或字符串关键字段变化不能被“结构相似”吞掉的测试。
  - Log:
    - 只有时间戳/UUID 变化时应命中近重复的测试。
    - 真实错误消息变化时不能被误压成“无意义 diff”的测试。
  - CLI:
    - `ztok json -` 与 `ztok log` 的 `[ztok diff ...]` 命中测试。
- 手动检查：
  - JSON：比较同结构但值不同的 payload，确认 diff 能指出关键字段。
  - 日志：比较只变时间戳和只变错误栈的两组样本，确认前者压缩、后者保真。
- 主要风险：
  - 结构化归一化过强，吞掉重要值变化。
  - 日志归一化把不同错误归成同一类。
- 触发信号：
  - JSON 只改字段顺序却仍产生大段 diff 噪声。
  - 不同错误码/错误文本被错误视作近重复。
- 回滚策略：
  - 对 JSON/log 单独禁用 near-diff，只保留 exact dedup 或 full output。
- 是否应拆独立 issue：
  - 必须拆。
  - 建议拆成 2 个 issue：`json type-aware near-diff`、`log type-aware near-diff`。

### 4. 压缩决策调试视图
- 自动化验证入口：
  - `cargo test -p codex-ztok`
  - 新增一个 CLI 文本测试，断言显式 flag/env 下能看到决策元数据。
- 应新增的测试类型：
  - decision view 序列化测试。
  - 各分支覆盖：`Full` / `ShortReference` / `Diff` / `Fallback`。
  - redaction 测试：调试输出不得泄露完整原文快照。
- 手动检查：
  - 用 `read/json/log/summary` 各跑一遍，确认能看到 signature、output kind、fallback reason，而不是只看终端正文猜。
- 主要风险：
  - 调试视图泄露敏感输出。
  - 把调试格式做成默认用户可见，污染正常输出。
- 触发信号：
  - 排查 near-diff 命中问题时必须手查 sqlite。
  - 用户无法区分“没命中”“低置信回退”“缓存坏了”。
- 回滚策略：
  - 仅保留显式 debug flag/env，撤回默认展示。
- 是否应拆独立 issue：
  - 应拆。
  - 理由：范围小、收益直接，可作为其它候选 issue 的观测前置。

### 5. 扩展共享压缩到更多高价值入口
- 自动化验证入口：
  - `cargo test -p codex-ztok`
  - 为每个新接入入口补最小命令级测试，不要一次性扩到 workspace 其它 crate。
- 应新增的测试类型：
  - `aws` / `curl`：
    - JSON 输出应走共享 JSON 压缩和 session dedup。
    - 非 JSON 输出保持可回退。
  - `docker logs` / `kubectl logs` / `docker compose logs`：
    - 重复运行命中 dedup。
    - near-diff 行为与 `ztok log` 一致。
  - 如继续扩到 build/test wrapper，再分别补 snapshot 或文本断言。
- 手动检查：
  - `aws sts get-caller-identity`、普通 `curl` JSON 返回、`docker/kubectl` 日志样本各验证一次。
  - 核对这些入口在 basic 模式下也保持无 session cache。
- 主要风险：
  - 旁路入口各自已有格式化逻辑，接共享内核时会改用户可见输出。
  - 非结构化输出被误判成 JSON 或日志。
- 触发信号：
  - 同类输出在 `ztok json/log` 与 `aws/curl/container` 上压缩行为不一致。
  - 重复调用这些入口没有任何 dedup 命中。
- 回滚策略：
  - 按入口家族单独回退，不影响 `read/json/log/summary` 现有主路径。
- 是否应拆独立 issue：
  - 应拆，而且至少拆成 2 组：
  - `structured fetchers`：`aws`、`curl`
  - `container/log surfaces`：`docker logs`、`kubectl logs`、`docker compose logs`

