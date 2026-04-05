---
type: qa-report
outputFor: [qa]
dependencies: [tech-review, tasks]
---

# QA 验证报告

## 摘要
- 2026-04-05 收尾修复：补上 `codex-rs/core/tests/suite/mod.rs` 对 `zmemory_e2e` 的模块注册，之前新增的 core e2e 因未被聚合，`cargo test -p codex-core --test all ...` 实际显示 `0 tests`；现在已纳入真实编译与执行路径。
- 当前“查看全部路径”能力以显式 `system://paths` / `system://paths/<domain>` 和 `codex zmemory export paths` 落地，不再把这个需求隐含在 `system://index`。
- `codex-tools` 侧补了窄单测，直接锁定 `zmemory` tool contract 中 `uri`/`limit` 对 `paths` 视图的 discoverability，避免再被无关的全量 toolset 测试噪音掩盖。
- 文档对齐基线完成后，本轮已完成 `update`（patch/append/metadata）与 `create`（保留 URI 写入并兼容 `parentUri + title`）两条兼容扩展。
- 在此基础上，本轮继续补齐 upstream `admin export` 的本地 CLI-only 对齐：新增 `codex zmemory export boot|index|recent|glossary`，底层仍复用 `read system://...`，不扩展 REST API/daemon。
- QA 验证仍遵循“改动在哪一层就跑哪一层”：本轮已串行跑通 `codex-zmemory`、`codex-core zmemory_tool_`、`codex-cli --test zmemory`、`codex-core --test all zmemory_`，避免多次抢锁，并确保 `zmemory_function_*` 与 `zmemory_mcp_*` 都进入验证链路。
- CLI run 手工链路已覆盖 patch/append 与 `parentUri + title` / 自动编号 create，确认 JSON 输出与路径生成符合预期。
- CLI `zmemory export defaults|workspace|boot` 命令现已加入 QA 验证链路，底层分别映射到相应 `system://defaults`/`system://workspace`/`system://boot` 视图；其中 `limit` 的正式合同仅覆盖 `boot|index|paths|recent|glossary|alias`，`defaults/workspace` 不宣称支持分页。
- 新增 `system://paths` / `export paths` 作为显式“全部路径”视图；QA 与 architecture/tech-review 说明它满足 agent 所需的观察能力，`limit` 参数的跨视图合同一并同步到文档/skill 资产。

## 当前已验证项
1. `RUSTC_WRAPPER= cargo test -p codex-zmemory --quiet` ✅（10 个单元测试通过，覆盖 patch/append/metadata 与 `parentUri + title` / 自动编号模式）。
2. `RUSTC_WRAPPER= cargo test -p codex-core zmemory_tool_ --quiet` ✅（7 个相关测试通过，确认 `oldString/newString/append/parentUri/title` 进入 schema 与说明文案）。
3. `RUSTC_WRAPPER= cargo test -p codex-cli --test zmemory --quiet` ✅（14 个 CLI 集成测试通过，覆盖 patch、append、metadata-only、`parentUri + title`、自动编号与冲突参数错误）。
4. `RUSTC_WRAPPER= cargo test -p codex-core --test all zmemory_ --quiet` ✅（当前已扩到 18 个 handler/e2e 场景，覆盖 `zmemory_function_*` 与新增 `zmemory_mcp_*` 映射测试）。
5. 手工 run `cargo run -p codex-cli -- zmemory create --parent-uri core:// --title agent-profile ...`、`cargo run ... create --parent-uri core:// --content ...`、`cargo run ... read core://agent-profile`、`cargo run ... read core://1` ✅，确认 parent/title 与自动编号链路有效。
6. `export` 对齐目标：新增 crate/CLI/core 验证，覆盖 `system://index/<domain>`、`system://recent/<n>` 与 `zmemory export ... --json` 的路径映射与输出一致性。
7. `RUSTC_WRAPPER= cargo test -p codex-zmemory --quiet` ✅（10 个测试通过，含 `index/<domain>` 与 `recent/<n>` system view 扩展）。
8. `RUSTC_WRAPPER= cargo test -p codex-cli --test zmemory --quiet` ✅（15 个 CLI 集成测试通过，含 `zmemory export glossary|index|recent --json`）。
9. `RUSTC_WRAPPER= cargo test -p codex-core zmemory_tool_ --quiet` ✅（7 个 spec 测试通过，system view URI 文案已同步扩展路径）。
10. `RUSTC_WRAPPER= cargo test -p codex-core --test all zmemory_ --quiet` ✅（当前 18 个 handler/e2e 场景通过，含 `system://index/core`、`system://paths` 合同与 MCP alias 映射调用）。

## 建议的 targeted validation 矩阵
| 层级 | 命令 | 场景 | 备注 |
|------|------|------|------|
| crate | `RUSTC_WRAPPER= cargo test -p codex-zmemory --quiet` | 核心 `tool_api`/`service` 变更 | 必跑；本轮覆盖 update 与 create 兼容模式。|
| spec | `RUSTC_WRAPPER= cargo test -p codex-core zmemory_tool_ --quiet` | 规格/schema/描述 | 必跑；本轮确认 `oldString/newString/append/parentUri/title` 全部落到合同。|
| core handler | `RUSTC_WRAPPER= cargo test -p codex-core --test all zmemory_ --quiet` | handler 输出/错误路径 | 当 handler/e2e 有变化时运行；用 `zmemory_` 过滤，确保 `zmemory_function_*` 与 `zmemory_mcp_*` 都被执行。|
| CLI | `RUSTC_WRAPPER= cargo test -p codex-cli --test zmemory --quiet` | CLI 参数/JSON 输出 | 变更 CLI 参数或输出时必须跑；本轮已执行一次并通过。|

## 已知未验证项
- `just fix -p codex-zmemory`、`just fix -p codex-cli`、`just fix -p codex-core` 已在全部验证完成后分别执行；按仓库规则，fix 后未再重跑测试。
- `just fix -p codex-core` 收尾时仍看到非阻塞 clippy warning，主要集中在 `expect_used` / `manual_async_fn`；这些 warning 不影响本轮功能结论，但 fix 不能等同于“工作区已完全无 lint 提示”。
- 未来如需引入 `admin export` / memory skill 或进一步改变 create 行为边界，需要新增对应 targeted validation。

## 文档与 skill 状态
- `.agents/zmemory/architecture.md` / `.agents/zmemory/tech-review.md` 现在明确 `system://paths` 是“查看全部路径”的显式视图，`system://workspace/defaults/alias` 作为本地分叉；`limit` 的正式合同覆盖 `boot/index/paths/recent/glossary/alias`，并已贴合 CLI/skill 展示。
- `.agents/zmemory/qa-report.md` 与 skill 参考材料同步：`limit` 的正式合同覆盖 `zmemory export boot|index|paths|recent|glossary|alias`，`defaults/workspace` 仅作为视图导出入口；`system://paths` 满足 agent 全路径需求，未知 system view 的错误合同已改为显式报错。
- `.codex/skills/memory` 引导仍通过当前 CLI `zmemory create/read/update/export` 等命令驱动，在文档中点出 `stats/doctor` 提供 path/alias-level governance 信号（priorityScore、aliasNodesMissingTriggers、coveragePercent）供 skill 参考。

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
- `system://alias` 视图新增 `coveragePercent` 与 `recommendations`，可直接看到 alias coverage 及缺 trigger 的推荐节点。
- `system://alias` 的 `recommendations` 现在直接带可复制的 `manage-triggers` 命令，skill 与人工 review 都能直接落动作。
- `system://alias` 的 `entries` / `recommendations` 现在还带 `reviewPriority` 与 `priorityScore`，能先治理缺 trigger 且 alias 扇出更高的节点。
- `system://alias` 进一步带上 `priorityReason` 与 `suggestedKeywords`，review 时可以直接看到“为什么先处理这个节点”以及建议补哪些关键词。
- 技能资产：新增 `references/cli-recipes.md` 与 `references/review-playbook.md`，让 skill 在当前 CLI 命令基础上可直接调度 recall/capture/refine/linking/review。
- 技能资产进一步完善：`cli-recipes` 新增 project-init / recall helper 片段，`review-playbook` 补全 alias/trigger review checklist 与 project init checklist，将 `system://alias` 视图编入可执行流程。

## 下一轮回归建议
1. 若继续改 `codex-rs/zmemory` 核心语义，先跑 `cargo test -p codex-zmemory --quiet`。
2. 若改 spec/schema 文案，再跑 `cargo test -p codex-core zmemory_tool_ --quiet`。
3. 若改 handler 输出或 CLI 参数，再跑 `cargo test -p codex-core --test all zmemory_ --quiet` 与 `cargo test -p codex-cli --test zmemory --quiet`。
4. 验证历史附录：
   - `RUSTC_WRAPPER= cargo test -p codex-zmemory --quiet` ✅（10 passed）
   - `RUSTC_WRAPPER= cargo test -p codex-core zmemory_tool_ --quiet` ✅（7 passed）
   - `RUSTC_WRAPPER= cargo test -p codex-cli --test zmemory --quiet` ✅（14 passed）
   - `RUSTC_WRAPPER= cargo test -p codex-core --test all zmemory_ --quiet` ✅（18 passed）
   - `cargo run -p codex-cli -- zmemory create/read ...` 手工链路 ✅

## 2026-03-29 discoverability follow-up
- 本轮不改 `zmemory` 核心语义，只做 discoverability 收口：
  - `codex-rs/core/src/tools/spec.rs` 补齐 `system://alias|alias/<n>` 文档合同。
  - `codex-rs/cli/src/zmemory_cmd.rs` 新增 `zmemory export alias [--limit N]`，与现有 `system://alias` 视图等价。
  - `.codex/skills/memory/` 与 `codex-rs/zmemory/README.md` 改为优先使用 `export alias` 作为 alias review 入口。
- 已验证：
  - `cargo test -p codex-cli --test zmemory --no-run --quiet` ✅（确认 CLI 代码和新增测试可编译）
  - `cargo test -p codex-core zmemory_tool_uri_parameter_documents_system_views --quiet` ✅
- 未完成：
  - `cargo test -p codex-cli --test zmemory --quiet` 在当前环境里会跑到不带 `zmemory` 子命令的 `codex` 可执行文件，无法把失败归因到本轮改动，因此本轮只保留编译验证，不宣称完整 CLI 运行验证通过。

## 2026-03-29 boot/domain/disclosure parity follow-up
- 本轮继续按 parity audit 顺序推进，收口 `boot / VALID_DOMAINS / CORE_MEMORY_URIS` 与 disclosure 治理：
  - `codex-rs/zmemory/src/config.rs` 新增 `VALID_DOMAINS` / `CORE_MEMORY_URIS` 配置读取与默认值。
  - `codex-rs/zmemory/src/service.rs` 对非 system 域做 allowlist 校验，并禁止写入 `system` 保留域。
  - `codex-rs/zmemory/src/system_views.rs` 将 `system://boot` 改为按 `CORE_MEMORY_URIS` 锚点导出，并显式返回 `missingUris`。
  - `codex-rs/zmemory/src/doctor.rs`、`service.rs` 为 disclosure 治理补齐 `pathsMissingDisclosure` / `disclosuresNeedingReview` 指标与告警。
  - `.codex/skills/memory/` 与 `codex-rs/zmemory/README.md` 同步把 disclosure review 纳入 `stats` / `doctor` / skill flow。
- 已验证：
  - `cargo test -p codex-zmemory --quiet` ✅
  - `cargo test -p codex-cli --test zmemory --no-run --quiet` ✅
- 仍未宣称完成：
  - `cargo test -p codex-cli --test zmemory --quiet` 仍受当前环境里的 `codex` 二进制解析限制影响；本轮继续只保留编译级 CLI 验证，不把运行期失败伪装成本轮回归失败。

## 2026-03-29 search parity + skill closure follow-up
- 本轮继续收口 search parity 与文档/skill 资产，不扩架构：
  - `codex-rs/zmemory/src/service.rs` 已对齐显式 domain 错误、separator-normalized alias 查询、node 级去重、priority/path/uri 排序、自定义 snippet 回退、glossary trigger 刷新，以及 CJK token-boundary 语义。
  - `codex-rs/zmemory/README.md` 现已同步这些 search 合同与 review 入口。
  - `.codex/skills/memory/` 补齐最小 `agents/openai.yaml`、`admin-workflows.md`、`path-conventions.md`、`usage-modes.md`、`skills-outline.md`，并把 search parity 约束写入 skill/reference。
- 已验证：
  - `cargo test -p codex-zmemory search_matches_alias_via_separator_normalized_query --quiet` ✅
  - `cargo test -p codex-zmemory search_dedupes_aliases_and_orders_by_priority_then_path_length --quiet` ✅
  - `cargo test -p codex-zmemory search_snippet_prefers_literal_then_token_then_fallback --quiet` ✅
  - `cargo test -p codex-zmemory search_snippet_falls_back_to_content_for_disclosure_and_uri_hits --quiet` ✅
  - `cargo test -p codex-zmemory search_snippet_preserves_multibyte_boundaries --quiet` ✅
  - `cargo test -p codex-zmemory glossary_add_and_remove_refresh_search_contract --quiet` ✅
  - `cargo test -p codex-zmemory search_uses_token_boundaries_instead_of_raw_cjk_substrings --quiet` ✅
- 说明：
  - 本轮 docs/skill 变更未新增 Rust 行为，因此未重复扩跑更大矩阵；CLI 端到端仍建议在干净环境里复核，以避免当前工作区无关构建噪音。
