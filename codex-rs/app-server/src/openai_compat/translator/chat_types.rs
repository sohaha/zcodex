use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub(super) struct ChatCompletionResponse {
    pub(super) id: String,
    pub(super) model: String,
    #[serde(default)]
    pub(super) created: Option<i64>,
    #[serde(default)]
    pub(super) choices: Vec<ChatChoice>,
    #[serde(default)]
    pub(super) usage: Option<ChatUsage>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ChatCompletionChunk {
    pub(super) id: String,
    pub(super) model: String,
    #[serde(default)]
    pub(super) created: Option<i64>,
    #[serde(default)]
    pub(super) choices: Vec<ChatChoice>,
    #[serde(default)]
    pub(super) usage: Option<ChatUsage>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ChatChoice {
    #[serde(default)]
    pub(super) message: Option<ChatMessage>,
    #[serde(default)]
    pub(super) delta: Option<ChatDelta>,
    #[serde(default)]
    pub(super) finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ChatMessage {
    #[serde(default)]
    pub(super) content: Option<Value>,
    #[serde(default)]
    pub(super) tool_calls: Vec<ChatToolCall>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ChatDelta {
    #[serde(default)]
    pub(super) content: Option<Value>,
    #[serde(default)]
    pub(super) tool_calls: Vec<ChatToolCallDelta>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ChatToolCall {
    pub(super) id: String,
    pub(super) function: ChatToolFunction,
}

#[derive(Debug, Deserialize)]
pub(super) struct ChatToolFunction {
    pub(super) name: String,
    pub(super) arguments: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct ChatToolCallDelta {
    #[serde(default)]
    pub(super) index: Option<u32>,
    #[serde(default)]
    pub(super) id: Option<String>,
    #[serde(default)]
    pub(super) function: Option<ChatToolFunctionDelta>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ChatToolFunctionDelta {
    #[serde(default)]
    pub(super) name: Option<String>,
    #[serde(default)]
    pub(super) arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ChatUsage {
    pub(super) prompt_tokens: i64,
    #[serde(default)]
    pub(super) prompt_tokens_details: Option<ChatPromptTokensDetails>,
    pub(super) completion_tokens: i64,
    #[serde(default)]
    pub(super) completion_tokens_details: Option<ChatCompletionTokensDetails>,
    pub(super) total_tokens: i64,
}

#[derive(Debug, Deserialize)]
pub(super) struct ChatPromptTokensDetails {
    #[serde(default)]
    pub(super) cached_tokens: i64,
}

#[derive(Debug, Deserialize)]
pub(super) struct ChatCompletionTokensDetails {
    #[serde(default)]
    pub(super) reasoning_tokens: i64,
}
