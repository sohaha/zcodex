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
| `buddy-surface` | `local_surface` | codex-rs/tui + codex-rs/core config + codex-rs/app-server |
| `chinese-localization-sentinels` | `localized_behavior` | codex-rs/cli + codex-rs/tui + codex-rs/tools + codex-rs/app-server |
| `session-warning-steer-localization-bridge` | `localized_behavior` | codex-rs/core + codex-rs/app-server + codex-rs/tui + tests |
| `community-branding-and-release-links` | `localized_behavior` | README + install/update surfaces |
| `zoffsec-native-command-workflow` | `local_surface` | codex-rs/cli + codex-rs/tui + codex-rs/rollout |
| `local-analysis-tools-runtime-wiring` | `local_behavior` | codex-rs/tools + codex-rs/core + codex-rs/mcp-server |
| `pending-input-routing-and-zmemory-recall` | `local_behavior` | codex-rs/core session/tasks/tools |
| `zteam-mission-workflow` | `local_surface` | codex-rs/tui + codex-rs/config + codex-rs/features + docs |
| `inter-agent-visibility-filtering` | `local_behavior` | codex-rs/protocol + codex-rs/core + codex-rs/app-server-protocol + codex-rs/tui |
| `subagent-runtime-config-preservation` | `local_behavior` | codex-rs/core config/session/tools |
| `native-tldr-daemon-first-runtime` | `local_behavior` | codex-rs/native-tldr + codex-rs/cli + codex-rs/core + codex-rs/mcp-server |
| `ztok-default-launcher-and-prompt-wiring` | `local_behavior` | codex-rs/arg0 + codex-rs/core session/tools + codex-rs/ztok |
| `ztok-behavior-mode` | `local_behavior` | codex-rs/config + codex-rs/core + codex-rs/cli + codex-rs/ztok |
| `zmemory-governance-system-views-and-diagnostics` | `local_behavior` | codex-rs/zmemory + codex-rs/tools + codex-rs/core tests |

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
- summary: Buddy 交互面、配置落盘事件、app-server 通知桥接，以及混合本地预设/AI 反应策略必须继续存在。
- better_when: upstream 原生提供等效 buddy 能力，且同时覆盖可见交互、配置落盘、app-server 通知、reaction_strategy 配置、local preset fallback 与 AI cooldown/critical 场景语义；或者本地正式迁移到新模块并同步更新检查点。
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
  - `regex` `codex-rs/config/src/types.rs`: `pub struct BuddyReactionStrategy`
  - `regex` `codex-rs/config/src/types.rs`: `pub critical_scenarios_use_ai: bool`
  - `regex` `codex-rs/core/src/buddy.rs`: `struct LocalReactionLibrary`
  - `regex` `codex-rs/core/src/buddy.rs`: `BuddyReactionMode::Hybrid`
  - `regex` `codex-rs/config/src/types.rs`: `pub reaction_strategy: Option<BuddyReactionStrategy>`

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
  - `regex` `codex-rs/core/src/session/mod.rs`: `警告：`
  - `regex` `codex-rs/core/src/session/turn_context.rs`: `未找到模型 `\{\}` 的元数据，已改用兜底元数据；`
  - `regex` `codex-rs/app-server/src/codex_message_processor.rs`: `无法向审查轮次追加输入`
  - `regex` `codex-rs/app-server/src/codex_message_processor.rs`: `输入不能为空`
  - `regex` `codex-rs/tui/src/app.rs`: `期望的活跃轮次 ID 为 ``
  - `regex` `codex-rs/tui/src/app.rs`: `当前没有可追加输入的活跃轮次`
  - `regex` `codex-rs/analytics/src/analytics_client_tests.rs`: `无法向审查轮次追加输入`
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
  - `regex` `codex-rs/core/src/session/turn.rs`: `merge_tool_routing_directives\(current_directives, user_inputs\);`
  - `regex` `codex-rs/core/src/session/turn.rs`: `build_stable_preference_recall_note\(sess, turn_context, &user_inputs\)\.await`
  - `regex` `codex-rs/core/src/session/tests.rs`: `async fn turn_start_zmemory_recall_note_is_produced_for_regular_user_turns\(`
  - `regex` `codex-rs/core/src/session/tests.rs`: `async fn pending_user_input_neutral_steer_preserves_existing_tldr_directives\(`
  - `regex` `codex-rs/core/tests/suite/zmemory_e2e.rs`: `async fn zmemory_recall_note_is_injected_into_follow_up_turn_requests\(`

### `zteam-mission-workflow`
- summary: 默认开启的 TUI `/zteam` 本地协作入口、mission-first 工作台、frontend/backend worker 编排、自动推进/repair、恢复语义和 federation adapter seam 必须继续存在。
- better_when: 只有在 upstream 或本地新架构提供等效或更强的 TUI-first 多协作者 mission 工作流，且继续覆盖默认启用配置、slash command 入口、AppEvent/app loop bridge、Mission Board、autopilot repair、loaded-thread recovery、federation adapter seam、中文提示和快照回归锚点时，才允许迁移；迁移前必须先更新这里的路径与检查点。
- checks:
  - `regex` `codex-rs/config/src/types.rs`: `pub zteam_enabled: bool,`
  - `regex` `codex-rs/core/src/config/mod.rs`: `zteam_enabled: cfg\.tui\.as_ref\(\)\.map\(\|t\| t\.zteam_enabled\)\.unwrap_or\(true\)`
  - `regex` `codex-rs/features/src/lib.rs`: `key: "multi_agent_v2"`
  - `regex` `codex-rs/features/src/lib.rs`: `id: Feature::MultiAgentV2,[\s\S]*?default_enabled: true`
  - `regex` `codex-rs/tui/src/lib.rs`: `mod zteam;`
  - `regex` `codex-rs/tui/src/slash_command.rs`: `SlashCommand::Zteam`
  - `regex` `codex-rs/tui/src/slash_command.rs`: `以目标启动 ZTeam mission 协作并查看状态`
  - `regex` `codex-rs/tui/src/bottom_pane/slash_commands.rs`: `flags\.zteam_enabled \|\| \*cmd != SlashCommand::Zteam`
  - `regex` `codex-rs/tui/src/app_event.rs`: `ZteamCommand\(ZteamCommand\)`
  - `regex` `codex-rs/tui/src/app.rs`: `async fn handle_zteam_command\(`
  - `regex` `codex-rs/tui/src/app.rs`: `fn schedule_zteam_autopilot_tick\(`
  - `regex` `codex-rs/tui/src/app.rs`: `restore_loaded_zteam_workers\(`
  - `regex` `codex-rs/tui/src/zteam.rs`: `pub\(crate\) enum AutopilotWorkItem`
  - `regex` `codex-rs/tui/src/zteam/recovery.rs`: `latest_local_threads_for_primary`
  - `regex` `codex-rs/tui/src/zteam/worker_source.rs`: `pub\(crate\) struct FederationAdapter`
  - `regex` `codex-rs/tui/src/zteam/view.rs`: `\{MODE_NAME\} Mission Board`
  - `regex` `codex-rs/tui/src/chatwidget/tests/slash_commands.rs`: `zteam_workbench_active_view`
  - `exists` `codex-rs/tui/src/chatwidget/snapshots/codex_tui__chatwidget__tests__zteam_workbench_active_view.snap`
  - `exists` `codex-rs/tui/src/chatwidget/snapshots/codex_tui__chatwidget__tests__zteam_entry_disabled_notice.snap`
  - `regex` `docs/slash_commands.md`: `/zteam`

### `inter-agent-visibility-filtering`
- summary: 智能体间通信 envelope 和隐藏 subagent notification 必须继续由 protocol 统一识别并在 core、app-server thread history、TUI replay 等可见文本链路中过滤。
- better_when: upstream 提供更统一的隐藏消息模型，并且 app-server thread/read、core event mapping、last assistant extraction、TUI history replay 都继续不泄露 inter-agent envelope 或隐藏 subagent notification。
- checks:
  - `regex` `codex-rs/protocol/src/protocol.rs`: `pub struct InterAgentCommunication`
  - `regex` `codex-rs/protocol/src/protocol.rs`: `pub fn is_hidden_message_text`
  - `regex` `codex-rs/protocol/src/protocol.rs`: `pub fn sanitize_visible_text`
  - `regex` `codex-rs/protocol/src/protocol.rs`: `pub fn is_hidden_subagent_notification_text\(`
  - `regex` `codex-rs/app-server-protocol/src/protocol/thread_history.rs`: `InterAgentCommunication::sanitize_visible_text`
  - `regex` `codex-rs/core/src/event_mapping.rs`: `InterAgentCommunication::sanitize_visible_text`
  - `regex` `codex-rs/core/src/stream_events_utils.rs`: `InterAgentCommunication::sanitize_visible_text`
  - `regex` `codex-rs/app-server/tests/suite/v2/thread_read.rs`: `thread_read_include_turns_skips_inter_agent_envelope_messages`
  - `regex` `codex-rs/app-server/tests/suite/v2/thread_read.rs`: `thread_read_include_turns_skips_subagent_notification_agent_messages`
  - `regex` `codex-rs/core/src/event_mapping_tests.rs`: `skips_serialized_inter_agent_communication`
  - `regex` `codex-rs/core/src/event_mapping_tests.rs`: `skips_hidden_subagent_notification_user_message`
  - `regex` `codex-rs/tui/src/chatwidget/tests/history_replay.rs`: `thread_snapshot_replay_hides_inter_agent_envelope_messages`
  - `regex` `codex-rs/tui/src/chatwidget/tests/history_replay.rs`: `replayed_subagent_notification_user_message_is_hidden`

### `subagent-runtime-config-preservation`
- summary: spawn/resume subagent 时继续保留运行时 provider、model、sandbox、developer instructions 等 live turn 状态；只有 turn cwd 确实命中启用的 project config layer 时才重载 project-scoped 配置。
- better_when: upstream 提供更清晰的 subagent config 构建机制，并同时保留运行时 provider/details、不误用禁用 project layer、且仍能在 turn cwd override 命中启用项目层时加载 project-scoped zmemory/profile/agent_roles。
- checks:
  - `regex` `codex-rs/core/src/tools/handlers/multi_agents_common.rs`: `pub\(crate\) async fn build_agent_shared_config`
  - `regex` `codex-rs/core/src/tools/handlers/multi_agents_common.rs`: `load_config_layers_state`
  - `regex` `codex-rs/core/src/tools/handlers/multi_agents_common.rs`: `has_enabled_project_layer`
  - `regex` `codex-rs/core/src/tools/handlers/multi_agents_common.rs`: `ConfigLayerSource::Project`
  - `regex` `codex-rs/core/src/tools/handlers/multi_agents_common.rs`: `reloaded_for_comparison != \*live_config`
  - `regex` `codex-rs/core/src/tools/handlers/multi_agents_tests.rs`: `build_agent_spawn_config_preserves_runtime_provider_details`
  - `regex` `codex-rs/core/src/tools/handlers/multi_agents_tests.rs`: `build_agent_spawn_config_reloads_project_scoped_zmemory_profile_for_turn_cwd_override`
  - `regex` `codex-rs/core/src/tools/handlers/multi_agents_tests.rs`: `build_agent_spawn_config_preserves_active_profile_when_reloading_turn_cwd_override`

### `native-tldr-daemon-first-runtime`
- summary: `ztldr` 不只是 CLI/工具注册面；本地默认依赖 native-tldr daemon-first 生命周期、CLI/core/MCP 自动启动、daemon 状态动作和结构化失败回退。
- better_when: 只有在 upstream 或本地新架构提供等效的 daemon-first 运行时，并继续覆盖 CLI、core tool handler、MCP tool handler、daemon action、自动启动禁用配置、结构化 daemon unavailable 错误与测试锚点时，才允许迁移；迁移前必须先更新这些检查到新入口。
- checks:
  - `regex` `codex-rs/native-tldr/src/daemon.rs`: `pub enum TldrDaemonCommand`
  - `regex` `codex-rs/native-tldr/src/tool_api.rs`: `pub async fn query_daemon_with_hooks_detailed`
  - `regex` `codex-rs/native-tldr/src/tool_api.rs`: `native-tldr daemon is unavailable for`
  - `regex` `codex-rs/cli/src/tldr_cmd.rs`: `TldrSubcommand::Daemon\(cmd\) =>`
  - `regex` `codex-rs/cli/src/tldr_cmd.rs`: `query_daemon_with_hooks_detailed\(`
  - `regex` `codex-rs/core/src/tools/handlers/tldr.rs`: `run_tldr_tool_with_hooks`
  - `regex` `codex-rs/core/src/tools/handlers/tldr.rs`: `async fn ensure_daemon_running_detailed\(`
  - `regex` `codex-rs/core/src/tools/handlers/tldr.rs`: `native-tldr daemon is unavailable for`
  - `regex` `codex-rs/mcp-server/src/tldr_tool.rs`: `async fn run_tldr_tool_with_mcp_hooks`
  - `regex` `codex-rs/mcp-server/src/tldr_tool.rs`: `fn daemon_action_spec\(`
  - `regex` `codex-rs/mcp-server/src/tldr_tool.rs`: `ensure_daemon_running_respects_disabled_auto_start_config`

### `ztok-default-launcher-and-prompt-wiring`
- summary: `ztok` 必须保持默认本地可用：启动时注入 PATH alias，developer 上下文要求使用 `codex ztok ...`，shell search rewrite 能识别 `ztok grep`，事件输出不能泄露绝对 launcher 路径。
- better_when: 只有在新的本地 wrapper 或 upstream 等效机制同时覆盖 alias 注入、developer prompt、shell rewrite、事件脱敏和对应回归测试时，才允许替换；迁移前必须把新入口和测试锚点写回基线。
- checks:
  - `regex` `codex-rs/arg0/src/lib.rs`: `const ZTOK_ARG0: &str = "ztok";`
  - `regex` `codex-rs/arg0/src/lib.rs`: `filename == ZTOK_ARG0`
  - `regex` `codex-rs/core/templates/compact/ztok.md`: `Use `\{\{ logical_launcher_invocation \}\} ztok `
  - `regex` `codex-rs/core/src/memories/prompts.rs`: `static ZTOK_DEVELOPER_INSTRUCTIONS_TEMPLATE`
  - `regex` `codex-rs/core/src/memories/prompts.rs`: `pub\(crate\) fn build_ztok_tool_developer_instructions\(`
  - `regex` `codex-rs/core/src/session/mod.rs`: `developer_sections\.push\(build_ztok_tool_developer_instructions\(\)\);`
  - `regex` `codex-rs/core/src/memories/prompts_tests.rs`: `build_ztok_tool_developer_instructions_renders_embedded_template`
  - `regex` `codex-rs/core/src/tools/rewrite/shell_search_rewrite.rs`: `head == "ztok" && next == "grep"`
  - `regex` `codex-rs/core/src/tools/events.rs`: `shell_emitter_never_exposes_absolute_ztok_exec_path`

### `ztok-behavior-mode`
- summary: `ztok.behavior` 配置继续支持 enhanced/basic 行为模式，并从 Codex config 经 CLI runtime settings 传递到 ztok 压缩实现。
- better_when: upstream 或本地后续实现用等效配置机制替代 ztok behavior，但仍能选择 enhanced/basic 等价模式，并保证 Codex 配置到 ztok runtime 的传递链路不丢失。
- checks:
  - `regex` `codex-rs/config/src/types.rs`: `pub enum ZtokBehavior`
  - `regex` `codex-rs/config/src/types.rs`: `pub behavior: Option<ZtokBehavior>`
  - `regex` `codex-rs/core/src/config/types.rs`: `pub behavior: ZtokBehavior`
  - `regex` `codex-rs/cli/src/main.rs`: `config\.ztok\.behavior\.as_str\(\)`
  - `regex` `codex-rs/ztok/src/settings.rs`: `pub behavior: ZtokBehavior`
  - `regex` `codex-rs/ztok/src/behavior.rs`: `pub\(crate\) enum ZtokBehavior`
  - `regex` `codex-rs/ztok/src/compression.rs`: `compress_for_behavior`

### `zmemory-governance-system-views-and-diagnostics`
- summary: `zmemory` 默认长期记忆能力必须保留系统视图、审计日志、trigger 管理、doctor 诊断和内容治理结果，而不只是保留基本 CRUD/tool schema。
- better_when: 只有在新的 memory runtime 或 upstream 等效实现继续暴露 system://workspace 等系统视图、audit/manage-triggers/doctor 动作、content governance 诊断字段和 e2e 覆盖时，才允许迁移；迁移前必须更新这些检查到新实现。
- checks:
  - `regex` `codex-rs/tools/src/zmemory_tool.rs`: `system://workspace`
  - `regex` `codex-rs/tools/src/zmemory_tool.rs`: `literal_str_prop\("action", "manage-triggers"`
  - `regex` `codex-rs/tools/src/zmemory_tool.rs`: `literal_str_prop\("action", "audit"`
  - `regex` `codex-rs/tools/src/zmemory_tool.rs`: `literal_str_prop\("action", "doctor"`
  - `regex` `codex-rs/zmemory/src/doctor.rs`: `content_governance_conflicts`
  - `regex` `codex-rs/zmemory/src/service/tests.rs`: `contentGovernanceIssueCount`
  - `regex` `codex-rs/zmemory/src/service/tests.rs`: `content_governance_conflicts`
  - `regex` `codex-rs/core/tests/suite/zmemory_e2e.rs`: `zmemory_function_audit_exposes_recent_entries`
  - `regex` `codex-rs/core/tests/suite/zmemory_e2e.rs`: `read system://workspace: workspace view`
  - `regex` `codex-rs/core/tests/suite/zmemory_e2e.rs`: `structured_content\["result"\]\["governance"\]\["status"\]`

## Latest Audit

- overall: `24/24` passed

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
| `buddy-surface` | `PASS` | codex-rs/tui + codex-rs/core config + codex-rs/app-server |
| `chinese-localization-sentinels` | `PASS` | codex-rs/cli + codex-rs/tui + codex-rs/tools + codex-rs/app-server |
| `session-warning-steer-localization-bridge` | `PASS` | codex-rs/core + codex-rs/app-server + codex-rs/tui + tests |
| `community-branding-and-release-links` | `PASS` | README + install/update surfaces |
| `zoffsec-native-command-workflow` | `PASS` | codex-rs/cli + codex-rs/tui + codex-rs/rollout |
| `local-analysis-tools-runtime-wiring` | `PASS` | codex-rs/tools + codex-rs/core + codex-rs/mcp-server |
| `pending-input-routing-and-zmemory-recall` | `PASS` | codex-rs/core session/tasks/tools |
| `zteam-mission-workflow` | `PASS` | codex-rs/tui + codex-rs/config + codex-rs/features + docs |
| `inter-agent-visibility-filtering` | `PASS` | codex-rs/protocol + codex-rs/core + codex-rs/app-server-protocol + codex-rs/tui |
| `subagent-runtime-config-preservation` | `PASS` | codex-rs/core config/session/tools |
| `native-tldr-daemon-first-runtime` | `PASS` | codex-rs/native-tldr + codex-rs/cli + codex-rs/core + codex-rs/mcp-server |
| `ztok-default-launcher-and-prompt-wiring` | `PASS` | codex-rs/arg0 + codex-rs/core session/tools + codex-rs/ztok |
| `ztok-behavior-mode` | `PASS` | codex-rs/config + codex-rs/core + codex-rs/cli + codex-rs/ztok |
| `zmemory-governance-system-views-and-diagnostics` | `PASS` | codex-rs/zmemory + codex-rs/tools + codex-rs/core tests |

### `wire-api-streaming-chat-anthropic`
- status: `PASS`
- kind: `local_behavior`
- summary: 为 WireApi::Chat 和 WireApi::Anthropic 提供真实 streaming，而不是 runtime panic 占位。
- better_when: upstream 以同等或更好的方式同时覆盖 Chat/Anthropic streaming，并继续透传 effort、summary、service_tier 与正确 endpoint telemetry。
- evidence:
  - `ok` `codex-rs/core/src/client.rs`: codex-rs/core/src/client.rs:1626 async fn stream_chat_api(
  - `ok` `codex-rs/core/src/client.rs`: codex-rs/core/src/client.rs:1714 async fn stream_anthropic_api(
  - `ok` `codex-rs/core/src/client.rs`: codex-rs/core/src/client.rs:144 const CHAT_COMPLETIONS_ENDPOINT: &str = "/chat/completions";
  - `ok` `codex-rs/core/src/client.rs`: codex-rs/core/src/client.rs:145 const ANTHROPIC_MESSAGES_ENDPOINT: &str = "/messages";
  - `ok` `codex-rs/codex-api/src/endpoint/anthropic.rs`: codex-rs/codex-api/src/endpoint/anthropic.rs:20 pub struct AnthropicClient<T: HttpTransport> {

### `responses-max-output-tokens-from-provider`
- status: `PASS`
- kind: `local_behavior`
- summary: Responses 请求继续从 provider 元数据读取 max_output_tokens，而不是静态写死。
- better_when: upstream 提供了更明确的 provider 级输出上限策略，且不会让本地 provider 配置回退成硬编码 None。
- evidence:
  - `ok` `codex-rs/core/src/client.rs`: codex-rs/core/src/client.rs:920 let max_output_tokens = self

### `zconfig-layer-loading`
- status: `PASS`
- kind: `local_behavior`
- summary: 显式加载 $CODEX_HOME/zconfig.toml，并把它放在 User 与 Project 之间。
- better_when: upstream 原生提供同等层级和优先级的 zconfig 装载逻辑，且不改变本地既有覆盖顺序。
- evidence:
  - `ok` `codex-rs/core/src/config_loader/mod.rs`: codex-rs/core/src/config_loader/mod.rs:14 use codex_config::ZCONFIG_TOML_FILE;
  - `ok` `codex-rs/core/src/config_loader/mod.rs`: codex-rs/core/src/config_loader/mod.rs:254 ConfigLayerSource::ZConfig {
  - `ok` `codex-rs/core/src/config_loader/mod.rs`: codex-rs/core/src/config_loader/mod.rs:261 layers.push(zconfig_layer);

### `models-manager-provider-overrides`
- status: `PASS`
- kind: `local_behavior`
- summary: 保留 provider.model_catalog 过滤、skip_reasoning_popup 传播、按 provider 选择默认远端模型目录，以及本地 synthetic/fallback ModelInfo 的字段完整性。
- better_when: upstream 把 provider.model_catalog、skip_reasoning_popup、Anthropic 默认模型目录和本地 synthetic ModelInfo 的字段补齐都整合成更完整的实现，且本地配置行为不退化。
- evidence:
  - `ok` `codex-rs/models-manager/src/manager.rs`: codex-rs/models-manager/src/manager.rs:463 let mut models = if let Some(catalog_slugs) = provider_info.model_catalog.as_ref() {
  - `ok` `codex-rs/models-manager/src/manager.rs`: codex-rs/models-manager/src/manager.rs:478 if provider_info.skip_reasoning_popup {
  - `ok` `codex-rs/models-manager/src/manager.rs`: codex-rs/models-manager/src/manager.rs:225 // Equivalent legacy anchor: default_remote_models_for_provider(&provider_info)
  - `ok` `codex-rs/models-manager/src/manager.rs`: codex-rs/models-manager/src/manager.rs:448 WireApi::Anthropic => model_info::anthropic_model_catalog(),
  - `ok` `codex-rs/models-manager/src/manager.rs`: codex-rs/models-manager/src/manager.rs:514 max_context_window: None,
  - `ok` `codex-rs/models-manager/src/model_info.rs`: codex-rs/models-manager/src/model_info.rs:181 max_context_window: None,

### `responses-reasoning-content-strip`
- status: `PASS`
- kind: `local_behavior`
- summary: Responses replay 时剥离 raw reasoning.content，保留 summary / encrypted_content，避免出站请求变成非法 payload。
- better_when: upstream 提供更靠近出站层的统一处理，并仍保证 raw reasoning_text 不会回传给 Responses API。
- evidence:
  - `ok` `codex-rs/core/src/client_common.rs`: codex-rs/core/src/client_common.rs:69 if let ResponseItem::Reasoning { content, .. } = item {
  - `ok` `codex-rs/core/src/client_common.rs`: codex-rs/core/src/client_common.rs:72 *content = None;
  - `ok` `codex-rs/protocol/src/models.rs`: codex-rs/protocol/src/models.rs:710 #[serde(default, skip_serializing_if = "should_serialize_reasoning_content")]

### `reference-context-reinjection-baseline`
- status: `PASS`
- kind: `local_behavior`
- summary: resume、compact 和 replacement history 之后继续维护 reference_context_item 基线与全量上下文重注入。
- better_when: upstream 改成新的上下文基线机制，但仍完整覆盖 replacement history、clear baseline 和 full reinjection 语义。
- evidence:
  - `ok` `codex-rs/core/src/session/mod.rs`: codex-rs/core/src/session/mod.rs:2730 pub(crate) async fn record_context_updates_and_set_reference_context_item(
  - `ok` `codex-rs/core/src/context_manager/history.rs`: codex-rs/core/src/context_manager/history.rs:190 pub(crate) fn replacement_reference_context_item(
  - `ok` `codex-rs/core/src/context_manager/history.rs`: codex-rs/core/src/context_manager/history.rs:450 self.reference_context_item = None;

### `auto-tldr-routing-default`
- status: `PASS`
- kind: `local_behavior`
- summary: 工具配置默认继续启用 auto_tldr_routing，并保留显式 with_auto_tldr_routing 链路。
- better_when: upstream 用新的工具路由配置替换了 auto_tldr_routing，且默认行为不回退。
- evidence:
  - `ok` `codex-rs/tools/src/tool_config.rs`: codex-rs/tools/src/tool_config.rs:247 auto_tldr_routing: AutoTldrRoutingMode::default(),
  - `ok` `codex-rs/tools/src/tool_config.rs`: codex-rs/tools/src/tool_config.rs:318 pub fn with_auto_tldr_routing(mut self, auto_tldr_routing: AutoTldrRoutingMode) -> Self {

### `local-crates-zmemory-ztok`
- status: `PASS`
- kind: `local_surface`
- summary: 本地分叉附加 crate `native-tldr`、`zmemory` 与 `ztok` 必须继续存在，并保持 workspace member / dependency 接线完整。
- better_when: 只有在本地确定把这些 crate 整体迁移或替换到新的路径，并同步更新这里的检查路径与 Cargo workspace 接线检查时，才允许变更。
- evidence:
  - `ok` `codex-rs/native-tldr`: codex-rs/native-tldr exists (dir)
  - `ok` `codex-rs/zmemory`: codex-rs/zmemory exists (dir)
  - `ok` `codex-rs/ztok`: codex-rs/ztok exists (dir)
  - `ok` `codex-rs/Cargo.toml`: codex-rs/Cargo.toml:54 "native-tldr",
  - `ok` `codex-rs/Cargo.toml`: codex-rs/Cargo.toml:55 "zmemory",
  - `ok` `codex-rs/Cargo.toml`: codex-rs/Cargo.toml:66 "ztok",
  - `ok` `codex-rs/Cargo.toml`: codex-rs/Cargo.toml:171 codex-native-tldr = { path = "native-tldr" }
  - `ok` `codex-rs/Cargo.toml`: codex-rs/Cargo.toml:222 codex-zmemory = { path = "zmemory" }
  - `ok` `codex-rs/Cargo.toml`: codex-rs/Cargo.toml:223 codex-ztok = { path = "ztok" }

### `cli-zmemory-ztok-ztldr-surface`
- status: `PASS`
- kind: `local_surface`
- summary: 顶层 `codex` CLI 必须继续暴露 `ztok`、`ztldr` 与 `zmemory` 子命令，并保留对应 dispatch 与 help 汉化哨兵。
- better_when: 只有在 upstream 原生提供等效 CLI surface，且本地不再需要这些分叉入口或其汉化收口时，才允许迁移；迁移前必须先把新的入口路径与哨兵更新到这里。
- evidence:
  - `ok` `codex-rs/cli/src/main.rs`: codex-rs/cli/src/main.rs:137 Ztok(ZtokArgs),
  - `ok` `codex-rs/cli/src/main.rs`: codex-rs/cli/src/main.rs:146 #[clap(name = "ztldr")]
  - `ok` `codex-rs/cli/src/main.rs`: codex-rs/cli/src/main.rs:150 Zmemory(ZmemoryCli),
  - `ok` `codex-rs/cli/src/main.rs`: codex-rs/cli/src/main.rs:180 #[clap(visible_alias = "r")]
  - `ok` `codex-rs/cli/src/main.rs`: codex-rs/cli/src/main.rs:954 tldr_cmd::run_tldr_command(tldr_cli).await?;
  - `ok` `codex-rs/cli/src/main.rs`: codex-rs/cli/src/main.rs:962 run_zmemory_command(zmemory_cli).await?;
  - `ok` `codex-rs/cli/src/main.rs`: codex-rs/cli/src/main.rs:1921 let rendered = localize_help_output(err.to_string());
  - `ok` `codex-rs/cli/src/main.rs`: codex-rs/cli/src/main.rs:1989 "显示帮助（使用 '-h' 查看摘要）",
  - `ok` `codex-rs/cli/src/main.rs`: codex-rs/cli/src/main.rs:1996 .replace("Print version", "显示版本")

### `resume-fork-provider-bridge`
- status: `PASS`
- kind: `local_behavior`
- summary: `resume` / `fork` 这类复用 `TuiCli` 的交互子命令，继续允许通过 `-P/--provider` 与 `--local-provider` 切换 model_provider，且 merge 后真正写入最终 interactive 配置。
- better_when: upstream 把 interactive CLI 参数合并统一收敛为等效或更强的实现，并继续保证 `resume` / `fork` 等子命令不会在 bridge 阶段静默丢失 provider / local-provider 等 interactive 参数；迁移前必须先把新的桥接点和回归测试锚点更新到这里。
- evidence:
  - `ok` `codex-rs/tui/src/cli.rs`: codex-rs/tui/src/cli.rs:73 pub provider: Option<String>,
  - `ok` `codex-rs/tui/src/cli.rs`: codex-rs/tui/src/cli.rs:81 pub oss_provider: Option<String>,
  - `ok` `codex-rs/cli/src/main.rs`: codex-rs/cli/src/main.rs:1858 interactive.provider = Some(provider);
  - `ok` `codex-rs/cli/src/main.rs`: codex-rs/cli/src/main.rs:1861 interactive.oss_provider = Some(oss_provider);
  - `ok` `codex-rs/cli/src/main.rs`: codex-rs/cli/src/main.rs:2465 fn resume_merges_option_flags_and_full_auto() {
  - `ok` `codex-rs/cli/src/main.rs`: codex-rs/cli/src/main.rs:2496 assert_eq!(interactive.provider.as_deref(), Some("oss"));
  - `ok` `codex-rs/cli/src/main.rs`: codex-rs/cli/src/main.rs:2528 fn fork_merges_provider_flags() {
  - `ok` `codex-rs/cli/src/main.rs`: codex-rs/cli/src/main.rs:2543 assert_eq!(interactive.oss_provider.as_deref(), Some("lmstudio"));

### `buddy-surface`
- status: `PASS`
- kind: `local_surface`
- summary: Buddy 交互面、配置落盘事件、app-server 通知桥接，以及混合本地预设/AI 反应策略必须继续存在。
- better_when: upstream 原生提供等效 buddy 能力，且同时覆盖可见交互、配置落盘、app-server 通知、reaction_strategy 配置、local preset fallback 与 AI cooldown/critical 场景语义；或者本地正式迁移到新模块并同步更新检查点。
- evidence:
  - `ok` `codex-rs/tui/src/buddy/mod.rs`: codex-rs/tui/src/buddy/mod.rs:91 "小伙伴已孵化：{} {}。",
  - `ok` `codex-rs/tui/src/chatwidget.rs`: codex-rs/tui/src/chatwidget.rs:6043 "小伙伴命令：`/buddy show`、`/buddy full`、`/buddy pet`、`/buddy hide`、`/buddy status`。".to_string(),
  - `ok` `codex-rs/tui/src/slash_command.rs`: codex-rs/tui/src/slash_command.rs:99 SlashCommand::Buddy => "孵化、抚摸或隐藏底部小伙伴",
  - `ok` `codex-rs/tui/src/app_event.rs`: codex-rs/tui/src/app_event.rs:615 PersistBuddyVisibility(bool),
  - `ok` `codex-rs/tui/src/app_event.rs`: codex-rs/tui/src/app_event.rs:618 PersistBuddyFullVisibility,
  - `ok` `codex-rs/tui/src/app.rs`: codex-rs/tui/src/app.rs:6339 AppEvent::PersistBuddyVisibility(visible) => {
  - `ok` `codex-rs/tui/src/app.rs`: codex-rs/tui/src/app.rs:6342 AppEvent::PersistBuddyFullVisibility => {
  - `ok` `codex-rs/app-server/src/bespoke_event_handling.rs`: codex-rs/app-server/src/bespoke_event_handling.rs:295 EventMsg::BuddySoulGenerated(event) => {
  - `ok` `codex-rs/app-server/src/bespoke_event_handling.rs`: codex-rs/app-server/src/bespoke_event_handling.rs:307 EventMsg::BuddyReaction(event) => {
  - `ok` `codex-rs/config/src/types.rs`: codex-rs/config/src/types.rs:721 pub struct BuddyReactionStrategy {
  - `ok` `codex-rs/config/src/types.rs`: codex-rs/config/src/types.rs:743 pub critical_scenarios_use_ai: bool,
  - `ok` `codex-rs/core/src/buddy.rs`: codex-rs/core/src/buddy.rs:212 struct LocalReactionLibrary {
  - `ok` `codex-rs/core/src/buddy.rs`: codex-rs/core/src/buddy.rs:692 BuddyReactionMode::Hybrid => {
  - `ok` `codex-rs/config/src/types.rs`: codex-rs/config/src/types.rs:684 pub reaction_strategy: Option<BuddyReactionStrategy>,

### `chinese-localization-sentinels`
- status: `PASS`
- kind: `localized_behavior`
- summary: 用高频哨兵文案检查中文化输出没有被 upstream 英文重新覆盖。
- better_when: 用户可见链路已迁移到新的源码位置，且新的实现保持自然中文表达；需要先更新这里的哨兵文案位置。
- evidence:
  - `ok` `codex-rs/cli/src/main.rs`: codex-rs/cli/src/main.rs:90 /// 若未指定子命令，选项会转发到交互式命令行界面。
  - `ok` `codex-rs/cli/src/main.rs`: codex-rs/cli/src/main.rs:120 /// 以非交互模式运行 Codex。
  - `ok` `codex-rs/cli/src/main.rs`: codex-rs/cli/src/main.rs:1447 println!("已在 config.toml 中启用功能 `{feature}`。");
  - `ok` `codex-rs/tui/src/slash_command.rs`: codex-rs/tui/src/slash_command.rs:80 SlashCommand::Init => "创建 AGENTS.md 文件，为 Codex 提供指令",
  - `ok` `codex-rs/tools/src/request_user_input_tool.rs`: codex-rs/tools/src/request_user_input_tool.rs:91 Some(format!("request_user_input 在 {mode_name} 模式不可用"))
  - `ok` `codex-rs/tui/src/bottom_pane/feedback_view.rs`: codex-rs/tui/src/bottom_pane/feedback_view.rs:325 Some(_) => format!("{prefix}请使用以下链接提交 Issue："),
  - `ok` `codex-rs/tui/src/app.rs`: codex-rs/tui/src/app.rs:211 const EXTERNAL_EDITOR_HINT: &str = "保存并关闭外部编辑器以继续。";
  - `ok` `codex-rs/tui/src/app.rs`: codex-rs/tui/src/app.rs:491 "因 SKILL.md 文件无效，已跳过加载 {error_count} 个技能。"
  - `ok` `codex-rs/tui/src/onboarding/trust_directory.rs`: codex-rs/tui/src/onboarding/trust_directory.rs:49 "你当前位于 ".bold(),
  - `ok` `codex-rs/tui/src/history_cell.rs`: codex-rs/tui/src/history_cell.rs:1258 " 开始使用时，请描述一个任务，或试试这些命令："
  - `ok` `codex-rs/features/src/lib.rs`: codex-rs/features/src/lib.rs:881 name: "外部配置迁移",
  - `ok` `codex-rs/app-server/src/bespoke_event_handling.rs`: codex-rs/app-server/src/bespoke_event_handling.rs:1944 "加载 rollout `{}` 失败：{err}",
  - `ok` `codex-rs/app-server/src/bespoke_event_handling.rs`: codex-rs/app-server/src/bespoke_event_handling.rs:2790 const REVIEW_FALLBACK_MESSAGE: &str = "审查器未输出任何回复。";

### `session-warning-steer-localization-bridge`
- status: `PASS`
- kind: `localized_behavior`
- summary: `core/src/session/mod.rs` 与 `core/src/session/turn_context.rs` 的中文 steer 错误和 warning 文案必须在 app-server 映射、tui 解析和回归测试里保持一致，避免同步上游英文实现时只改一层导致桥接回归。
- better_when: upstream 把这条错误/警告链路统一收敛成不依赖脆弱字符串解析的等效或更强实现，并同步覆盖 warning 前缀、active-turn race、fallback 模型 warning 和 steer 错误映射；迁移前必须先把新的桥接点与回归测试锚点更新到这里。
- evidence:
  - `ok` `codex-rs/core/src/session/mod.rs`: codex-rs/core/src/session/mod.rs:226 message: "当前没有可追加输入的活跃轮次".to_string(),
  - `ok` `codex-rs/core/src/session/mod.rs`: codex-rs/core/src/session/mod.rs:230 message: format!("期望的活跃轮次 ID 为 `{expected}`，但当前为 `{actual}`"),
  - `ok` `codex-rs/core/src/session/mod.rs`: codex-rs/core/src/session/mod.rs:2363 text: format!("警告：{}", message.into()),
  - `ok` `codex-rs/core/src/session/turn_context.rs`: codex-rs/core/src/session/turn_context.rs:716 "未找到模型 `{}` 的元数据，已改用兜底元数据；这可能降低性能并导致异常。",
  - `ok` `codex-rs/app-server/src/codex_message_processor.rs`: codex-rs/app-server/src/codex_message_processor.rs:7551 "无法向审查轮次追加输入".to_string(),
  - `ok` `codex-rs/app-server/src/codex_message_processor.rs`: codex-rs/app-server/src/codex_message_processor.rs:7585 "输入不能为空".to_string(),
  - `ok` `codex-rs/tui/src/app.rs`: codex-rs/tui/src/app.rs:1146 let mismatch_prefix = "期望的活跃轮次 ID 为 `";
  - `ok` `codex-rs/tui/src/app.rs`: codex-rs/tui/src/app.rs:1140 if source.message == "当前没有可追加输入的活跃轮次" {
  - `ok` `codex-rs/analytics/src/analytics_client_tests.rs`: codex-rs/analytics/src/analytics_client_tests.rs:379 message: "无法向审查轮次追加输入".to_string(),
  - `ok` `codex-rs/core/tests/suite/safety_check_downgrade.rs`: codex-rs/core/tests/suite/safety_check_downgrade.rs:96 ContentItem::InputText { text } if text.starts_with("警告：")
  - `ok` `codex-rs/app-server/tests/suite/v2/safety_check_downgrade.rs`: codex-rs/app-server/tests/suite/v2/safety_check_downgrade.rs:420 UserInput::Text { text, .. } if text.starts_with("警告：") => Some(text.as_str()),
  - `ok` `codex-rs/core/src/session/tests.rs`: codex-rs/core/src/session/tests.rs:4880 text: "警告：too many unified exec processes".to_string(),
  - `ok` `codex-rs/core/src/session/tests.rs`: codex-rs/core/src/session/tests.rs:4905 "未找到模型 `mystery-model` 的元数据，已改用兜底元数据；这可能降低性能并导致异常。"

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
  - `ok` `codex-rs/cli/src/main.rs`: codex-rs/cli/src/main.rs:815 Some(Subcommand::Zoffsec(zoffsec_cli)) => {
  - `ok` `codex-rs/cli/src/main.rs`: codex-rs/cli/src/main.rs:1790 fn finalize_zoffsec_resume_interactive(
  - `ok` `codex-rs/cli/src/main.rs`: codex-rs/cli/src/main.rs:2663 fn zoffsec_subcommand_registers_at_top_level() {
  - `ok` `codex-rs/cli/src/main.rs`: codex-rs/cli/src/main.rs:2678 fn finalize_zoffsec_resume_enables_clean_before_resume() {
  - `ok` `codex-rs/cli/src/zoffsec_cmd.rs`: codex-rs/cli/src/zoffsec_cmd.rs:22 pub struct ZoffsecCommand {
  - `ok` `codex-rs/cli/src/zoffsec_cmd.rs`: codex-rs/cli/src/zoffsec_cmd.rs:119 pub async fn run_zoffsec_clean_command(
  - `ok` `codex-rs/cli/src/zoffsec_config.rs`: codex-rs/cli/src/zoffsec_config.rs:3 pub const ZOFFSEC_SESSION_MARKER: &str = "codex-zoffsec";
  - `ok` `codex-rs/tui/src/cli.rs`: codex-rs/tui/src/cli.rs:43 pub resume_zoffsec_clean: bool,
  - `ok` `codex-rs/tui/src/lib.rs`: codex-rs/tui/src/lib.rs:1402 if cli.resume_zoffsec_clean {
  - `ok` `codex-rs/tui/src/zoffsec_resume.rs`: codex-rs/tui/src/zoffsec_resume.rs:16 pub(crate) async fn clean_resume_selection_if_needed(
  - `ok` `codex-rs/rollout/src/patch.rs`: codex-rs/rollout/src/patch.rs:113 pub async fn clean_zoffsec_rollout(

### `local-analysis-tools-runtime-wiring`
- status: `PASS`
- kind: `local_behavior`
- summary: `ztldr` 与 `zmemory` 不能只保留 crate 和 CLI；它们必须继续接入共享 tool registry plan、handler 映射和 tests/all 聚合面。
- better_when: 只有在 upstream 或本地新架构提供等效的运行时工具接线，并继续保证 `ztldr`/`zmemory` 真正暴露给模型、handler 可分发且 e2e 聚合测试仍覆盖时，才允许迁移；迁移前必须先把新的 plan、handler 与测试锚点更新到这里。
- evidence:
  - `ok` `codex-rs/tools/src/tool_registry_plan.rs`: codex-rs/tools/src/tool_registry_plan.rs:268 let spec = create_tldr_tool();
  - `ok` `codex-rs/tools/src/tool_registry_plan.rs`: codex-rs/tools/src/tool_registry_plan.rs:275 plan.register_handler(name, ToolHandlerKind::Tldr);
  - `ok` `codex-rs/tools/src/tool_registry_plan.rs`: codex-rs/tools/src/tool_registry_plan.rs:279 for spec in std::iter::once(create_zmemory_tool()).chain(create_zmemory_mcp_tools()) {
  - `ok` `codex-rs/tools/src/tool_registry_plan.rs`: codex-rs/tools/src/tool_registry_plan.rs:286 plan.register_handler(name, ToolHandlerKind::Zmemory);
  - `ok` `codex-rs/core/src/tools/spec.rs`: codex-rs/core/src/tools/spec.rs:263 ToolHandlerKind::Tldr => {
  - `ok` `codex-rs/core/src/tools/spec.rs`: codex-rs/core/src/tools/spec.rs:293 ToolHandlerKind::Zmemory => {
  - `ok` `codex-rs/core/tests/suite/mod.rs`: codex-rs/core/tests/suite/mod.rs:101 mod tldr_e2e;
  - `ok` `codex-rs/core/tests/suite/mod.rs`: codex-rs/core/tests/suite/mod.rs:118 mod zmemory_e2e;
  - `ok` `codex-rs/core/tests/suite/tldr_e2e.rs`: codex-rs/core/tests/suite/tldr_e2e.rs:169 assert!(tool_names(&body).contains(&"ztldr".to_string()));
  - `ok` `codex-rs/core/tests/suite/zmemory_e2e.rs`: codex-rs/core/tests/suite/zmemory_e2e.rs:2365 async fn zmemory_recall_note_is_injected_into_follow_up_turn_requests() -> Result<()> {

### `pending-input-routing-and-zmemory-recall`
- status: `PASS`
- kind: `local_behavior`
- summary: turn 起始和 mid-turn 的 `pending_input` 必须保留现有 tool routing 基线、按最新 steer 合并指令，并把 zmemory recall note 注入到后续 developer 上下文。
- better_when: 只有在 upstream 用新的 turn-local 状态机制等效覆盖 pending_input 路由合并、regular turn recall 生产、follow-up developer 注入和相关回归测试时，才允许迁移；迁移前必须先把新的状态流锚点更新到这里。
- evidence:
  - `ok` `codex-rs/core/src/tasks/mod.rs`: codex-rs/core/src/tasks/mod.rs:348 // merge_tool_routing_directives(current_directives, &pending_turn_inputs);
  - `ok` `codex-rs/core/src/tasks/mod.rs`: codex-rs/core/src/tasks/mod.rs:349 // self.set_pending_zmemory_recall_note(turn_context.sub_id.as_str(), recall_note)
  - `ok` `codex-rs/core/src/session/mod.rs`: codex-rs/core/src/session/mod.rs:2496 // pending_zmemory_recall_note_for(current_context.sub_id.as_str())
  - `ok` `codex-rs/core/src/session/mod.rs`: codex-rs/core/src/session/mod.rs:2497 // build_developer_update_item(vec![recall_note])
  - `ok` `codex-rs/core/src/session/turn.rs`: codex-rs/core/src/session/turn.rs:2322 pub(crate) async fn apply_pending_user_input_side_effects(
  - `ok` `codex-rs/core/src/session/turn.rs`: codex-rs/core/src/session/turn.rs:2331 merge_tool_routing_directives(current_directives, user_inputs);
  - `ok` `codex-rs/core/src/session/turn.rs`: codex-rs/core/src/session/turn.rs:2337 // build_stable_preference_recall_note(sess, turn_context, &user_inputs).await
  - `ok` `codex-rs/core/src/session/tests.rs`: codex-rs/core/src/session/tests.rs:4913 async fn turn_start_zmemory_recall_note_is_produced_for_regular_user_turns() {
  - `ok` `codex-rs/core/src/session/tests.rs`: codex-rs/core/src/session/tests.rs:4918 async fn pending_user_input_neutral_steer_preserves_existing_tldr_directives() {
  - `ok` `codex-rs/core/tests/suite/zmemory_e2e.rs`: codex-rs/core/tests/suite/zmemory_e2e.rs:2365 async fn zmemory_recall_note_is_injected_into_follow_up_turn_requests() -> Result<()> {

### `zteam-mission-workflow`
- status: `PASS`
- kind: `local_surface`
- summary: 默认开启的 TUI `/zteam` 本地协作入口、mission-first 工作台、frontend/backend worker 编排、自动推进/repair、恢复语义和 federation adapter seam 必须继续存在。
- better_when: 只有在 upstream 或本地新架构提供等效或更强的 TUI-first 多协作者 mission 工作流，且继续覆盖默认启用配置、slash command 入口、AppEvent/app loop bridge、Mission Board、autopilot repair、loaded-thread recovery、federation adapter seam、中文提示和快照回归锚点时，才允许迁移；迁移前必须先更新这里的路径与检查点。
- evidence:
  - `ok` `codex-rs/config/src/types.rs`: codex-rs/config/src/types.rs:585 pub zteam_enabled: bool,
  - `ok` `codex-rs/core/src/config/mod.rs`: codex-rs/core/src/config/mod.rs:2562 zteam_enabled: cfg.tui.as_ref().map(|t| t.zteam_enabled).unwrap_or(true),
  - `ok` `codex-rs/features/src/lib.rs`: codex-rs/features/src/lib.rs:807 key: "multi_agent_v2",
  - `ok` `codex-rs/features/src/lib.rs`: codex-rs/features/src/lib.rs:806 id: Feature::MultiAgentV2,
  - `ok` `codex-rs/tui/src/lib.rs`: codex-rs/tui/src/lib.rs:185 mod zteam;
  - `ok` `codex-rs/tui/src/slash_command.rs`: codex-rs/tui/src/slash_command.rs:87 SlashCommand::Zteam => "以目标启动 ZTeam mission 协作并查看状态",
  - `ok` `codex-rs/tui/src/slash_command.rs`: codex-rs/tui/src/slash_command.rs:87 SlashCommand::Zteam => "以目标启动 ZTeam mission 协作并查看状态",
  - `ok` `codex-rs/tui/src/bottom_pane/slash_commands.rs`: codex-rs/tui/src/bottom_pane/slash_commands.rs:37 .filter(|(_, cmd)| flags.zteam_enabled || *cmd != SlashCommand::Zteam)
  - `ok` `codex-rs/tui/src/app_event.rs`: codex-rs/tui/src/app_event.rs:115 ZteamCommand(ZteamCommand),
  - `ok` `codex-rs/tui/src/app.rs`: codex-rs/tui/src/app.rs:2163 async fn handle_zteam_command(
  - `ok` `codex-rs/tui/src/app.rs`: codex-rs/tui/src/app.rs:2100 fn schedule_zteam_autopilot_tick(&self) {
  - `ok` `codex-rs/tui/src/app.rs`: codex-rs/tui/src/app.rs:3939 self.restore_loaded_zteam_workers(app_server).await;
  - `ok` `codex-rs/tui/src/zteam.rs`: codex-rs/tui/src/zteam.rs:324 pub(crate) enum AutopilotWorkItem {
  - `ok` `codex-rs/tui/src/zteam/recovery.rs`: codex-rs/tui/src/zteam/recovery.rs:51 pub(crate) fn latest_local_threads_for_primary(
  - `ok` `codex-rs/tui/src/zteam/worker_source.rs`: codex-rs/tui/src/zteam/worker_source.rs:17 pub(crate) struct FederationAdapter {
  - `ok` `codex-rs/tui/src/zteam/view.rs`: codex-rs/tui/src/zteam/view.rs:126 Paragraph::new(Line::from(format!("{MODE_NAME} Mission Board").bold()))
  - `ok` `codex-rs/tui/src/chatwidget/tests/slash_commands.rs`: codex-rs/tui/src/chatwidget/tests/slash_commands.rs:1488 "zteam_workbench_active_view",
  - `ok` `codex-rs/tui/src/chatwidget/snapshots/codex_tui__chatwidget__tests__zteam_workbench_active_view.snap`: codex-rs/tui/src/chatwidget/snapshots/codex_tui__chatwidget__tests__zteam_workbench_active_view.snap exists (file)
  - `ok` `codex-rs/tui/src/chatwidget/snapshots/codex_tui__chatwidget__tests__zteam_entry_disabled_notice.snap`: codex-rs/tui/src/chatwidget/snapshots/codex_tui__chatwidget__tests__zteam_entry_disabled_notice.snap exists (file)
  - `ok` `docs/slash_commands.md`: docs/slash_commands.md:11 `/zteam` 是 TUI 内的本地协作入口。当前底层仍固定复用两个本地 worker，但推荐心智已经从“手动管理 frontend/backend 双 worker”切到“先给目标，再进入 ZTeam mission 协作”。

### `inter-agent-visibility-filtering`
- status: `PASS`
- kind: `local_behavior`
- summary: 智能体间通信 envelope 和隐藏 subagent notification 必须继续由 protocol 统一识别并在 core、app-server thread history、TUI replay 等可见文本链路中过滤。
- better_when: upstream 提供更统一的隐藏消息模型，并且 app-server thread/read、core event mapping、last assistant extraction、TUI history replay 都继续不泄露 inter-agent envelope 或隐藏 subagent notification。
- evidence:
  - `ok` `codex-rs/protocol/src/protocol.rs`: codex-rs/protocol/src/protocol.rs:831 pub struct InterAgentCommunication {
  - `ok` `codex-rs/protocol/src/protocol.rs`: codex-rs/protocol/src/protocol.rs:879 pub fn is_hidden_message_text(text: &str) -> bool {
  - `ok` `codex-rs/protocol/src/protocol.rs`: codex-rs/protocol/src/protocol.rs:890 pub fn sanitize_visible_text(text: &str) -> String {
  - `ok` `codex-rs/protocol/src/protocol.rs`: codex-rs/protocol/src/protocol.rs:898 pub fn is_hidden_subagent_notification_text(text: &str) -> bool {
  - `ok` `codex-rs/app-server-protocol/src/protocol/thread_history.rs`: codex-rs/app-server-protocol/src/protocol/thread_history.rs:267 let sanitized_message = InterAgentCommunication::sanitize_visible_text(&payload.message);
  - `ok` `codex-rs/core/src/event_mapping.rs`: codex-rs/core/src/event_mapping.rs:88 let text = InterAgentCommunication::sanitize_visible_text(text);
  - `ok` `codex-rs/core/src/stream_events_utils.rs`: codex-rs/core/src/stream_events_utils.rs:68 let visible_text = InterAgentCommunication::sanitize_visible_text(&without_citations);
  - `ok` `codex-rs/app-server/tests/suite/v2/thread_read.rs`: codex-rs/app-server/tests/suite/v2/thread_read.rs:173 async fn thread_read_include_turns_skips_inter_agent_envelope_messages() -> Result<()> {
  - `ok` `codex-rs/app-server/tests/suite/v2/thread_read.rs`: codex-rs/app-server/tests/suite/v2/thread_read.rs:324 async fn thread_read_include_turns_skips_subagent_notification_agent_messages() -> Result<()> {
  - `ok` `codex-rs/core/src/event_mapping_tests.rs`: codex-rs/core/src/event_mapping_tests.rs:142 fn skips_serialized_inter_agent_communication() {
  - `ok` `codex-rs/core/src/event_mapping_tests.rs`: codex-rs/core/src/event_mapping_tests.rs:165 fn skips_hidden_subagent_notification_user_message() {
  - `ok` `codex-rs/tui/src/chatwidget/tests/history_replay.rs`: codex-rs/tui/src/chatwidget/tests/history_replay.rs:89 async fn thread_snapshot_replay_hides_inter_agent_envelope_messages() {
  - `ok` `codex-rs/tui/src/chatwidget/tests/history_replay.rs`: codex-rs/tui/src/chatwidget/tests/history_replay.rs:137 async fn replayed_subagent_notification_user_message_is_hidden() {

### `subagent-runtime-config-preservation`
- status: `PASS`
- kind: `local_behavior`
- summary: spawn/resume subagent 时继续保留运行时 provider、model、sandbox、developer instructions 等 live turn 状态；只有 turn cwd 确实命中启用的 project config layer 时才重载 project-scoped 配置。
- better_when: upstream 提供更清晰的 subagent config 构建机制，并同时保留运行时 provider/details、不误用禁用 project layer、且仍能在 turn cwd override 命中启用项目层时加载 project-scoped zmemory/profile/agent_roles。
- evidence:
  - `ok` `codex-rs/core/src/tools/handlers/multi_agents_common.rs`: codex-rs/core/src/tools/handlers/multi_agents_common.rs:234 pub(crate) async fn build_agent_shared_config(
  - `ok` `codex-rs/core/src/tools/handlers/multi_agents_common.rs`: codex-rs/core/src/tools/handlers/multi_agents_common.rs:8 use crate::config_loader::load_config_layers_state;
  - `ok` `codex-rs/core/src/tools/handlers/multi_agents_common.rs`: codex-rs/core/src/tools/handlers/multi_agents_common.rs:315 let has_enabled_project_layer = reloaded_config
  - `ok` `codex-rs/core/src/tools/handlers/multi_agents_common.rs`: codex-rs/core/src/tools/handlers/multi_agents_common.rs:322 .any(|layer| matches!(layer.name, ConfigLayerSource::Project { .. }));
  - `ok` `codex-rs/core/src/tools/handlers/multi_agents_common.rs`: codex-rs/core/src/tools/handlers/multi_agents_common.rs:324 if has_enabled_project_layer && reloaded_for_comparison != *live_config {
  - `ok` `codex-rs/core/src/tools/handlers/multi_agents_tests.rs`: codex-rs/core/src/tools/handlers/multi_agents_tests.rs:3694 async fn build_agent_spawn_config_preserves_runtime_provider_details() {
  - `ok` `codex-rs/core/src/tools/handlers/multi_agents_tests.rs`: codex-rs/core/src/tools/handlers/multi_agents_tests.rs:3700 async fn build_agent_spawn_config_reloads_project_scoped_zmemory_profile_for_turn_cwd_override() {
  - `ok` `codex-rs/core/src/tools/handlers/multi_agents_tests.rs`: codex-rs/core/src/tools/handlers/multi_agents_tests.rs:3706 async fn build_agent_spawn_config_preserves_active_profile_when_reloading_turn_cwd_override() {

### `native-tldr-daemon-first-runtime`
- status: `PASS`
- kind: `local_behavior`
- summary: `ztldr` 不只是 CLI/工具注册面；本地默认依赖 native-tldr daemon-first 生命周期、CLI/core/MCP 自动启动、daemon 状态动作和结构化失败回退。
- better_when: 只有在 upstream 或本地新架构提供等效的 daemon-first 运行时，并继续覆盖 CLI、core tool handler、MCP tool handler、daemon action、自动启动禁用配置、结构化 daemon unavailable 错误与测试锚点时，才允许迁移；迁移前必须先更新这些检查到新入口。
- evidence:
  - `ok` `codex-rs/native-tldr/src/daemon.rs`: codex-rs/native-tldr/src/daemon.rs:86 pub enum TldrDaemonCommand {
  - `ok` `codex-rs/native-tldr/src/tool_api.rs`: codex-rs/native-tldr/src/tool_api.rs:1596 pub async fn query_daemon_with_hooks_detailed<Q, E>(
  - `ok` `codex-rs/native-tldr/src/tool_api.rs`: codex-rs/native-tldr/src/tool_api.rs:1489 "native-tldr daemon is unavailable for {project}: {} (hint: {hint})",
  - `ok` `codex-rs/cli/src/tldr_cmd.rs`: codex-rs/cli/src/tldr_cmd.rs:492 TldrSubcommand::Daemon(cmd) => {
  - `ok` `codex-rs/cli/src/tldr_cmd.rs`: codex-rs/cli/src/tldr_cmd.rs:1785 query_daemon_with_hooks_detailed(
  - `ok` `codex-rs/core/src/tools/handlers/tldr.rs`: codex-rs/core/src/tools/handlers/tldr.rs:24 use codex_native_tldr::tool_api::run_tldr_tool_with_hooks;
  - `ok` `codex-rs/core/src/tools/handlers/tldr.rs`: codex-rs/core/src/tools/handlers/tldr.rs:591 async fn ensure_daemon_running_detailed(project_root: &Path) -> Result<DaemonReadyResult> {
  - `ok` `codex-rs/core/src/tools/handlers/tldr.rs`: codex-rs/core/src/tools/handlers/tldr.rs:535 .filter(|_| error.contains("native-tldr daemon is unavailable for"))
  - `ok` `codex-rs/mcp-server/src/tldr_tool.rs`: codex-rs/mcp-server/src/tldr_tool.rs:88 async fn run_tldr_tool_with_mcp_hooks<Q, E>(
  - `ok` `codex-rs/mcp-server/src/tldr_tool.rs`: codex-rs/mcp-server/src/tldr_tool.rs:186 fn daemon_action_spec(args: &TldrToolCallParam) -> Option<(&'static str, TldrDaemonCommand)> {
  - `ok` `codex-rs/mcp-server/src/tldr_tool.rs`: codex-rs/mcp-server/src/tldr_tool.rs:2413 async fn ensure_daemon_running_respects_disabled_auto_start_config() {

### `ztok-default-launcher-and-prompt-wiring`
- status: `PASS`
- kind: `local_behavior`
- summary: `ztok` 必须保持默认本地可用：启动时注入 PATH alias，developer 上下文要求使用 `codex ztok ...`，shell search rewrite 能识别 `ztok grep`，事件输出不能泄露绝对 launcher 路径。
- better_when: 只有在新的本地 wrapper 或 upstream 等效机制同时覆盖 alias 注入、developer prompt、shell rewrite、事件脱敏和对应回归测试时，才允许替换；迁移前必须把新入口和测试锚点写回基线。
- evidence:
  - `ok` `codex-rs/arg0/src/lib.rs`: codex-rs/arg0/src/lib.rs:15 const ZTOK_ARG0: &str = "ztok";
  - `ok` `codex-rs/arg0/src/lib.rs`: codex-rs/arg0/src/lib.rs:356 } else if filename == ZTOK_ARG0 {
  - `ok` `codex-rs/core/templates/compact/ztok.md`: codex-rs/core/templates/compact/ztok.md:3 Use `{{ logical_launcher_invocation }} ztok ...` in user-facing commentary and
  - `ok` `codex-rs/core/src/memories/prompts.rs`: codex-rs/core/src/memories/prompts.rs:46 static ZTOK_DEVELOPER_INSTRUCTIONS_TEMPLATE: LazyLock<Template> = LazyLock::new(|| {
  - `ok` `codex-rs/core/src/memories/prompts.rs`: codex-rs/core/src/memories/prompts.rs:314 pub(crate) fn build_ztok_tool_developer_instructions() -> String {
  - `ok` `codex-rs/core/src/session/mod.rs`: codex-rs/core/src/session/mod.rs:2566 developer_sections.push(build_ztok_tool_developer_instructions());
  - `ok` `codex-rs/core/src/memories/prompts_tests.rs`: codex-rs/core/src/memories/prompts_tests.rs:165 fn build_ztok_tool_developer_instructions_renders_embedded_template() {
  - `ok` `codex-rs/core/src/tools/rewrite/shell_search_rewrite.rs`: codex-rs/core/src/tools/rewrite/shell_search_rewrite.rs:81 [head, next, tail @ ..] if head == "ztok" && next == "grep" => tail,
  - `ok` `codex-rs/core/src/tools/events.rs`: codex-rs/core/src/tools/events.rs:650 async fn shell_emitter_never_exposes_absolute_ztok_exec_path() {

### `ztok-behavior-mode`
- status: `PASS`
- kind: `local_behavior`
- summary: `ztok.behavior` 配置继续支持 enhanced/basic 行为模式，并从 Codex config 经 CLI runtime settings 传递到 ztok 压缩实现。
- better_when: upstream 或本地后续实现用等效配置机制替代 ztok behavior，但仍能选择 enhanced/basic 等价模式，并保证 Codex 配置到 ztok runtime 的传递链路不丢失。
- evidence:
  - `ok` `codex-rs/config/src/types.rs`: codex-rs/config/src/types.rs:92 pub enum ZtokBehavior {
  - `ok` `codex-rs/config/src/types.rs`: codex-rs/config/src/types.rs:1024 pub behavior: Option<ZtokBehavior>,
  - `ok` `codex-rs/core/src/config/types.rs`: codex-rs/core/src/config/types.rs:71 pub behavior: ZtokBehavior,
  - `ok` `codex-rs/cli/src/main.rs`: codex-rs/cli/src/main.rs:1413 config.ztok.behavior.as_str(),
  - `ok` `codex-rs/ztok/src/settings.rs`: codex-rs/ztok/src/settings.rs:13 pub behavior: ZtokBehavior,
  - `ok` `codex-rs/ztok/src/behavior.rs`: codex-rs/ztok/src/behavior.rs:2 pub(crate) enum ZtokBehavior {
  - `ok` `codex-rs/ztok/src/compression.rs`: codex-rs/ztok/src/compression.rs:164 pub(crate) fn compress_for_behavior(

### `zmemory-governance-system-views-and-diagnostics`
- status: `PASS`
- kind: `local_behavior`
- summary: `zmemory` 默认长期记忆能力必须保留系统视图、审计日志、trigger 管理、doctor 诊断和内容治理结果，而不只是保留基本 CRUD/tool schema。
- better_when: 只有在新的 memory runtime 或 upstream 等效实现继续暴露 system://workspace 等系统视图、audit/manage-triggers/doctor 动作、content governance 诊断字段和 e2e 覆盖时，才允许迁移；迁移前必须更新这些检查到新实现。
- evidence:
  - `ok` `codex-rs/tools/src/zmemory_tool.rs`: codex-rs/tools/src/zmemory_tool.rs:114 "目标 URI。支持系统视图：system://boot|defaults|workspace|index|index/<domain>|paths|paths/<domain>|recent|recent/<n>|glossary|...
  - `ok` `codex-rs/tools/src/zmemory_tool.rs`: codex-rs/tools/src/zmemory_tool.rs:259 literal_str_prop("action", "manage-triggers", Some("管理触发词。")),
  - `ok` `codex-rs/tools/src/zmemory_tool.rs`: codex-rs/tools/src/zmemory_tool.rs:321 literal_str_prop("action", "audit", Some("查看最近审计日志。")),
  - `ok` `codex-rs/tools/src/zmemory_tool.rs`: codex-rs/tools/src/zmemory_tool.rs:335 literal_str_prop("action", "doctor", Some("健康检查。")),
  - `ok` `codex-rs/zmemory/src/doctor.rs`: codex-rs/zmemory/src/doctor.rs:117 "content_governance_conflicts"
  - `ok` `codex-rs/zmemory/src/service/tests.rs`: codex-rs/zmemory/src/service/tests.rs:1481 assert_eq!(stats["result"]["contentGovernanceIssueCount"], 1);
  - `ok` `codex-rs/zmemory/src/service/tests.rs`: codex-rs/zmemory/src/service/tests.rs:1683 .any(|issue| issue["code"] == "content_governance_conflicts")
  - `ok` `codex-rs/core/tests/suite/zmemory_e2e.rs`: codex-rs/core/tests/suite/zmemory_e2e.rs:342 async fn zmemory_function_audit_exposes_recent_entries() -> Result<()> {
  - `ok` `codex-rs/core/tests/suite/zmemory_e2e.rs`: codex-rs/core/tests/suite/zmemory_e2e.rs:1755 assert!(output.contains("read system://workspace: workspace view"));
  - `ok` `codex-rs/core/tests/suite/zmemory_e2e.rs`: codex-rs/core/tests/suite/zmemory_e2e.rs:2639 contract_memory.structured_content["result"]["governance"]["status"],
