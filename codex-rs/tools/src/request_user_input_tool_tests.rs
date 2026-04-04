use super::*;
use codex_protocol::config_types::ModeKind;
use pretty_assertions::assert_eq;
use std::collections::BTreeMap;

#[test]
fn request_user_input_tool_includes_questions_schema() {
    assert_eq!(
        create_request_user_input_tool("Ask the user to choose.".to_string()),
        ToolSpec::Function(ResponsesApiTool {
            name: "request_user_input".to_string(),
            description: "Ask the user to choose.".to_string(),
            strict: false,
            defer_loading: None,
            parameters: JsonSchema::Object {
                properties: BTreeMap::from([(
                    "questions".to_string(),
                    JsonSchema::Array {
                        description: Some(
                            "展示给用户的问题列表。建议 1 个，最多不超过 3 个。".to_string(),
                        ),
                        items: Box::new(JsonSchema::Object {
                            properties: BTreeMap::from([
                                (
                                    "header".to_string(),
                                    JsonSchema::String {
                                        description: Some(
                                            "显示在界面里的简短标题（不超过 12 个字符）。"
                                                .to_string(),
                                        ),
                                    },
                                ),
                                (
                                    "id".to_string(),
                                    JsonSchema::String {
                                        description: Some(
                                            "用于映射答案的稳定标识（snake_case）。"
                                                .to_string(),
                                        ),
                                    },
                                ),
                                (
                                    "options".to_string(),
                                    JsonSchema::Array {
                                        description: Some(
                                            "提供 2 到 3 个互斥选项。把推荐选项放在最前面，并在标签后加上“（推荐）”。不要在这里加入“其他”选项；客户端会自动补一个可自由填写的“其他”。"
                                                .to_string(),
                                        ),
                                        items: Box::new(JsonSchema::Object {
                                            properties: BTreeMap::from([
                                                (
                                                    "description".to_string(),
                                                    JsonSchema::String {
                                                        description: Some(
                                                            "一句话说明选中该项时的影响或取舍。"
                                                                .to_string(),
                                                        ),
                                                    },
                                                ),
                                                (
                                                    "label".to_string(),
                                                    JsonSchema::String {
                                                        description: Some(
                                                            "展示给用户的标签（1 到 5 个词）。"
                                                                .to_string(),
                                                        ),
                                                    },
                                                ),
                                            ]),
                                            required: Some(vec![
                                                "label".to_string(),
                                                "description".to_string(),
                                            ]),
                                            additional_properties: Some(false.into()),
                                        }),
                                    },
                                ),
                                (
                                    "question".to_string(),
                                    JsonSchema::String {
                                        description: Some(
                                            "展示给用户的单句提问。".to_string(),
                                        ),
                                    },
                                ),
                            ]),
                            required: Some(vec![
                                "id".to_string(),
                                "header".to_string(),
                                "question".to_string(),
                                "options".to_string(),
                            ]),
                            additional_properties: Some(false.into()),
                        }),
                    },
                )]),
                required: Some(vec!["questions".to_string()]),
                additional_properties: Some(false.into()),
            },
            output_schema: None,
        })
    );
}

#[test]
fn request_user_input_unavailable_messages_respect_default_mode_feature_flag() {
    assert_eq!(
        request_user_input_unavailable_message(
            ModeKind::Plan,
            /*default_mode_request_user_input*/ false
        ),
        None
    );
    assert_eq!(
        request_user_input_unavailable_message(
            ModeKind::Default,
            /*default_mode_request_user_input*/ false
        ),
        Some("request_user_input 在 默认 模式不可用".to_string())
    );
    assert_eq!(
        request_user_input_unavailable_message(
            ModeKind::Default,
            /*default_mode_request_user_input*/ true
        ),
        None
    );
    assert_eq!(
        request_user_input_unavailable_message(
            ModeKind::Execute,
            /*default_mode_request_user_input*/ false
        ),
        Some("request_user_input 在 执行 模式不可用".to_string())
    );
    assert_eq!(
        request_user_input_unavailable_message(
            ModeKind::PairProgramming,
            /*default_mode_request_user_input*/ false
        ),
        Some("request_user_input 在 结对编程 模式不可用".to_string())
    );
}

#[test]
fn request_user_input_tool_description_mentions_available_modes() {
    assert_eq!(
        request_user_input_tool_description(/*default_mode_request_user_input*/ false),
        "向用户发起 1 到 3 个简短问题并等待回复。这个工具仅在 计划 模式下可用。".to_string()
    );
    assert_eq!(
        request_user_input_tool_description(/*default_mode_request_user_input*/ true),
        "向用户发起 1 到 3 个简短问题并等待回复。这个工具仅在 默认 或 计划 模式下可用。"
            .to_string()
    );
}
