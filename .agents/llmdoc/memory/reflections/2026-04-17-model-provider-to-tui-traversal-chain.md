# 反思：model_providers 配置到 TUI 的完整遍历链路

## 背景
用户需要在 `model_providers.xxx` 中添加 `skip_reasoning_popup: true`，使得在模型选择界面选定模型后直接使用默认推理级别，不弹出推理级别二次选择界面。

## 根因分析
这是典型的**配置 → 模型元数据 → TUI UI 行为**链路问题。需要沿线每个节点都加字段。

## 遍历链路（从配置到 UI）

```
~/.codex/config.toml
  [model_providers.xxx]
    skip_reasoning_popup = true

→ codex_model_provider_info::ModelProviderInfo
    pub skip_reasoning_popup: bool

→ codex_models_manager::ModelsManagerConfig
    pub skip_reasoning_popup: bool
    (via Config::to_models_manager_config)

→ codex_protocol::openai_models::ModelInfo
    pub skip_reasoning_popup: bool
    (via with_config_overrides)

→ codex_protocol::openai_models::ModelPreset
    pub skip_reasoning_popup: bool
    (via From<ModelInfo>)

→ codex_tui::chatwidget::ChatWidget::open_all_models_popup
    dismiss_on_select: single_supported_effort || preset.skip_reasoning_popup
    action: if skip_reasoning_popup {
        直接发送 UpdateModel + UpdateReasoningEffort
    } else {
        发送 OpenReasoningPopup
    }
```

### 关键教训

1. **`dismiss_on_select` 与 action 需协同修改**
   - 仅设置 `dismiss_on_select: true` 只能关闭弹窗，但 action 里的 `OpenReasoningPopup` 仍会被发送，导致界面闪烁后立即弹出推理级别选择。
   - 必须在 action 分支里同时处理 skip 逻辑。

2. **每层都需要添加字段和初始化**
   - `ModelProviderInfo`：`#[serde(default)]` + `false` + 所有 builder/constructor
   - `ModelsManagerConfig`：普通 `bool`
   - `ModelInfo`：`#[serde(default)]` + 所有 `ModelInfo` 字面量（含测试）
   - `ModelPreset`：`#[serde(default)]` + `From<ModelInfo>` impl

3. **`with_config_overrides` 是配置注入元数据的唯一注入点**
   - 所有来自 `model_providers` 配置的模型级别元数据覆盖，都通过这个函数注入。
   - `anthropic_model_catalog()` 等不含配置的初始化路径，需要手动加 `false`。

4. **`model_info_from_slug` 是 fallback 路径**
   - 解析失败时的 fallback 元数据，也需要加字段，否则下游 `ModelPreset::from` 会缺失字段。

5. **测试里的 struct 字面量是常见遗漏点**
   - `core/src/config/config_tests.rs`（8 处）、`models-manager/src/manager_tests.rs`、`protocol/src/openai_models.rs` 测试模块
   - 每次在 prod 结构加字段，应同时更新对应测试的字面量

## 验证方式
- `cargo check -p codex-model-provider-info -p codex-models-manager -p codex-protocol -p codex-config`
- `cargo test -p codex-model-provider-info -p codex-protocol -p codex-models-manager -p codex-config`
- `cargo fmt`

## 可复用模式
- 添加 provider 级别的模型元数据字段：沿 `ModelProviderInfo → ModelsManagerConfig → ModelInfo → ModelPreset` 链路遍历。
- 测试补位：grep 所有包含目标结构体字面量的测试文件，用脚本批量添加 `field: false,` 或 `field: None,`。
