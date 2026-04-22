# Local Fork Features

这个文件由 `local-fork-features.json` 渲染生成，不手工编辑。

## 文件角色

- 权威基线：`/workspace/.codex/skills/sync-openai-codex-pr/references/local-fork-features.json`
- 展示报告：当前文件
- 候选变更：默认放在临时路径，由 `discover` 产出、经人工或主代理审阅后再 `promote`

## 命令

```bash
node /workspace/.codex/skills/sync-openai-codex-pr/scripts/local_fork_feature_audit.mjs discover --repo /workspace --base-ref <sha> --head-ref HEAD --output /tmp/sync-openai-codex-pr-discover.json
node /workspace/.codex/skills/sync-openai-codex-pr/scripts/local_fork_feature_audit.mjs merge-candidates --dir /tmp/sync-openai-codex-pr-candidates --output /tmp/sync-openai-codex-pr-candidate-ops.json
node /workspace/.codex/skills/sync-openai-codex-pr/scripts/local_fork_feature_audit.mjs promote --candidate /tmp/sync-openai-codex-pr-candidate-ops.json
node /workspace/.codex/skills/sync-openai-codex-pr/scripts/local_fork_feature_audit.mjs render --repo /workspace
node /workspace/.codex/skills/sync-openai-codex-pr/scripts/local_fork_feature_audit.mjs check --repo /workspace
```

`refresh` 是 `render --repo <repo>` 的兼容别名。
`discover` 默认只会从 `STATE.md:last_sync_commit` 推断范围，而且该提交必须仍是 `HEAD` 的祖先。
不会再隐式回退到 `last_synced_sha`；如果你刻意要看更宽的区间，显式传 `--base-ref <last_synced_sha>` 或 `--merge-base-ref <ref>`。
`--base-ref` 和 `--merge-base-ref` 互斥；脚本会拒绝含糊调用。
`merge-candidates` 会把子代理目录里的 candidate ops 合并成一个待审阅文件；同一 feature id 出现互相矛盾的 upsert/remove 会直接失败。

## Candidate Ops Shape

```json
{
  "operations": [
    { "action": "upsert", "feature": { "...": "full feature object" } },
    { "action": "remove", "id": "obsolete-feature-id", "reason": "why it is obsolete" }
  ]
}
```

## Approved Baseline

| ID | Kind | Area |
| --- | --- | --- |
| `wire-api-streaming-chat-anthropic` | `local_behavior` | codex-rs/core + codex-rs/codex-api |
| `responses-max-output-tokens-from-provider` | `local_behavior` | codex-rs/core |
| `zconfig-layer-loading` | `local_behavior` | codex-rs/core config |
| `models-manager-provider-overrides` | `local_behavior` | codex-rs/models-manager |
| `responses-reasoning-content-strip` | `local_behavior` | codex-rs/core + codex-rs/protocol |
| `reference-context-reinjection-baseline` | `local_behavior` | codex-rs/core session/context_manager |
| `auto-tldr-routing-default` | `local_behavior` | codex-rs/tools |
| `local-crates-zmemory-ztok` | `local_surface` | codex-rs workspace |
| `cli-zmemory-ztok-ztldr-surface` | `local_surface` | codex-rs/cli |
| `resume-fork-provider-bridge` | `local_behavior` | codex-rs/cli + codex-rs/tui |
| `buddy-surface` | `local_surface` | codex-rs/tui + codex-rs/app-server |
| `chinese-localization-sentinels` | `localized_behavior` | codex-rs/cli + codex-rs/tui + codex-rs/tools + codex-rs/app-server |
| `session-warning-steer-localization-bridge` | `localized_behavior` | codex-rs/core + codex-rs/app-server + codex-rs/tui + tests |
| `community-branding-and-release-links` | `localized_behavior` | README + install/update surfaces |
| `zoffsec-native-command-workflow` | `local_surface` | codex-rs/cli + codex-rs/tui + codex-rs/rollout |
| `local-analysis-tools-runtime-wiring` | `local_behavior` | codex-rs/tools + codex-rs/core + codex-rs/mcp-server |
| `pending-input-routing-and-zmemory-recall` | `local_behavior` | codex-rs/core session/tasks/tools |

### `wire-api-streaming-chat-anthropic`
- summary: 为 WireApi::Chat 和 WireApi::Anthropic 提供真实 streaming，而不是 runtime panic 占位。
- better_when: upstream 以同等或更好的方式同时覆盖 Chat/Anthropic streaming，并继续透传 effort、summary、service_tier 与正确 endpoint telemetry。
- checks:
  - `regex` `codex-rs/core/src/client.rs`: `async fn stream_chat_api\(`
  - `regex` `codex-rs/core/src/client.rs`: `async fn stream_anthropic_api\(`
  - `regex` `codex-rs/core/src/client.rs`: `CHAT_COMPLETIONS_ENDPOINT`
  - `regex` `codex-rs/core/src/client.rs`: `ANTHROPIC_MESSAGES_ENDPOINT`
  - `regex` `codex-rs/codex-api/src/endpoint/anthropic.rs`: `pub struct AnthropicClient`

### `responses-max-output-tokens-from-provider`
- summary: Responses 请求继续从 provider 元数据读取 max_output_tokens，而不是静态写死。
- better_when: upstream 提供了更明确的 provider 级输出上限策略，且不会让本地 provider 配置回退成硬编码 None。
- checks:
  - `regex` `codex-rs/core/src/client.rs`: `let max_output_tokens = self\s*\.\s*client\s*\.\s*state\s*\.\s*provider\s*\.\s*info\(\)\s*\.\s*max_output_tokens\s*\.\s*filter\(\|v\| \*v > 0\)`

### `zconfig-layer-loading`
- summary: 显式加载 $CODEX_HOME/zconfig.toml，并把它放在 User 与 Project 之间。
- better_when: upstream 原生提供同等层级和优先级的 zconfig 装载逻辑，且不改变本地既有覆盖顺序。
- checks:
  - `regex` `codex-rs/core/src/config_loader/mod.rs`: `ZCONFIG_TOML_FILE`
  - `regex` `codex-rs/core/src/config_loader/mod.rs`: `ConfigLayerSource::ZConfig`
  - `regex` `codex-rs/core/src/config_loader/mod.rs`: `layers\.push\(zconfig_layer\)`

### `models-manager-provider-overrides`
- summary: 保留 provider.model_catalog 过滤、skip_reasoning_popup 传播、按 provider 选择默认远端模型目录，以及本地 synthetic/fallback ModelInfo 的字段完整性。
- better_when: upstream 把 provider.model_catalog、skip_reasoning_popup、Anthropic 默认模型目录和本地 synthetic ModelInfo 的字段补齐都整合成更完整的实现，且本地配置行为不退化。
- checks:
  - `regex` `codex-rs/models-manager/src/manager.rs`: `provider_info\.model_catalog`
  - `regex` `codex-rs/models-manager/src/manager.rs`: `provider_info\.skip_reasoning_popup`
  - `regex` `codex-rs/models-manager/src/manager.rs`: `default_remote_models_for_provider\(&provider_info\)`
  - `regex` `codex-rs/models-manager/src/manager.rs`: `anthropic_model_catalog\(`
  - `regex` `codex-rs/models-manager/src/manager.rs`: `max_context_window: None`
  - `regex` `codex-rs/models-manager/src/model_info.rs`: `max_context_window: None`

### `responses-reasoning-content-strip`
- summary: Responses replay 时剥离 raw reasoning.content，保留 summary / encrypted_content，避免出站请求变成非法 payload。
- better_when: upstream 提供更靠近出站层的统一处理，并仍保证 raw reasoning_text 不会回传给 Responses API。
- checks:
  - `regex` `codex-rs/core/src/client_common.rs`: `ResponseItem::Reasoning \{ content, \.\. \}`
  - `regex` `codex-rs/core/src/client_common.rs`: `\*content = None;`
  - `regex` `codex-rs/protocol/src/models.rs`: `skip_serializing_if = "should_serialize_reasoning_content"`

### `reference-context-reinjection-baseline`
- summary: resume、compact 和 replacement history 之后继续维护 reference_context_item 基线与全量上下文重注入。
- better_when: upstream 改成新的上下文基线机制，但仍完整覆盖 replacement history、clear baseline 和 full reinjection 语义。
- checks:
  - `regex` `codex-rs/core/src/session/mod.rs`: `record_context_updates_and_set_reference_context_item`
  - `regex` `codex-rs/core/src/context_manager/history.rs`: `replacement_reference_context_item`
  - `regex` `codex-rs/core/src/context_manager/history.rs`: `self\.reference_context_item = None;`

### `auto-tldr-routing-default`
- summary: 工具配置默认继续启用 auto_tldr_routing，并保留显式 with_auto_tldr_routing 链路。
- better_when: upstream 用新的工具路由配置替换了 auto_tldr_routing，且默认行为不回退。
- checks:
  - `regex` `codex-rs/tools/src/tool_config.rs`: `AutoTldrRoutingMode::default\(\)`
  - `regex` `codex-rs/tools/src/tool_config.rs`: `with_auto_tldr_routing`

### `local-crates-zmemory-ztok`
- summary: 本地分叉附加 crate `native-tldr`、`zmemory` 与 `ztok` 必须继续存在，并保持 workspace member / dependency 接线完整。
- better_when: 只有在本地确定把这些 crate 整体迁移或替换到新的路径，并同步更新这里的检查路径与 Cargo workspace 接线检查时，才允许变更。
- checks:
  - `exists` `codex-rs/native-tldr`
  - `exists` `codex-rs/zmemory`
  - `exists` `codex-rs/ztok`
  - `regex` `codex-rs/Cargo.toml`: `"native-tldr"`
  - `regex` `codex-rs/Cargo.toml`: `"zmemory"`
  - `regex` `codex-rs/Cargo.toml`: `"ztok"`
  - `regex` `codex-rs/Cargo.toml`: `codex-native-tldr\s*=\s*\{\s*path\s*=\s*"native-tldr"`
  - `regex` `codex-rs/Cargo.toml`: `codex-zmemory\s*=\s*\{\s*path\s*=\s*"zmemory"`
  - `regex` `codex-rs/Cargo.toml`: `codex-ztok\s*=\s*\{\s*path\s*=\s*"ztok"`

### `cli-zmemory-ztok-ztldr-surface`
- summary: 顶层 `codex` CLI 必须继续暴露 `ztok`、`ztldr` 与 `zmemory` 子命令，并保留对应 dispatch 与 help 汉化哨兵。
- better_when: 只有在 upstream 原生提供等效 CLI surface，且本地不再需要这些分叉入口或其汉化收口时，才允许迁移；迁移前必须先把新的入口路径与哨兵更新到这里。
- checks:
  - `regex` `codex-rs/cli/src/main.rs`: `Ztok\(ZtokArgs\)`
  - `regex` `codex-rs/cli/src/main.rs`: `name = "ztldr"`
  - `regex` `codex-rs/cli/src/main.rs`: `Zmemory\(ZmemoryCli\)`
  - `regex` `codex-rs/cli/src/main.rs`: `visible_alias = "r"`
  - `regex` `codex-rs/cli/src/main.rs`: `run_tldr_command\(tldr_cli\)`
  - `regex` `codex-rs/cli/src/main.rs`: `run_zmemory_command\(zmemory_cli\)`
  - `regex` `codex-rs/cli/src/main.rs`: `localize_help_output`
  - `regex` `codex-rs/cli/src/main.rs`: `显示帮助（使用 '-h' 查看摘要）`
  - `regex` `codex-rs/cli/src/main.rs`: `显示版本`

### `resume-fork-provider-bridge`
- summary: `resume` / `fork` 这类复用 `TuiCli` 的交互子命令，继续允许通过 `-P/--provider` 与 `--local-provider` 切换 model_provider，且 merge 后真正写入最终 interactive 配置。
- better_when: upstream 把 interactive CLI 参数合并统一收敛为等效或更强的实现，并继续保证 `resume` / `fork` 等子命令不会在 bridge 阶段静默丢失 provider / local-provider 等 interactive 参数；迁移前必须先把新的桥接点和回归测试锚点更新到这里。
- checks:
  - `regex` `codex-rs/tui/src/cli.rs`: `pub provider: Option<String>,`
  - `regex` `codex-rs/tui/src/cli.rs`: `pub oss_provider: Option<String>,`
  - `regex` `codex-rs/cli/src/main.rs`: `interactive\.provider = Some\(provider\);`
  - `regex` `codex-rs/cli/src/main.rs`: `interactive\.oss_provider = Some\(oss_provider\);`
  - `regex` `codex-rs/cli/src/main.rs`: `fn resume_merges_option_flags_and_full_auto\(`
  - `regex` `codex-rs/cli/src/main.rs`: `assert_eq!\(interactive\.provider\.as_deref\(\), Some\("oss"\)\);`
  - `regex` `codex-rs/cli/src/main.rs`: `fn fork_merges_provider_flags\(`
  - `regex` `codex-rs/cli/src/main.rs`: `assert_eq!\(interactive\.oss_provider\.as_deref\(\), Some\("lmstudio"\)\);`

### `buddy-surface`
- summary: Buddy 交互面、配置落盘事件和 app-server 通知桥接仍然存在，不被 upstream TUI/app-server 改动吞掉。
- better_when: upstream 原生提供等效 buddy 能力且本地不再需要维护分叉实现，或者本地把 buddy 正式迁移到新模块并同步更新检查点。
- checks:
  - `regex` `codex-rs/tui/src/buddy/mod.rs`: `小伙伴已孵化`
  - `regex` `codex-rs/tui/src/chatwidget.rs`: `小伙伴命令：`
  - `regex` `codex-rs/tui/src/slash_command.rs`: `SlashCommand::Buddy`
  - `regex` `codex-rs/tui/src/app_event.rs`: `PersistBuddyVisibility\(bool\)`
  - `regex` `codex-rs/tui/src/app_event.rs`: `PersistBuddyFullVisibility`
  - `regex` `codex-rs/tui/src/app.rs`: `AppEvent::PersistBuddyVisibility\(visible\)`
  - `regex` `codex-rs/tui/src/app.rs`: `AppEvent::PersistBuddyFullVisibility`
  - `regex` `codex-rs/app-server/src/bespoke_event_handling.rs`: `EventMsg::BuddySoulGenerated`
  - `regex` `codex-rs/app-server/src/bespoke_event_handling.rs`: `EventMsg::BuddyReaction`

### `chinese-localization-sentinels`
- summary: 用高频哨兵文案检查中文化输出没有被 upstream 英文重新覆盖。
- better_when: 用户可见链路已迁移到新的源码位置，且新的实现保持自然中文表达；需要先更新这里的哨兵文案位置。
- checks:
  - `regex` `codex-rs/cli/src/main.rs`: `若未指定子命令，选项会转发到交互式命令行界面`
  - `regex` `codex-rs/cli/src/main.rs`: `以非交互模式运行 Codex`
  - `regex` `codex-rs/cli/src/main.rs`: `已在 config\.toml 中启用功能`
  - `regex` `codex-rs/tui/src/slash_command.rs`: `创建 AGENTS\.md 文件，为 Codex 提供指令`
  - `regex` `codex-rs/tools/src/request_user_input_tool.rs`: `request_user_input 在 \{mode_name\} 模式不可用`
  - `regex` `codex-rs/tui/src/bottom_pane/feedback_view.rs`: `请使用以下链接提交 Issue`
  - `regex` `codex-rs/tui/src/app.rs`: `保存并关闭外部编辑器以继续`
  - `regex` `codex-rs/tui/src/app.rs`: `因 SKILL\.md 文件无效，已跳过加载 \{error_count\} 个技能`
  - `regex` `codex-rs/tui/src/onboarding/trust_directory.rs`: `你当前位于 `
  - `regex` `codex-rs/tui/src/history_cell.rs`: `开始使用时，请描述一个任务，或试试这些命令`
  - `regex` `codex-rs/features/src/lib.rs`: `外部配置迁移`
  - `regex` `codex-rs/app-server/src/bespoke_event_handling.rs`: `加载 rollout`
  - `regex` `codex-rs/app-server/src/bespoke_event_handling.rs`: `审查器未输出任何回复`

### `session-warning-steer-localization-bridge`
- summary: `core/src/session/mod.rs` 与 `core/src/session/turn_context.rs` 的中文 steer 错误和 warning 文案必须在 app-server 映射、tui 解析和回归测试里保持一致，避免同步上游英文实现时只改一层导致桥接回归。
- better_when: upstream 把这条错误/警告链路统一收敛成不依赖脆弱字符串解析的等效或更强实现，并同步覆盖 warning 前缀、active-turn race、fallback 模型 warning 和 steer 错误映射；迁移前必须先把新的桥接点与回归测试锚点更新到这里。
- checks:
  - `regex` `codex-rs/core/src/session/mod.rs`: `当前没有可追加输入的活跃轮次`
  - `regex` `codex-rs/core/src/session/mod.rs`: `期望的活跃轮次 ID 为`
  - `regex` `codex-rs/core/src/session/mod.rs`: `已为此会话禁用 `js_repl``
  - `regex` `codex-rs/core/src/session/mod.rs`: `警告：`
  - `regex` `codex-rs/core/src/session/turn_context.rs`: `未找到模型 `\{\}` 的元数据，已改用兜底元数据；`
  - `regex` `codex-rs/app-server/src/codex_message_processor.rs`: `无法向审查轮次追加输入`
  - `regex` `codex-rs/app-server/src/codex_message_processor.rs`: `输入不能为空`
  - `regex` `codex-rs/tui/src/app.rs`: `期望的活跃轮次 ID 为 ``
  - `regex` `codex-rs/tui/src/app.rs`: `当前没有可追加输入的活跃轮次`
  - `regex` `codex-rs/analytics/src/analytics_client_tests.rs`: `无法向审查轮次追加输入`
  - `regex` `codex-rs/core/tests/suite/js_repl.rs`: `已为此会话禁用 `js_repl``
  - `regex` `codex-rs/core/tests/suite/safety_check_downgrade.rs`: `警告：`
  - `regex` `codex-rs/app-server/tests/suite/v2/safety_check_downgrade.rs`: `警告：`
  - `regex` `codex-rs/core/src/session/tests.rs`: `警告：too many unified exec processes`
  - `regex` `codex-rs/core/src/session/tests.rs`: `未找到模型 `mystery-model` 的元数据，已改用兜底元数据；`

### `community-branding-and-release-links`
- summary: 社区分叉 branding 与 release/install 链接继续指向 sohaha/zcodex。
- better_when: 仓库决定统一回官方 branding，或者 branding 入口迁移到新文件并同步更新这里的检查路径。
- checks:
  - `regex` `README.md`: `zcodex.*社区维护分支|社区维护分支.*zcodex|由 <a href="https://github\.com/sohaha">sohaha</a> 维护`
  - `regex` `codex-rs/README.md`: `https://github\.com/sohaha/zcodex/releases`
  - `regex` `codex-rs/tui/src/update_action.rs`: `@sohaha/zcodex`
  - `regex` `docs/install.md`: `https://github\.com/sohaha/zcodex\.git`

### `zoffsec-native-command-workflow`
- summary: `codex zoffsec` 顶层子命令、base-instructions 注入、rollout clean，以及 resume 前 clean 的原生 CLI/TUI 工作流必须继续存在。
- better_when: 只有在 upstream 或本地新架构提供等效的原生命令工作流，并继续覆盖模板注入、clean、resume-clean bridge 与 rollout 清理语义时，才允许迁移；迁移前必须先把新的入口和回归锚点更新到这里。
- checks:
  - `regex` `codex-rs/cli/src/main.rs`: `Some\(Subcommand::Zoffsec\(zoffsec_cli\)\) =>`
  - `regex` `codex-rs/cli/src/main.rs`: `fn finalize_zoffsec_resume_interactive\(`
  - `regex` `codex-rs/cli/src/main.rs`: `fn zoffsec_subcommand_registers_at_top_level\(`
  - `regex` `codex-rs/cli/src/main.rs`: `fn finalize_zoffsec_resume_enables_clean_before_resume\(`
  - `regex` `codex-rs/cli/src/zoffsec_cmd.rs`: `pub struct ZoffsecCommand`
  - `regex` `codex-rs/cli/src/zoffsec_cmd.rs`: `pub async fn run_zoffsec_clean_command\(`
  - `regex` `codex-rs/cli/src/zoffsec_config.rs`: `pub const ZOFFSEC_SESSION_MARKER: &str = "codex-zoffsec";`
  - `regex` `codex-rs/tui/src/cli.rs`: `pub resume_zoffsec_clean: bool,`
  - `regex` `codex-rs/tui/src/lib.rs`: `if cli\.resume_zoffsec_clean \{`
  - `regex` `codex-rs/tui/src/zoffsec_resume.rs`: `pub\(crate\) async fn clean_resume_selection_if_needed\(`
  - `regex` `codex-rs/rollout/src/patch.rs`: `pub async fn clean_zoffsec_rollout\(`

### `local-analysis-tools-runtime-wiring`
- summary: `ztldr` 与 `zmemory` 不能只保留 crate 和 CLI；它们必须继续接入共享 tool registry plan、handler 映射和 tests/all 聚合面。
- better_when: 只有在 upstream 或本地新架构提供等效的运行时工具接线，并继续保证 `ztldr`/`zmemory` 真正暴露给模型、handler 可分发且 e2e 聚合测试仍覆盖时，才允许迁移；迁移前必须先把新的 plan、handler 与测试锚点更新到这里。
- checks:
  - `regex` `codex-rs/tools/src/tool_registry_plan.rs`: `let spec = create_tldr_tool\(\);`
  - `regex` `codex-rs/tools/src/tool_registry_plan.rs`: `plan\.register_handler\(name, ToolHandlerKind::Tldr\);`
  - `regex` `codex-rs/tools/src/tool_registry_plan.rs`: `create_zmemory_tool\(\)\)\.chain\(create_zmemory_mcp_tools\(\)\)`
  - `regex` `codex-rs/tools/src/tool_registry_plan.rs`: `plan\.register_handler\(name, ToolHandlerKind::Zmemory\);`
  - `regex` `codex-rs/core/src/tools/spec.rs`: `ToolHandlerKind::Tldr => \{`
  - `regex` `codex-rs/core/src/tools/spec.rs`: `ToolHandlerKind::Zmemory => \{`
  - `regex` `codex-rs/core/tests/suite/mod.rs`: `mod tldr_e2e;`
  - `regex` `codex-rs/core/tests/suite/mod.rs`: `mod zmemory_e2e;`
  - `regex` `codex-rs/core/tests/suite/tldr_e2e.rs`: `assert!\(tool_names\(&body\)\.contains\(&"ztldr"\.to_string\(\)\)\);`
  - `regex` `codex-rs/core/tests/suite/zmemory_e2e.rs`: `async fn zmemory_recall_note_is_injected_into_follow_up_turn_requests\(`

### `pending-input-routing-and-zmemory-recall`
- summary: turn 起始和 mid-turn 的 `pending_input` 必须保留现有 tool routing 基线、按最新 steer 合并指令，并把 zmemory recall note 注入到后续 developer 上下文。
- better_when: 只有在 upstream 用新的 turn-local 状态机制等效覆盖 pending_input 路由合并、regular turn recall 生产、follow-up developer 注入和相关回归测试时，才允许迁移；迁移前必须先把新的状态流锚点更新到这里。
- checks:
  - `regex` `codex-rs/core/src/tasks/mod.rs`: `merge_tool_routing_directives\(current_directives, &pending_turn_inputs\);`
  - `regex` `codex-rs/core/src/tasks/mod.rs`: `self\.set_pending_zmemory_recall_note\(turn_context\.sub_id\.as_str\(\), recall_note\)`
  - `regex` `codex-rs/core/src/session/mod.rs`: `pending_zmemory_recall_note_for\(current_context\.sub_id\.as_str\(\)\)`
  - `regex` `codex-rs/core/src/session/mod.rs`: `build_developer_update_item\(vec!\[recall_note\]\)`
  - `regex` `codex-rs/core/src/session/turn.rs`: `pub\(crate\) async fn apply_pending_user_input_side_effects\(`
  - `regex` `codex-rs/core/src/session/turn.rs`: `merge_tool_routing_directives\(current_directives, &routing_inputs\);`
  - `regex` `codex-rs/core/src/session/turn.rs`: `build_stable_preference_recall_note\(sess, turn_context, &user_inputs\)\.await`
  - `regex` `codex-rs/core/src/session/tests.rs`: `async fn turn_start_zmemory_recall_note_is_produced_for_regular_user_turns\(`
  - `regex` `codex-rs/core/src/session/tests.rs`: `async fn pending_user_input_neutral_steer_preserves_existing_tldr_directives\(`
  - `regex` `codex-rs/core/tests/suite/zmemory_e2e.rs`: `async fn zmemory_recall_note_is_injected_into_follow_up_turn_requests\(`

## Latest Audit

- overall: `17/17` passed

| ID | Status | Area |
| --- | --- | --- |
| `wire-api-streaming-chat-anthropic` | `PASS` | codex-rs/core + codex-rs/codex-api |
| `responses-max-output-tokens-from-provider` | `PASS` | codex-rs/core |
| `zconfig-layer-loading` | `PASS` | codex-rs/core config |
| `models-manager-provider-overrides` | `PASS` | codex-rs/models-manager |
| `responses-reasoning-content-strip` | `PASS` | codex-rs/core + codex-rs/protocol |
| `reference-context-reinjection-baseline` | `PASS` | codex-rs/core session/context_manager |
| `auto-tldr-routing-default` | `PASS` | codex-rs/tools |
| `local-crates-zmemory-ztok` | `PASS` | codex-rs workspace |
| `cli-zmemory-ztok-ztldr-surface` | `PASS` | codex-rs/cli |
| `resume-fork-provider-bridge` | `PASS` | codex-rs/cli + codex-rs/tui |
| `buddy-surface` | `PASS` | codex-rs/tui + codex-rs/app-server |
| `chinese-localization-sentinels` | `PASS` | codex-rs/cli + codex-rs/tui + codex-rs/tools + codex-rs/app-server |
| `session-warning-steer-localization-bridge` | `PASS` | codex-rs/core + codex-rs/app-server + codex-rs/tui + tests |
| `community-branding-and-release-links` | `PASS` | README + install/update surfaces |
| `zoffsec-native-command-workflow` | `PASS` | codex-rs/cli + codex-rs/tui + codex-rs/rollout |
| `local-analysis-tools-runtime-wiring` | `PASS` | codex-rs/tools + codex-rs/core + codex-rs/mcp-server |
| `pending-input-routing-and-zmemory-recall` | `PASS` | codex-rs/core session/tasks/tools |

### `wire-api-streaming-chat-anthropic`
- status: `PASS`
- kind: `local_behavior`
- summary: 为 WireApi::Chat 和 WireApi::Anthropic 提供真实 streaming，而不是 runtime panic 占位。
- better_when: upstream 以同等或更好的方式同时覆盖 Chat/Anthropic streaming，并继续透传 effort、summary、service_tier 与正确 endpoint telemetry。
- evidence:
  - `ok` `codex-rs/core/src/client.rs`: codex-rs/core/src/client.rs:1561 async fn stream_chat_api(
  - `ok` `codex-rs/core/src/client.rs`: codex-rs/core/src/client.rs:1645 async fn stream_anthropic_api(
  - `ok` `codex-rs/core/src/client.rs`: codex-rs/core/src/client.rs:140 const CHAT_COMPLETIONS_ENDPOINT: &str = "/chat/completions";
  - `ok` `codex-rs/core/src/client.rs`: codex-rs/core/src/client.rs:141 const ANTHROPIC_MESSAGES_ENDPOINT: &str = "/messages";
  - `ok` `codex-rs/codex-api/src/endpoint/anthropic.rs`: codex-rs/codex-api/src/endpoint/anthropic.rs:20 pub struct AnthropicClient<T: HttpTransport> {

### `responses-max-output-tokens-from-provider`
- status: `PASS`
- kind: `local_behavior`
- summary: Responses 请求继续从 provider 元数据读取 max_output_tokens，而不是静态写死。
- better_when: upstream 提供了更明确的 provider 级输出上限策略，且不会让本地 provider 配置回退成硬编码 None。
- evidence:
  - `ok` `codex-rs/core/src/client.rs`: codex-rs/core/src/client.rs:895 let max_output_tokens = self

### `zconfig-layer-loading`
- status: `PASS`
- kind: `local_behavior`
- summary: 显式加载 $CODEX_HOME/zconfig.toml，并把它放在 User 与 Project 之间。
- better_when: upstream 原生提供同等层级和优先级的 zconfig 装载逻辑，且不改变本地既有覆盖顺序。
- evidence:
  - `ok` `codex-rs/core/src/config_loader/mod.rs`: codex-rs/core/src/config_loader/mod.rs:12 use codex_config::ZCONFIG_TOML_FILE;
  - `ok` `codex-rs/core/src/config_loader/mod.rs`: codex-rs/core/src/config_loader/mod.rs:223 ConfigLayerSource::ZConfig {
  - `ok` `codex-rs/core/src/config_loader/mod.rs`: codex-rs/core/src/config_loader/mod.rs:230 layers.push(zconfig_layer);

### `models-manager-provider-overrides`
- status: `PASS`
- kind: `local_behavior`
- summary: 保留 provider.model_catalog 过滤、skip_reasoning_popup 传播、按 provider 选择默认远端模型目录，以及本地 synthetic/fallback ModelInfo 的字段完整性。
- better_when: upstream 把 provider.model_catalog、skip_reasoning_popup、Anthropic 默认模型目录和本地 synthetic ModelInfo 的字段补齐都整合成更完整的实现，且本地配置行为不退化。
- evidence:
  - `ok` `codex-rs/models-manager/src/manager.rs`: codex-rs/models-manager/src/manager.rs:263 let remote_models = if let Some(ref catalog_slugs) = provider_info.model_catalog {
  - `ok` `codex-rs/models-manager/src/manager.rs`: codex-rs/models-manager/src/manager.rs:764 if provider_info.skip_reasoning_popup {
  - `ok` `codex-rs/models-manager/src/manager.rs`: codex-rs/models-manager/src/manager.rs:262 .unwrap_or_else(|| Self::default_remote_models_for_provider(&provider_info));
  - `ok` `codex-rs/models-manager/src/manager.rs`: codex-rs/models-manager/src/manager.rs:666 WireApi::Anthropic => model_info::anthropic_model_catalog(),
  - `ok` `codex-rs/models-manager/src/manager.rs`: codex-rs/models-manager/src/manager.rs:708 max_context_window: None,
  - `ok` `codex-rs/models-manager/src/model_info.rs`: codex-rs/models-manager/src/model_info.rs:181 max_context_window: None,

### `responses-reasoning-content-strip`
- status: `PASS`
- kind: `local_behavior`
- summary: Responses replay 时剥离 raw reasoning.content，保留 summary / encrypted_content，避免出站请求变成非法 payload。
- better_when: upstream 提供更靠近出站层的统一处理，并仍保证 raw reasoning_text 不会回传给 Responses API。
- evidence:
  - `ok` `codex-rs/core/src/client_common.rs`: codex-rs/core/src/client_common.rs:52 if let ResponseItem::Reasoning { content, .. } = item {
  - `ok` `codex-rs/core/src/client_common.rs`: codex-rs/core/src/client_common.rs:55 *content = None;
  - `ok` `codex-rs/protocol/src/models.rs`: codex-rs/protocol/src/models.rs:278 #[serde(default, skip_serializing_if = "should_serialize_reasoning_content")]

### `reference-context-reinjection-baseline`
- status: `PASS`
- kind: `local_behavior`
- summary: resume、compact 和 replacement history 之后继续维护 reference_context_item 基线与全量上下文重注入。
- better_when: upstream 改成新的上下文基线机制，但仍完整覆盖 replacement history、clear baseline 和 full reinjection 语义。
- evidence:
  - `ok` `codex-rs/core/src/session/mod.rs`: codex-rs/core/src/session/mod.rs:2561 pub(crate) async fn record_context_updates_and_set_reference_context_item(
  - `ok` `codex-rs/core/src/context_manager/history.rs`: codex-rs/core/src/context_manager/history.rs:190 pub(crate) fn replacement_reference_context_item(
  - `ok` `codex-rs/core/src/context_manager/history.rs`: codex-rs/core/src/context_manager/history.rs:450 self.reference_context_item = None;

### `auto-tldr-routing-default`
- status: `PASS`
- kind: `local_behavior`
- summary: 工具配置默认继续启用 auto_tldr_routing，并保留显式 with_auto_tldr_routing 链路。
- better_when: upstream 用新的工具路由配置替换了 auto_tldr_routing，且默认行为不回退。
- evidence:
  - `ok` `codex-rs/tools/src/tool_config.rs`: codex-rs/tools/src/tool_config.rs:249 auto_tldr_routing: AutoTldrRoutingMode::default(),
  - `ok` `codex-rs/tools/src/tool_config.rs`: codex-rs/tools/src/tool_config.rs:307 pub fn with_auto_tldr_routing(mut self, auto_tldr_routing: AutoTldrRoutingMode) -> Self {

### `local-crates-zmemory-ztok`
- status: `PASS`
- kind: `local_surface`
- summary: 本地分叉附加 crate `native-tldr`、`zmemory` 与 `ztok` 必须继续存在，并保持 workspace member / dependency 接线完整。
- better_when: 只有在本地确定把这些 crate 整体迁移或替换到新的路径，并同步更新这里的检查路径与 Cargo workspace 接线检查时，才允许变更。
- evidence:
  - `ok` `codex-rs/native-tldr`: codex-rs/native-tldr exists (dir)
  - `ok` `codex-rs/zmemory`: codex-rs/zmemory exists (dir)
  - `ok` `codex-rs/ztok`: codex-rs/ztok exists (dir)
  - `ok` `codex-rs/Cargo.toml`: codex-rs/Cargo.toml:49 "native-tldr",
  - `ok` `codex-rs/Cargo.toml`: codex-rs/Cargo.toml:50 "zmemory",
  - `ok` `codex-rs/Cargo.toml`: codex-rs/Cargo.toml:60 "ztok",
  - `ok` `codex-rs/Cargo.toml`: codex-rs/Cargo.toml:158 codex-native-tldr = { path = "native-tldr" }
  - `ok` `codex-rs/Cargo.toml`: codex-rs/Cargo.toml:207 codex-zmemory = { path = "zmemory" }
  - `ok` `codex-rs/Cargo.toml`: codex-rs/Cargo.toml:208 codex-ztok = { path = "ztok" }

### `cli-zmemory-ztok-ztldr-surface`
- status: `PASS`
- kind: `local_surface`
- summary: 顶层 `codex` CLI 必须继续暴露 `ztok`、`ztldr` 与 `zmemory` 子命令，并保留对应 dispatch 与 help 汉化哨兵。
- better_when: 只有在 upstream 原生提供等效 CLI surface，且本地不再需要这些分叉入口或其汉化收口时，才允许迁移；迁移前必须先把新的入口路径与哨兵更新到这里。
- evidence:
  - `ok` `codex-rs/cli/src/main.rs`: codex-rs/cli/src/main.rs:127 Ztok(ZtokArgs),
  - `ok` `codex-rs/cli/src/main.rs`: codex-rs/cli/src/main.rs:133 #[clap(name = "ztldr")]
  - `ok` `codex-rs/cli/src/main.rs`: codex-rs/cli/src/main.rs:137 Zmemory(ZmemoryCli),
  - `ok` `codex-rs/cli/src/main.rs`: codex-rs/cli/src/main.rs:167 #[clap(visible_alias = "r")]
  - `ok` `codex-rs/cli/src/main.rs`: codex-rs/cli/src/main.rs:891 tldr_cmd::run_tldr_command(tldr_cli).await?;
  - `ok` `codex-rs/cli/src/main.rs`: codex-rs/cli/src/main.rs:899 run_zmemory_command(zmemory_cli).await?;
  - `ok` `codex-rs/cli/src/main.rs`: codex-rs/cli/src/main.rs:1783 let rendered = localize_help_output(err.to_string());
  - `ok` `codex-rs/cli/src/main.rs`: codex-rs/cli/src/main.rs:1851 "显示帮助（使用 '-h' 查看摘要）",
  - `ok` `codex-rs/cli/src/main.rs`: codex-rs/cli/src/main.rs:1858 .replace("Print version", "显示版本")

### `resume-fork-provider-bridge`
- status: `PASS`
- kind: `local_behavior`
- summary: `resume` / `fork` 这类复用 `TuiCli` 的交互子命令，继续允许通过 `-P/--provider` 与 `--local-provider` 切换 model_provider，且 merge 后真正写入最终 interactive 配置。
- better_when: upstream 把 interactive CLI 参数合并统一收敛为等效或更强的实现，并继续保证 `resume` / `fork` 等子命令不会在 bridge 阶段静默丢失 provider / local-provider 等 interactive 参数；迁移前必须先把新的桥接点和回归测试锚点更新到这里。
- evidence:
  - `ok` `codex-rs/tui/src/cli.rs`: codex-rs/tui/src/cli.rs:90 pub provider: Option<String>,
  - `ok` `codex-rs/tui/src/cli.rs`: codex-rs/tui/src/cli.rs:98 pub oss_provider: Option<String>,
  - `ok` `codex-rs/cli/src/main.rs`: codex-rs/cli/src/main.rs:1717 interactive.provider = Some(provider);
  - `ok` `codex-rs/cli/src/main.rs`: codex-rs/cli/src/main.rs:1720 interactive.oss_provider = Some(oss_provider);
  - `ok` `codex-rs/cli/src/main.rs`: codex-rs/cli/src/main.rs:2269 fn resume_merges_option_flags_and_full_auto() {
  - `ok` `codex-rs/cli/src/main.rs`: codex-rs/cli/src/main.rs:2300 assert_eq!(interactive.provider.as_deref(), Some("oss"));
  - `ok` `codex-rs/cli/src/main.rs`: codex-rs/cli/src/main.rs:2332 fn fork_merges_provider_flags() {
  - `ok` `codex-rs/cli/src/main.rs`: codex-rs/cli/src/main.rs:2347 assert_eq!(interactive.oss_provider.as_deref(), Some("lmstudio"));

### `buddy-surface`
- status: `PASS`
- kind: `local_surface`
- summary: Buddy 交互面、配置落盘事件和 app-server 通知桥接仍然存在，不被 upstream TUI/app-server 改动吞掉。
- better_when: upstream 原生提供等效 buddy 能力且本地不再需要维护分叉实现，或者本地把 buddy 正式迁移到新模块并同步更新检查点。
- evidence:
  - `ok` `codex-rs/tui/src/buddy/mod.rs`: codex-rs/tui/src/buddy/mod.rs:91 "小伙伴已孵化：{} {}。",
  - `ok` `codex-rs/tui/src/chatwidget.rs`: codex-rs/tui/src/chatwidget.rs:5284 "小伙伴命令：`/buddy show`、`/buddy full`、`/buddy pet`、`/buddy hide`、`/buddy status`。".to_string(),
  - `ok` `codex-rs/tui/src/slash_command.rs`: codex-rs/tui/src/slash_command.rs:95 SlashCommand::Buddy => "孵化、抚摸或隐藏底部小伙伴",
  - `ok` `codex-rs/tui/src/app_event.rs`: codex-rs/tui/src/app_event.rs:531 PersistBuddyVisibility(bool),
  - `ok` `codex-rs/tui/src/app_event.rs`: codex-rs/tui/src/app_event.rs:534 PersistBuddyFullVisibility,
  - `ok` `codex-rs/tui/src/app.rs`: codex-rs/tui/src/app.rs:5667 AppEvent::PersistBuddyVisibility(visible) => {
  - `ok` `codex-rs/tui/src/app.rs`: codex-rs/tui/src/app.rs:5670 AppEvent::PersistBuddyFullVisibility => {
  - `ok` `codex-rs/app-server/src/bespoke_event_handling.rs`: codex-rs/app-server/src/bespoke_event_handling.rs:289 EventMsg::BuddySoulGenerated(event) => {
  - `ok` `codex-rs/app-server/src/bespoke_event_handling.rs`: codex-rs/app-server/src/bespoke_event_handling.rs:301 EventMsg::BuddyReaction(event) => {

### `chinese-localization-sentinels`
- status: `PASS`
- kind: `localized_behavior`
- summary: 用高频哨兵文案检查中文化输出没有被 upstream 英文重新覆盖。
- better_when: 用户可见链路已迁移到新的源码位置，且新的实现保持自然中文表达；需要先更新这里的哨兵文案位置。
- evidence:
  - `ok` `codex-rs/cli/src/main.rs`: codex-rs/cli/src/main.rs:80 /// 若未指定子命令，选项会转发到交互式命令行界面。
  - `ok` `codex-rs/cli/src/main.rs`: codex-rs/cli/src/main.rs:110 /// 以非交互模式运行 Codex。
  - `ok` `codex-rs/cli/src/main.rs`: codex-rs/cli/src/main.rs:1359 println!("已在 config.toml 中启用功能 `{feature}`。");
  - `ok` `codex-rs/tui/src/slash_command.rs`: codex-rs/tui/src/slash_command.rs:77 SlashCommand::Init => "创建 AGENTS.md 文件，为 Codex 提供指令",
  - `ok` `codex-rs/tools/src/request_user_input_tool.rs`: codex-rs/tools/src/request_user_input_tool.rs:91 Some(format!("request_user_input 在 {mode_name} 模式不可用"))
  - `ok` `codex-rs/tui/src/bottom_pane/feedback_view.rs`: codex-rs/tui/src/bottom_pane/feedback_view.rs:325 Some(_) => format!("{prefix}请使用以下链接提交 Issue："),
  - `ok` `codex-rs/tui/src/app.rs`: codex-rs/tui/src/app.rs:183 const EXTERNAL_EDITOR_HINT: &str = "保存并关闭外部编辑器以继续。";
  - `ok` `codex-rs/tui/src/app.rs`: codex-rs/tui/src/app.rs:464 "因 SKILL.md 文件无效，已跳过加载 {error_count} 个技能。"
  - `ok` `codex-rs/tui/src/onboarding/trust_directory.rs`: codex-rs/tui/src/onboarding/trust_directory.rs:49 "你当前位于 ".bold(),
  - `ok` `codex-rs/tui/src/history_cell.rs`: codex-rs/tui/src/history_cell.rs:1207 " 开始使用时，请描述一个任务，或试试这些命令："
  - `ok` `codex-rs/features/src/lib.rs`: codex-rs/features/src/lib.rs:869 name: "外部配置迁移",
  - `ok` `codex-rs/app-server/src/bespoke_event_handling.rs`: codex-rs/app-server/src/bespoke_event_handling.rs:1902 "加载 rollout `{}` 失败：{err}",
  - `ok` `codex-rs/app-server/src/bespoke_event_handling.rs`: codex-rs/app-server/src/bespoke_event_handling.rs:2671 const REVIEW_FALLBACK_MESSAGE: &str = "审查器未输出任何回复。";

### `session-warning-steer-localization-bridge`
- status: `PASS`
- kind: `localized_behavior`
- summary: `core/src/session/mod.rs` 与 `core/src/session/turn_context.rs` 的中文 steer 错误和 warning 文案必须在 app-server 映射、tui 解析和回归测试里保持一致，避免同步上游英文实现时只改一层导致桥接回归。
- better_when: upstream 把这条错误/警告链路统一收敛成不依赖脆弱字符串解析的等效或更强实现，并同步覆盖 warning 前缀、active-turn race、fallback 模型 warning 和 steer 错误映射；迁移前必须先把新的桥接点与回归测试锚点更新到这里。
- evidence:
  - `ok` `codex-rs/core/src/session/mod.rs`: codex-rs/core/src/session/mod.rs:208 message: "当前没有可追加输入的活跃轮次".to_string(),
  - `ok` `codex-rs/core/src/session/mod.rs`: codex-rs/core/src/session/mod.rs:212 message: format!("期望的活跃轮次 ID 为 `{expected}`，但实际是 `{actual}`"),
  - `ok` `codex-rs/core/src/session/mod.rs`: codex-rs/core/src/session/mod.rs:488 format!("已为此会话禁用 `js_repl`，因为配置的 Node 运行时不可用或版本不兼容。{err}")
  - `ok` `codex-rs/core/src/session/mod.rs`: codex-rs/core/src/session/mod.rs:2214 text: format!("警告：{}", message.into()),
  - `ok` `codex-rs/core/src/session/turn_context.rs`: codex-rs/core/src/session/turn_context.rs:730 "未找到模型 `{}` 的元数据，已改用兜底元数据；",
  - `ok` `codex-rs/app-server/src/codex_message_processor.rs`: codex-rs/app-server/src/codex_message_processor.rs:7459 "无法向审查轮次追加输入".to_string(),
  - `ok` `codex-rs/app-server/src/codex_message_processor.rs`: codex-rs/app-server/src/codex_message_processor.rs:7493 "输入不能为空".to_string(),
  - `ok` `codex-rs/tui/src/app.rs`: codex-rs/tui/src/app.rs:1110 let mismatch_prefix = "期望的活跃轮次 ID 为 `";
  - `ok` `codex-rs/tui/src/app.rs`: codex-rs/tui/src/app.rs:1104 if source.message == "当前没有可追加输入的活跃轮次" {
  - `ok` `codex-rs/analytics/src/analytics_client_tests.rs`: codex-rs/analytics/src/analytics_client_tests.rs:364 message: "无法向审查轮次追加输入".to_string(),
  - `ok` `codex-rs/core/tests/suite/js_repl.rs`: codex-rs/core/tests/suite/js_repl.rs:207 EventMsg::Warning(ev) if ev.message.contains("已为此会话禁用 `js_repl`") => {
  - `ok` `codex-rs/core/tests/suite/safety_check_downgrade.rs`: codex-rs/core/tests/suite/safety_check_downgrade.rs:90 ContentItem::InputText { text } if text.starts_with("警告：")
  - `ok` `codex-rs/app-server/tests/suite/v2/safety_check_downgrade.rs`: codex-rs/app-server/tests/suite/v2/safety_check_downgrade.rs:192 UserInput::Text { text, .. } if text.starts_with("警告：") => Some(text.as_str()),
  - `ok` `codex-rs/core/src/session/tests.rs`: codex-rs/core/src/session/tests.rs:4325 text: "警告：too many unified exec processes".to_string(),
  - `ok` `codex-rs/core/src/session/tests.rs`: codex-rs/core/src/session/tests.rs:4353 "未找到模型 `mystery-model` 的元数据，已改用兜底元数据；".to_string()

### `community-branding-and-release-links`
- status: `PASS`
- kind: `localized_behavior`
- summary: 社区分叉 branding 与 release/install 链接继续指向 sohaha/zcodex。
- better_when: 仓库决定统一回官方 branding，或者 branding 入口迁移到新文件并同步更新这里的检查路径。
- evidence:
  - `ok` `README.md`: README.md:1 <p align="center"><strong>zcodex</strong> — 社区分支版本的 Codex CLI</p>
  - `ok` `codex-rs/README.md`: codex-rs/README.md:14 你也可以通过 Homebrew（`brew install --cask codex`）安装，或直接从 [GitHub Releases](https://github.com/sohaha/zcodex/releases) 下载平台...
  - `ok` `codex-rs/tui/src/update_action.rs`: codex-rs/tui/src/update_action.rs:4 /// Update via `npm install -g @sohaha/zcodex@latest`.
  - `ok` `docs/install.md`: docs/install.md:19 git clone https://github.com/sohaha/zcodex.git

### `zoffsec-native-command-workflow`
- status: `PASS`
- kind: `local_surface`
- summary: `codex zoffsec` 顶层子命令、base-instructions 注入、rollout clean，以及 resume 前 clean 的原生 CLI/TUI 工作流必须继续存在。
- better_when: 只有在 upstream 或本地新架构提供等效的原生命令工作流，并继续覆盖模板注入、clean、resume-clean bridge 与 rollout 清理语义时，才允许迁移；迁移前必须先把新的入口和回归锚点更新到这里。
- evidence:
  - `ok` `codex-rs/cli/src/main.rs`: codex-rs/cli/src/main.rs:763 Some(Subcommand::Zoffsec(zoffsec_cli)) => {
  - `ok` `codex-rs/cli/src/main.rs`: codex-rs/cli/src/main.rs:1662 fn finalize_zoffsec_resume_interactive(
  - `ok` `codex-rs/cli/src/main.rs`: codex-rs/cli/src/main.rs:2467 fn zoffsec_subcommand_registers_at_top_level() {
  - `ok` `codex-rs/cli/src/main.rs`: codex-rs/cli/src/main.rs:2482 fn finalize_zoffsec_resume_enables_clean_before_resume() {
  - `ok` `codex-rs/cli/src/zoffsec_cmd.rs`: codex-rs/cli/src/zoffsec_cmd.rs:22 pub struct ZoffsecCommand {
  - `ok` `codex-rs/cli/src/zoffsec_cmd.rs`: codex-rs/cli/src/zoffsec_cmd.rs:119 pub async fn run_zoffsec_clean_command(
  - `ok` `codex-rs/cli/src/zoffsec_config.rs`: codex-rs/cli/src/zoffsec_config.rs:3 pub const ZOFFSEC_SESSION_MARKER: &str = "codex-zoffsec";
  - `ok` `codex-rs/tui/src/cli.rs`: codex-rs/tui/src/cli.rs:50 pub resume_zoffsec_clean: bool,
  - `ok` `codex-rs/tui/src/lib.rs`: codex-rs/tui/src/lib.rs:1391 if cli.resume_zoffsec_clean {
  - `ok` `codex-rs/tui/src/zoffsec_resume.rs`: codex-rs/tui/src/zoffsec_resume.rs:16 pub(crate) async fn clean_resume_selection_if_needed(
  - `ok` `codex-rs/rollout/src/patch.rs`: codex-rs/rollout/src/patch.rs:113 pub async fn clean_zoffsec_rollout(

### `local-analysis-tools-runtime-wiring`
- status: `PASS`
- kind: `local_behavior`
- summary: `ztldr` 与 `zmemory` 不能只保留 crate 和 CLI；它们必须继续接入共享 tool registry plan、handler 映射和 tests/all 聚合面。
- better_when: 只有在 upstream 或本地新架构提供等效的运行时工具接线，并继续保证 `ztldr`/`zmemory` 真正暴露给模型、handler 可分发且 e2e 聚合测试仍覆盖时，才允许迁移；迁移前必须先把新的 plan、handler 与测试锚点更新到这里。
- evidence:
  - `ok` `codex-rs/tools/src/tool_registry_plan.rs`: codex-rs/tools/src/tool_registry_plan.rs:260 let spec = create_tldr_tool();
  - `ok` `codex-rs/tools/src/tool_registry_plan.rs`: codex-rs/tools/src/tool_registry_plan.rs:267 plan.register_handler(name, ToolHandlerKind::Tldr);
  - `ok` `codex-rs/tools/src/tool_registry_plan.rs`: codex-rs/tools/src/tool_registry_plan.rs:271 for spec in std::iter::once(create_zmemory_tool()).chain(create_zmemory_mcp_tools()) {
  - `ok` `codex-rs/tools/src/tool_registry_plan.rs`: codex-rs/tools/src/tool_registry_plan.rs:278 plan.register_handler(name, ToolHandlerKind::Zmemory);
  - `ok` `codex-rs/core/src/tools/spec.rs`: codex-rs/core/src/tools/spec.rs:268 ToolHandlerKind::Tldr => {
  - `ok` `codex-rs/core/src/tools/spec.rs`: codex-rs/core/src/tools/spec.rs:298 ToolHandlerKind::Zmemory => {
  - `ok` `codex-rs/core/tests/suite/mod.rs`: codex-rs/core/tests/suite/mod.rs:99 mod tldr_e2e;
  - `ok` `codex-rs/core/tests/suite/mod.rs`: codex-rs/core/tests/suite/mod.rs:116 mod zmemory_e2e;
  - `ok` `codex-rs/core/tests/suite/tldr_e2e.rs`: codex-rs/core/tests/suite/tldr_e2e.rs:156 assert!(tool_names(&body).contains(&"ztldr".to_string()));
  - `ok` `codex-rs/core/tests/suite/zmemory_e2e.rs`: codex-rs/core/tests/suite/zmemory_e2e.rs:2339 async fn zmemory_recall_note_is_injected_into_follow_up_turn_requests() -> Result<()> {

### `pending-input-routing-and-zmemory-recall`
- status: `PASS`
- kind: `local_behavior`
- summary: turn 起始和 mid-turn 的 `pending_input` 必须保留现有 tool routing 基线、按最新 steer 合并指令，并把 zmemory recall note 注入到后续 developer 上下文。
- better_when: 只有在 upstream 用新的 turn-local 状态机制等效覆盖 pending_input 路由合并、regular turn recall 生产、follow-up developer 注入和相关回归测试时，才允许迁移；迁移前必须先把新的状态流锚点更新到这里。
- evidence:
  - `ok` `codex-rs/core/src/tasks/mod.rs`: codex-rs/core/src/tasks/mod.rs:283 merge_tool_routing_directives(current_directives, &pending_turn_inputs);
  - `ok` `codex-rs/core/src/tasks/mod.rs`: codex-rs/core/src/tasks/mod.rs:299 self.set_pending_zmemory_recall_note(turn_context.sub_id.as_str(), recall_note)
  - `ok` `codex-rs/core/src/session/mod.rs`: codex-rs/core/src/session/mod.rs:1509 state.pending_zmemory_recall_note_for(current_context.sub_id.as_str())
  - `ok` `codex-rs/core/src/session/mod.rs`: codex-rs/core/src/session/mod.rs:1514 crate::context_manager::updates::build_developer_update_item(vec![recall_note])
  - `ok` `codex-rs/core/src/session/turn.rs`: codex-rs/core/src/session/turn.rs:748 pub(crate) async fn apply_pending_user_input_side_effects(
  - `ok` `codex-rs/core/src/session/turn.rs`: codex-rs/core/src/session/turn.rs:760 merge_tool_routing_directives(current_directives, &routing_inputs);
  - `ok` `codex-rs/core/src/session/turn.rs`: codex-rs/core/src/session/turn.rs:779 build_stable_preference_recall_note(sess, turn_context, &user_inputs).await
  - `ok` `codex-rs/core/src/session/tests.rs`: codex-rs/core/src/session/tests.rs:4717 async fn turn_start_zmemory_recall_note_is_produced_for_regular_user_turns() {
  - `ok` `codex-rs/core/src/session/tests.rs`: codex-rs/core/src/session/tests.rs:5962 async fn pending_user_input_neutral_steer_preserves_existing_tldr_directives() {
  - `ok` `codex-rs/core/tests/suite/zmemory_e2e.rs`: codex-rs/core/tests/suite/zmemory_e2e.rs:2339 async fn zmemory_recall_note_is_injected_into_follow_up_turn_requests() -> Result<()> {
