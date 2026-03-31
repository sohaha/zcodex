---
type: qa-report
outputFor: [devops, boss]
dependencies: [tasks]
---

# QA 测试报告

## 报告信息
- **功能名称**：tldr-agent-first-optimization
- **版本**：1.6
- **测试日期**：2026-03-31
- **测试者**：QA Agent
- **测试环境**：Ubuntu 22.04 + Rust 1.94.1 toolchain，仓库路径 `/workspace/codex-rs`

## 摘要

> Boss Agent 请优先阅读本节判断是否通过质量门禁。

- **总体结论**：✅ `T-002~T-008` 已有代码落地，外加补齐了“直接调用 tldr 且未传 `project` 时默认回落到 repo root”的修复，并把一条对 ONNX Runtime 动态库有隐式依赖的 handler 测试改成环境无关；更大范围的 67 条相关测试均已通过。
- **已验证通过**：`auto_tldr`、`read_gate`、`router dispatch`、`shell_search_rewrite`、subagent guidance / warm / context 记录、`tldr` 缺省 `project` 的 repo-root 回落，以及完整 `tools::handlers::tldr` 测试组。
- **P0/P1 Bug**：无。
- **E2E 状态**：未执行；当前阶段聚焦 runtime rewrite，不存在独立 UI/API E2E 交付面。
- **阶段 3 状态**：已完成；最新功能回归已验证。随后再次执行了 `just fmt` 与 `just fix -p codex-core`，按仓库约定未再追加测试重跑。

## 1. 测试概要

### 1.1 测试范围
- 功能测试：通过代码审阅确认 rewrite/dispatch、subagent guidance、warm、上下文记录，以及 `tldr` 缺省 `project` 的 repo-root 回落均已接线。
- 单元测试：先对 10 条关键定向测试做 exact 重跑；随后又扩大到 `tools::rewrite::`、`tools::handlers::tldr::`、`build_initial_context_*` 与 `runtime_dispatch_routes_structural_read_file_to_tldr_handler` 共 67 条相关测试。
- 集成测试：未执行完整 crate 级集成套件。
- E2E 测试：未执行。
- 性能/安全测试：未执行。

### 1.2 测试结论

| 指标 | 结果 |
|------|------|
| **总体状态** | 🟢 关键路径与扩大范围回归均完成 |
| **阻塞问题** | 无 |
| **建议发布** | 可以继续后续阶段或按当前范围收口 |

## 2. 验收标准验证

### 2.1 功能需求验证

| FR ID | 描述 | 状态 | 备注 |
|-------|------|------|------|
| FR-001 | 结构化 / 事实 / 混合三类问题统一语义 | 🟢 通过 | `classification.rs` 已落地，`auto_tldr` / `read_gate` / subagent guidance 共享使用 |
| FR-002 | 原生工具拦截 / 重写层（grep/read/shell） | 🟢 通过 | `grep_files`、`read_file`、shell 拦截均已有定向测试通过 |
| FR-002a | 直接 `tldr` 调用的缺省 `project` 回落 | 🟢 通过 | 未传 `project` 时现在默认使用 repo root，避免落到子目录 |
| FR-003 | 工具描述 / agent-first 文档对齐 | 🟢 通过 | `tools/spec.rs` 与 `docs/tldr-agent-first-guidance/tool-description.md` 已更新 |
| FR-004 | 正向 / 负向路由指令共存 | 🟢 通过 | `force_raw_*` / `disable_auto_tldr_once` 已实现，subagent 也能收到 `tldr-first` 指引 |

### 2.2 故事 / AC 验证

| Story | AC ID | 描述 | 状态 | 备注 |
|-------|-------|------|------|------|
| S-001 | AC-1 | 文档里描述三类问题流 | 🟢 通过 | 设计文档与实现一致 |
| S-001 | AC-2 | 结构化问题默认 `tldr-first` | 🟢 通过 | `routes_symbol_searches_to_tldr_context_by_default` 已通过 |
| S-002 | AC-1 | `grep_files` / `read_file` 运行时改写 | 🟢 通过 | 相关测试已通过 |
| S-002 | AC-2 | Shell 搜索 / 计划调用共享语义 | 🟢 通过 | shell 拦截单测通过，subagent guidance 已补齐 |
| S-004 | AC-1 | structural 子任务收到 `tldr-first` 指引 | 🟢 通过 | `build_initial_context_includes_tldr_first_guidance_for_structural_subagents` 已通过 |
| S-004 | AC-2 | degradedMode 被正确传递到子任务上下文 | 🟢 通过 | 同一测试覆盖 degraded mode 文案注入 |
| S-005 | AC-1 | 成功 `tldr` 后上下文记录完整 | 🟢 通过 | `record_result_captures_action_problem_kind_and_degraded_mode` 已通过 |
| S-005 | AC-2 | 首个结构化问题触发 warm | 🟢 通过 | `first_structural_queries_trigger_warm_once` 已通过 |
| S-005 | AC-3 | 直接 `tldr` 未传 `project` 时回落 repo root | 🟢 通过 | `missing_project_defaults_to_repo_root` 已通过 |

## 3. 自动化测试结果

### 3.1 测试汇总

| 测试类型 | 总数 | 通过 | 失败 | 跳过 | 通过率 |
|----------|------|------|------|------|--------|
| 定向单元/运行时测试 | 10 | 10 | 0 | 0 | 100% |
| 扩大范围相关测试 | 67 | 67 | 0 | 0 | 100% |
| 集成测试 | 未运行 | 0 | 0 | 0 | N/A |
| E2E 测试 | 未运行 | 0 | 0 | 0 | N/A |

### 3.2 已通过测试

| 测试 | 命令 | 结果 |
|------|------|------|
| `routes_symbol_searches_to_tldr_context_by_default` | `/tmp/cargo-target-tldr-a4/debug/deps/codex_core-ea2456cbee888947 'tools::rewrite::auto_tldr::tests::routes_symbol_searches_to_tldr_context_by_default' --exact` | 通过 |
| `structural_reads_route_to_tldr_extract` | `/tmp/cargo-target-tldr-a4/debug/deps/codex_core-ea2456cbee888947 'tools::rewrite::read_gate::tests::structural_reads_route_to_tldr_extract' --exact` | 通过 |
| `force_tldr_can_reuse_last_language_for_extensionless_reads` | `/tmp/cargo-target-tldr-a4/debug/deps/codex_core-ea2456cbee888947 'tools::rewrite::read_gate::tests::force_tldr_can_reuse_last_language_for_extensionless_reads' --exact` | 通过 |
| `intercepts_structural_rg_searches_with_tldr_context_suggestion` | `/tmp/cargo-target-tldr-a4/debug/deps/codex_core-ea2456cbee888947 'tools::rewrite::shell_search_rewrite::tests::intercepts_structural_rg_searches_with_tldr_context_suggestion' --exact` | 通过 |
| `intercepts_mixed_find_xargs_rg_searches` | `/tmp/cargo-target-tldr-a4/debug/deps/codex_core-ea2456cbee888947 'tools::rewrite::shell_search_rewrite::tests::intercepts_mixed_find_xargs_rg_searches' --exact` | 通过 |
| `intercepts_globbed_rg_searches_without_promoting_glob_to_pattern` | `/tmp/cargo-target-tldr-a4/debug/deps/codex_core-ea2456cbee888947 'tools::rewrite::shell_search_rewrite::tests::intercepts_globbed_rg_searches_without_promoting_glob_to_pattern' --exact` | 通过 |
| `factual_queries_stay_on_shell_path` | `/tmp/cargo-target-tldr-a4/debug/deps/codex_core-ea2456cbee888947 'tools::rewrite::shell_search_rewrite::tests::factual_queries_stay_on_shell_path' --exact` | 通过 |
| `regex_queries_stay_on_shell_path` | `/tmp/cargo-target-tldr-a4/debug/deps/codex_core-ea2456cbee888947 'tools::rewrite::shell_search_rewrite::tests::regex_queries_stay_on_shell_path' --exact` | 通过 |
| `first_structural_warm_only_marks_requested_after_success` | `/tmp/cargo-target-tldr-a4/debug/deps/codex_core-ea2456cbee888947 'tools::handlers::tldr::tests::first_structural_warm_only_marks_requested_after_success' --exact` | 通过 |
| `missing_project_defaults_to_repo_root` | `/tmp/cargo-target-tldr-a4/debug/deps/codex_core-ea2456cbee888947 'tools::handlers::tldr::tests::missing_project_defaults_to_repo_root' --exact` | 通过 |

### 3.3 扩大范围回归

| 组别 | 数量 | 命令 | 结果 |
|------|------|------|------|
| rewrite 相关 | 26 | `/tmp/cargo-target-tldr-a4/debug/deps/codex_core-ea2456cbee888947 'tools::rewrite::' --test-threads=1` | 通过 |
| tldr handler 相关 | 29 | `/tmp/cargo-target-tldr-a4/debug/deps/codex_core-ea2456cbee888947 'tools::handlers::tldr::' --test-threads=1` | 通过 |
| subagent/build_initial_context 相关 | 11 | `/tmp/cargo-target-tldr-a4/debug/deps/codex_core-ea2456cbee888947 'build_initial_context_' --test-threads=1` | 通过 |
| router dispatch 相关 | 1 | `/tmp/cargo-target-tldr-a4/debug/deps/codex_core-ea2456cbee888947 'runtime_dispatch_routes_structural_read_file_to_tldr_handler' --test-threads=1` | 通过 |

### 3.4 测试工具与方法

| 测试类型 | 使用工具 | 执行命令/方法 | 备注 |
|----------|----------|---------------|------|
| 单元/运行时测试 | libtest 二进制 exact / 前缀过滤重跑 | 见上表 | 使用独立 `CARGO_HOME` / `CARGO_TARGET_DIR` 编译，并直接复用隔离 target 里的 `codex_core-*` 测试二进制执行 exact 与组级过滤，避免 Cargo 锁冲突 |
| 格式化 | `just fmt` | `just fmt` | 通过 |
| 静态修复 | `just fix -p codex-core` | `just fix -p codex-core` | 通过；本轮在格式化/静态修复后未再追加测试重跑，遵循仓库“fix/fmt 后不再重跑测试”的本地约定 |
| 集成测试 | 未执行 | - | - |
| E2E 测试 | 未执行 | - | - |

## 4. 发现的 Bug

- 本轮未发现 P0 / P1 行为回归。
- 已修复一条测试层问题：`run_tldr_handler_with_hooks_emits_end_log` 之前会因 mock daemon 未返回 semantic payload 而落入本地 semantic 路径，隐式依赖 `libonnxruntime.so`；现已改成提供完整 daemon semantic payload，测试不再受环境动态库影响。
- 剩余风险较低；唯一需要显式说明的是，`just fmt` 与 `just fix -p codex-core` 在 10 条定向测试之后执行，因此最终状态属于“功能已验证 + 格式/静态修复已通过”的组合结论。

## 5. 质量门禁判断

- Gate 0：`just fmt` 与 `just fix -p codex-core` 已通过。
- Gate 1：**passed**。
  - 正向证据：使用独立 `CARGO_HOME=/tmp/cargo-home-tldr-a4` 与 `CARGO_TARGET_DIR=/tmp/cargo-target-tldr-a4` 完成编译，并对 10 条目标测试逐条执行 `--exact`，10/10 全部通过。
  - 补充说明：随后在同一隔离 target 上继续执行了 67 条扩大范围相关测试，全部通过；之前受 `libonnxruntime.so` 影响的 handler 日志测试已改成环境无关。
- Gate 2：不适用（本轮 `skipDeploy=true`）。

> 结论：阶段 3 的剩余实现已补齐，包含直接 `tldr` 调用时的 repo-root 回落修复；当前范围内可按已验证完成处理。
