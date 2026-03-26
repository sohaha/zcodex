use std::collections::HashSet;

use codex_api::ResponsesApiRequest;
use codex_api::chat_completions;
use codex_protocol::models::LocalShellAction;
use codex_protocol::models::LocalShellExecAction;
use codex_protocol::models::LocalShellStatus;
use codex_protocol::models::ResponseItem;
use reqwest::header::HeaderMap;
use serde_json::Value;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::bytes::Bytes;

use super::ApiError;
use super::adapter::CompatEndpoint;

mod chat_types;
mod response_body;
mod response_stream;

use chat_types::LocalShellArgs;

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
    fn tool_call_response_item(
        &self,
        tool_call: &chat_types::ChatToolCall,
    ) -> Result<ResponseItem, ApiError> {
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
