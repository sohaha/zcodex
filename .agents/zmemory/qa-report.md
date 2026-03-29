---
type: qa-report
outputFor: [qa]
dependencies: [tech-review, tasks]
---

# QA 验证报告

## 摘要
- 文档对齐基线完成后，本轮已完成 `update`（patch/append/metadata）与 `create`（保留 URI 写入并兼容 `parentUri + title`）两条兼容扩展。
- 在此基础上，本轮继续补齐 upstream `admin export` 的本地 CLI-only 对齐：新增 `codex zmemory export boot|index|recent|glossary`，底层仍复用 `read system://...`，不扩展 REST API/daemon。
- QA 验证仍遵循“改动在哪一层就跑哪一层”：本轮已串行跑通 `codex-zmemory`、`codex-core zmemory_tool_`、`codex-cli --test zmemory`、`codex-core --test all zmemory_function_`，避免多次抢锁。
- CLI run 手工链路已覆盖 patch/append 与 `parentUri + title` / 自动编号 create，确认 JSON 输出与路径生成符合预期。

## 当前已验证项
1. `RUSTC_WRAPPER= cargo test -p codex-zmemory --quiet` ✅（10 个单元测试通过，覆盖 patch/append/metadata 与 `parentUri + title` / 自动编号模式）。
2. `RUSTC_WRAPPER= cargo test -p codex-core zmemory_tool_ --quiet` ✅（7 个相关测试通过，确认 `oldString/newString/append/parentUri/title` 进入 schema 与说明文案）。
3. `RUSTC_WRAPPER= cargo test -p codex-cli --test zmemory --quiet` ✅（14 个 CLI 集成测试通过，覆盖 patch、append、metadata-only、`parentUri + title`、自动编号与冲突参数错误）。
4. `RUSTC_WRAPPER= cargo test -p codex-core --test all zmemory_function_ --quiet` ✅（4 个 handler/e2e 场景通过，含新的 `parentUri + title` create 场景）。
5. 手工 run `cargo run -p codex-cli -- zmemory create --parent-uri core:// --title agent-profile ...`、`cargo run ... create --parent-uri core:// --content ...`、`cargo run ... read core://agent-profile`、`cargo run ... read core://1` ✅，确认 parent/title 与自动编号链路有效。
6. `export` 对齐目标：新增 crate/CLI/core 验证，覆盖 `system://index/<domain>`、`system://recent/<n>` 与 `zmemory export ... --json` 的路径映射与输出一致性。
7. `RUSTC_WRAPPER= cargo test -p codex-zmemory --quiet` ✅（10 个测试通过，含 `index/<domain>` 与 `recent/<n>` system view 扩展）。
8. `RUSTC_WRAPPER= cargo test -p codex-cli --test zmemory --quiet` ✅（15 个 CLI 集成测试通过，含 `zmemory export glossary|index|recent --json`）。
9. `RUSTC_WRAPPER= cargo test -p codex-core zmemory_tool_ --quiet` ✅（7 个 spec 测试通过，system view URI 文案已同步扩展路径）。
10. `RUSTC_WRAPPER= cargo test -p codex-core --test all zmemory_function_ --quiet` ✅（5 个 handler/e2e 场景通过，含 `system://index/core` 调用）。

## 建议的 targeted validation 矩阵
| 层级 | 命令 | 场景 | 备注 |
|------|------|------|------|
| crate | `RUSTC_WRAPPER= cargo test -p codex-zmemory --quiet` | 核心 `tool_api`/`service` 变更 | 必跑；本轮覆盖 update 与 create 兼容模式。|
| spec | `RUSTC_WRAPPER= cargo test -p codex-core zmemory_tool_ --quiet` | 规格/schema/描述 | 必跑；本轮确认 `oldString/newString/append/parentUri/title` 全部落到合同。|
| core handler | `RUSTC_WRAPPER= cargo test -p codex-core --test all zmemory_function_ --quiet` | handler 输出/错误路径 | 当 handler/e2e 有变化时运行；本轮已执行并通过。|
| CLI | `RUSTC_WRAPPER= cargo test -p codex-cli --test zmemory --quiet` | CLI 参数/JSON 输出 | 变更 CLI 参数或输出时必须跑；本轮已执行一次并通过。|

## 已知未验证项
- `RUSTC_WRAPPER= CARGO_INCREMENTAL= just fix -p codex-zmemory -p codex-cli -p codex-core` 已在全部验证完成后执行；按仓库规则，fix 后未再重跑测试。
- `just fix -p codex-zmemory -p codex-cli -p codex-core` 在本轮 export 收尾时尝试执行两次，但因工作区构建体量与锁等待超时，未拿到完整结束结果；已看到相关 crate 编译推进至 `codex-zmemory` / `codex-core` / `codex-cli`，但不能宣称 fix 完成。
- 未来如需引入 `admin export` / memory skill 或进一步改变 create 行为边界，需要新增对应 targeted validation。

## T-006 评估结论
- 本轮继续对齐 upstream `memory skill`，但仅落到“动作映射与边界说明”，没有新增 Rust 接口或 CLI 子命令。
- 结论是：`codex-zmemory` 继续做动作层，memory skill 留在仓库根级 skill / agent 编排层；因此本轮无需新增 crate/CLI/core 测试。
- 本轮验证口径为文档一致性检查：README、tasks、tech-review、qa-report 结论保持一致，且不宣称 upstream baseline 再次前进。

## review/admin parity 收口
- 下一轮 parity audit 选中的主缺口是 review/admin 信号：本地此前缺少 upstream 风格的 `orphaned` / `deprecated` 治理压力暴露。
- 本轮实现后，`stats` 新增 `orphanedMemoryCount` / `deprecatedMemoryCount`，`doctor` 新增对应告警，同时保留本地已有的 FTS / active-memory / dangling-keyword 检查。
- 仓库根级新增最小 `.codex/skills/memory/SKILL.md`，把 bootstrap/recall/capture/refine/linking/review/handoff 映射到现有 `zmemory` 动作。
- 验证结果：
  - `bash /workspace/.mise/tasks/rs-ext cargo test -p codex-zmemory --quiet` ✅（11 passed）
  - `bash /workspace/.mise/tasks/rs-ext cargo test -p codex-cli --test zmemory --quiet` ✅（16 passed）
- alias/trigger parity补充：
  - `stats` 现在也返回 `aliasNodeCount` / `triggerNodeCount`，方便判断 alias/trigger coverage。
  - `doctor` 增加 `aliasNodesMissingTriggers` 告警，帮助 review 识别需要补 trigger 的 alias 目标。
  - CLI 测试 `zmemory_stats_and_doctor_surface_review_pressure` 覆盖 alias/trigger metrics 与 doctor issue。
- 技能资产：新增 `references/cli-recipes.md` 与 `references/review-playbook.md`，让 skill 在当前 CLI 命令基础上可直接调度 recall/capture/refine/linking/review。

## 下一轮回归建议
1. 若继续改 `codex-rs/zmemory` 核心语义，先跑 `cargo test -p codex-zmemory --quiet`。
2. 若改 spec/schema 文案，再跑 `cargo test -p codex-core zmemory_tool_ --quiet`。
3. 若改 handler 输出或 CLI 参数，再跑 `cargo test -p codex-core --test all zmemory_function_ --quiet` 与 `cargo test -p codex-cli --test zmemory --quiet`。
4. 验证历史附录：
   - `RUSTC_WRAPPER= cargo test -p codex-zmemory --quiet` ✅（10 passed）
   - `RUSTC_WRAPPER= cargo test -p codex-core zmemory_tool_ --quiet` ✅（7 passed）
   - `RUSTC_WRAPPER= cargo test -p codex-cli --test zmemory --quiet` ✅（14 passed）
   - `RUSTC_WRAPPER= cargo test -p codex-core --test all zmemory_function_ --quiet` ✅（4 passed）
   - `cargo run -p codex-cli -- zmemory create/read ...` 手工链路 ✅
