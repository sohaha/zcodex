status: recommend
recommendation_summary: |
  建议采用独立 `[ztok]` 配置块，并把该开关定义为“行为配置档”而不是 feature flag：`behavior = "codex" | "rtk_compatible"`，默认 `codex`。`rtk_compatible` 的首个落地点应只切断 session dedup / near-diff / SQLite 快照落库，不在第一期强拆 `compression.rs` 分发层。这样能先消除 `summary` 现有的身份过粗与原始输出落库风险，同时把实现成本控制在 CLI 桥接 + ztok 运行时 gating 的最小闭环内。
tradeoffs: |
  方案一（推荐）：单一行为枚举放在 `[ztok]`。实现成本低，调用语义清晰，测试矩阵简单，后续可继续扩成更多兼容档。代价是 `rtk_compatible` 初期不会做到所有内部实现都与 RTK 完全同构，只保证用户可见行为更接近 RTK。

  方案二（不推荐）：放进 `[features]` 或拆成多个低层布尔开关（如 `disable_session_dedup` / `disable_near_diff` / `disable_shared_compression`）。短期看更“灵活”，但会把产品语义拆成实现细节，制造无效组合与更高测试负担，也更容易让 CLI、alias、嵌入式 shell rewrite 走出不同口径。
risks: |
  1. 如果只关掉短引用输出而不关掉 `session_dedup` 的 SQLite 读写，`summary` 的原始 stdout/stderr 仍会落库，核心风险没有真正消除。
  2. 如果只在 `summary` 命令单点处理，而不在共享 dedup seam 处理，`read/json/log` 仍会保留相同的 session cache 行为，模式语义会变得不一致。
  3. 如果行为模式只在 `codex ztok` 主路径读取，而 alias 或其它桥接路径不复用同一来源，运行面之间会出现“同配置不同输出”。
  4. 若第一期就强拆 `compression.rs`，实现与验证成本会明显上升，但对风险消减的边际收益不高。
validation_steps: |
  1. 配置层：验证 `ConfigToml` / schema / 文档都出现 `[ztok].behavior`，且 `[features]` 中没有等价新键。
  2. 桥接层：验证 `codex-rs/cli/src/main.rs` 在设置 `CODEX_THREAD_ID` 之外，也把统一的 ztok 行为模式传给 `codex-ztok`。
  3. 兼容模式：验证 `read/json/log/summary` 在 `rtk_compatible` 下不再产生 `[ztok dedup ...]` / `[ztok diff ...]`，并且不会创建或写入 `.ztok-cache/*.sqlite`。
  4. 默认模式：验证 `codex` 默认行为保持现状，现有 dedup/near-diff 测试继续成立。
artifacts_created:
  - .agents/results/architecture/architecture-review-ztok-behavior-mode.md
  - .agents/results/result-architecture.md
