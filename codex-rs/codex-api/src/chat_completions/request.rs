use crate::common::ResponsesApiRequest;
use crate::common::TextFormat;
use crate::error::ApiError;
use codex_protocol::models::ContentItem;
use codex_protocol::models::FunctionCallOutputPayload;
use codex_protocol::models::LocalShellAction;
use codex_protocol::models::ReasoningItemContent;
use codex_protocol::models::ReasoningItemReasoningSummary;
use codex_protocol::models::ResponseItem;
use codex_protocol::models::SearchToolCallParams;
use codex_protocol::models::ShellToolCallParams;
use codex_protocol::models::WebSearchAction;
use serde_json::Map;
use serde_json::Value;
use serde_json::json;
use std::collections::HashSet;

const CHAT_TOOL_INPUT_FIELD: &str = "input";

pub struct ChatCompletionsRequest {
    pub body: Value,
    pub custom_tool_names: HashSet<String>,
    pub tool_search_tool_names: HashSet<String>,
    pub local_shell_tool_names: HashSet<String>,
}

pub fn build_request(request: &ResponsesApiRequest) -> Result<ChatCompletionsRequest, ApiError> {
    build_request_with_stream(request, /*stream*/ true)
}

pub fn build_request_with_stream(
    request: &ResponsesApiRequest,
    stream: bool,
) -> Result<ChatCompletionsRequest, ApiError> {
    let mut messages = Vec::<Value>::new();
    let mut system_segments = Vec::new();
    push_system_segment(&mut system_segments, &request.instructions);

    for item in &request.input {
        match item {
            ResponseItem::Message { role, content, .. } => match role.as_str() {
                "system" | "developer" => {
                    ensure_no_images(role, content)?;
                    let text = content_text(content);
                    push_system_segment(&mut system_segments, &text);
                }
                "assistant" => {
                    ensure_no_images(role, content)?;
                    let text = content_text(content);
                    if !text.is_empty() {
                        messages.push(chat_text_message("assistant", text));
                    }
                }
                "user" => {
                    let content = user_content(content);
                    if !content_is_empty(&content) {
                        messages.push(json!({
                            "role": "user",
                            "content": content,
                        }));
                    }
                }
                other => {
                    return Err(ApiError::InvalidRequest {
                        message: format!("chat completions does not support message role {other}"),
                    });
                }
            },
            ResponseItem::FunctionCall {
                name,
                call_id,
                arguments,
                ..
            } => messages.push(chat_tool_call_message(
                name.clone(),
                call_id.clone(),
                arguments.clone(),
            )),
            ResponseItem::CustomToolCall {
                name,
                call_id,
                input,
                ..
            } => messages.push(chat_tool_call_message(
                name.clone(),
                call_id.clone(),
                json!({ CHAT_TOOL_INPUT_FIELD: input }).to_string(),
            )),
            ResponseItem::ToolSearchCall {
                call_id,
                execution,
                arguments,
                ..
            } => messages.push(chat_tool_call_message(
                "tool_search".to_string(),
                call_id
                    .clone()
                    .unwrap_or_else(|| "tool_search_call".to_string()),
                json!({
                    "execution": execution,
                    "arguments": arguments,
                })
                .to_string(),
            )),
            ResponseItem::LocalShellCall {
                call_id,
                id,
                action,
                ..
            } => messages.push(chat_tool_call_message(
                "local_shell".to_string(),
                call_id
                    .clone()
                    .or_else(|| id.clone())
                    .unwrap_or_else(|| "local_shell_call".to_string()),
                serde_json::to_string(&local_shell_request_body(action)).map_err(|err| {
                    ApiError::InvalidRequest {
                        message: format!(
                            "failed to encode local_shell call for chat completions: {err}"
                        ),
                    }
                })?,
            )),
            ResponseItem::FunctionCallOutput { call_id, output }
            | ResponseItem::CustomToolCallOutput {
                call_id, output, ..
            } => messages.push(chat_tool_result_message(
                call_id.clone(),
                tool_result_text(output),
            )),
            ResponseItem::ToolSearchOutput {
                call_id,
                status,
                execution,
                tools,
            } => messages.push(chat_tool_result_message(
                call_id
                    .clone()
                    .unwrap_or_else(|| "tool_search_call".to_string()),
                json!({
                    "status": status,
                    "execution": execution,
                    "tools": tools,
                })
                .to_string(),
            )),
            ResponseItem::Reasoning {
                summary, content, ..
            } => {
                let text = reasoning_history_text(summary, content.as_deref());
                if !text.is_empty() {
                    messages.push(chat_text_message("assistant", text));
                }
            }
            ResponseItem::WebSearchCall { status, action, .. } => messages.push(chat_text_message(
                "assistant",
                web_search_history_text(status.as_deref(), action.as_ref()),
            )),
            ResponseItem::ImageGenerationCall {
                status,
                revised_prompt,
                result,
                ..
            } => messages.push(chat_text_message(
                "assistant",
                image_generation_history_text(status, revised_prompt.as_deref(), result),
            )),
            ResponseItem::GhostSnapshot { .. }
            | ResponseItem::Compaction { .. }
            | ResponseItem::Other => {}
        }
    }

    if !system_segments.is_empty() {
        messages.insert(0, chat_text_message("system", system_segments.join("\n\n")));
    }

    if messages.is_empty() {
        messages.push(json!({
            "role": "user",
            "content": "",
        }));
    }

    let mut body = json!({
        "model": request.model,
        "messages": messages,
        "stream": stream,
    });

    let mut custom_tool_names = HashSet::new();
    let mut tool_search_tool_names = HashSet::new();
    let mut local_shell_tool_names = HashSet::new();
    let mut tool_names = HashSet::new();
    let tools = chat_tools(
        &request.tools,
        &mut tool_names,
        &mut custom_tool_names,
        &mut tool_search_tool_names,
        &mut local_shell_tool_names,
    )?;
    let tool_choice = chat_tool_choice(&request.tool_choice, &tool_names)?;

    if let Some(object) = body.as_object_mut() {
        if !tools.is_empty() {
            object.insert("tools".to_string(), Value::Array(tools));
            object.insert("tool_choice".to_string(), tool_choice);
            object.insert(
                "parallel_tool_calls".to_string(),
                Value::Bool(request.parallel_tool_calls),
            );
        }

        if let Some(max_tokens) = request.max_output_tokens {
            object.insert("max_tokens".to_string(), Value::Number(max_tokens.into()));
        }
        if let Some(service_tier) = request.service_tier.as_ref() {
            object.insert(
                "service_tier".to_string(),
                Value::String(service_tier.clone()),
            );
        }
        if let Some(reasoning) = request.reasoning.as_ref().and_then(|r| r.effort.as_ref()) {
            object.insert(
                "reasoning_effort".to_string(),
                serde_json::to_value(reasoning).map_err(|err| ApiError::InvalidRequest {
                    message: format!("failed to encode reasoning_effort: {err}"),
                })?,
            );
        }
        if let Some(text) = request.text.as_ref()
            && let Some(response_format) = response_format(text)?
        {
            object.insert("response_format".to_string(), response_format);
        }
        if stream {
            object.insert(
                "stream_options".to_string(),
                json!({ "include_usage": true }),
            );
        }
    }

    Ok(ChatCompletionsRequest {
        body,
        custom_tool_names,
        tool_search_tool_names,
        local_shell_tool_names,
    })
}

fn push_system_segment(segments: &mut Vec<String>, text: &str) {
    let text = text.trim();
    if !text.is_empty() {
        segments.push(text.to_string());
    }
}

fn ensure_no_images(role: &str, content: &[ContentItem]) -> Result<(), ApiError> {
    if content
        .iter()
        .any(|item| matches!(item, ContentItem::InputImage { .. }))
    {
        return Err(ApiError::InvalidRequest {
            message: format!(
                "chat completions only supports images in user messages; {role} messages must be text-only"
            ),
        });
    }
    Ok(())
}

fn content_text(content: &[ContentItem]) -> String {
    content
        .iter()
        .filter_map(|item| match item {
            ContentItem::InputText { text } | ContentItem::OutputText { text } => Some(text),
            ContentItem::InputImage { .. } => None,
        })
        .cloned()
        .collect::<Vec<_>>()
        .join("")
}

fn user_content(content: &[ContentItem]) -> Value {
    let has_images = content
        .iter()
        .any(|item| matches!(item, ContentItem::InputImage { .. }));
    if !has_images {
        return Value::String(content_text(content));
    }

    Value::Array(
        content
            .iter()
            .map(|item| match item {
                ContentItem::InputText { text } | ContentItem::OutputText { text } => {
                    json!({ "type": "text", "text": text })
                }
                ContentItem::InputImage { image_url } => json!({
                    "type": "image_url",
                    "image_url": { "url": image_url },
                }),
            })
            .collect(),
    )
}

fn content_is_empty(content: &Value) -> bool {
    match content {
        Value::String(text) => text.is_empty(),
        Value::Array(items) => items.is_empty(),
        _ => false,
    }
}

fn local_shell_request_body(action: &LocalShellAction) -> Value {
    match action {
        LocalShellAction::Exec(exec) => json!({
            "command": exec.command,
            "workdir": exec.working_directory,
            "timeout_ms": exec.timeout_ms,
        }),
    }
}

fn tool_result_text(output: &FunctionCallOutputPayload) -> String {
    output
        .body
        .to_text()
        .unwrap_or_else(|| serde_json::to_string(output).unwrap_or_default())
}

fn chat_tool_call_message(name: String, call_id: String, arguments: String) -> Value {
    json!({
        "role": "assistant",
        "content": Value::Null,
        "tool_calls": [{
            "id": call_id,
            "type": "function",
            "function": {
                "name": name,
                "arguments": arguments,
            }
        }],
    })
}

fn chat_tool_result_message(call_id: String, content: String) -> Value {
    json!({
        "role": "tool",
        "tool_call_id": call_id,
        "content": content,
    })
}

fn chat_text_message(role: &str, content: String) -> Value {
    json!({
        "role": role,
        "content": content,
    })
}

fn chat_tool_choice(tool_choice: &str, tool_names: &HashSet<String>) -> Result<Value, ApiError> {
    match tool_choice {
        "auto" | "none" | "required" => Ok(Value::String(tool_choice.to_string())),
        _ => {
            let Some(tool_name) = tool_choice.strip_prefix("required:") else {
                return Err(ApiError::InvalidRequest {
                    message: format!("chat completions does not support tool_choice {tool_choice}"),
                });
            };
            if !tool_names.contains(tool_name) {
                return Err(ApiError::InvalidRequest {
                    message: format!(
                        "chat completions tool_choice requires unknown tool {tool_name}"
                    ),
                });
            }
            Ok(json!({
                "type": "function",
                "function": {
                    "name": tool_name,
                },
            }))
        }
    }
}

fn response_format(text: &crate::common::TextControls) -> Result<Option<Value>, ApiError> {
    let Some(format) = text.format.as_ref() else {
        return Ok(None);
    };

    Ok(Some(chat_response_format(format)?))
}

fn chat_response_format(format: &TextFormat) -> Result<Value, ApiError> {
    let mut schema = Map::new();
    schema.insert("name".to_string(), Value::String(format.name.clone()));
    schema.insert("schema".to_string(), format.schema.clone());
    if format.strict {
        schema.insert("strict".to_string(), Value::Bool(true));
    }
    Ok(json!({
        "type": "json_schema",
        "json_schema": schema,
    }))
}

fn chat_tools(
    tools: &[Value],
    tool_names: &mut HashSet<String>,
    custom_tool_names: &mut HashSet<String>,
    tool_search_tool_names: &mut HashSet<String>,
    local_shell_tool_names: &mut HashSet<String>,
) -> Result<Vec<Value>, ApiError> {
    tools.iter()
        .map(|tool| match tool.get("type").and_then(Value::as_str) {
            Some("function") => {
                let name = required_string(tool, "name")?;
                tool_names.insert(name.clone());
                Ok(json!({
                    "type": "function",
                    "function": {
                        "name": name,
                        "description": required_string(tool, "description")?,
                        "parameters": tool.get("parameters").cloned().unwrap_or_else(|| json!({"type":"object","properties":{}})),
                        "strict": tool.get("strict").and_then(Value::as_bool).unwrap_or(false),
                    }
                }))
            }
            Some("custom") => {
                let name = required_string(tool, "name")?;
                tool_names.insert(name.clone());
                custom_tool_names.insert(name.clone());
                let description = required_string(tool, "description")?;
                let definition = tool
                    .get("format")
                    .and_then(Value::as_object)
                    .and_then(|format| format.get("definition"))
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                Ok(json!({
                    "type": "function",
                    "function": {
                        "name": name,
                        "description": description_with_suffix(&description, definition),
                        "parameters": {
                            "type": "object",
                            "properties": {
                                CHAT_TOOL_INPUT_FIELD: {
                                    "type": "string",
                                    "description": "Freeform tool input."
                                }
                            },
                            "required": [CHAT_TOOL_INPUT_FIELD],
                            "additionalProperties": false,
                        },
                        "strict": true,
                    }
                }))
            }
            Some("tool_search") => {
                let name = "tool_search".to_string();
                tool_names.insert(name.clone());
                tool_search_tool_names.insert(name.clone());
                Ok(json!({
                    "type": "function",
                    "function": {
                        "name": name,
                        "description": required_string(tool, "description")?,
                        "parameters": tool.get("parameters").cloned().unwrap_or_else(|| json!({"type":"object","properties":{}})),
                        "strict": false,
                    }
                }))
            }
            Some("local_shell") => {
                let name = "local_shell".to_string();
                tool_names.insert(name.clone());
                local_shell_tool_names.insert(name.clone());
                Ok(json!({
                    "type": "function",
                    "function": {
                        "name": name,
                        "description": "Execute a shell command on the local machine.",
                        "parameters": {
                            "type": "object",
                            "properties": {
                                "command": {
                                    "type": "array",
                                    "items": { "type": "string" }
                                },
                                "workdir": { "type": "string" },
                                "timeout_ms": { "type": "integer" },
                                "justification": { "type": "string" },
                                "prefix_rule": {
                                    "type": "array",
                                    "items": { "type": "string" }
                                }
                            },
                            "required": ["command"],
                            "additionalProperties": true,
                        },
                        "strict": false,
                    }
                }))
            }
            Some("image_generation") | Some("web_search") => Err(ApiError::InvalidRequest {
                message: format!(
                    "chat completions does not support tool type {}; use wire_api = \"responses\" for hosted tools",
                    tool.get("type").and_then(Value::as_str).unwrap_or("unknown")
                ),
            }),
            Some(other) => Err(ApiError::InvalidRequest {
                message: format!("unsupported chat completions tool type {other}"),
            }),
            None => Err(ApiError::InvalidRequest {
                message: "chat completions tool is missing type".to_string(),
            }),
        })
        .collect()
}

fn required_string(value: &Value, field: &str) -> Result<String, ApiError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| ApiError::InvalidRequest {
            message: format!("chat completions value is missing string field {field}"),
        })
}

fn description_with_suffix(description: &str, suffix: &str) -> String {
    if suffix.trim().is_empty() {
        description.to_string()
    } else {
        format!("{description}\n\n{suffix}")
    }
}

pub(super) fn parse_custom_tool_input(arguments: &str) -> String {
    serde_json::from_str::<Value>(arguments)
        .ok()
        .and_then(|value| {
            value
                .get(CHAT_TOOL_INPUT_FIELD)
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| arguments.to_string())
}

pub(super) fn parse_tool_search_arguments(
    arguments: &str,
) -> Result<SearchToolCallParams, ApiError> {
    let value = serde_json::from_str::<Value>(arguments).map_err(|err| {
        ApiError::Stream(format!(
            "failed to decode tool_search arguments from chat completions: {err}"
        ))
    })?;
    let params = match value {
        Value::Object(mut object) => match object.remove("arguments") {
            Some(arguments) => serde_json::from_value(arguments),
            None => serde_json::from_value(Value::Object(object)),
        },
        value => serde_json::from_value(value),
    }
    .map_err(|err| {
        ApiError::Stream(format!(
            "failed to parse tool_search arguments from chat completions: {err}"
        ))
    })?;
    Ok(params)
}

pub(super) fn parse_local_shell_arguments(
    arguments: &str,
) -> Result<ShellToolCallParams, ApiError> {
    serde_json::from_str(arguments).map_err(|err| {
        ApiError::Stream(format!(
            "failed to parse local_shell arguments from chat completions: {err}"
        ))
    })
}

fn reasoning_history_text(
    summary: &[ReasoningItemReasoningSummary],
    content: Option<&[ReasoningItemContent]>,
) -> String {
    let summary_text = summary
        .iter()
        .map(|item| match item {
            ReasoningItemReasoningSummary::SummaryText { text } => text.as_str(),
        })
        .filter(|text| !text.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    let content_text = content
        .unwrap_or_default()
        .iter()
        .map(|item| match item {
            ReasoningItemContent::ReasoningText { text } | ReasoningItemContent::Text { text } => {
                text.as_str()
            }
        })
        .filter(|text| !text.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n");

    match (summary_text.is_empty(), content_text.is_empty()) {
        (true, true) => String::new(),
        (false, true) => format!("[reasoning]\n{summary_text}"),
        (true, false) => format!("[reasoning]\n{content_text}"),
        (false, false) => {
            format!("[reasoning summary]\n{summary_text}\n\n[reasoning content]\n{content_text}")
        }
    }
}

fn web_search_history_text(status: Option<&str>, action: Option<&WebSearchAction>) -> String {
    let detail = action
        .map(|action| match action {
            WebSearchAction::Search { query, queries } => {
                let queries = queries
                    .as_ref()
                    .map(|queries| queries.join(", "))
                    .unwrap_or_default();
                let query = query.clone().unwrap_or_default();
                if query.is_empty() {
                    queries
                } else if queries.is_empty() {
                    query
                } else {
                    format!("{query}; {queries}")
                }
            }
            WebSearchAction::OpenPage { url } => url.clone().unwrap_or_default(),
            WebSearchAction::FindInPage { url, pattern } => match (url, pattern) {
                (Some(url), Some(pattern)) => format!("{pattern} @ {url}"),
                (Some(url), None) => url.clone(),
                (None, Some(pattern)) => pattern.clone(),
                (None, None) => String::new(),
            },
            WebSearchAction::Other => String::new(),
        })
        .unwrap_or_default();
    let status = status.unwrap_or("unknown");
    if detail.is_empty() {
        format!("[web_search] status={status}")
    } else {
        format!("[web_search] status={status}\n{detail}")
    }
}

fn image_generation_history_text(
    status: &str,
    revised_prompt: Option<&str>,
    result: &str,
) -> String {
    let mut lines = vec![format!("[image_generation] status={status}")];
    if let Some(revised_prompt) = revised_prompt
        && !revised_prompt.trim().is_empty()
    {
        lines.push(format!("revised_prompt: {revised_prompt}"));
    }
    if !result.is_empty() {
        lines.push("result: omitted_binary_image_payload".to_string());
    }
    lines.join("\n")
}
