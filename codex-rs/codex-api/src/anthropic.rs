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
use codex_protocol::models::ReasoningItemContent;
use codex_protocol::models::ReasoningItemReasoningSummary;
use codex_protocol::models::ResponseItem;
use codex_protocol::openai_models::ReasoningEffort as ReasoningEffortConfig;
use codex_protocol::protocol::TokenUsage;
use eventsource_stream::Eventsource;
use futures::StreamExt;
use serde::Deserialize;
use serde_json::Map;
use serde_json::Value;
use serde_json::json;
use std::collections::BTreeMap;
use std::collections::HashSet;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::Instant;
use tokio::time::timeout;
use tracing::debug;
use tracing::trace;

const ANTHROPIC_DEFAULT_MAX_TOKENS: u64 = 8_192;
const ANTHROPIC_LOW_THINKING_BUDGET_TOKENS: u64 = 1_024;
const ANTHROPIC_MEDIUM_THINKING_BUDGET_TOKENS: u64 = 2_048;
const ANTHROPIC_HIGH_THINKING_BUDGET_TOKENS: u64 = 4_096;
const ANTHROPIC_OUTPUT_SCHEMA_INSTRUCTIONS: &str =
    "Respond with JSON only. It must strictly match this schema:";
const TOOL_INPUT_FIELD: &str = "input";
const THINK_START_TAGS: &[&str] = &["<think>"];
const THINK_END_TAGS: &[&str] = &["</think>", "<\\/think>"];
const TOOL_CALL_START_TAGS: &[&str] = &["<tool_call>"];
const TOOL_CALL_END_TAGS: &[&str] = &["</tool_call>", "<\\/tool_call>"];
const INLINE_TEXT_TAGS: &[&str] = &[
    "<think>",
    "</think>",
    "<\\/think>",
    "<tool_call>",
    "</tool_call>",
    "<\\/tool_call>",
];

pub(crate) fn build_request_body(request: &ResponsesApiRequest) -> Value {
    build_request_body_with_stream(request, /*stream*/ true)
}

pub(crate) fn build_request_body_with_stream(request: &ResponsesApiRequest, stream: bool) -> Value {
    let mut messages = Vec::<Value>::new();
    let mut system_segments = vec![request.instructions.clone()];
    if let Some(output_schema) = output_schema(request) {
        system_segments.push(anthropic_output_schema_instruction(output_schema));
    }

    for item in &request.input {
        match item {
            ResponseItem::Message { role, content, .. } => {
                if matches!(role.as_str(), "system" | "developer") {
                    let text = content_text(content);
                    if !text.trim().is_empty() {
                        system_segments.push(text);
                    }
                } else if matches!(role.as_str(), "user" | "assistant") {
                    let blocks = content_blocks(content);
                    if !blocks.is_empty() {
                        push_message_blocks(&mut messages, role, blocks);
                    }
                }
            }
            ResponseItem::FunctionCall {
                name,
                call_id,
                arguments,
                ..
            } => push_message_blocks(
                &mut messages,
                "assistant",
                vec![tool_use_block(
                    name.clone(),
                    call_id.clone(),
                    parse_json_object_or_wrapped(arguments),
                )],
            ),
            ResponseItem::CustomToolCall {
                name,
                call_id,
                input,
                ..
            } => push_message_blocks(
                &mut messages,
                "assistant",
                vec![tool_use_block(
                    name.clone(),
                    call_id.clone(),
                    parse_json_object_or_wrapped(input),
                )],
            ),
            ResponseItem::ToolSearchCall {
                call_id,
                execution,
                arguments,
                ..
            } => push_message_blocks(
                &mut messages,
                "assistant",
                vec![tool_use_block(
                    "tool_search".to_string(),
                    call_id
                        .clone()
                        .unwrap_or_else(|| "tool_search_call".to_string()),
                    tool_search_input(execution, arguments),
                )],
            ),
            ResponseItem::LocalShellCall {
                call_id,
                id,
                action,
                ..
            } => push_message_blocks(
                &mut messages,
                "assistant",
                vec![tool_use_block(
                    "local_shell".to_string(),
                    call_id
                        .clone()
                        .or_else(|| id.clone())
                        .unwrap_or_else(|| "local_shell_call".to_string()),
                    local_shell_input(action),
                )],
            ),
            ResponseItem::FunctionCallOutput { call_id, output }
            | ResponseItem::CustomToolCallOutput {
                call_id, output, ..
            } => push_message_blocks(
                &mut messages,
                "user",
                vec![tool_result_block(
                    call_id.clone(),
                    tool_result_text(output),
                    output.success == Some(false),
                )],
            ),
            ResponseItem::ToolSearchOutput {
                call_id,
                status,
                execution,
                tools,
            } => push_message_blocks(
                &mut messages,
                "user",
                vec![tool_result_block(
                    call_id
                        .clone()
                        .unwrap_or_else(|| "tool_search_call".to_string()),
                    json!({
                        "status": status,
                        "execution": execution,
                        "tools": tools,
                    })
                    .to_string(),
                    status != "completed",
                )],
            ),
            _ => {}
        }
    }

    if messages.is_empty() {
        messages.push(json!({
            "role": "user",
            "content": [{ "type": "text", "text": "" }],
        }));
    }

    let system = system_segments
        .into_iter()
        .map(|segment| segment.trim().to_string())
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    let tools = anthropic_tools(&request.tools);
    let mut body = json!({
        "model": request.model,
        "max_tokens": ANTHROPIC_DEFAULT_MAX_TOKENS,
        "messages": messages,
        "stream": stream,
    });
    if let Some(object) = body.as_object_mut() {
        if !system.is_empty() {
            object.insert("system".to_string(), Value::String(system));
        }
        if !tools.is_empty() {
            object.insert("tools".to_string(), Value::Array(tools));
            if !request.parallel_tool_calls {
                object.insert(
                    "tool_choice".to_string(),
                    json!({
                        "type": "auto",
                        "disable_parallel_tool_use": true,
                    }),
                );
            }
        }
        if let Some(thinking) = anthropic_thinking(request) {
            object.insert("thinking".to_string(), thinking);
        }
    }
    body
}

pub(crate) fn freeform_tool_names(tools: &[Value]) -> HashSet<String> {
    tools
        .iter()
        .filter_map(|tool| {
            (tool.get("type").and_then(Value::as_str) == Some("custom"))
                .then(|| tool.get("name").and_then(Value::as_str).map(str::to_string))
                .flatten()
        })
        .collect()
}

pub(crate) fn spawn_response_stream(
    stream_response: StreamResponse,
    idle_timeout: Duration,
    telemetry: Option<std::sync::Arc<dyn SseTelemetry>>,
    turn_state: Option<std::sync::Arc<std::sync::OnceLock<String>>>,
    freeform_tool_names: HashSet<String>,
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
        freeform_tool_names,
    ));
    ResponseStream { rx_event }
}

async fn process_sse(
    stream: ByteStream,
    tx_event: mpsc::Sender<Result<ResponseEvent, ApiError>>,
    idle_timeout: Duration,
    telemetry: Option<std::sync::Arc<dyn SseTelemetry>>,
    freeform_tool_names: HashSet<String>,
) {
    let mut stream = stream.eventsource();
    let mut response_error: Option<ApiError> = None;
    let mut state = AnthropicStreamState::new(freeform_tool_names);

    loop {
        let start = Instant::now();
        let response = timeout(idle_timeout, stream.next()).await;
        if let Some(t) = telemetry.as_ref() {
            t.on_sse_poll(&response, start.elapsed());
        }
        let sse = match response {
            Ok(Some(Ok(sse))) => sse,
            Ok(Some(Err(err))) => {
                debug!("SSE Error: {err:#}");
                if state.can_finish_after_disconnect() && response_error.is_none() {
                    let _ = state.finish(&tx_event).await;
                } else {
                    let _ = tx_event.send(Err(ApiError::Stream(err.to_string()))).await;
                }
                return;
            }
            Ok(None) => {
                if state.can_finish_after_disconnect() && response_error.is_none() {
                    let _ = state.finish(&tx_event).await;
                } else {
                    let error = response_error.unwrap_or(ApiError::Stream(
                        "stream closed before anthropic message_stop".to_string(),
                    ));
                    let _ = tx_event.send(Err(error)).await;
                }
                return;
            }
            Err(_) => {
                let _ = tx_event
                    .send(Err(ApiError::Stream(
                        "idle timeout waiting for Anthropic SSE".to_string(),
                    )))
                    .await;
                return;
            }
        };

        trace!(raw_sse_data = %sse.data, "anthropic raw SSE");
        let event: AnthropicStreamEvent = match serde_json::from_str(&sse.data) {
            Ok(event) => event,
            Err(err) => {
                debug!(
                    "Failed to parse Anthropic SSE event: {err}, data: {}",
                    &sse.data
                );
                continue;
            }
        };

        match state.handle_event(event, &tx_event).await {
            Ok(should_finish) => {
                if should_finish {
                    return;
                }
            }
            Err(err) => {
                response_error = Some(err);
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct AnthropicStreamEvent {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    message: Option<AnthropicMessage>,
    #[serde(default)]
    delta: Option<AnthropicDelta>,
    #[serde(default)]
    usage: Option<AnthropicUsage>,
    #[serde(default)]
    content_block: Option<Value>,
    #[serde(default)]
    index: Option<usize>,
    #[serde(default)]
    error: Option<AnthropicErrorPayload>,
}

#[derive(Debug, Deserialize)]
struct AnthropicMessage {
    id: String,
    #[serde(default)]
    model: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnthropicDelta {
    #[serde(rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    thinking: Option<String>,
    #[serde(default)]
    partial_json: Option<String>,
    #[serde(default)]
    stop_reason: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
struct AnthropicUsage {
    #[serde(default)]
    input_tokens: Option<i64>,
    #[serde(default)]
    output_tokens: Option<i64>,
    #[serde(default)]
    cache_read_input_tokens: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct AnthropicErrorPayload {
    #[serde(rename = "type")]
    error_type: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

#[derive(Default)]
struct AnthropicStreamState {
    response_id: Option<String>,
    message_id: Option<String>,
    message_item_started: bool,
    reasoning_item_started: bool,
    reasoning_item_done: bool,
    completed: bool,
    text_blocks: BTreeMap<usize, String>,
    reasoning_blocks: BTreeMap<usize, String>,
    tool_blocks: BTreeMap<usize, ToolUseState>,
    reasoning_summary_started: HashSet<usize>,
    usage: Option<AnthropicUsage>,
    stop_reason: Option<String>,
    freeform_tool_names: HashSet<String>,
    /// Tracks whether we are currently inside an inline `<think>...</think>` block
    /// that some non-Anthropic endpoints embed inside `text_delta` events.
    in_inline_think: bool,
    /// Buffer for partial inline think/tool-call tag prefixes that were split across chunks.
    inline_tag_buf: String,
    /// Buffer for inline `<tool_call>...</tool_call>` text emitted by models that
    /// cannot produce structured tool-use events.
    inline_tool_call_buf: Option<String>,
}

#[derive(Default)]
struct ToolUseState {
    id: Option<String>,
    name: Option<String>,
    input: Option<Value>,
    partial_json: String,
}

impl AnthropicStreamState {
    fn new(freeform_tool_names: HashSet<String>) -> Self {
        Self {
            freeform_tool_names,
            ..Self::default()
        }
    }

    async fn handle_event(
        &mut self,
        event: AnthropicStreamEvent,
        tx_event: &mpsc::Sender<Result<ResponseEvent, ApiError>>,
    ) -> Result<bool, ApiError> {
        match event.kind.as_str() {
            "message_start" => {
                let Some(message) = event.message else {
                    return Ok(false);
                };
                self.response_id = Some(message.id.clone());
                self.message_id = Some(message.id);
                self.message_item_started = false;
                self.reasoning_item_started = false;
                self.reasoning_item_done = false;
                self.completed = false;
                self.text_blocks.clear();
                self.reasoning_blocks.clear();
                self.tool_blocks.clear();
                self.reasoning_summary_started.clear();
                self.usage = None;
                self.stop_reason = None;
                send_event(tx_event, ResponseEvent::Created).await?;
                if let Some(model) = message.model {
                    send_event(tx_event, ResponseEvent::ServerModel(model)).await?;
                }
            }
            "content_block_start" => {
                if let (Some(index), Some(content_block)) = (event.index, event.content_block) {
                    self.handle_content_block_start(index, content_block);
                }
            }
            "content_block_delta" => {
                if let (Some(index), Some(delta)) = (event.index, event.delta) {
                    self.handle_content_block_delta(index, delta, tx_event)
                        .await?;
                }
            }
            "message_delta" => {
                self.usage = event.usage;
                if let Some(delta) = event.delta {
                    self.stop_reason = delta.stop_reason;
                }
            }
            "message_stop" => {
                self.finish(tx_event).await?;
                return Ok(true);
            }
            "error" => {
                return Err(map_stream_error(event.error));
            }
            "ping" | "content_block_stop" => {}
            _ => {
                trace!("unhandled anthropic event: {}", event.kind);
            }
        }

        Ok(false)
    }

    fn handle_content_block_start(&mut self, index: usize, content_block: Value) {
        let Some(content_type) = content_block.get("type").and_then(Value::as_str) else {
            return;
        };

        match content_type {
            "text" => {
                let text = content_block
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                self.text_blocks.insert(index, text);
            }
            "tool_use" => {
                let entry = self.tool_blocks.entry(index).or_default();
                if let Some(id) = content_block.get("id").and_then(Value::as_str) {
                    entry.id = Some(id.to_string());
                }
                if let Some(name) = content_block.get("name").and_then(Value::as_str) {
                    entry.name = Some(name.to_string());
                }
                if let Some(input) = content_block.get("input") {
                    entry.input = Some(input.clone());
                }
            }
            _ => {}
        }
    }

    async fn handle_content_block_delta(
        &mut self,
        index: usize,
        delta: AnthropicDelta,
        tx_event: &mpsc::Sender<Result<ResponseEvent, ApiError>>,
    ) -> Result<(), ApiError> {
        match delta.kind.as_deref() {
            Some("text_delta") => {
                let text = delta.text.unwrap_or_default();
                self.handle_text_delta(index, text, tx_event).await?;
            }
            Some("thinking_delta") => {
                let thinking = delta.thinking.unwrap_or_default();
                self.handle_reasoning_delta(index, thinking, tx_event)
                    .await?;
            }
            Some("input_json_delta") => {
                self.tool_blocks
                    .entry(index)
                    .or_default()
                    .partial_json
                    .push_str(delta.partial_json.as_deref().unwrap_or_default());
            }
            _ => {}
        }
        Ok(())
    }

    async fn handle_reasoning_delta(
        &mut self,
        index: usize,
        thinking: String,
        tx_event: &mpsc::Sender<Result<ResponseEvent, ApiError>>,
    ) -> Result<(), ApiError> {
        if !self.reasoning_item_started {
            self.reasoning_item_started = true;
            self.reasoning_item_done = false;
            send_event(
                tx_event,
                ResponseEvent::OutputItemAdded(ResponseItem::Reasoning {
                    id: self.message_id.clone().unwrap_or_default(),
                    summary: Vec::new(),
                    content: None,
                    encrypted_content: None,
                }),
            )
            .await?;
        }

        if self.reasoning_summary_started.insert(index) {
            send_event(
                tx_event,
                ResponseEvent::ReasoningSummaryPartAdded {
                    summary_index: index as i64,
                },
            )
            .await?;
        }

        self.reasoning_blocks
            .entry(index)
            .or_default()
            .push_str(&thinking);
        send_event(
            tx_event,
            ResponseEvent::ReasoningSummaryDelta {
                delta: thinking,
                summary_index: index as i64,
            },
        )
        .await
    }

    async fn handle_text_delta(
        &mut self,
        index: usize,
        text: String,
        tx_event: &mpsc::Sender<Result<ResponseEvent, ApiError>>,
    ) -> Result<(), ApiError> {
        let combined = if self.inline_tag_buf.is_empty() {
            text
        } else {
            let mut s = std::mem::take(&mut self.inline_tag_buf);
            s.push_str(&text);
            s
        };
        let mut remaining = combined.as_str();
        loop {
            if remaining.is_empty() {
                break;
            }
            if self.inline_tool_call_buf.is_some() {
                if let Some((end, end_tag)) = find_first_tag(remaining, TOOL_CALL_END_TAGS) {
                    let chunk = &remaining[..end];
                    self.inline_tool_call_buf.as_mut().unwrap().push_str(chunk);
                    remaining = &remaining[end + end_tag.len()..];
                    let buf = self.inline_tool_call_buf.take().unwrap();
                    let tool_index = self.tool_blocks.len();
                    if let Some(tool) = parse_inline_tool_call(&buf) {
                        self.tool_blocks.insert(tool_index, tool);
                    }
                } else {
                    let tag_prefix = longest_tag_prefix(remaining, TOOL_CALL_END_TAGS);
                    let chunk = &remaining[..remaining.len() - tag_prefix.len()];
                    self.inline_tool_call_buf.as_mut().unwrap().push_str(chunk);
                    if !tag_prefix.is_empty() {
                        self.inline_tag_buf.push_str(tag_prefix);
                    }
                    break;
                }
            } else if self.in_inline_think {
                if let Some((end, end_tag)) = find_first_tag(remaining, THINK_END_TAGS) {
                    let think_chunk = &remaining[..end];
                    if !think_chunk.is_empty() {
                        self.handle_reasoning_delta(index, think_chunk.to_string(), tx_event)
                            .await?;
                    }
                    self.in_inline_think = false;
                    remaining = &remaining[end + end_tag.len()..];
                } else {
                    let tag_prefix = longest_tag_prefix(remaining, THINK_END_TAGS);
                    let think_chunk = &remaining[..remaining.len() - tag_prefix.len()];
                    if !think_chunk.is_empty() {
                        self.handle_reasoning_delta(index, think_chunk.to_string(), tx_event)
                            .await?;
                    }
                    if !tag_prefix.is_empty() {
                        self.inline_tag_buf.push_str(tag_prefix);
                    }
                    break;
                }
            } else if let Some((start, start_tag)) = find_first_tag(remaining, TOOL_CALL_START_TAGS)
            {
                let before = &remaining[..start];
                if !before.is_empty() {
                    self.ensure_message_started(tx_event).await?;
                    self.text_blocks.entry(index).or_default().push_str(before);
                    send_event(tx_event, ResponseEvent::OutputTextDelta(before.to_string()))
                        .await?;
                }
                let after = &remaining[start + start_tag.len()..];
                let after = after.trim_start_matches("<tool_call>");
                self.inline_tool_call_buf = Some(String::new());
                remaining = after;
            } else if let Some((start, start_tag)) = find_first_tag(remaining, THINK_START_TAGS) {
                let before = &remaining[..start];
                if !before.is_empty() {
                    self.ensure_message_started(tx_event).await?;
                    self.text_blocks.entry(index).or_default().push_str(before);
                    send_event(tx_event, ResponseEvent::OutputTextDelta(before.to_string()))
                        .await?;
                }
                self.in_inline_think = true;
                remaining = &remaining[start + start_tag.len()..];
            } else {
                let tag_prefix = longest_tag_prefix(remaining, INLINE_TEXT_TAGS);
                let emit = &remaining[..remaining.len() - tag_prefix.len()];
                if !emit.is_empty() {
                    self.ensure_message_started(tx_event).await?;
                    self.text_blocks.entry(index).or_default().push_str(emit);
                    send_event(tx_event, ResponseEvent::OutputTextDelta(emit.to_string())).await?;
                }
                if !tag_prefix.is_empty() {
                    self.inline_tag_buf.push_str(tag_prefix);
                }
                break;
            }
        }
        Ok(())
    }

    async fn finish_reasoning_if_needed(
        &mut self,
        tx_event: &mpsc::Sender<Result<ResponseEvent, ApiError>>,
    ) -> Result<(), ApiError> {
        if !self.reasoning_item_started || self.reasoning_item_done {
            return Ok(());
        }
        self.reasoning_item_done = true;

        let text = self
            .reasoning_blocks
            .values()
            .map(String::as_str)
            .collect::<String>();
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
                id: self.message_id.clone().unwrap_or_default(),
                summary,
                content,
                encrypted_content: None,
            }),
        )
        .await
    }

    async fn ensure_message_started(
        &mut self,
        tx_event: &mpsc::Sender<Result<ResponseEvent, ApiError>>,
    ) -> Result<(), ApiError> {
        if self.message_item_started {
            return Ok(());
        }

        self.finish_reasoning_if_needed(tx_event).await?;
        self.message_item_started = true;
        send_event(
            tx_event,
            ResponseEvent::OutputItemAdded(ResponseItem::Message {
                id: self.message_id.clone(),
                role: "assistant".to_string(),
                content: Vec::new(),
                end_turn: None,
                phase: None,
            }),
        )
        .await
    }

    async fn finish(
        &mut self,
        tx_event: &mpsc::Sender<Result<ResponseEvent, ApiError>>,
    ) -> Result<(), ApiError> {
        if self.completed {
            return Ok(());
        }
        self.completed = true;

        self.finish_reasoning_if_needed(tx_event).await?;
        if !self.message_item_started && !self.text_blocks.is_empty() {
            self.ensure_message_started(tx_event).await?;
        }

        if self.message_item_started {
            let text = self
                .text_blocks
                .values()
                .map(String::as_str)
                .collect::<String>();
            send_event(
                tx_event,
                ResponseEvent::OutputItemDone(ResponseItem::Message {
                    id: self.message_id.clone(),
                    role: "assistant".to_string(),
                    content: vec![ContentItem::OutputText { text }],
                    end_turn: self.message_end_turn(),
                    phase: None,
                }),
            )
            .await?;
        }

        for (index, tool) in &self.tool_blocks {
            if let Some(item) = tool_use_to_response_item(*index, tool, &self.freeform_tool_names) {
                send_event(tx_event, ResponseEvent::OutputItemDone(item)).await?;
            }
        }

        send_event(
            tx_event,
            ResponseEvent::Completed {
                response_id: self
                    .response_id
                    .clone()
                    .unwrap_or_else(|| "anthropic-response".to_string()),
                token_usage: self.token_usage(),
            },
        )
        .await
    }

    fn can_finish_after_disconnect(&self) -> bool {
        self.stop_reason.is_some()
    }

    fn message_end_turn(&self) -> Option<bool> {
        match self.stop_reason.as_deref() {
            Some("tool_use") => Some(false),
            Some("end_turn") => Some(true),
            _ => None,
        }
    }

    fn token_usage(&self) -> Option<TokenUsage> {
        self.usage.as_ref().map(|usage| {
            let input_tokens = usage.input_tokens.unwrap_or_default();
            let output_tokens = usage.output_tokens.unwrap_or_default();
            let cached_input_tokens = usage.cache_read_input_tokens.unwrap_or_default();
            TokenUsage {
                input_tokens,
                cached_input_tokens,
                output_tokens,
                reasoning_output_tokens: 0,
                total_tokens: input_tokens + cached_input_tokens + output_tokens,
            }
        })
    }
}

fn output_schema(request: &ResponsesApiRequest) -> Option<&Value> {
    request
        .text
        .as_ref()
        .and_then(|text| text.format.as_ref())
        .map(|TextFormat { schema, .. }| schema)
}

fn anthropic_output_schema_instruction(output_schema: &Value) -> String {
    let schema = serde_json::to_string(output_schema).unwrap_or_else(|_| output_schema.to_string());
    format!("{ANTHROPIC_OUTPUT_SCHEMA_INSTRUCTIONS} {schema}")
}

fn find_first_tag<'a>(s: &'a str, tags: &'a [&'a str]) -> Option<(usize, &'a str)> {
    tags.iter()
        .filter_map(|tag| s.find(tag).map(|index| (index, *tag)))
        .min_by_key(|(index, _)| *index)
}

/// Returns the longest suffix of `s` that is a proper prefix of one of `tags`.
/// Used to detect cross-chunk tag splits so the tag buffer can be held until the next delta.
fn longest_tag_prefix<'a>(s: &'a str, tags: &[&str]) -> &'a str {
    let bytes = s.as_bytes();
    for tag in tags {
        let tag_bytes = tag.as_bytes();
        for len in (1..tag_bytes.len()).rev() {
            if bytes.len() >= len && &bytes[bytes.len() - len..] == &tag_bytes[..len] {
                return &s[s.len() - len..];
            }
        }
    }
    ""
}

fn content_text(content: &[ContentItem]) -> String {
    content
        .iter()
        .map(|item| match item {
            ContentItem::InputText { text } | ContentItem::OutputText { text } => text.clone(),
            ContentItem::InputImage { image_url } if parse_base64_data_url(image_url).is_some() => {
                "[image: data-url]".to_string()
            }
            ContentItem::InputImage { image_url } => format!("[image: {image_url}]"),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn content_blocks(content: &[ContentItem]) -> Vec<Value> {
    let mut blocks = Vec::new();
    for item in content {
        match item {
            ContentItem::InputText { text } | ContentItem::OutputText { text }
                if !text.is_empty() =>
            {
                blocks.push(json!({
                    "type": "text",
                    "text": text,
                }));
            }
            ContentItem::InputImage { image_url } => {
                if let Some((media_type, data)) = parse_base64_data_url(image_url) {
                    blocks.push(json!({
                        "type": "image",
                        "source": {
                            "type": "base64",
                            "media_type": media_type,
                            "data": data,
                        },
                    }));
                } else if image_url.starts_with("data:") {
                    blocks.push(json!({
                        "type": "text",
                        "text": "[image: data-url]",
                    }));
                } else {
                    blocks.push(json!({
                        "type": "text",
                        "text": format!("[image: {image_url}]"),
                    }));
                }
            }
            ContentItem::InputText { .. } | ContentItem::OutputText { .. } => {}
        }
    }
    blocks
}

fn parse_base64_data_url(image_url: &str) -> Option<(&str, &str)> {
    let rest = image_url.strip_prefix("data:")?;
    let (meta, data) = rest.split_once(',')?;
    if data.trim().is_empty() {
        return None;
    }

    let mut parts = meta.split(';');
    let media_type = parts.next()?.trim();
    if media_type.is_empty() {
        return None;
    }
    let is_base64 = parts.any(|part| part.trim().eq_ignore_ascii_case("base64"));
    if !is_base64 {
        return None;
    }

    Some((media_type, data))
}

fn tool_use_block(name: String, call_id: String, input: Value) -> Value {
    json!({
        "type": "tool_use",
        "id": call_id,
        "name": name,
        "input": input,
    })
}

fn tool_result_block(call_id: String, output: String, is_error: bool) -> Value {
    let mut block = json!({
        "type": "tool_result",
        "tool_use_id": call_id,
        "content": output,
    });
    if is_error && let Some(object) = block.as_object_mut() {
        object.insert("is_error".to_string(), Value::Bool(true));
    }
    block
}

fn tool_result_text(output: &FunctionCallOutputPayload) -> String {
    output.body.to_text().unwrap_or_else(|| output.to_string())
}

fn push_message_blocks(messages: &mut Vec<Value>, role: &str, blocks: Vec<Value>) {
    if blocks.is_empty() {
        return;
    }

    if let Some(existing_blocks) = messages.last_mut().and_then(|message| {
        if message.get("role").and_then(Value::as_str) == Some(role) {
            message.get_mut("content").and_then(Value::as_array_mut)
        } else {
            None
        }
    }) {
        existing_blocks.extend(blocks);
        return;
    }

    messages.push(json!({
        "role": role,
        "content": blocks,
    }));
}

fn local_shell_input(action: &LocalShellAction) -> Value {
    match action {
        LocalShellAction::Exec(exec) => json!({
            "command": exec.command,
            "workdir": exec.working_directory,
            "timeout_ms": exec.timeout_ms,
        }),
    }
}

fn tool_search_input(execution: &str, arguments: &Value) -> Value {
    let mut object = Map::new();
    object.insert(
        "execution".to_string(),
        Value::String(execution.to_string()),
    );
    object.insert("arguments".to_string(), arguments.clone());
    Value::Object(object)
}

fn anthropic_thinking(request: &ResponsesApiRequest) -> Option<Value> {
    let reasoning = request.reasoning.as_ref()?;
    let budget_tokens = match reasoning.effort {
        Some(ReasoningEffortConfig::Minimal | ReasoningEffortConfig::Low) => {
            ANTHROPIC_LOW_THINKING_BUDGET_TOKENS
        }
        Some(ReasoningEffortConfig::Medium) => ANTHROPIC_MEDIUM_THINKING_BUDGET_TOKENS,
        Some(ReasoningEffortConfig::High | ReasoningEffortConfig::XHigh) => {
            ANTHROPIC_HIGH_THINKING_BUDGET_TOKENS
        }
        Some(ReasoningEffortConfig::None) => {
            if reasoning.summary.is_some() {
                ANTHROPIC_MEDIUM_THINKING_BUDGET_TOKENS
            } else {
                return None;
            }
        }
        None => {
            if reasoning.summary.is_some() {
                ANTHROPIC_MEDIUM_THINKING_BUDGET_TOKENS
            } else {
                return None;
            }
        }
    };

    Some(json!({
        "type": "enabled",
        "budget_tokens": budget_tokens,
    }))
}

fn parse_json_object_or_wrapped(input: &str) -> Value {
    match serde_json::from_str::<Value>(input) {
        Ok(Value::Object(object)) => Value::Object(object),
        Ok(Value::Null) => Value::Object(Map::new()),
        Ok(other) => {
            let mut object = Map::new();
            object.insert(TOOL_INPUT_FIELD.to_string(), other);
            Value::Object(object)
        }
        Err(_) => {
            let mut object = Map::new();
            object.insert(
                TOOL_INPUT_FIELD.to_string(),
                Value::String(input.to_string()),
            );
            Value::Object(object)
        }
    }
}

fn anthropic_tools(tools: &[Value]) -> Vec<Value> {
    tools.iter().filter_map(anthropic_tool).collect()
}

fn anthropic_tool(tool: &Value) -> Option<Value> {
    let tool_type = tool.get("type")?.as_str()?;
    match tool_type {
        "function" => Some(json!({
            "name": tool.get("name")?,
            "description": tool.get("description").cloned().unwrap_or_else(|| Value::String(String::new())),
            "input_schema": tool.get("parameters").cloned().unwrap_or_else(|| json!({ "type": "object" })),
        })),
        "custom" => Some(json!({
            "name": tool.get("name")?,
            "description": tool.get("description").cloned().unwrap_or_else(|| Value::String(String::new())),
            "input_schema": {
                "type": "object",
                "properties": {
                    TOOL_INPUT_FIELD: {
                        "type": "string",
                        "description": "Raw freeform tool input.",
                    }
                },
                "required": [TOOL_INPUT_FIELD],
                "additionalProperties": false,
            },
        })),
        "local_shell" => Some(json!({
            "name": "local_shell",
            "description": "Runs a local shell command and returns its output.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "command": { "type": "array", "items": { "type": "string" } },
                    "workdir": { "type": "string" },
                    "timeout_ms": { "type": "number" },
                    "sandbox_permissions": { "type": "string" },
                    "justification": { "type": "string" },
                    "prefix_rule": { "type": "array", "items": { "type": "string" } },
                },
                "required": ["command"],
                "additionalProperties": false,
            },
        })),
        "tool_search" => Some(json!({
            "name": "tool_search",
            "description": tool.get("description").cloned().unwrap_or_else(|| Value::String(String::new())),
            "input_schema": tool.get("parameters").cloned().unwrap_or_else(|| json!({ "type": "object" })),
        })),
        "web_search" => Some(json!({
            "name": "web_search",
            "description": "Searches the web for public information.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "external_web_access": { "type": "boolean" },
                    "filters": { "type": "object" },
                    "user_location": { "type": "object" },
                    "search_context_size": { "type": "string" },
                    "search_content_types": { "type": "array", "items": { "type": "string" } },
                },
                "additionalProperties": false,
            },
        })),
        "image_generation" => None,
        _ => None,
    }
}

/// Parse an inline `<tool_call>` buffer into a `ToolUseState`.
///
/// Expected buffer content (after stripping outer `<tool_call>` tags and any
/// duplicate opening tag):
///
/// ```text
/// shell<arg_key>command</arg_key><arg_value>ls -la</arg_value>
/// ```
///
/// The tool name is the leading plain text before the first `<`.  Arguments
/// are collected as alternating `<arg_key>`/`<arg_value>` pairs and assembled
/// into a JSON object.
fn parse_inline_tool_call(buf: &str) -> Option<ToolUseState> {
    let buf = buf.trim();
    let buf = buf.trim_start_matches("<tool_call>");
    let tool_name_end = buf.find('<').unwrap_or(buf.len());
    let tool_name = buf[..tool_name_end].trim().to_string();
    if tool_name.is_empty() {
        return None;
    }
    let rest = &buf[tool_name_end..];
    let mut args: Map<String, Value> = Map::new();
    let mut cursor = rest;
    while !cursor.is_empty() {
        let key_start = match cursor.find("<arg_key>") {
            Some(pos) => pos + "<arg_key>".len(),
            None => break,
        };
        let key_end = match cursor[key_start..].find("</arg_key>") {
            Some(pos) => key_start + pos,
            None => break,
        };
        let key = cursor[key_start..key_end].trim().to_string();
        cursor = &cursor[key_end + "</arg_key>".len()..];
        let val_start = match cursor.find("<arg_value>") {
            Some(pos) => pos + "<arg_value>".len(),
            None => break,
        };
        let val_end = match cursor[val_start..].find("</arg_value>") {
            Some(pos) => val_start + pos,
            None => break,
        };
        let val = cursor[val_start..val_end].trim().to_string();
        cursor = &cursor[val_end + "</arg_value>".len()..];
        if !key.is_empty() {
            args.insert(key, Value::String(val));
        }
    }
    Some(ToolUseState {
        id: None,
        name: Some(tool_name),
        input: Some(Value::Object(args)),
        partial_json: String::new(),
    })
}

fn tool_use_to_response_item(
    index: usize,
    tool: &ToolUseState,
    freeform_tool_names: &HashSet<String>,
) -> Option<ResponseItem> {
    let name = tool
        .name
        .clone()
        .unwrap_or_else(|| format!("anthropic_tool_missing_name_{index}"));
    let call_id = tool
        .id
        .clone()
        .unwrap_or_else(|| format!("anthropic_tool_{index}"));
    let input = tool_input_value(tool);
    if name == "tool_search" {
        return Some(tool_search_response_item(call_id, input));
    }
    if freeform_tool_names.contains(&name) {
        let text = match input {
            Value::Object(mut object) => match object.remove(TOOL_INPUT_FIELD) {
                Some(Value::String(text)) => text,
                Some(value) => value.to_string(),
                None => Value::Object(object).to_string(),
            },
            value => value.to_string(),
        };
        return Some(ResponseItem::CustomToolCall {
            id: None,
            status: None,
            call_id,
            name,
            input: text,
        });
    }

    Some(ResponseItem::FunctionCall {
        id: None,
        name,
        namespace: None,
        arguments: serde_json::to_string(&input).unwrap_or_else(|_| "{}".to_string()),
        call_id,
    })
}

fn tool_search_response_item(call_id: String, input: Value) -> ResponseItem {
    if let Value::Object(mut object) = input {
        let execution = object
            .remove("execution")
            .and_then(|value| value.as_str().map(str::to_string))
            .unwrap_or_else(|| "client".to_string());
        let arguments = object.remove("arguments").unwrap_or(Value::Object(object));
        return ResponseItem::ToolSearchCall {
            id: None,
            call_id: Some(call_id),
            status: None,
            execution,
            arguments,
        };
    }

    ResponseItem::ToolSearchCall {
        id: None,
        call_id: Some(call_id),
        status: None,
        execution: "client".to_string(),
        arguments: input,
    }
}

fn tool_input_value(tool: &ToolUseState) -> Value {
    let start_input = tool
        .input
        .clone()
        .unwrap_or_else(|| Value::Object(Map::new()));
    if tool.partial_json.is_empty() {
        return start_input;
    }

    if let Ok(Value::Object(partial_object)) = serde_json::from_str::<Value>(&tool.partial_json) {
        return merge_object_values(start_input, Value::Object(partial_object));
    }

    let mut object = match start_input {
        Value::Object(object) => object,
        value => {
            let mut object = Map::new();
            object.insert(TOOL_INPUT_FIELD.to_string(), value);
            object
        }
    };
    object.insert(
        "raw_partial_json".to_string(),
        Value::String(tool.partial_json.clone()),
    );
    Value::Object(object)
}

fn merge_object_values(base: Value, overlay: Value) -> Value {
    match (base, overlay) {
        (Value::Object(mut base), Value::Object(overlay)) => {
            base.extend(overlay);
            Value::Object(base)
        }
        (_, overlay) => overlay,
    }
}

pub(crate) fn is_context_window_error_message(message: &str) -> bool {
    let normalized = message.to_ascii_lowercase();
    normalized.contains("context window")
        || normalized.contains("context length")
        || normalized.contains("exceed context limit")
        || ((normalized.contains("prompt is too long") || normalized.contains("input is too long"))
            && normalized.contains("token")
            && (normalized.contains("max") || normalized.contains("context")))
}

fn map_stream_error(error: Option<AnthropicErrorPayload>) -> ApiError {
    let Some(error) = error else {
        return ApiError::Stream("anthropic stream error".to_string());
    };
    let message = error
        .message
        .unwrap_or_else(|| "anthropic stream error".to_string());
    match error.error_type.as_deref() {
        Some("invalid_request_error") => {
            if is_context_window_error_message(&message) {
                ApiError::ContextWindowExceeded
            } else {
                ApiError::InvalidRequest { message }
            }
        }
        Some("rate_limit_error") => ApiError::Retryable {
            message,
            delay: None,
        },
        Some("overloaded_error") => ApiError::ServerOverloaded,
        Some("authentication_error") => ApiError::Api {
            status: http::StatusCode::UNAUTHORIZED,
            message,
        },
        Some("permission_error") => ApiError::Api {
            status: http::StatusCode::FORBIDDEN,
            message,
        },
        _ => ApiError::Stream(message),
    }
}

async fn send_event(
    tx_event: &mpsc::Sender<Result<ResponseEvent, ApiError>>,
    event: ResponseEvent,
) -> Result<(), ApiError> {
    tx_event
        .send(Ok(event))
        .await
        .map_err(|_| ApiError::Stream("response stream channel closed".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use bytes::Bytes;
    use codex_client::StreamResponse;
    use codex_client::TransportError;
    use codex_protocol::models::FunctionCallOutputBody;
    use codex_protocol::openai_models::ReasoningEffort;
    use futures::TryStreamExt;
    use futures::stream;
    use http::HeaderMap;
    use pretty_assertions::assert_eq;
    use tokio::sync::mpsc;
    use tokio_test::io::Builder as IoBuilder;
    use tokio_util::io::ReaderStream;

    #[test]
    fn builds_anthropic_request_with_tool_and_output_schema() {
        let request = ResponsesApiRequest {
            model: "claude-3-7-sonnet".to_string(),
            instructions: "You are helpful.".to_string(),
            input: vec![
                ResponseItem::Message {
                    id: None,
                    role: "user".to_string(),
                    content: vec![ContentItem::InputText {
                        text: "hello".to_string(),
                    }],
                    end_turn: None,
                    phase: None,
                },
                ResponseItem::FunctionCallOutput {
                    call_id: "call-1".to_string(),
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text("done".to_string()),
                        success: Some(true),
                    },
                },
            ],
            tools: vec![json!({
                "type": "function",
                "name": "shell",
                "description": "run shell",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "command": { "type": "string" }
                    },
                    "required": ["command"]
                }
            })],
            tool_choice: "auto".to_string(),
            parallel_tool_calls: false,
            reasoning: Some(crate::common::Reasoning {
                effort: Some(ReasoningEffort::High),
                summary: Some(codex_protocol::config_types::ReasoningSummary::Auto),
            }),
            store: false,
            stream: true,
            include: Vec::new(),
            service_tier: None,
            client_metadata: None,
            prompt_cache_key: None,
            text: Some(crate::common::TextControls {
                verbosity: None,
                format: Some(TextFormat {
                    r#type: crate::common::TextFormatType::JsonSchema,
                    strict: true,
                    schema: json!({
                        "type": "object",
                        "properties": {
                            "ok": { "type": "boolean" }
                        },
                        "required": ["ok"]
                    }),
                    name: "schema".to_string(),
                }),
            }),
        };

        let body = build_request_body(&request);
        assert_eq!(body["model"], "claude-3-7-sonnet");
        assert_eq!(body["stream"], true);
        assert_eq!(body["tool_choice"]["disable_parallel_tool_use"], true);
        assert_eq!(body["tools"][0]["name"], "shell");
        assert_eq!(body["thinking"]["type"], "enabled");
        assert_eq!(
            body["thinking"]["budget_tokens"],
            ANTHROPIC_HIGH_THINKING_BUDGET_TOKENS
        );
        assert!(
            body["system"]
                .as_str()
                .expect("system string")
                .contains(ANTHROPIC_OUTPUT_SCHEMA_INSTRUCTIONS)
        );
    }

    #[test]
    fn builds_anthropic_request_merges_tool_result_with_following_user_message() {
        let request = ResponsesApiRequest {
            model: "claude-3-7-sonnet".to_string(),
            instructions: "You are helpful.".to_string(),
            input: vec![
                ResponseItem::Message {
                    id: None,
                    role: "user".to_string(),
                    content: vec![ContentItem::InputText {
                        text: "run it".to_string(),
                    }],
                    end_turn: None,
                    phase: None,
                },
                ResponseItem::FunctionCall {
                    id: None,
                    call_id: "call-1".to_string(),
                    name: "shell".to_string(),
                    namespace: None,
                    arguments: "{\"command\":\"pwd\"}".to_string(),
                },
                ResponseItem::FunctionCallOutput {
                    call_id: "call-1".to_string(),
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text("/workspace".to_string()),
                        success: Some(true),
                    },
                },
                ResponseItem::Message {
                    id: None,
                    role: "user".to_string(),
                    content: vec![ContentItem::InputText {
                        text: "继续".to_string(),
                    }],
                    end_turn: None,
                    phase: None,
                },
            ],
            tools: vec![json!({
                "type": "function",
                "name": "shell",
                "description": "run shell",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "command": { "type": "string" }
                    },
                    "required": ["command"]
                }
            })],
            tool_choice: "auto".to_string(),
            parallel_tool_calls: false,
            reasoning: None,
            store: false,
            stream: true,
            include: Vec::new(),
            service_tier: None,
            client_metadata: None,
            prompt_cache_key: None,
            text: None,
        };

        let body = build_request_body(&request);
        let messages = body["messages"].as_array().expect("messages array");
        assert_eq!(messages.len(), 3);
        assert_eq!(
            messages[2],
            json!({
                "role": "user",
                "content": [
                    {
                        "type": "tool_result",
                        "tool_use_id": "call-1",
                        "content": "/workspace",
                    },
                    {
                        "type": "text",
                        "text": "继续",
                    }
                ],
            })
        );
    }

    #[tokio::test]
    async fn anthropic_stream_maps_text_and_tool_events() {
        let payload = concat!(
            "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"model\":\"claude-3-7-sonnet\"}}\n\n",
            "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}\n\n",
            "data: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_use\",\"id\":\"tool_1\",\"name\":\"apply_patch\",\"input\":{}}}\n\n",
            "data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"input\\\":\\\"*** Begin Patch\\\"}\"}}\n\n",
            "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"input_tokens\":3,\"output_tokens\":2}}\n\n",
            "data: {\"type\":\"message_stop\"}\n\n"
        );

        let mut builder = IoBuilder::new();
        builder.read(payload.as_bytes());
        let reader = builder.build();
        let stream =
            ReaderStream::new(reader).map_err(|err| TransportError::Network(err.to_string()));
        let (tx, mut rx) = mpsc::channel::<Result<ResponseEvent, ApiError>>(16);

        tokio::spawn(process_sse(
            Box::pin(stream),
            tx,
            Duration::from_secs(5),
            None,
            HashSet::from(["apply_patch".to_string()]),
        ));

        let mut events = Vec::new();
        while let Some(event) = rx.recv().await {
            events.push(event.expect("event"));
        }

        assert_matches!(events[0], ResponseEvent::Created);
        assert_matches!(
            &events[1],
            ResponseEvent::ServerModel(model) if model == "claude-3-7-sonnet"
        );
        assert_matches!(
            &events[2],
            ResponseEvent::OutputItemAdded(ResponseItem::Message { id, role, content, end_turn, phase })
                if id.as_deref() == Some("msg_1")
                    && role == "assistant"
                    && content.is_empty()
                    && end_turn.is_none()
                    && phase.is_none()
        );
        assert_matches!(
            &events[3],
            ResponseEvent::OutputTextDelta(text) if text == "Hello"
        );
        assert_matches!(
            &events[4],
            ResponseEvent::OutputItemDone(ResponseItem::Message {
                id,
                role,
                content,
                end_turn,
                phase,
            })
                if id.as_deref() == Some("msg_1")
                    && role == "assistant"
                    && *end_turn == Some(false)
                    && phase.is_none()
                    && content
                        == &vec![ContentItem::OutputText {
                            text: "Hello".to_string(),
                        }]
        );
        assert_matches!(
            &events[5],
            ResponseEvent::OutputItemDone(ResponseItem::CustomToolCall {
                id,
                status,
                call_id,
                name,
                input,
            })
                if id.is_none()
                    && status.is_none()
                    && call_id == "tool_1"
                    && name == "apply_patch"
                    && input == "*** Begin Patch"
        );
        assert_matches!(
            &events[6],
            ResponseEvent::Completed {
                response_id,
                token_usage,
            }
                if response_id == "msg_1"
                    && token_usage
                        == &Some(TokenUsage {
                            input_tokens: 3,
                            cached_input_tokens: 0,
                            output_tokens: 2,
                            reasoning_output_tokens: 0,
                            total_tokens: 5,
                        })
        );
    }

    #[tokio::test]
    async fn anthropic_stream_state_tolerates_escaped_inline_closing_tags() {
        let (tx, mut rx) = mpsc::channel::<Result<ResponseEvent, ApiError>>(32);
        let mut state = AnthropicStreamState::new(HashSet::new());
        state.response_id = Some("msg_1".to_string());
        state.message_id = Some("msg_1".to_string());
        state.stop_reason = Some("end_turn".to_string());

        state
            .handle_text_delta(0, "<think>reasoning<\\/thi".to_string(), &tx)
            .await
            .expect("first delta");
        state
            .handle_text_delta(
                0,
                "nk>mid<tool_call>shell<arg_key>command</arg_key><arg_value>pwd</arg_value><\\/tool_"
                    .to_string(),
                &tx,
            )
            .await
            .expect("second delta");
        state
            .handle_text_delta(0, "call>suffix".to_string(), &tx)
            .await
            .expect("third delta");
        state.finish(&tx).await.expect("finish");
        drop(tx);

        let mut events = Vec::new();
        while let Some(event) = rx.recv().await {
            events.push(event.expect("event"));
        }

        assert_matches!(
            &events[0],
            ResponseEvent::OutputItemAdded(ResponseItem::Reasoning {
                id,
                summary,
                content,
                encrypted_content,
            }) if id == "msg_1"
                && summary.is_empty()
                && content.is_none()
                && encrypted_content.is_none()
        );
        assert_matches!(
            &events[1],
            ResponseEvent::ReasoningSummaryPartAdded { summary_index } if *summary_index == 0
        );
        assert_matches!(
            &events[2],
            ResponseEvent::ReasoningSummaryDelta { delta, summary_index }
                if delta == "reasoning" && *summary_index == 0
        );
        assert_matches!(
            &events[3],
            ResponseEvent::OutputItemDone(ResponseItem::Reasoning {
                id,
                summary,
                content,
                encrypted_content,
            }) if id == "msg_1"
                && summary
                    == &vec![ReasoningItemReasoningSummary::SummaryText {
                        text: "reasoning".to_string(),
                    }]
                && content
                    == &Some(vec![ReasoningItemContent::ReasoningText {
                        text: "reasoning".to_string(),
                    }])
                && encrypted_content.is_none()
        );
        assert_matches!(
            &events[4],
            ResponseEvent::OutputItemAdded(ResponseItem::Message {
                id,
                role,
                content,
                end_turn,
                phase,
            }) if id.as_deref() == Some("msg_1")
                && role == "assistant"
                && content.is_empty()
                && end_turn.is_none()
                && phase.is_none()
        );
        assert_matches!(
            &events[5],
            ResponseEvent::OutputTextDelta(text) if text == "mid"
        );
        assert_matches!(
            &events[6],
            ResponseEvent::OutputTextDelta(text) if text == "suffix"
        );
        assert_matches!(
            &events[7],
            ResponseEvent::OutputItemDone(ResponseItem::Message {
                id,
                role,
                content,
                end_turn,
                phase,
            }) if id.as_deref() == Some("msg_1")
                && role == "assistant"
                && *end_turn == Some(true)
                && phase.is_none()
                && content
                    == &vec![ContentItem::OutputText {
                        text: "midsuffix".to_string(),
                    }]
        );
        assert_matches!(
            &events[8],
            ResponseEvent::OutputItemDone(ResponseItem::FunctionCall {
                id,
                name,
                namespace,
                arguments,
                call_id,
            }) if id.is_none()
                && name == "shell"
                && namespace.is_none()
                && arguments == "{\"command\":\"pwd\"}"
                && call_id == "anthropic_tool_0"
        );
        assert_matches!(
            &events[9],
            ResponseEvent::Completed {
                response_id,
                token_usage,
            } if response_id == "msg_1" && token_usage.is_none()
        );
    }

    #[tokio::test]
    async fn anthropic_stream_completes_when_connection_closes_after_stop_reason() {
        let payload = concat!(
            "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"done\"}}\n\n",
            "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"input_tokens\":2,\"output_tokens\":1}}\n\n"
        );

        let mut builder = IoBuilder::new();
        builder.read(payload.as_bytes());
        let reader = builder.build();
        let stream =
            ReaderStream::new(reader).map_err(|err| TransportError::Network(err.to_string()));
        let (tx, mut rx) = mpsc::channel::<Result<ResponseEvent, ApiError>>(16);

        tokio::spawn(process_sse(
            Box::pin(stream),
            tx,
            Duration::from_secs(5),
            None,
            HashSet::new(),
        ));

        let mut events = Vec::new();
        while let Some(event) = rx.recv().await {
            events.push(event.expect("event"));
        }

        assert_matches!(
            &events[0],
            ResponseEvent::OutputItemAdded(ResponseItem::Message {
                id,
                role,
                content,
                end_turn,
                phase,
            }) if id.is_none()
                && role == "assistant"
                && content.is_empty()
                && end_turn.is_none()
                && phase.is_none()
        );
        assert_matches!(
            &events[1],
            ResponseEvent::OutputTextDelta(text) if text == "done"
        );
        assert_matches!(
            &events[2],
            ResponseEvent::OutputItemDone(ResponseItem::Message {
                id,
                role,
                content,
                end_turn,
                phase,
            }) if id.is_none()
                && role == "assistant"
                && *end_turn == Some(true)
                && phase.is_none()
                && content
                    == &vec![ContentItem::OutputText {
                        text: "done".to_string(),
                    }]
        );
        assert_matches!(
            &events[3],
            ResponseEvent::Completed {
                response_id,
                token_usage,
            }
                if response_id == "anthropic-response"
                    && token_usage
                        == &Some(TokenUsage {
                            input_tokens: 2,
                            cached_input_tokens: 0,
                            output_tokens: 1,
                            reasoning_output_tokens: 0,
                            total_tokens: 3,
                        })
        );
    }

    #[tokio::test]
    async fn anthropic_stream_errors_when_connection_closes_before_stop_reason() {
        let payload = concat!(
            "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\"}}\n\n",
            "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"dangling\"}}\n\n"
        );

        let mut builder = IoBuilder::new();
        builder.read(payload.as_bytes());
        let reader = builder.build();
        let stream =
            ReaderStream::new(reader).map_err(|err| TransportError::Network(err.to_string()));
        let (tx, mut rx) = mpsc::channel::<Result<ResponseEvent, ApiError>>(16);

        tokio::spawn(process_sse(
            Box::pin(stream),
            tx,
            Duration::from_secs(5),
            None,
            HashSet::new(),
        ));

        let mut events = Vec::new();
        while let Some(event) = rx.recv().await {
            events.push(event);
        }

        assert!(
            matches!(events.last(), Some(Err(ApiError::Stream(message))) if message == "stream closed before anthropic message_stop")
        );
    }

    #[tokio::test]
    async fn spawn_response_stream_sets_turn_state() {
        let turn_state = std::sync::Arc::new(std::sync::OnceLock::new());
        let stream_response = StreamResponse {
            status: http::StatusCode::OK,
            headers: HeaderMap::from_iter([(
                http::header::HeaderName::from_static("x-codex-turn-state"),
                http::HeaderValue::from_static("sticky"),
            )]),
            bytes: Box::pin(stream::iter(vec![Ok::<Bytes, TransportError>(Bytes::from_static(
                b"data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\"}}\n\ndata: {\"type\":\"message_stop\"}\n\n",
            ))])),
        };

        let mut response_stream = spawn_response_stream(
            stream_response,
            Duration::from_secs(5),
            None,
            Some(turn_state.clone()),
            HashSet::new(),
        );
        while let Some(event) = response_stream.rx_event.recv().await {
            if matches!(event, Ok(ResponseEvent::Completed { .. })) {
                break;
            }
        }

        assert_eq!(turn_state.get().map(String::as_str), Some("sticky"));
    }

    #[test]
    fn map_stream_error_maps_context_window_invalid_request() {
        let error = AnthropicErrorPayload {
            error_type: Some("invalid_request_error".to_string()),
            message: Some("prompt is too long: 220000 tokens > 200000 max".to_string()),
        };

        assert_matches!(
            map_stream_error(Some(error)),
            ApiError::ContextWindowExceeded
        );
    }

    #[test]
    fn map_stream_error_keeps_regular_invalid_requests() {
        let error = AnthropicErrorPayload {
            error_type: Some("invalid_request_error".to_string()),
            message: Some("prompt is too long for tool name validation".to_string()),
        };

        assert_matches!(
            map_stream_error(Some(error)),
            ApiError::InvalidRequest { message }
                if message == "prompt is too long for tool name validation"
        );
    }

    #[test]
    fn tool_search_round_trips_to_tool_search_call() {
        let item = tool_use_to_response_item(
            0,
            &ToolUseState {
                id: Some("tool_1".to_string()),
                name: Some("tool_search".to_string()),
                input: Some(json!({
                    "execution": "client",
                    "arguments": {
                        "query": "shell",
                        "limit": 2,
                    },
                })),
                partial_json: String::new(),
            },
            &HashSet::new(),
        );

        assert_matches!(
            item,
            Some(ResponseItem::ToolSearchCall {
                id: None,
                call_id: Some(call_id),
                status: None,
                execution,
                arguments,
            }) if call_id == "tool_1"
                && execution == "client"
                && arguments == json!({
                    "query": "shell",
                    "limit": 2,
                })
        );
    }
}
