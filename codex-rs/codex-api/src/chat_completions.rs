use crate::common::ResponseEvent;
use crate::common::ResponseStream;
use crate::common::ResponsesApiRequest;
use crate::common::TextFormat;
use crate::error::ApiError;
use crate::telemetry::SseTelemetry;
use codex_client::ByteStream;
use codex_client::StreamResponse;
use codex_protocol::models::ContentItem;
use codex_protocol::models::FunctionCallOutputPayload;
use codex_protocol::models::LocalShellAction;
use codex_protocol::models::LocalShellExecAction;
use codex_protocol::models::LocalShellStatus;
use codex_protocol::models::ReasoningItemContent;
use codex_protocol::models::ReasoningItemReasoningSummary;
use codex_protocol::models::ResponseItem;
use codex_protocol::models::SearchToolCallParams;
use codex_protocol::models::ShellToolCallParams;
use codex_protocol::models::WebSearchAction;
use codex_protocol::protocol::TokenUsage;
use eventsource_stream::Eventsource;
use futures::StreamExt;
use serde::Deserialize;
use serde_json::Map;
use serde_json::Value;
use serde_json::json;
use std::collections::BTreeMap;
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::Instant;
use tokio::time::timeout;
use tracing::debug;

const CHAT_TOOL_INPUT_FIELD: &str = "input";

pub(crate) struct ChatCompletionsRequest {
    pub(crate) body: Value,
    pub(crate) custom_tool_names: HashSet<String>,
    pub(crate) tool_search_tool_names: HashSet<String>,
    pub(crate) local_shell_tool_names: HashSet<String>,
}

pub(crate) fn build_request(
    request: &ResponsesApiRequest,
) -> Result<ChatCompletionsRequest, ApiError> {
    build_request_with_stream(request, /*stream*/ true)
}

pub(crate) fn build_request_with_stream(
    request: &ResponsesApiRequest,
    stream: bool,
) -> Result<ChatCompletionsRequest, ApiError> {
    let mut messages = Vec::<Value>::new();
    if !request.instructions.trim().is_empty() {
        messages.push(json!({
            "role": "system",
            "content": request.instructions,
        }));
    }

    for item in &request.input {
        match item {
            ResponseItem::Message { role, content, .. } => match role.as_str() {
                "system" | "developer" | "assistant" => {
                    ensure_no_images(role, content)?;
                    let text = content_text(content);
                    if !text.is_empty() {
                        messages.push(json!({
                            "role": role,
                            "content": text,
                        }));
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
    let tools = chat_tools(
        &request.tools,
        &mut custom_tool_names,
        &mut tool_search_tool_names,
        &mut local_shell_tool_names,
    )?;

    if let Some(object) = body.as_object_mut() {
        if !tools.is_empty() {
            object.insert("tools".to_string(), Value::Array(tools));
            object.insert(
                "tool_choice".to_string(),
                chat_tool_choice(&request.tool_choice)?,
            );
            object.insert(
                "parallel_tool_calls".to_string(),
                Value::Bool(request.parallel_tool_calls),
            );
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

pub(crate) fn spawn_response_stream(
    stream_response: StreamResponse,
    idle_timeout: Duration,
    telemetry: Option<Arc<dyn SseTelemetry>>,
    turn_state: Option<Arc<OnceLock<String>>>,
    custom_tool_names: HashSet<String>,
    tool_search_tool_names: HashSet<String>,
    local_shell_tool_names: HashSet<String>,
) -> ResponseStream {
    if let Some(turn_state) = turn_state.as_ref()
        && let Some(header_value) = stream_response
            .headers
            .get("x-codex-turn-state")
            .and_then(|v| v.to_str().ok())
    {
        let _ = turn_state.set(header_value.to_string());
    }

    let (tx_event, rx_event) = mpsc::channel::<Result<ResponseEvent, ApiError>>(1600);
    tokio::spawn(process_sse(
        stream_response.bytes,
        tx_event,
        idle_timeout,
        telemetry,
        ChatStreamState::new(
            custom_tool_names,
            tool_search_tool_names,
            local_shell_tool_names,
        ),
    ));
    ResponseStream { rx_event }
}

async fn process_sse(
    stream: ByteStream,
    tx_event: mpsc::Sender<Result<ResponseEvent, ApiError>>,
    idle_timeout: Duration,
    telemetry: Option<Arc<dyn SseTelemetry>>,
    mut state: ChatStreamState,
) {
    let mut stream = stream.eventsource();
    loop {
        let start = Instant::now();
        let response = timeout(idle_timeout, stream.next()).await;
        if let Some(t) = telemetry.as_ref() {
            t.on_sse_poll(&response, start.elapsed());
        }

        match response {
            Ok(Some(Ok(event))) => {
                if event.data == "[DONE]" {
                    let _ = state.complete(&tx_event).await;
                    return;
                }
                match serde_json::from_str::<ChatCompletionChunk>(&event.data) {
                    Ok(chunk) => {
                        if let Err(err) = state.process_chunk(chunk, &tx_event).await {
                            let _ = tx_event.send(Err(err)).await;
                            return;
                        }
                    }
                    Err(err) => {
                        debug!("failed to parse chat completions SSE chunk: {err}");
                        let _ = tx_event
                            .send(Err(ApiError::Stream(format!(
                                "failed to decode chat completions stream chunk: {err}"
                            ))))
                            .await;
                        return;
                    }
                }
            }
            Ok(Some(Err(err))) => {
                let _ = tx_event
                    .send(Err(ApiError::Stream(format!(
                        "chat completions stream error: {err}"
                    ))))
                    .await;
                return;
            }
            Ok(None) => {
                let _ = state.complete(&tx_event).await;
                return;
            }
            Err(_) => {
                let _ = tx_event
                    .send(Err(ApiError::Stream(format!(
                        "chat completions stream idle for more than {}s",
                        idle_timeout.as_secs()
                    ))))
                    .await;
                return;
            }
        }
    }
}

struct ChatStreamState {
    response_id: Option<String>,
    created_sent: bool,
    server_model_sent: bool,
    output_text: String,
    output_text_done: bool,
    message_item_started: bool,
    tool_calls: BTreeMap<u32, PendingToolCall>,
    completed: bool,
    token_usage: Option<TokenUsage>,
    custom_tool_names: HashSet<String>,
    tool_search_tool_names: HashSet<String>,
    local_shell_tool_names: HashSet<String>,
}

impl ChatStreamState {
    fn new(
        custom_tool_names: HashSet<String>,
        tool_search_tool_names: HashSet<String>,
        local_shell_tool_names: HashSet<String>,
    ) -> Self {
        Self {
            response_id: None,
            created_sent: false,
            server_model_sent: false,
            output_text: String::new(),
            output_text_done: false,
            message_item_started: false,
            tool_calls: BTreeMap::new(),
            completed: false,
            token_usage: None,
            custom_tool_names,
            tool_search_tool_names,
            local_shell_tool_names,
        }
    }

    async fn process_chunk(
        &mut self,
        chunk: ChatCompletionChunk,
        tx_event: &mpsc::Sender<Result<ResponseEvent, ApiError>>,
    ) -> Result<(), ApiError> {
        if !self.created_sent {
            send_event(tx_event, ResponseEvent::Created).await?;
            self.created_sent = true;
        }
        if self.response_id.is_none() {
            self.response_id = Some(chunk.id.clone());
        }
        if !self.server_model_sent {
            send_event(tx_event, ResponseEvent::ServerModel(chunk.model.clone())).await?;
            self.server_model_sent = true;
        }
        if let Some(usage) = chunk.usage {
            self.token_usage = Some(usage.into());
        }

        for choice in chunk.choices {
            if let Some(delta) = choice.delta {
                if let Some(text) = delta_content_text(delta.content.as_ref()) {
                    self.ensure_message_started(tx_event).await?;
                    self.output_text.push_str(&text);
                    send_event(tx_event, ResponseEvent::OutputTextDelta(text)).await?;
                }
                for tool_call in delta.tool_calls {
                    self.merge_tool_call(tool_call);
                }
            }

            if choice.finish_reason.is_some() {
                self.flush_output_items(tx_event).await?;
            }
        }

        Ok(())
    }

    fn merge_tool_call(&mut self, tool_call: ChatToolCallDelta) {
        let Some(index) = tool_call.index else {
            return;
        };

        let entry = self.tool_calls.entry(index).or_default();
        if let Some(id) = tool_call.id {
            entry.id = Some(id);
        }
        if let Some(function) = tool_call.function {
            if let Some(name) = function.name {
                entry.name = Some(name);
            }
            if let Some(arguments) = function.arguments {
                entry.arguments.push_str(&arguments);
            }
        }
    }

    async fn flush_output_items(
        &mut self,
        tx_event: &mpsc::Sender<Result<ResponseEvent, ApiError>>,
    ) -> Result<(), ApiError> {
        if !self.output_text_done && !self.output_text.is_empty() {
            send_event(
                tx_event,
                ResponseEvent::OutputItemDone(ResponseItem::Message {
                    id: None,
                    role: "assistant".to_string(),
                    content: vec![ContentItem::OutputText {
                        text: self.output_text.clone(),
                    }],
                    end_turn: None,
                    phase: None,
                }),
            )
            .await?;
            self.output_text_done = true;
        }

        for tool_call in self.tool_calls.values() {
            let Some(item) = tool_call.to_response_item(
                &self.custom_tool_names,
                &self.tool_search_tool_names,
                &self.local_shell_tool_names,
            )?
            else {
                continue;
            };
            send_event(tx_event, ResponseEvent::OutputItemDone(item)).await?;
        }
        self.tool_calls.clear();
        Ok(())
    }

    async fn ensure_message_started(
        &mut self,
        tx_event: &mpsc::Sender<Result<ResponseEvent, ApiError>>,
    ) -> Result<(), ApiError> {
        if self.message_item_started {
            return Ok(());
        }

        send_event(
            tx_event,
            ResponseEvent::OutputItemAdded(ResponseItem::Message {
                id: None,
                role: "assistant".to_string(),
                content: Vec::new(),
                end_turn: None,
                phase: None,
            }),
        )
        .await?;
        self.message_item_started = true;
        Ok(())
    }

    async fn complete(
        &mut self,
        tx_event: &mpsc::Sender<Result<ResponseEvent, ApiError>>,
    ) -> Result<(), ApiError> {
        if self.completed {
            return Ok(());
        }
        self.flush_output_items(tx_event).await?;
        send_event(
            tx_event,
            ResponseEvent::Completed {
                response_id: self.response_id.clone().unwrap_or_default(),
                token_usage: self.token_usage.clone(),
            },
        )
        .await?;
        self.completed = true;
        Ok(())
    }
}

#[derive(Default)]
struct PendingToolCall {
    id: Option<String>,
    name: Option<String>,
    arguments: String,
}

impl PendingToolCall {
    fn to_response_item(
        &self,
        custom_tool_names: &HashSet<String>,
        tool_search_tool_names: &HashSet<String>,
        local_shell_tool_names: &HashSet<String>,
    ) -> Result<Option<ResponseItem>, ApiError> {
        let Some(call_id) = self.id.clone() else {
            return Ok(None);
        };
        let Some(name) = self.name.clone() else {
            return Ok(None);
        };

        if custom_tool_names.contains(&name) {
            let input = parse_custom_tool_input(&self.arguments);
            return Ok(Some(ResponseItem::CustomToolCall {
                id: None,
                status: None,
                call_id,
                name,
                input,
            }));
        }

        if tool_search_tool_names.contains(&name) {
            let params = parse_tool_search_arguments(&self.arguments)?;
            return Ok(Some(ResponseItem::ToolSearchCall {
                id: None,
                call_id: Some(call_id),
                status: None,
                execution: "client".to_string(),
                arguments: serde_json::to_value(params)
                    .map_err(|err| ApiError::Stream(err.to_string()))?,
            }));
        }

        if local_shell_tool_names.contains(&name) {
            let params = parse_local_shell_arguments(&self.arguments)?;
            return Ok(Some(ResponseItem::LocalShellCall {
                id: None,
                call_id: Some(call_id),
                status: LocalShellStatus::InProgress,
                action: LocalShellAction::Exec(LocalShellExecAction {
                    command: params.command,
                    timeout_ms: params.timeout_ms,
                    working_directory: params.workdir,
                    env: None,
                    user: None,
                }),
            }));
        }

        Ok(Some(ResponseItem::FunctionCall {
            id: None,
            name,
            namespace: None,
            arguments: self.arguments.clone(),
            call_id,
        }))
    }
}

#[derive(Debug, Deserialize)]
struct ChatCompletionChunk {
    id: String,
    model: String,
    #[serde(default)]
    choices: Vec<ChatChoice>,
    #[serde(default)]
    usage: Option<ChatUsage>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    #[serde(default)]
    delta: Option<ChatDelta>,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatDelta {
    #[serde(default)]
    content: Option<Value>,
    #[serde(default)]
    tool_calls: Vec<ChatToolCallDelta>,
}

#[derive(Debug, Deserialize)]
struct ChatToolCallDelta {
    #[serde(default)]
    index: Option<u32>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: Option<ChatToolFunctionDelta>,
}

#[derive(Debug, Deserialize)]
struct ChatToolFunctionDelta {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatUsage {
    prompt_tokens: i64,
    #[serde(default)]
    prompt_tokens_details: Option<ChatPromptTokensDetails>,
    completion_tokens: i64,
    #[serde(default)]
    completion_tokens_details: Option<ChatCompletionTokensDetails>,
    total_tokens: i64,
}

impl From<ChatUsage> for TokenUsage {
    fn from(value: ChatUsage) -> Self {
        Self {
            input_tokens: value.prompt_tokens,
            cached_input_tokens: value
                .prompt_tokens_details
                .map(|details| details.cached_tokens)
                .unwrap_or(0),
            output_tokens: value.completion_tokens,
            reasoning_output_tokens: value
                .completion_tokens_details
                .map(|details| details.reasoning_tokens)
                .unwrap_or(0),
            total_tokens: value.total_tokens,
        }
    }
}

#[derive(Debug, Deserialize)]
struct ChatPromptTokensDetails {
    #[serde(default)]
    cached_tokens: i64,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionTokensDetails {
    #[serde(default)]
    reasoning_tokens: i64,
}

async fn send_event(
    tx_event: &mpsc::Sender<Result<ResponseEvent, ApiError>>,
    event: ResponseEvent,
) -> Result<(), ApiError> {
    tx_event
        .send(Ok(event))
        .await
        .map_err(|err| ApiError::Stream(format!("failed to send chat completions event: {err}")))
}

fn delta_content_text(content: Option<&Value>) -> Option<String> {
    match content? {
        Value::String(text) => Some(text.clone()),
        Value::Array(parts) => {
            let text = parts
                .iter()
                .filter_map(|part| {
                    (part.get("type").and_then(Value::as_str) == Some("text"))
                        .then(|| part.get("text").and_then(Value::as_str))
                        .flatten()
                })
                .collect::<Vec<_>>()
                .join("");
            (!text.is_empty()).then_some(text)
        }
        _ => None,
    }
}

fn ensure_no_images(role: &str, content: &[ContentItem]) -> Result<(), ApiError> {
    if content
        .iter()
        .any(|item| matches!(item, ContentItem::InputImage { .. }))
    {
        return Err(ApiError::InvalidRequest {
            message: format!("chat completions does not support images in {role} messages"),
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

fn chat_tool_choice(tool_choice: &str) -> Result<Value, ApiError> {
    match tool_choice {
        "auto" | "none" | "required" => Ok(Value::String(tool_choice.to_string())),
        unsupported => Err(ApiError::InvalidRequest {
            message: format!("chat completions does not support tool_choice {unsupported}"),
        }),
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
    custom_tool_names: &mut HashSet<String>,
    tool_search_tool_names: &mut HashSet<String>,
    local_shell_tool_names: &mut HashSet<String>,
) -> Result<Vec<Value>, ApiError> {
    tools.iter()
        .map(|tool| match tool.get("type").and_then(Value::as_str) {
            Some("function") => Ok(json!({
                "type": "function",
                "function": {
                    "name": required_string(tool, "name")?,
                    "description": required_string(tool, "description")?,
                    "parameters": tool.get("parameters").cloned().unwrap_or_else(|| json!({"type":"object","properties":{}})),
                    "strict": tool.get("strict").and_then(Value::as_bool).unwrap_or(false),
                }
            })),
            Some("custom") => {
                let name = required_string(tool, "name")?;
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
                    "chat completions does not support tool type {}",
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

fn parse_custom_tool_input(arguments: &str) -> String {
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

fn parse_tool_search_arguments(arguments: &str) -> Result<SearchToolCallParams, ApiError> {
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

fn parse_local_shell_arguments(arguments: &str) -> Result<ShellToolCallParams, ApiError> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream;
    use pretty_assertions::assert_eq;

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
            text: Some(crate::common::TextControls {
                verbosity: None,
                format: Some(TextFormat {
                    r#type: crate::common::TextFormatType::JsonSchema,
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
}
