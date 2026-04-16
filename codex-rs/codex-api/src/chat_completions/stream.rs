use super::request::parse_custom_tool_input;
use super::request::parse_local_shell_arguments;
use super::request::parse_tool_search_arguments;
use crate::common::ResponseEvent;
use crate::common::ResponseStream;
use crate::error::ApiError;
use crate::telemetry::SseTelemetry;
use codex_client::ByteStream;
use codex_client::StreamResponse;
use codex_protocol::models::ContentItem;
use codex_protocol::models::LocalShellAction;
use codex_protocol::models::LocalShellExecAction;
use codex_protocol::models::LocalShellStatus;
use codex_protocol::models::ReasoningItemContent;
use codex_protocol::models::ReasoningItemReasoningSummary;
use codex_protocol::models::ResponseItem;
use codex_protocol::protocol::TokenUsage;
use eventsource_stream::Eventsource;
use futures::StreamExt;
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::Instant;
use tokio::time::timeout;
use tracing::debug;

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
                // Check for SSE error events before attempting to parse as ChatCompletionChunk
                if let Some(error_response) = parse_sse_error(&event.data) {
                    let _ = tx_event.send(Err(error_response)).await;
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
    in_inline_think: bool,
    think_tag_buf: String,
    reasoning_text: String,
    reasoning_item_started: bool,
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
            in_inline_think: false,
            think_tag_buf: String::new(),
            reasoning_text: String::new(),
            reasoning_item_started: false,
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
                    self.handle_text_delta(text, tx_event).await?;
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

    async fn handle_reasoning_delta(
        &mut self,
        chunk: String,
        tx_event: &mpsc::Sender<Result<ResponseEvent, ApiError>>,
    ) -> Result<(), ApiError> {
        if !self.reasoning_item_started {
            self.reasoning_item_started = true;
            send_event(
                tx_event,
                ResponseEvent::OutputItemAdded(ResponseItem::Reasoning {
                    id: self.response_id.clone().unwrap_or_default(),
                    summary: Vec::new(),
                    content: None,
                    encrypted_content: None,
                }),
            )
            .await?;
            send_event(
                tx_event,
                ResponseEvent::ReasoningSummaryPartAdded { summary_index: 0 },
            )
            .await?;
        }
        self.reasoning_text.push_str(&chunk);
        send_event(
            tx_event,
            ResponseEvent::ReasoningSummaryDelta {
                delta: chunk,
                summary_index: 0,
            },
        )
        .await
    }

    async fn finish_reasoning_if_needed(
        &mut self,
        tx_event: &mpsc::Sender<Result<ResponseEvent, ApiError>>,
    ) -> Result<(), ApiError> {
        if !self.reasoning_item_started {
            return Ok(());
        }
        let text = std::mem::take(&mut self.reasoning_text);
        let summary = if text.is_empty() {
            Vec::new()
        } else {
            vec![ReasoningItemReasoningSummary::SummaryText { text: text.clone() }]
        };
        let content = if text.is_empty() {
            None
        } else {
            Some(vec![ReasoningItemContent::ReasoningText { text }])
        };
        send_event(
            tx_event,
            ResponseEvent::OutputItemDone(ResponseItem::Reasoning {
                id: self.response_id.clone().unwrap_or_default(),
                summary,
                content,
                encrypted_content: None,
            }),
        )
        .await
    }

    async fn handle_text_delta(
        &mut self,
        text: String,
        tx_event: &mpsc::Sender<Result<ResponseEvent, ApiError>>,
    ) -> Result<(), ApiError> {
        let combined = if self.think_tag_buf.is_empty() {
            text
        } else {
            let mut s = std::mem::take(&mut self.think_tag_buf);
            s.push_str(&text);
            s
        };
        let mut remaining = combined.as_str();
        loop {
            if remaining.is_empty() {
                break;
            }
            if self.in_inline_think {
                if let Some(end) = remaining.find("</think>") {
                    let think_chunk = &remaining[..end];
                    if !think_chunk.is_empty() {
                        self.handle_reasoning_delta(think_chunk.to_string(), tx_event)
                            .await?;
                    }
                    self.in_inline_think = false;
                    remaining = &remaining[end + "</think>".len()..];
                } else {
                    self.handle_reasoning_delta(remaining.to_string(), tx_event)
                        .await?;
                    break;
                }
            } else if let Some(start) = remaining.find("<think>") {
                let before = &remaining[..start];
                if !before.is_empty() {
                    self.ensure_message_started(tx_event).await?;
                    self.output_text.push_str(before);
                    send_event(tx_event, ResponseEvent::OutputTextDelta(before.to_string()))
                        .await?;
                }
                self.in_inline_think = true;
                remaining = &remaining[start + "<think>".len()..];
            } else {
                let tag_prefix = longest_tag_prefix(remaining);
                let emit = &remaining[..remaining.len() - tag_prefix.len()];
                if !emit.is_empty() {
                    self.ensure_message_started(tx_event).await?;
                    self.output_text.push_str(emit);
                    send_event(tx_event, ResponseEvent::OutputTextDelta(emit.to_string())).await?;
                }
                if !tag_prefix.is_empty() {
                    self.think_tag_buf.push_str(tag_prefix);
                }
                break;
            }
        }
        Ok(())
    }

    async fn flush_output_items(
        &mut self,
        tx_event: &mpsc::Sender<Result<ResponseEvent, ApiError>>,
    ) -> Result<(), ApiError> {
        self.finish_reasoning_if_needed(tx_event).await?;
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
    #[serde(default)]
    id: String,
    #[serde(default)]
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

fn longest_tag_prefix(s: &str) -> &str {
    const TAGS: &[&str] = &["<think>", "</think>"];
    let bytes = s.as_bytes();
    for tag in TAGS {
        let tag_bytes = tag.as_bytes();
        for len in (1..tag_bytes.len()).rev() {
            if bytes.len() >= len && &bytes[bytes.len() - len..] == &tag_bytes[..len] {
                return &s[s.len() - len..];
            }
        }
    }
    ""
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

/// Parses SSE error events and returns an ApiError if the data contains an error object.
/// Returns None if the data is not an error event.
fn parse_sse_error(data: &str) -> Option<ApiError> {
    let value: Value = serde_json::from_str(data).ok()?;
    let error_obj = value.get("error")?;
    let error_map = error_obj.as_object()?;

    let code = error_map
        .get("code")
        .and_then(|v| v.as_str())
        .map(String::from);
    let message = error_map
        .get("message")
        .and_then(|v| v.as_str())
        .map(String::from);

    // Check for server overloaded errors
    if let Some(code_str) = code.as_deref() {
        if matches!(code_str, "server_is_overloaded" | "slow_down" | "1305") {
            return Some(ApiError::ServerOverloaded);
        }
    }

    // For other errors, return a generic stream error
    let msg = message.unwrap_or_else(|| "SSE error event received".to_string());
    Some(ApiError::Stream(msg))
}
