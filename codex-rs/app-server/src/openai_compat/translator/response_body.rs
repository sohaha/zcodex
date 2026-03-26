use codex_protocol::models::ContentItem;
use codex_protocol::models::ResponseItem;
use serde_json::Value;
use serde_json::json;

use super::ApiError;
use super::ChatCompletionsResponseTranslator;
use super::chat_types::ChatChoice;
use super::chat_types::ChatCompletionResponse;

impl ChatCompletionsResponseTranslator {
    pub(super) fn translate_success_response_body(&self, body: &str) -> Result<String, ApiError> {
        let completion = serde_json::from_str::<ChatCompletionResponse>(body).map_err(|err| {
            ApiError::bad_gateway(format!(
                "failed to decode upstream /v1/chat/completions response for /v1/responses compatibility: {err}"
            ))
        })?;

        let mut output = Vec::new();
        for choice in &completion.choices {
            output.extend(self.choice_output_items(choice)?);
        }
        let usage = completion
            .usage
            .map(super::response_stream::chat_usage_json);

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

        if let Some(text) = super::response_stream::chat_message_text(message.content.as_ref())
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
}
