use std::collections::BTreeMap;

use eventsource_stream::Eventsource;
use futures::StreamExt;
use serde_json::Value;
use serde_json::json;
use tokio::sync::mpsc;
use tokio_util::bytes::Bytes;

use super::ApiError;
use super::ChatCompletionsResponseTranslator;
use super::chat_types::ChatCompletionChunk;
use super::chat_types::ChatToolCallDelta;
use super::chat_types::ChatUsage;

impl ChatCompletionsResponseTranslator {
    pub(super) async fn translate_success_response_stream(
        self,
        upstream: reqwest::Response,
        tx: mpsc::Sender<Result<Bytes, std::convert::Infallible>>,
    ) {
        let mut state = ChatCompletionsSseState::new(self);
        let mut stream = upstream.bytes_stream().eventsource();

        while let Some(event) = stream.next().await {
            match event {
                Ok(event) if event.data == "[DONE]" => {
                    if let Err(err) = state.complete(&tx).await {
                        let _ = send_failed_event(&tx, &state, err).await;
                    }
                    return;
                }
                Ok(event) => {
                    let chunk = match serde_json::from_str::<ChatCompletionChunk>(&event.data) {
                        Ok(chunk) => chunk,
                        Err(err) => {
                            let _ = send_failed_event(
                                &tx,
                                &state,
                                ApiError::bad_gateway(format!(
                                    "failed to decode upstream /v1/chat/completions stream chunk for /v1/responses compatibility: {err}"
                                )),
                            )
                            .await;
                            return;
                        }
                    };
                    if let Err(err) = state.process_chunk(chunk, &tx).await {
                        let _ = send_failed_event(&tx, &state, err).await;
                        return;
                    }
                }
                Err(err) => {
                    let _ = send_failed_event(
                        &tx,
                        &state,
                        ApiError::bad_gateway(format!(
                            "chat completions stream error during /v1/responses translation: {err}"
                        )),
                    )
                    .await;
                    return;
                }
            }
        }

        if let Err(err) = state.complete(&tx).await {
            let _ = send_failed_event(&tx, &state, err).await;
        }
    }
}

struct ChatCompletionsSseState {
    translator: ChatCompletionsResponseTranslator,
    response_id: Option<String>,
    model: Option<String>,
    created_at: Option<i64>,
    created_sent: bool,
    output_text: String,
    output_text_done: bool,
    message_item_started: bool,
    tool_calls: BTreeMap<u32, PendingToolCall>,
    output_items: Vec<Value>,
    token_usage: Option<Value>,
    completed: bool,
    next_output_index: i64,
}

impl ChatCompletionsSseState {
    fn new(translator: ChatCompletionsResponseTranslator) -> Self {
        Self {
            translator,
            response_id: None,
            model: None,
            created_at: None,
            created_sent: false,
            output_text: String::new(),
            output_text_done: false,
            message_item_started: false,
            tool_calls: BTreeMap::new(),
            output_items: Vec::new(),
            token_usage: None,
            completed: false,
            next_output_index: 0,
        }
    }

    async fn process_chunk(
        &mut self,
        chunk: ChatCompletionChunk,
        tx: &mpsc::Sender<Result<Bytes, std::convert::Infallible>>,
    ) -> Result<(), ApiError> {
        if self.response_id.is_none() {
            self.response_id = Some(chunk.id.clone());
        }
        if self.model.is_none() {
            self.model = Some(chunk.model.clone());
        }
        if self.created_at.is_none() {
            self.created_at = chunk.created;
        }
        if !self.created_sent {
            self.send_created(tx).await?;
        }
        if let Some(usage) = chunk.usage {
            self.token_usage = Some(chat_usage_json(usage));
        }

        for choice in chunk.choices {
            if let Some(delta) = choice.delta {
                if let Some(text) = chat_message_text(delta.content.as_ref())
                    && !text.is_empty()
                {
                    self.ensure_message_started(tx).await?;
                    self.output_text.push_str(&text);
                    send_sse_json(
                        tx,
                        "response.output_text.delta",
                        json!({
                            "type": "response.output_text.delta",
                            "item_id": self.message_item_id(),
                            "output_index": 0,
                            "content_index": 0,
                            "delta": text,
                        }),
                    )
                    .await?;
                }
                for tool_call in delta.tool_calls {
                    self.merge_tool_call(tool_call);
                }
            }

            if choice.finish_reason.is_some() {
                self.flush_output_items(tx).await?;
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

    async fn send_created(
        &mut self,
        tx: &mpsc::Sender<Result<Bytes, std::convert::Infallible>>,
    ) -> Result<(), ApiError> {
        send_sse_json(
            tx,
            "response.created",
            json!({
                "type": "response.created",
                "response": {
                    "id": self.response_id(),
                    "object": "response",
                    "created_at": self.created_at,
                    "status": "in_progress",
                    "error": Value::Null,
                    "incomplete_details": Value::Null,
                    "model": self.model.clone(),
                    "output": [],
                    "usage": Value::Null,
                }
            }),
        )
        .await?;
        self.created_sent = true;
        Ok(())
    }

    async fn ensure_message_started(
        &mut self,
        tx: &mpsc::Sender<Result<Bytes, std::convert::Infallible>>,
    ) -> Result<(), ApiError> {
        if self.message_item_started {
            return Ok(());
        }

        send_sse_json(
            tx,
            "response.output_item.added",
            json!({
                "type": "response.output_item.added",
                "output_index": 0,
                "item": {
                    "id": self.message_item_id(),
                    "type": "message",
                    "role": "assistant",
                    "content": [],
                }
            }),
        )
        .await?;
        self.message_item_started = true;
        Ok(())
    }

    async fn flush_output_items(
        &mut self,
        tx: &mpsc::Sender<Result<Bytes, std::convert::Infallible>>,
    ) -> Result<(), ApiError> {
        if !self.output_text_done && !self.output_text.is_empty() {
            let message_item = json!({
                "id": self.message_item_id(),
                "type": "message",
                "role": "assistant",
                "content": [{
                    "type": "output_text",
                    "text": self.output_text,
                }],
            });
            send_sse_json(
                tx,
                "response.output_item.done",
                json!({
                    "type": "response.output_item.done",
                    "output_index": 0,
                    "item": message_item.clone(),
                }),
            )
            .await?;
            self.output_items.push(message_item);
            self.output_text_done = true;
            self.next_output_index = 1;
        }

        for tool_call in self.tool_calls.values() {
            let Some(call_id) = tool_call.id.clone() else {
                continue;
            };
            let Some(name) = tool_call.name.clone() else {
                continue;
            };
            let item = self.translator.tool_call_response_item_from_parts(
                call_id.clone(),
                name,
                tool_call.arguments.clone(),
            )?;
            let item = serde_json::to_value(item).map_err(|err| {
                ApiError::internal(format!(
                    "failed to encode translated tool call stream item: {err}"
                ))
            })?;
            let item = with_item_id(item, format!("{}_{}", self.response_id(), call_id));
            send_sse_json(
                tx,
                "response.output_item.added",
                json!({
                    "type": "response.output_item.added",
                    "output_index": self.next_output_index,
                    "item": item.clone(),
                }),
            )
            .await?;
            send_sse_json(
                tx,
                "response.output_item.done",
                json!({
                    "type": "response.output_item.done",
                    "output_index": self.next_output_index,
                    "item": item.clone(),
                }),
            )
            .await?;
            self.output_items.push(item);
            self.next_output_index += 1;
        }
        self.tool_calls.clear();
        Ok(())
    }

    async fn complete(
        &mut self,
        tx: &mpsc::Sender<Result<Bytes, std::convert::Infallible>>,
    ) -> Result<(), ApiError> {
        if self.completed {
            return Ok(());
        }

        self.flush_output_items(tx).await?;
        send_sse_json(
            tx,
            "response.completed",
            json!({
                "type": "response.completed",
                "response": {
                    "id": self.response_id(),
                    "object": "response",
                    "created_at": self.created_at,
                    "status": "completed",
                    "error": Value::Null,
                    "incomplete_details": Value::Null,
                    "model": self.model.clone(),
                    "output": self.output_items.clone(),
                    "usage": self.token_usage,
                }
            }),
        )
        .await?;
        self.completed = true;
        Ok(())
    }

    fn response_id(&self) -> String {
        self.response_id
            .clone()
            .unwrap_or_else(|| "chatcmpl-compat".to_string())
    }

    fn message_item_id(&self) -> String {
        format!("{}_message_0", self.response_id())
    }
}

#[derive(Default)]
struct PendingToolCall {
    id: Option<String>,
    name: Option<String>,
    arguments: String,
}

async fn send_failed_event(
    tx: &mpsc::Sender<Result<Bytes, std::convert::Infallible>>,
    state: &ChatCompletionsSseState,
    err: ApiError,
) -> Result<(), ApiError> {
    let error_type = compat_error_type(err.status);
    send_sse_json(
        tx,
        "response.failed",
        json!({
            "type": "response.failed",
            "response": {
                "id": state.response_id(),
                "object": "response",
                "created_at": state.created_at,
                "status": "failed",
                "model": state.model,
                "output": state.output_items.clone(),
                "usage": state.token_usage.clone(),
                "error": {
                    "type": error_type,
                    "code": error_type,
                    "message": err.message,
                }
            }
        }),
    )
    .await
}

async fn send_sse_json(
    tx: &mpsc::Sender<Result<Bytes, std::convert::Infallible>>,
    event: &str,
    payload: Value,
) -> Result<(), ApiError> {
    let data = serde_json::to_string(&payload)
        .map_err(|err| ApiError::internal(format!("failed to encode SSE payload: {err}")))?;
    let chunk = format!("event: {event}\ndata: {data}\n\n");
    tx.send(Ok(Bytes::from(chunk)))
        .await
        .map_err(|err| ApiError::internal(format!("failed to send translated SSE chunk: {err}")))
}

pub(super) fn chat_usage_json(usage: ChatUsage) -> Value {
    json!({
        "input_tokens": usage.prompt_tokens,
        "input_tokens_details": usage.prompt_tokens_details.map(|details| json!({
            "cached_tokens": details.cached_tokens,
        })),
        "output_tokens": usage.completion_tokens,
        "output_tokens_details": usage.completion_tokens_details.map(|details| json!({
            "reasoning_tokens": details.reasoning_tokens,
        })),
        "total_tokens": usage.total_tokens,
    })
}

fn compat_error_type(status: reqwest::StatusCode) -> &'static str {
    match status {
        reqwest::StatusCode::BAD_REQUEST => "invalid_request_error",
        reqwest::StatusCode::BAD_GATEWAY => "api_connection_error",
        _ => "server_error",
    }
}

fn with_item_id(mut item: Value, id: String) -> Value {
    if let Some(object) = item.as_object_mut() {
        object.insert("id".to_string(), Value::String(id));
    }
    item
}

pub(super) fn chat_message_text(content: Option<&Value>) -> Option<String> {
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
