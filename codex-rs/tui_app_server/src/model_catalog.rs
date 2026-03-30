use codex_core::models_manager::collaboration_mode_presets::CollaborationModesConfig;
use codex_protocol::config_types::CollaborationModeMask;
use codex_protocol::config_types::ModeKind;
use codex_protocol::config_types::TUI_VISIBLE_COLLABORATION_MODES;
use codex_protocol::openai_models::ModelPreset;
use codex_protocol::openai_models::ReasoningEffort;
use std::convert::Infallible;

const COLLABORATION_MODE_PLAN: &str =
    include_str!("../../core/templates/collaboration_mode/plan.md");
const COLLABORATION_MODE_DEFAULT: &str =
    include_str!("../../core/templates/collaboration_mode/default.md");
const KNOWN_MODE_NAMES_PLACEHOLDER: &str = "{{KNOWN_MODE_NAMES}}";
const REQUEST_USER_INPUT_AVAILABILITY_PLACEHOLDER: &str = "{{REQUEST_USER_INPUT_AVAILABILITY}}";
const ASKING_QUESTIONS_GUIDANCE_PLACEHOLDER: &str = "{{ASKING_QUESTIONS_GUIDANCE}}";

#[derive(Debug, Clone)]
pub(crate) struct ModelCatalog {
    models: Vec<ModelPreset>,
    collaboration_modes_config: CollaborationModesConfig,
}

impl ModelCatalog {
    pub(crate) fn new(
        models: Vec<ModelPreset>,
        collaboration_modes_config: CollaborationModesConfig,
    ) -> Self {
        Self {
            models,
            collaboration_modes_config,
        }
    }

    pub(crate) fn try_list_models(&self) -> Result<Vec<ModelPreset>, Infallible> {
        Ok(self.models.clone())
    }

    pub(crate) fn list_collaboration_modes(&self) -> Vec<CollaborationModeMask> {
        builtin_collaboration_mode_presets(self.collaboration_modes_config)
    }
}

fn builtin_collaboration_mode_presets(
    collaboration_modes_config: CollaborationModesConfig,
) -> Vec<CollaborationModeMask> {
    vec![plan_preset(), default_preset(collaboration_modes_config)]
}

fn plan_preset() -> CollaborationModeMask {
    CollaborationModeMask {
        name: ModeKind::Plan.display_name().to_string(),
        mode: Some(ModeKind::Plan),
        model: None,
        reasoning_effort: Some(Some(ReasoningEffort::Medium)),
        developer_instructions: Some(Some(COLLABORATION_MODE_PLAN.to_string())),
    }
}

fn default_preset(collaboration_modes_config: CollaborationModesConfig) -> CollaborationModeMask {
    CollaborationModeMask {
        name: ModeKind::Default.display_name().to_string(),
        mode: Some(ModeKind::Default),
        model: None,
        reasoning_effort: None,
        developer_instructions: Some(Some(default_mode_instructions(collaboration_modes_config))),
    }
}

fn default_mode_instructions(collaboration_modes_config: CollaborationModesConfig) -> String {
    let known_mode_names = format_mode_names(&TUI_VISIBLE_COLLABORATION_MODES);
    let request_user_input_availability = request_user_input_availability_message(
        ModeKind::Default,
        collaboration_modes_config.default_mode_request_user_input,
    );
    let asking_questions_guidance = asking_questions_guidance_message(
        collaboration_modes_config.default_mode_request_user_input,
    );
    COLLABORATION_MODE_DEFAULT
        .replace(KNOWN_MODE_NAMES_PLACEHOLDER, &known_mode_names)
        .replace(
            REQUEST_USER_INPUT_AVAILABILITY_PLACEHOLDER,
            &request_user_input_availability,
        )
        .replace(
            ASKING_QUESTIONS_GUIDANCE_PLACEHOLDER,
            &asking_questions_guidance,
        )
}

fn format_mode_names(modes: &[ModeKind]) -> String {
    let mode_names: Vec<&str> = modes.iter().map(|mode| mode.display_name()).collect();
    match mode_names.as_slice() {
        [] => "无".to_string(),
        [mode_name] => (*mode_name).to_string(),
        [first, second] => format!("{first} 和 {second}"),
        [..] => mode_names.join("、"),
    }
}

fn request_user_input_availability_message(
    mode: ModeKind,
    default_mode_request_user_input: bool,
) -> String {
    let mode_name = mode.display_name();
    if mode.allows_request_user_input()
        || (default_mode_request_user_input && mode == ModeKind::Default)
    {
        format!("`request_user_input` 工具可在 {mode_name} 模式下使用。")
    } else {
        format!(
            "`request_user_input` 工具在 {mode_name} 模式下不可用；如果你在 {mode_name} 模式下调用它，将会返回错误。"
        )
    }
}

fn asking_questions_guidance_message(default_mode_request_user_input: bool) -> String {
    if default_mode_request_user_input {
        "在默认模式下，应优先基于合理假设直接执行用户请求，而不是停下来提问。只有在确实无法从本地上下文得出答案，且贸然假设风险较高时，才考虑提问；此时优先使用 `request_user_input` 工具，而不是以普通文本助手消息的形式写出多项选择题。不要以普通文本助手消息的形式写多项选择题。".to_string()
    } else {
        "在默认模式下，应优先基于合理假设直接执行用户请求，而不是停下来提问。只有在确实无法从本地上下文得出答案，且贸然假设风险较高时，才用简洁的纯文本问题直接询问用户。不要以普通文本助手消息的形式写多项选择题。".to_string()
    }
}
