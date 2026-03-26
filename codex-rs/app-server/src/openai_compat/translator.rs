use std::collections::HashSet;

use codex_api::ResponsesApiRequest;
use codex_api::chat_completions;
use codex_protocol::models::ContentItem;
use codex_protocol::models::LocalShellAction;
use codex_protocol::models::LocalShellExecAction;
use codex_protocol::models::LocalShellStatus;
use codex_protocol::models::ResponseItem;
use serde::Deserialize;
use serde_json::Value;
use serde_json::json;

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
    if request.stream {
        return Err(ApiError::bad_request(
            "current upstream provider uses wire_api = \"chat\"; streamed /v1/responses translation is not available yet, use /v1/chat/completions for streaming",
        ));
    }

    let chat_request = chat_completions::build_request_with_stream(&request, /*stream*/ false)
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
        let usage = completion.usage.map(|usage| {
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
        });

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
        let name = tool_call.function.name.clone();
        let call_id = tool_call.id.clone();
        let arguments = tool_call.function.arguments.clone();

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
        Value::Object(mut object) => Ok(object
            .remove("arguments")
            .unwrap_or_else(|| Value::Object(object))),
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
struct ChatChoice {
    #[serde(default)]
    message: Option<ChatMessage>,
}

#[derive(Debug, Deserialize)]
struct ChatMessage {
    #[serde(default)]
    content: Option<Value>,
    #[serde(default)]
    tool_calls: Vec<ChatToolCall>,
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
