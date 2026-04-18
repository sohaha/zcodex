# Local Fork Features

这是 `sync-openai-codex-pr` 的本地分叉特性事实源。

用途：

- 同步 upstream 前，先刷新当前分支的基线
- worktree 冲突解决后，用它做第一次保留审查
- 合并回当前分支时，再用它做一次 merge-back gate
- 若本地特性被 rename、move 或被更好的方式等效替换，要先更新这里，再重新刷新

命令：

```bash
node /workspace/.codex/skills/sync-openai-codex-pr/scripts/local_fork_feature_audit.mjs refresh --repo /workspace
node /workspace/.codex/skills/sync-openai-codex-pr/scripts/local_fork_feature_audit.mjs check --repo /workspace
```

维护规则：

- `Spec` 区块是手工维护的源数据
- `Latest Report` 区块由脚本重写，不手工编辑
- 新增本地分叉特性时，先补 `Spec`，再跑一次 `refresh`
- 如果旧特性被更好的实现等效替换，先改 `Spec` 里的检查条件和 `better_when`，再跑一次 `refresh`
- 这个脚本没有外部依赖，可被 cron、CI 或定期巡检任务直接调用，用来持续刷新这份基线
- `refresh` 只应该在特性状态或证据变化时产生 diff；定时巡检本身不应仅因为时间戳而改文件

## Spec

<!-- local-fork-feature-spec:start -->
```json
[
  {
    "id": "wire-api-streaming-chat-anthropic",
    "kind": "local_behavior",
    "area": "codex-rs/core + codex-rs/codex-api",
    "summary": "为 WireApi::Chat 和 WireApi::Anthropic 提供真实 streaming，而不是 runtime panic 占位。",
    "better_when": "upstream 以同等或更好的方式同时覆盖 Chat/Anthropic streaming，并继续透传 effort、summary、service_tier 与正确 endpoint telemetry。",
    "checks": [
      {
        "type": "regex",
        "path": "codex-rs/core/src/client.rs",
        "pattern": "async fn stream_chat_api\\("
      },
      {
        "type": "regex",
        "path": "codex-rs/core/src/client.rs",
        "pattern": "async fn stream_anthropic_api\\("
      },
      {
        "type": "regex",
        "path": "codex-rs/core/src/client.rs",
        "pattern": "CHAT_COMPLETIONS_ENDPOINT"
      },
      {
        "type": "regex",
        "path": "codex-rs/core/src/client.rs",
        "pattern": "ANTHROPIC_MESSAGES_ENDPOINT"
      },
      {
        "type": "regex",
        "path": "codex-rs/codex-api/src/endpoint/anthropic.rs",
        "pattern": "pub struct AnthropicClient"
      }
    ]
  },
  {
    "id": "responses-max-output-tokens-from-provider",
    "kind": "local_behavior",
    "area": "codex-rs/core",
    "summary": "Responses 请求继续从 provider 元数据读取 max_output_tokens，而不是静态写死。",
    "better_when": "upstream 提供了更明确的 provider 级输出上限策略，且不会让本地 provider 配置回退成硬编码 None。",
    "checks": [
      {
        "type": "regex",
        "path": "codex-rs/core/src/client.rs",
        "pattern": "let max_output_tokens = self\\s*\\.\\s*client\\s*\\.\\s*state\\s*\\.\\s*provider\\s*\\.\\s*info\\(\\)\\s*\\.\\s*max_output_tokens\\s*\\.\\s*filter\\(\\|v\\| \\*v > 0\\)"
      }
    ]
  },
  {
    "id": "zconfig-layer-loading",
    "kind": "local_behavior",
    "area": "codex-rs/core config",
    "summary": "显式加载 $CODEX_HOME/zconfig.toml，并把它放在 User 与 Project 之间。",
    "better_when": "upstream 原生提供同等层级和优先级的 zconfig 装载逻辑，且不改变本地既有覆盖顺序。",
    "checks": [
      {
        "type": "regex",
        "path": "codex-rs/core/src/config_loader/mod.rs",
        "pattern": "ZCONFIG_TOML_FILE"
      },
      {
        "type": "regex",
        "path": "codex-rs/core/src/config_loader/mod.rs",
        "pattern": "ConfigLayerSource::ZConfig"
      },
      {
        "type": "regex",
        "path": "codex-rs/core/src/config_loader/mod.rs",
        "pattern": "layers\\.push\\(zconfig_layer\\)"
      }
    ]
  },
  {
    "id": "models-manager-provider-overrides",
    "kind": "local_behavior",
    "area": "codex-rs/models-manager",
    "summary": "保留 provider.model_catalog 过滤、skip_reasoning_popup 传播，以及按 provider 选择默认远端模型目录。",
    "better_when": "upstream 把 provider.model_catalog、skip_reasoning_popup 和 Anthropic 默认模型目录都整合成更完整的实现，且本地配置行为不退化。",
    "checks": [
      {
        "type": "regex",
        "path": "codex-rs/models-manager/src/manager.rs",
        "pattern": "provider_info\\.model_catalog"
      },
      {
        "type": "regex",
        "path": "codex-rs/models-manager/src/manager.rs",
        "pattern": "provider_info\\.skip_reasoning_popup"
      },
      {
        "type": "regex",
        "path": "codex-rs/models-manager/src/manager.rs",
        "pattern": "default_remote_models_for_provider\\(&provider_info\\)"
      },
      {
        "type": "regex",
        "path": "codex-rs/models-manager/src/manager.rs",
        "pattern": "anthropic_model_catalog\\("
      }
    ]
  },
  {
    "id": "responses-reasoning-content-strip",
    "kind": "local_behavior",
    "area": "codex-rs/core + codex-rs/protocol",
    "summary": "Responses replay 时剥离 raw reasoning.content，保留 summary / encrypted_content，避免出站请求变成非法 payload。",
    "better_when": "upstream 提供更靠近出站层的统一处理，并仍保证 raw reasoning_text 不会回传给 Responses API。",
    "checks": [
      {
        "type": "regex",
        "path": "codex-rs/core/src/client_common.rs",
        "pattern": "ResponseItem::Reasoning \\{ content, \\.\\. \\}"
      },
      {
        "type": "regex",
        "path": "codex-rs/core/src/client_common.rs",
        "pattern": "\\*content = None;"
      },
      {
        "type": "regex",
        "path": "codex-rs/protocol/src/models.rs",
        "pattern": "skip_serializing_if = \"should_serialize_reasoning_content\""
      }
    ]
  },
  {
    "id": "reference-context-reinjection-baseline",
    "kind": "local_behavior",
    "area": "codex-rs/core session/context_manager",
    "summary": "resume、compact 和 replacement history 之后继续维护 reference_context_item 基线与全量上下文重注入。",
    "better_when": "upstream 改成新的上下文基线机制，但仍完整覆盖 replacement history、clear baseline 和 full reinjection 语义。",
    "checks": [
      {
        "type": "regex",
        "path": "codex-rs/core/src/session/mod.rs",
        "pattern": "record_context_updates_and_set_reference_context_item"
      },
      {
        "type": "regex",
        "path": "codex-rs/core/src/context_manager/history.rs",
        "pattern": "replacement_reference_context_item"
      },
      {
        "type": "regex",
        "path": "codex-rs/core/src/context_manager/history.rs",
        "pattern": "self\\.reference_context_item = None;"
      }
    ]
  },
  {
    "id": "auto-tldr-routing-default",
    "kind": "local_behavior",
    "area": "codex-rs/tools",
    "summary": "工具配置默认继续启用 auto_tldr_routing，并保留显式 with_auto_tldr_routing 链路。",
    "better_when": "upstream 用新的工具路由配置替换了 auto_tldr_routing，且默认行为不回退。",
    "checks": [
      {
        "type": "regex",
        "path": "codex-rs/tools/src/tool_config.rs",
        "pattern": "AutoTldrRoutingMode::default\\(\\)"
      },
      {
        "type": "regex",
        "path": "codex-rs/tools/src/tool_config.rs",
        "pattern": "with_auto_tldr_routing"
      }
    ]
  },
  {
    "id": "local-crates-zmemory-ztok",
    "kind": "local_surface",
    "area": "codex-rs workspace",
    "summary": "本地分叉附加 crate `zmemory` 与 `ztok` 必须继续存在。",
    "better_when": "只有在本地确定把它们整体迁移或替换到新的路径，并同步更新这里的检查路径时，才允许变更。",
    "checks": [
      {
        "type": "exists",
        "path": "codex-rs/zmemory"
      },
      {
        "type": "exists",
        "path": "codex-rs/ztok"
      }
    ]
  },
  {
    "id": "buddy-surface",
    "kind": "local_surface",
    "area": "codex-rs/tui",
    "summary": "Buddy 交互面和中文提示仍然存在，不被 upstream TUI 改动吞掉。",
    "better_when": "upstream 原生提供等效 buddy 能力且本地不再需要维护分叉实现，或者本地把 buddy 正式迁移到新模块并同步更新检查点。",
    "checks": [
      {
        "type": "regex",
        "path": "codex-rs/tui/src/buddy/mod.rs",
        "pattern": "小伙伴已孵化"
      },
      {
        "type": "regex",
        "path": "codex-rs/tui/src/chatwidget.rs",
        "pattern": "小伙伴命令："
      },
      {
        "type": "regex",
        "path": "codex-rs/tui/src/slash_command.rs",
        "pattern": "SlashCommand::Buddy"
      }
    ]
  },
  {
    "id": "chinese-localization-sentinels",
    "kind": "localized_behavior",
    "area": "codex-rs/tui + codex-rs/tools",
    "summary": "用高频哨兵文案检查中文化输出没有被 upstream 英文重新覆盖。",
    "better_when": "用户可见链路已迁移到新的源码位置，且新的实现保持自然中文表达；需要先更新这里的哨兵文案位置。",
    "checks": [
      {
        "type": "regex",
        "path": "codex-rs/tui/src/slash_command.rs",
        "pattern": "创建 AGENTS\\.md 文件，为 Codex 提供指令"
      },
      {
        "type": "regex",
        "path": "codex-rs/tools/src/request_user_input_tool.rs",
        "pattern": "request_user_input 在 \\{mode_name\\} 模式不可用"
      },
      {
        "type": "regex",
        "path": "codex-rs/tui/src/bottom_pane/feedback_view.rs",
        "pattern": "请使用以下链接提交 Issue"
      }
    ]
  },
  {
    "id": "community-branding-and-release-links",
    "kind": "localized_behavior",
    "area": "README + install/update surfaces",
    "summary": "社区分叉 branding 与 release/install 链接继续指向 sohaha/zcodex。",
    "better_when": "仓库决定统一回官方 branding，或者 branding 入口迁移到新文件并同步更新这里的检查路径。",
    "checks": [
      {
        "type": "regex",
        "path": "README.md",
        "pattern": "@sohaha/zcodex"
      },
      {
        "type": "regex",
        "path": "codex-rs/README.md",
        "pattern": "https://github\\.com/sohaha/zcodex/releases"
      },
      {
        "type": "regex",
        "path": "codex-rs/tui/src/update_action.rs",
        "pattern": "@sohaha/zcodex"
      },
      {
        "type": "regex",
        "path": "docs/install.md",
        "pattern": "https://github\\.com/sohaha/zcodex\\.git"
      }
    ]
  }
]
```
<!-- local-fork-feature-spec:end -->

## Latest Report

<!-- local-fork-feature-report:start -->
- overall: `11/11` passed

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
| `buddy-surface` | `PASS` | codex-rs/tui |
| `chinese-localization-sentinels` | `PASS` | codex-rs/tui + codex-rs/tools |
| `community-branding-and-release-links` | `PASS` | README + install/update surfaces |

### `wire-api-streaming-chat-anthropic`
- status: `PASS`
- kind: `local_behavior`
- summary: 为 WireApi::Chat 和 WireApi::Anthropic 提供真实 streaming，而不是 runtime panic 占位。
- better_when: upstream 以同等或更好的方式同时覆盖 Chat/Anthropic streaming，并继续透传 effort、summary、service_tier 与正确 endpoint telemetry。
- evidence:
  - `ok` `codex-rs/core/src/client.rs`: codex-rs/core/src/client.rs:1527 async fn stream_chat_api(
  - `ok` `codex-rs/core/src/client.rs`: codex-rs/core/src/client.rs:1611 async fn stream_anthropic_api(
  - `ok` `codex-rs/core/src/client.rs`: codex-rs/core/src/client.rs:140 const CHAT_COMPLETIONS_ENDPOINT: &str = "/chat/completions";
  - `ok` `codex-rs/core/src/client.rs`: codex-rs/core/src/client.rs:141 const ANTHROPIC_MESSAGES_ENDPOINT: &str = "/messages";
  - `ok` `codex-rs/codex-api/src/endpoint/anthropic.rs`: codex-rs/codex-api/src/endpoint/anthropic.rs:21 pub struct AnthropicClient<T: HttpTransport> {

### `responses-max-output-tokens-from-provider`
- status: `PASS`
- kind: `local_behavior`
- summary: Responses 请求继续从 provider 元数据读取 max_output_tokens，而不是静态写死。
- better_when: upstream 提供了更明确的 provider 级输出上限策略，且不会让本地 provider 配置回退成硬编码 None。
- evidence:
  - `ok` `codex-rs/core/src/client.rs`: codex-rs/core/src/client.rs:861 let max_output_tokens = self

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
- summary: 保留 provider.model_catalog 过滤、skip_reasoning_popup 传播，以及按 provider 选择默认远端模型目录。
- better_when: upstream 把 provider.model_catalog、skip_reasoning_popup 和 Anthropic 默认模型目录都整合成更完整的实现，且本地配置行为不退化。
- evidence:
  - `ok` `codex-rs/models-manager/src/manager.rs`: codex-rs/models-manager/src/manager.rs:232 let remote_models = if let Some(ref catalog_slugs) = provider_info.model_catalog {
  - `ok` `codex-rs/models-manager/src/manager.rs`: codex-rs/models-manager/src/manager.rs:614 if provider_info.skip_reasoning_popup {
  - `ok` `codex-rs/models-manager/src/manager.rs`: codex-rs/models-manager/src/manager.rs:231 .unwrap_or_else(|| Self::default_remote_models_for_provider(&provider_info));
  - `ok` `codex-rs/models-manager/src/manager.rs`: codex-rs/models-manager/src/manager.rs:517 WireApi::Anthropic => model_info::anthropic_model_catalog(),

### `responses-reasoning-content-strip`
- status: `PASS`
- kind: `local_behavior`
- summary: Responses replay 时剥离 raw reasoning.content，保留 summary / encrypted_content，避免出站请求变成非法 payload。
- better_when: upstream 提供更靠近出站层的统一处理，并仍保证 raw reasoning_text 不会回传给 Responses API。
- evidence:
  - `ok` `codex-rs/core/src/client_common.rs`: codex-rs/core/src/client_common.rs:52 if let ResponseItem::Reasoning { content, .. } = item {
  - `ok` `codex-rs/core/src/client_common.rs`: codex-rs/core/src/client_common.rs:55 *content = None;
  - `ok` `codex-rs/protocol/src/models.rs`: codex-rs/protocol/src/models.rs:267 #[serde(default, skip_serializing_if = "should_serialize_reasoning_content")]

### `reference-context-reinjection-baseline`
- status: `PASS`
- kind: `local_behavior`
- summary: resume、compact 和 replacement history 之后继续维护 reference_context_item 基线与全量上下文重注入。
- better_when: upstream 改成新的上下文基线机制，但仍完整覆盖 replacement history、clear baseline 和 full reinjection 语义。
- evidence:
  - `ok` `codex-rs/core/src/session/mod.rs`: codex-rs/core/src/session/mod.rs:2520 pub(crate) async fn record_context_updates_and_set_reference_context_item(
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
- summary: 本地分叉附加 crate `zmemory` 与 `ztok` 必须继续存在。
- better_when: 只有在本地确定把它们整体迁移或替换到新的路径，并同步更新这里的检查路径时，才允许变更。
- evidence:
  - `ok` `codex-rs/zmemory`: codex-rs/zmemory exists (dir)
  - `ok` `codex-rs/ztok`: codex-rs/ztok exists (dir)

### `buddy-surface`
- status: `PASS`
- kind: `local_surface`
- summary: Buddy 交互面和中文提示仍然存在，不被 upstream TUI 改动吞掉。
- better_when: upstream 原生提供等效 buddy 能力且本地不再需要维护分叉实现，或者本地把 buddy 正式迁移到新模块并同步更新检查点。
- evidence:
  - `ok` `codex-rs/tui/src/buddy/mod.rs`: codex-rs/tui/src/buddy/mod.rs:91 "小伙伴已孵化：{} {}。",
  - `ok` `codex-rs/tui/src/chatwidget.rs`: codex-rs/tui/src/chatwidget.rs:5282 "小伙伴命令：`/buddy show`、`/buddy full`、`/buddy pet`、`/buddy hide`、`/buddy status`。".to_string(),
  - `ok` `codex-rs/tui/src/slash_command.rs`: codex-rs/tui/src/slash_command.rs:95 SlashCommand::Buddy => "孵化、抚摸或隐藏底部小伙伴",

### `chinese-localization-sentinels`
- status: `PASS`
- kind: `localized_behavior`
- summary: 用高频哨兵文案检查中文化输出没有被 upstream 英文重新覆盖。
- better_when: 用户可见链路已迁移到新的源码位置，且新的实现保持自然中文表达；需要先更新这里的哨兵文案位置。
- evidence:
  - `ok` `codex-rs/tui/src/slash_command.rs`: codex-rs/tui/src/slash_command.rs:77 SlashCommand::Init => "创建 AGENTS.md 文件，为 Codex 提供指令",
  - `ok` `codex-rs/tools/src/request_user_input_tool.rs`: codex-rs/tools/src/request_user_input_tool.rs:91 Some(format!("request_user_input 在 {mode_name} 模式不可用"))
  - `ok` `codex-rs/tui/src/bottom_pane/feedback_view.rs`: codex-rs/tui/src/bottom_pane/feedback_view.rs:325 Some(_) => format!("{prefix}请使用以下链接提交 Issue："),

### `community-branding-and-release-links`
- status: `PASS`
- kind: `localized_behavior`
- summary: 社区分叉 branding 与 release/install 链接继续指向 sohaha/zcodex。
- better_when: 仓库决定统一回官方 branding，或者 branding 入口迁移到新文件并同步更新这里的检查路径。
- evidence:
  - `ok` `README.md`: README.md:24 npm install -g @sohaha/zcodex
  - `ok` `codex-rs/README.md`: codex-rs/README.md:14 你也可以通过 Homebrew（`brew install --cask codex`）安装，或直接从 [GitHub Releases](https://github.com/sohaha/zcodex/releases) 下载平台...
  - `ok` `codex-rs/tui/src/update_action.rs`: codex-rs/tui/src/update_action.rs:4 /// Update via `npm install -g @sohaha/zcodex@latest`.
  - `ok` `docs/install.md`: docs/install.md:19 git clone https://github.com/sohaha/zcodex.git
<!-- local-fork-feature-report:end -->
