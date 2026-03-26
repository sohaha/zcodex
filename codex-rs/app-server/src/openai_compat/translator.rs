use std::collections::BTreeMap;
use std::collections::HashSet;

use codex_api::ResponsesApiRequest;
use codex_api::chat_completions;
use codex_protocol::models::ContentItem;
use codex_protocol::models::LocalShellAction;
use codex_protocol::models::LocalShellExecAction;
use codex_protocol::models::LocalShellStatus;
use codex_protocol::models::ResponseItem;
use eventsource_stream::Eventsource;
use futures::StreamExt;
use reqwest::header::HeaderMap;
use serde::Deserialize;
use serde_json::Value;
use serde_json::json;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::bytes::Bytes;

use super::ApiError;
use super::adapter::CompatEndpoint;

#[derive(Clone)]
pub(super) enum UpstreamTranslator {
    Passthrough,
    ResponsesToChat,
}

#[derive(Clone)]
pub(super) enum ResponseTranslation {
    Passthrough,
    ChatCompletionsToResponses(ChatCompletionsResponseTranslator),
}

#[derive(Clone)]
pub(super) struct ChatCompletionsResponseTranslator {
    custom_tool_names: HashSet<String>,
    tool_search_tool_names: HashSet<String>,
    local_shell_tool_names: HashSet<String>,
}

pub(super) struct TranslatedRequest {
    pub(super) body: Option<String>,
    pub(super) response_translation: ResponseTranslation,
}

impl UpstreamTranslator {
    pub(super) fn passthrough() -> Self {
        Self::Passthrough
    }

    pub(super) fn translate_request(
        &self,
        endpoint: CompatEndpoint,
        body: Option<String>,
    ) -> Result<TranslatedRequest, ApiError> {
        match self {
            Self::Passthrough => Ok(TranslatedRequest {
                body,
                response_translation: ResponseTranslation::Passthrough,
            }),
            Self::ResponsesToChat => translate_responses_to_chat(endpoint, body),
        }
    }
}

impl ResponseTranslation {
    pub(super) fn translate_success_response_body(
        &self,
        body: &str,
    ) -> Result<Option<String>, ApiError> {
        match self {
            Self::Passthrough => Ok(None),
            Self::ChatCompletionsToResponses(translator) => {
                translator.translate_success_response_body(body).map(Some)
            }
        }
    }

    pub(super) fn should_translate_stream(&self, headers: &HeaderMap) -> bool {
        matches!(self, Self::ChatCompletionsToResponses(_))
            && headers
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| value.starts_with("text/event-stream"))
    }

    pub(super) fn translate_success_response_stream(
        self,
        upstream: reqwest::Response,
    ) -> ReceiverStream<Result<Bytes, std::convert::Infallible>> {
        let (tx, rx) = mpsc::channel(32);
        tokio::spawn(async move {
            let Self::ChatCompletionsToResponses(translator) = self else {
                return;
            };
            translator
                .translate_success_response_stream(upstream, tx)
                .await;
        });
        ReceiverStream::new(rx)
    }
}

fn translate_responses_to_chat(
    endpoint: CompatEndpoint,
    body: Option<String>,
) -> Result<TranslatedRequest, ApiError> {
    if endpoint != CompatEndpoint::Responses {
        return Ok(TranslatedRequest {
            body,
            response_translation: ResponseTranslation::Passthrough,
        });
    }

    let body = body.ok_or_else(|| ApiError::bad_request("missing JSON request body"))?;
    let request = serde_json::from_str::<ResponsesApiRequest>(&body).map_err(|err| {
        ApiError::bad_request(format!(
            "failed to decode /v1/responses request for chat upstream translation: {err}"
        ))
    })?;

    let chat_request = chat_completions::build_request_with_stream(&request, request.stream)
        .map_err(|err| {
            ApiError::bad_request(format!(
                "failed to translate /v1/responses request into /v1/chat/completions: {err}"
            ))
        })?;
    let translated_body = serde_json::to_string(&chat_request.body).map_err(|err| {
        ApiError::internal(format!(
            "failed to encode translated /v1/chat/completions request body: {err}"
        ))
    })?;

    Ok(TranslatedRequest {
        body: Some(translated_body),
        response_translation: ResponseTranslation::ChatCompletionsToResponses(
            ChatCompletionsResponseTranslator {
                custom_tool_names: chat_request.custom_tool_names,
                tool_search_tool_names: chat_request.tool_search_tool_names,
                local_shell_tool_names: chat_request.local_shell_tool_names,
            },
        ),
    })
}

impl ChatCompletionsResponseTranslator {
    fn translate_success_response_body(&self, body: &str) -> Result<String, ApiError> {
        let completion = serde_json::from_str::<ChatCompletionResponse>(body).map_err(|err| {
            ApiError::bad_gateway(format!(
                "failed to decode upstream /v1/chat/completions response for /v1/responses compatibility: {err}"
            ))
        })?;

        let mut output = Vec::new();
        for choice in &completion.choices {
            output.extend(self.choice_output_items(choice)?);
        }
        let usage = completion.usage.map(chat_usage_json);

        serde_json::to_string(&json!({
            "id": completion.id,
            "object": "response",
            "created_at": completion.created,
            "status": "completed",
            "model": completion.model,
            "output": output,
            "usage": usage,
        }))
        .map_err(|err| ApiError::internal(format!("failed to encode translated response: {err}")))
    }

    async fn translate_success_response_stream(
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

    fn choice_output_items(&self, choice: &ChatChoice) -> Result<Vec<Value>, ApiError> {
        let mut items = Vec::new();
        let Some(message) = choice.message.as_ref() else {
            return Ok(items);
        };

        if let Some(text) = chat_message_text(message.content.as_ref())
            && !text.is_empty()
        {
            items.push(
                serde_json::to_value(ResponseItem::Message {
                    id: None,
                    role: "assistant".to_string(),
                    content: vec![ContentItem::OutputText { text }],
                    end_turn: None,
                    phase: None,
                })
                .map_err(|err| {
                    ApiError::internal(format!("failed to encode message item: {err}"))
                })?,
            );
        }

        for tool_call in &message.tool_calls {
            let item = self.tool_call_response_item(tool_call)?;
            items.push(serde_json::to_value(item).map_err(|err| {
                ApiError::internal(format!("failed to encode tool call item: {err}"))
            })?);
        }

        Ok(items)
    }

    fn tool_call_response_item(&self, tool_call: &ChatToolCall) -> Result<ResponseItem, ApiError> {
        self.tool_call_response_item_from_parts(
            tool_call.id.clone(),
            tool_call.function.name.clone(),
            tool_call.function.arguments.clone(),
        )
    }

    fn tool_call_response_item_from_parts(
        &self,
        call_id: String,
        name: String,
        arguments: String,
    ) -> Result<ResponseItem, ApiError> {
        if self.custom_tool_names.contains(&name) {
            return Ok(ResponseItem::CustomToolCall {
                id: None,
                status: None,
                call_id,
                name,
                input: parse_custom_tool_input(&arguments),
            });
        }

        if self.tool_search_tool_names.contains(&name) {
            return Ok(ResponseItem::ToolSearchCall {
                id: None,
                call_id: Some(call_id),
                status: None,
                execution: "client".to_string(),
                arguments: parse_tool_search_arguments_value(&arguments)?,
            });
        }

        if self.local_shell_tool_names.contains(&name) {
            let params = parse_local_shell_arguments(&arguments)?;
            return Ok(ResponseItem::LocalShellCall {
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
            });
        }

        Ok(ResponseItem::FunctionCall {
            id: None,
            name,
            namespace: None,
            arguments,
            call_id,
        })
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
                    "model": self.model.clone(),
                    "output": [],
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
            send_sse_json(
                tx,
                "response.output_item.done",
                json!({
                    "type": "response.output_item.done",
                    "output_index": 0,
                    "item": {
                        "id": self.message_item_id(),
                        "type": "message",
                        "role": "assistant",
                        "content": [{
                            "type": "output_text",
                            "text": self.output_text,
                        }],
                    }
                }),
            )
            .await?;
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
                "response.output_item.done",
                json!({
                    "type": "response.output_item.done",
                    "output_index": self.next_output_index,
                    "item": item,
                }),
            )
            .await?;
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
                    "model": self.model.clone(),
                    "output": [],
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
                "error": {
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

fn chat_usage_json(usage: ChatUsage) -> Value {
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

fn with_item_id(mut item: Value, id: String) -> Value {
    if let Some(object) = item.as_object_mut() {
        object.insert("id".to_string(), Value::String(id));
    }
    item
}

fn chat_message_text(content: Option<&Value>) -> Option<String> {
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

fn parse_custom_tool_input(arguments: &str) -> String {
    serde_json::from_str::<Value>(arguments)
        .ok()
        .and_then(|value| {
            value
                .get("input")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| arguments.to_string())
}

fn parse_tool_search_arguments_value(arguments: &str) -> Result<Value, ApiError> {
    let value = serde_json::from_str::<Value>(arguments).map_err(|err| {
        ApiError::bad_gateway(format!(
            "failed to decode tool_search arguments from upstream /v1/chat/completions response: {err}"
        ))
    })?;
    match value {
        Value::Object(mut object) => {
            Ok(object.remove("arguments").unwrap_or(Value::Object(object)))
        }
        value => Ok(value),
    }
}

fn parse_local_shell_arguments(arguments: &str) -> Result<LocalShellArgs, ApiError> {
    serde_json::from_str(arguments).map_err(|err| {
        ApiError::bad_gateway(format!(
            "failed to decode local_shell arguments from upstream /v1/chat/completions response: {err}"
        ))
    })
}

#[derive(Debug, Deserialize)]
struct LocalShellArgs {
    command: Vec<String>,
    #[serde(default)]
    workdir: Option<String>,
    #[serde(default)]
    timeout_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    id: String,
    model: String,
    #[serde(default)]
    created: Option<i64>,
    #[serde(default)]
    choices: Vec<ChatChoice>,
    #[serde(default)]
    usage: Option<ChatUsage>,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionChunk {
    id: String,
    model: String,
    #[serde(default)]
    created: Option<i64>,
    #[serde(default)]
    choices: Vec<ChatChoice>,
    #[serde(default)]
    usage: Option<ChatUsage>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    #[serde(default)]
    message: Option<ChatMessage>,
    #[serde(default)]
    delta: Option<ChatDelta>,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatMessage {
    #[serde(default)]
    content: Option<Value>,
    #[serde(default)]
    tool_calls: Vec<ChatToolCall>,
}

#[derive(Debug, Deserialize)]
struct ChatDelta {
    #[serde(default)]
    content: Option<Value>,
    #[serde(default)]
    tool_calls: Vec<ChatToolCallDelta>,
}

#[derive(Debug, Deserialize)]
struct ChatToolCall {
    id: String,
    function: ChatToolFunction,
}

#[derive(Debug, Deserialize)]
struct ChatToolFunction {
    name: String,
    arguments: String,
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
