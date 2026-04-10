---
title: 上游 code-mode output_schema 同步时先验本地行为再接收上游测试
date: 2026-04-10
tags: [sync, upstream, code-mode, tools, tests]
paths: [codex-rs/code-mode, codex-rs/tools, codex-rs/rmcp-client, codex-rs/core/tests]
---

## 现象

同步 `openai/codex` 的 `7bbe3b601 Add output_schema to code mode render (#17210)` 时，冲突集中在 `codex-rs/tools/src/tool_registry_plan.rs` 和 `codex-rs/tools/src/tool_registry_plan_tests.rs`。表面上看，上游把 `collect_code_mode_tool_definitions` 切到了 `collect_code_mode_exec_prompt_tool_definitions`，同时更新了测试里对 MCP 工具 TS 声明的断言。

## 误判

我一开始把上游测试改动当成“渲染行为整体都应跟随上游”，直接把 `tool_registry_plan_tests.rs` 的断言改成了上游版本，结果 `cargo nextest run -p codex-tools` 只剩 1 个失败：本地 fork 的单个 MCP 工具描述仍然保留宽类型渲染，`response_length` 仍是 `string`，`tagged_list` 的枚举项也仍收敛成 `string`。

## 根因

这次上游提交改变的是 code-mode exec prompt 的工具定义来源和 `output_schema` 渲染，不等于本地所有“单工具描述”也必须同步到上游的更窄 TS 字面量类型。对分叉仓库来说，“上游测试变了”不是足够的合并依据；必须先验证本地真实行为是否本来就故意不同。

## 处理

1. `tool_registry_plan.rs` 只融合必需部分：
   - 保留本地 `ZMEMORY_TOOL_NAME`、`ZMEMORY_MCP_TOOL_NAMES`、`ztldr`、`request_user_input` 注册链路。
   - 接入上游 `collect_code_mode_exec_prompt_tool_definitions(...)`，让 code-mode exec prompt 吃到新的 `output_schema` 渲染能力。
2. 对失败测试先取证而不是继续猜：
   - 临时打印实际 description。
   - 证实单工具描述仍是本地旧行为后，恢复为本地断言而不是强行跟上游测试文本。

## 验证

- `CARGO_INCREMENTAL=0 RUSTC_WRAPPER= cargo nextest run -p codex-tools`
- `CARGO_INCREMENTAL=0 RUSTC_WRAPPER= cargo nextest run -p codex-code-mode`
- `CARGO_INCREMENTAL=0 RUSTC_WRAPPER= cargo test -p codex-rmcp-client --bins --no-run`
- `CARGO_INCREMENTAL=0 RUSTC_WRAPPER= cargo test -p codex-core --test all --no-run`

## 后续规则

- 上游同步里，测试冲突分三层判断：
  1. 上游测试仅反映上游新行为，且本地无分叉语义：直接跟。
  2. 上游测试涉及本地长期保留行为：先打印/构造真实输出取证，再决定断言。
  3. 不能证明本地行为应被替换时，默认保留本地行为，只吸收可独立融合的上游实现。
- Clouddev / CNB 环境下跑 Rust 验证时，若默认 `CARGO_INCREMENTAL=1` 与 `sccache` 冲突，直接用 `CARGO_INCREMENTAL=0 RUSTC_WRAPPER=` 跑定向验证，避免被环境噪音阻断同步判断。
