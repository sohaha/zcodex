use crate::JsonSchema;
use crate::ResponsesApiTool;
use crate::ToolSpec;
use codex_protocol::config_types::ModeKind;
use codex_protocol::config_types::TUI_VISIBLE_COLLABORATION_MODES;
use codex_protocol::request_user_input::RequestUserInputArgs;
use std::collections::BTreeMap;

pub const REQUEST_USER_INPUT_TOOL_NAME: &str = "request_user_input";

pub fn create_request_user_input_tool(description: String) -> ToolSpec {
    let option_props = BTreeMap::from([
        (
            "label".to_string(),
            JsonSchema::String {
                description: Some("展示给用户的标签（1 到 5 个词）。".to_string()),
            },
        ),
        (
            "description".to_string(),
            JsonSchema::String {
                description: Some(
                    "一句话说明选中该项时的影响或取舍。".to_string(),
                ),
            },
        ),
    ]);

    let options_schema = JsonSchema::Array {
        description: Some(
            "提供 2 到 3 个互斥选项。把推荐选项放在最前面，并在标签后加上“（推荐）”。不要在这里加入“其他”选项；客户端会自动补一个可自由填写的“其他”。"
                .to_string(),
        ),
        items: Box::new(JsonSchema::Object {
            properties: option_props,
            required: Some(vec!["label".to_string(), "description".to_string()]),
            additional_properties: Some(false.into()),
        }),
    };

    let question_props = BTreeMap::from([
        (
            "id".to_string(),
            JsonSchema::String {
                description: Some(
                    "用于映射答案的稳定标识（snake_case）。".to_string(),
                ),
            },
        ),
        (
            "header".to_string(),
            JsonSchema::String {
                description: Some(
                    "显示在界面里的简短标题（不超过 12 个字符）。".to_string(),
                ),
            },
        ),
        (
            "question".to_string(),
            JsonSchema::String {
                description: Some("展示给用户的单句提问。".to_string()),
            },
        ),
        ("options".to_string(), options_schema),
    ]);

    let questions_schema = JsonSchema::Array {
        description: Some("展示给用户的问题列表。建议 1 个，最多不超过 3 个。".to_string()),
        items: Box::new(JsonSchema::Object {
            properties: question_props,
            required: Some(vec![
                "id".to_string(),
                "header".to_string(),
                "question".to_string(),
                "options".to_string(),
            ]),
            additional_properties: Some(false.into()),
        }),
    };

    let properties = BTreeMap::from([("questions".to_string(), questions_schema)]);

    ToolSpec::Function(ResponsesApiTool {
        name: REQUEST_USER_INPUT_TOOL_NAME.to_string(),
        description,
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["questions".to_string()]),
            additional_properties: Some(false.into()),
        },
        output_schema: None,
    })
}

pub fn request_user_input_unavailable_message(
    mode: ModeKind,
    default_mode_request_user_input: bool,
) -> Option<String> {
    if request_user_input_is_available(mode, default_mode_request_user_input) {
        None
    } else {
        let mode_name = mode.display_name();
        Some(format!("request_user_input 在 {mode_name} 模式不可用"))
    }
}

pub fn normalize_request_user_input_args(
    mut args: RequestUserInputArgs,
) -> Result<RequestUserInputArgs, String> {
    let missing_options = args
        .questions
        .iter()
        .any(|question| question.options.as_ref().is_none_or(Vec::is_empty));
    if missing_options {
        return Err("request_user_input 要求每个问题都提供非空选项".to_string());
    }

    for question in &mut args.questions {
        question.is_other = true;
    }

    Ok(args)
}

pub fn request_user_input_tool_description(default_mode_request_user_input: bool) -> String {
    let allowed_modes = format_allowed_modes(default_mode_request_user_input);
    format!(
        "向用户发起 1 到 3 个简短问题并等待回复。这个工具仅在 {allowed_modes} 模式下可用。"
    )
}

fn request_user_input_is_available(mode: ModeKind, default_mode_request_user_input: bool) -> bool {
    mode.allows_request_user_input()
        || (default_mode_request_user_input && mode == ModeKind::Default)
}

fn format_allowed_modes(default_mode_request_user_input: bool) -> String {
    let mode_names: Vec<&str> = TUI_VISIBLE_COLLABORATION_MODES
        .into_iter()
        .filter(|mode| request_user_input_is_available(*mode, default_mode_request_user_input))
        .map(ModeKind::display_name)
        .collect();

    match mode_names.as_slice() {
        [] => "no modes".to_string(),
        [mode] => format!("{mode} mode"),
        [first, second] => format!("{first} or {second} mode"),
        [..] => format!("modes: {}", mode_names.join(",")),
    }
}

#[cfg(test)]
#[path = "request_user_input_tool_tests.rs"]
mod tests;
