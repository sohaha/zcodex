use super::*;
use crate::common::ResponseEvent;
use crate::common::ResponsesApiRequest;
use crate::common::TextControls;
use crate::common::TextFormat;
use crate::common::TextFormatType;
use crate::error::ApiError;
use codex_client::StreamResponse;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ReasoningItemContent;
use codex_protocol::models::ReasoningItemReasoningSummary;
use codex_protocol::models::ResponseItem;
use codex_protocol::models::SearchToolCallParams;
use codex_protocol::models::WebSearchAction;
use codex_protocol::protocol::TokenUsage;
use futures::stream;
use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;
use std::collections::HashSet;
use std::time::Duration;

fn request_with_tools(tools: Vec<Value>, input: Vec<ResponseItem>) -> ResponsesApiRequest {
    ResponsesApiRequest {
        model: "gpt-test".to_string(),
        instructions: "system rules".to_string(),
        input,
        tools,
        tool_choice: "auto".to_string(),
        parallel_tool_calls: true,
        reasoning: None,
        store: false,
        stream: true,
        include: Vec::new(),
        service_tier: Some("priority".to_string()),
        prompt_cache_key: None,
        text: None,
    }
}

#[test]
fn build_request_maps_function_custom_and_special_tools() {
    let request = request_with_tools(
        vec![
            json!({
                "type": "function",
                "name": "read_file",
                "description": "Read a file",
                "parameters": { "type": "object", "properties": {} },
                "strict": true,
            }),
            json!({
                "type": "custom",
                "name": "apply_patch",
                "description": "Apply patch",
                "format": { "definition": "Unified diff" },
            }),
            json!({
                "type": "tool_search",
                "description": "Find tools",
                "parameters": {
                    "type": "object",
                    "properties": { "query": { "type": "string" } },
                    "required": ["query"],
                }
            }),
            json!({ "type": "local_shell" }),
        ],
        vec![
            ResponseItem::Message {
                id: None,
                role: "user".to_string(),
                content: vec![ContentItem::InputText {
                    text: "hello".to_string(),
                }],
                end_turn: None,
                phase: None,
            },
            ResponseItem::CustomToolCall {
                id: None,
                status: None,
                call_id: "call-custom".to_string(),
                name: "apply_patch".to_string(),
                input: "*** Begin Patch".to_string(),
            },
        ],
    );

    let chat = build_request_with_stream(&request, /*stream*/ false).expect("build request");
    let body = chat.body.as_object().expect("body object");
    assert_eq!(
        body.get("model"),
        Some(&Value::String("gpt-test".to_string()))
    );
    assert_eq!(
        body.get("service_tier"),
        Some(&Value::String("priority".to_string()))
    );
    assert_eq!(
        chat.custom_tool_names,
        HashSet::from(["apply_patch".to_string()])
    );
    assert_eq!(
        chat.tool_search_tool_names,
        HashSet::from(["tool_search".to_string()])
    );
    assert_eq!(
        chat.local_shell_tool_names,
        HashSet::from(["local_shell".to_string()])
    );

    let messages = body
        .get("messages")
        .and_then(Value::as_array)
        .expect("messages array");
    assert_eq!(messages.len(), 3);
    assert_eq!(messages[0]["role"], "system");
    assert_eq!(messages[1]["role"], "user");
    assert_eq!(
        messages[2]["tool_calls"][0]["function"]["name"],
        "apply_patch"
    );
    assert_eq!(
        body["tools"][1]["function"]["parameters"]["required"],
        json!(["input"])
    );
}

#[test]
fn build_request_folds_system_and_developer_history() {
    let request = ResponsesApiRequest {
        model: "gpt-test".to_string(),
        instructions: "base system".to_string(),
        input: vec![
            ResponseItem::Message {
                id: None,
                role: "developer".to_string(),
                content: vec![ContentItem::InputText {
                    text: "dev note".to_string(),
                }],
                end_turn: None,
                phase: None,
            },
            ResponseItem::Message {
                id: None,
                role: "system".to_string(),
                content: vec![ContentItem::InputText {
                    text: "legacy system".to_string(),
                }],
                end_turn: None,
                phase: None,
            },
            ResponseItem::Message {
                id: None,
                role: "assistant".to_string(),
                content: vec![ContentItem::OutputText {
                    text: "prior answer".to_string(),
                }],
                end_turn: None,
                phase: None,
            },
            ResponseItem::Message {
                id: None,
                role: "user".to_string(),
                content: vec![ContentItem::InputText {
                    text: "hello".to_string(),
                }],
                end_turn: None,
                phase: None,
            },
        ],
        tools: Vec::new(),
        tool_choice: "auto".to_string(),
        parallel_tool_calls: false,
        reasoning: None,
        store: false,
        stream: false,
        include: Vec::new(),
        service_tier: None,
        prompt_cache_key: None,
        text: None,
    };

    let chat = build_request_with_stream(&request, /*stream*/ false).expect("build request");
    let messages = chat.body["messages"].as_array().expect("messages array");
    assert_eq!(messages.len(), 3);
    assert_eq!(messages[0]["role"], "system");
    assert_eq!(
        messages[0]["content"],
        Value::String("base system\n\ndev note\n\nlegacy system".to_string())
    );
    assert!(
        messages
            .iter()
            .all(|message| message["role"] != "developer")
    );
    assert_eq!(messages[1]["role"], "assistant");
    assert_eq!(messages[2]["role"], "user");
}

#[test]
fn build_request_keeps_mixed_history_items_without_error() {
    let request = request_with_tools(
        Vec::new(),
        vec![
            ResponseItem::Reasoning {
                id: "rs_1".to_string(),
                summary: vec![ReasoningItemReasoningSummary::SummaryText {
                    text: "thinking".to_string(),
                }],
                content: Some(vec![ReasoningItemContent::ReasoningText {
                    text: "detail".to_string(),
                }]),
                encrypted_content: None,
            },
            ResponseItem::WebSearchCall {
                id: None,
                status: Some("completed".to_string()),
                action: Some(WebSearchAction::Search {
                    query: Some("weather".to_string()),
                    queries: None,
                }),
            },
            ResponseItem::ImageGenerationCall {
                id: "ig_1".to_string(),
                status: "completed".to_string(),
                revised_prompt: Some("lobster".to_string()),
                result: "Zm9v".to_string(),
            },
            ResponseItem::Compaction {
                encrypted_content: "secret".to_string(),
            },
            ResponseItem::Other,
        ],
    );

    let chat = build_request_with_stream(&request, /*stream*/ false).expect("build request");
    let messages = chat.body["messages"].as_array().expect("messages array");
    let contents = messages
        .iter()
        .skip(1)
        .map(|message| message["content"].as_str().unwrap_or_default().to_string())
        .collect::<Vec<_>>();

    assert_eq!(
        contents,
        vec![
            "[reasoning summary]\nthinking\n\n[reasoning content]\ndetail".to_string(),
            "[web_search] status=completed\nweather".to_string(),
            "[image_generation] status=completed\nrevised_prompt: lobster\nresult: omitted_binary_image_payload".to_string(),
        ]
    );
}

#[tokio::test]
async fn chat_stream_emits_text_and_usage() {
    let chunks = vec![
        Ok(bytes::Bytes::from(
            "data: {\"id\":\"chatcmpl-1\",\"model\":\"gpt-test\",\"choices\":[{\"delta\":{\"content\":\"Hel\"}}]}\n\n",
        )),
        Ok(bytes::Bytes::from(
            "data: {\"id\":\"chatcmpl-1\",\"model\":\"gpt-test\",\"choices\":[{\"delta\":{\"content\":\"lo\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":11,\"prompt_tokens_details\":{\"cached_tokens\":2},\"completion_tokens\":5,\"completion_tokens_details\":{\"reasoning_tokens\":1},\"total_tokens\":16}}\n\n",
        )),
        Ok(bytes::Bytes::from("data: [DONE]\n\n")),
    ];
    let stream_response = StreamResponse {
        status: http::StatusCode::OK,
        headers: http::HeaderMap::new(),
        bytes: Box::pin(stream::iter(chunks)),
    };

    let mut stream = spawn_response_stream(
        stream_response,
        Duration::from_secs(1),
        None,
        None,
        HashSet::new(),
        HashSet::new(),
        HashSet::new(),
    );
    let mut events = Vec::new();
    while let Some(event) = stream.rx_event.recv().await {
        let event = event.expect("event ok");
        let done = matches!(event, ResponseEvent::Completed { .. });
        events.push(event);
        if done {
            break;
        }
    }

    assert!(matches!(events[0], ResponseEvent::Created));
    assert!(matches!(
        &events[1],
        ResponseEvent::ServerModel(model) if model == "gpt-test"
    ));
    assert!(matches!(
        &events[2],
        ResponseEvent::OutputItemAdded(ResponseItem::Message { role, content, .. })
            if role == "assistant" && content.is_empty()
    ));
    assert!(matches!(
        &events[3],
        ResponseEvent::OutputTextDelta(text) if text == "Hel"
    ));
    assert!(matches!(
        &events[4],
        ResponseEvent::OutputTextDelta(text) if text == "lo"
    ));
    assert!(matches!(
        &events[5],
        ResponseEvent::OutputItemDone(ResponseItem::Message {
            role,
            content,
            ..
        }) if role == "assistant"
            && content == &vec![ContentItem::OutputText {
                text: "Hello".to_string(),
            }]
    ));
    assert!(matches!(
        &events[6],
        ResponseEvent::Completed {
            response_id,
            token_usage: Some(TokenUsage {
                input_tokens: 11,
                cached_input_tokens: 2,
                output_tokens: 5,
                reasoning_output_tokens: 1,
                total_tokens: 16,
            }),
        } if response_id == "chatcmpl-1"
    ));
}

#[tokio::test]
async fn chat_stream_maps_custom_tool_calls() {
    let chunks = vec![
        Ok(bytes::Bytes::from(concat!(
            "data: {\"id\":\"chatcmpl-2\",\"model\":\"gpt-test\",\"choices\":[",
            "{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call-1\",\"function\":{\"name\":\"apply_patch\",\"arguments\":\"{\\\"input\\\":\\\"*** Begin\"}}]}}",
            "]}\n\n",
        ))),
        Ok(bytes::Bytes::from(concat!(
            "data: {\"id\":\"chatcmpl-2\",\"model\":\"gpt-test\",\"choices\":[",
            "{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\" Patch\\\"}\"}}],\"content\":\"\"},\"finish_reason\":\"tool_calls\"}",
            "]}\n\n",
        ))),
        Ok(bytes::Bytes::from("data: [DONE]\n\n")),
    ];
    let stream_response = StreamResponse {
        status: http::StatusCode::OK,
        headers: http::HeaderMap::new(),
        bytes: Box::pin(stream::iter(chunks)),
    };

    let mut stream = spawn_response_stream(
        stream_response,
        Duration::from_secs(1),
        None,
        None,
        HashSet::from(["apply_patch".to_string()]),
        HashSet::new(),
        HashSet::new(),
    );
    let mut events = Vec::new();
    while let Some(event) = stream.rx_event.recv().await {
        let event = event.expect("event ok");
        let done = matches!(event, ResponseEvent::Completed { .. });
        events.push(event);
        if done {
            break;
        }
    }

    assert!(events.iter().any(|event| matches!(
        event,
        ResponseEvent::OutputItemDone(ResponseItem::CustomToolCall {
            call_id,
            name,
            input,
            ..
        }) if call_id == "call-1" && name == "apply_patch" && input == "*** Begin Patch"
    )));
}

#[tokio::test]
async fn chat_stream_reports_decode_errors() {
    let stream_response = StreamResponse {
        status: http::StatusCode::OK,
        headers: http::HeaderMap::new(),
        bytes: Box::pin(stream::iter(vec![Ok(bytes::Bytes::from(
            "data: {not-json}\n\n",
        ))])),
    };

    let mut stream = spawn_response_stream(
        stream_response,
        Duration::from_secs(1),
        None,
        None,
        HashSet::new(),
        HashSet::new(),
        HashSet::new(),
    );
    let first = stream.rx_event.recv().await.expect("event");
    assert!(matches!(first, Err(ApiError::Stream(_))));
}

#[test]
fn parse_tool_search_arguments_accepts_wrapped_shape() {
    let params = parse_tool_search_arguments(
        &json!({
            "execution": "client",
            "arguments": { "query": "find", "limit": 2 }
        })
        .to_string(),
    )
    .expect("parse arguments");

    assert_eq!(
        params,
        SearchToolCallParams {
            query: "find".to_string(),
            limit: Some(2),
        }
    );
}

#[test]
fn build_request_maps_reasoning_response_format_and_tool_controls() {
    let request = ResponsesApiRequest {
        model: "gpt-test".to_string(),
        instructions: "system".to_string(),
        input: vec![ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: "hello".to_string(),
            }],
            end_turn: None,
            phase: None,
        }],
        tools: vec![json!({
            "type": "function",
            "name": "read_file",
            "description": "Read a file",
            "parameters": {
                "type": "object",
                "properties": {
                    "file_path": { "type": "string" }
                },
                "required": ["file_path"]
            },
            "strict": true,
        })],
        tool_choice: "required".to_string(),
        parallel_tool_calls: false,
        reasoning: Some(crate::common::Reasoning {
            effort: Some(codex_protocol::openai_models::ReasoningEffort::High),
            summary: None,
        }),
        store: false,
        stream: true,
        include: Vec::new(),
        service_tier: Some("priority".to_string()),
        prompt_cache_key: None,
        text: Some(TextControls {
            verbosity: None,
            format: Some(TextFormat {
                r#type: TextFormatType::JsonSchema,
                strict: true,
                schema: json!({
                    "type": "object",
                    "properties": {
                        "answer": { "type": "string" }
                    },
                    "required": ["answer"]
                }),
                name: "answer_format".to_string(),
            }),
        }),
    };

    let chat = build_request(&request).expect("build request");
    let body = chat.body.as_object().expect("body object");
    assert_eq!(
        body.get("reasoning_effort"),
        Some(&Value::String("high".to_string()))
    );
    assert_eq!(
        body.get("service_tier"),
        Some(&Value::String("priority".to_string()))
    );
    assert_eq!(
        body.get("tool_choice"),
        Some(&Value::String("required".to_string()))
    );
    assert_eq!(body.get("parallel_tool_calls"), Some(&Value::Bool(false)));
    assert_eq!(
        body["response_format"]["json_schema"]["name"],
        Value::String("answer_format".to_string())
    );
    assert_eq!(
        body["response_format"]["json_schema"]["strict"],
        Value::Bool(true)
    );
    assert_eq!(body["stream_options"]["include_usage"], Value::Bool(true));
}
