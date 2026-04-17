use crate::anthropic;
use crate::auth::AuthProvider;
use crate::common::CompactionInput;
use crate::common::ResponsesApiRequest;
use crate::common::create_text_param_for_request;
use crate::endpoint::session::EndpointSession;
use crate::error::ApiError;
use crate::provider::Provider;
use crate::provider::WireApi;
use codex_client::HttpTransport;
use codex_client::RequestTelemetry;
use codex_protocol::models::ResponseItem;
use http::HeaderMap;
use http::HeaderValue;
use http::Method;
use http::header::ACCEPT;
use http::header::CONTENT_TYPE;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use serde_json::json;
use serde_json::to_value;
use std::sync::Arc;

pub struct CompactClient<T: HttpTransport, A: AuthProvider> {
    session: EndpointSession<T, A>,
}

impl<T: HttpTransport, A: AuthProvider> CompactClient<T, A> {
    pub fn new(transport: T, provider: Provider, auth: A) -> Self {
        Self {
            session: EndpointSession::new(transport, provider, auth),
        }
    }

    pub fn with_telemetry(self, request: Option<Arc<dyn RequestTelemetry>>) -> Self {
        Self {
            session: self.session.with_request_telemetry(request),
        }
    }

    fn path() -> &'static str {
        "responses/compact"
    }

    pub async fn compact(
        &self,
        body: serde_json::Value,
        extra_headers: HeaderMap,
    ) -> Result<Vec<ResponseItem>, ApiError> {
        let resp = self
            .session
            .execute(Method::POST, Self::path(), extra_headers, Some(body))
            .await?;
        let parsed: CompactHistoryResponse =
            serde_json::from_slice(&resp.body).map_err(|e| ApiError::Stream(e.to_string()))?;
        Ok(parsed.output)
    }

    pub async fn compact_input(
        &self,
        input: &CompactionInput<'_>,
        extra_headers: HeaderMap,
    ) -> Result<Vec<ResponseItem>, ApiError> {
        if self.session.provider().wire_api == WireApi::Anthropic {
            return self.compact_input_anthropic(input, extra_headers).await;
        }

        let body = to_value(input)
            .map_err(|e| ApiError::Stream(format!("failed to encode compaction input: {e}")))?;
        self.compact(body, extra_headers).await
    }

    async fn compact_input_anthropic(
        &self,
        input: &CompactionInput<'_>,
        extra_headers: HeaderMap,
    ) -> Result<Vec<ResponseItem>, ApiError> {
        let request = ResponsesApiRequest {
            model: input.model.to_string(),
            instructions: input.instructions.to_string(),
            input: input.input.to_vec(),
            tools: Vec::new(),
            tool_choice: "auto".to_string(),
            parallel_tool_calls: false,
            reasoning: None,
            store: false,
            stream: false,
            include: Vec::new(),
            service_tier: None,
            client_metadata: None,
            prompt_cache_key: None,
            text: create_text_param_for_request(
               /*verbosity*/ None,
               &Some(compaction_output_schema()),
           ),
           max_output_tokens: None,
       };
       let body = anthropic::build_request_body_with_stream(&request, /*stream*/ false);
        let resp = self
            .session
            .execute_with(Method::POST, "messages", extra_headers, Some(body), |req| {
                req.headers
                    .insert(ACCEPT, HeaderValue::from_static("application/json"));
                req.headers
                    .insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
            })
            .await?;
        let content = match serde_json::from_slice::<AnthropicMessageResponse>(&resp.body) {
            Ok(parsed) => parsed.content,
            Err(_) => {
                // Some providers (e.g. MiniMax) return SSE even when stream:false is requested.
                // Fall back to extracting text from SSE data lines.
                let body_str = String::from_utf8_lossy(&resp.body);
                let mut blocks = Vec::new();
                for line in body_str.lines() {
                    let data = line.strip_prefix("data:").map(str::trim).unwrap_or("");
                    if data.is_empty() || data == "[DONE]" {
                        continue;
                    }
                    if let Ok(chunk) = serde_json::from_str::<serde_json::Value>(data) {
                        if let Some(delta) = chunk
                            .get("delta")
                            .and_then(|d| d.get("text"))
                            .and_then(|t| t.as_str())
                        {
                            if !delta.is_empty() {
                                blocks.push(AnthropicContentBlock {
                                    kind: "text".to_string(),
                                    text: Some(delta.to_string()),
                                });
                            }
                        }
                    }
                }
                if blocks.is_empty() {
                    let preview = String::from_utf8_lossy(&resp.body[..resp.body.len().min(256)]);
                    return Err(ApiError::Stream(format!(
                        "could not parse compaction response as json or sse; body preview: {preview}"
                    )));
                }
                blocks
            }
        };
        let summary = extract_compaction_summary(content)?;
        Ok(vec![ResponseItem::Compaction {
            encrypted_content: summary,
        }])
    }
}

#[derive(Debug, Deserialize)]
struct CompactHistoryResponse {
    output: Vec<ResponseItem>,
}

#[derive(Debug, Deserialize)]
struct AnthropicMessageResponse {
    content: Vec<AnthropicContentBlock>,
}

#[derive(Debug, Deserialize)]
struct AnthropicContentBlock {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    text: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct AnthropicCompactResponse {
    summary: String,
}

fn compaction_output_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "summary": { "type": "string" }
        },
        "required": ["summary"],
        "additionalProperties": false
    })
}

fn extract_compaction_summary(content: Vec<AnthropicContentBlock>) -> Result<String, ApiError> {
    let text = content
        .into_iter()
        .filter(|block| block.kind == "text")
        .filter_map(|block| block.text)
        .collect::<Vec<_>>()
        .join("");
    if text.trim().is_empty() {
        return Err(ApiError::Stream(
            "anthropic compaction response did not include text output".to_string(),
        ));
    }
    let parsed: AnthropicCompactResponse =
        serde_json::from_str(&text).map_err(|e| ApiError::Stream(e.to_string()))?;
    let summary = parsed.summary.trim();
    Ok(if summary.is_empty() {
        "(no summary available)".to_string()
    } else {
        summary.to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use codex_client::Request;
    use codex_client::Response;
    use codex_client::StreamResponse;
    use codex_client::TransportError;
    use http::StatusCode;
    use pretty_assertions::assert_eq;
    use serde_json::json;
    use std::sync::Mutex;
    use std::time::Duration;

    #[derive(Clone, Default)]
    struct DummyTransport;

    #[async_trait]
    impl HttpTransport for DummyTransport {
        async fn execute(&self, _req: Request) -> Result<Response, TransportError> {
            Err(TransportError::Build("execute should not run".to_string()))
        }

        async fn stream(&self, _req: Request) -> Result<StreamResponse, TransportError> {
            Err(TransportError::Build("stream should not run".to_string()))
        }
    }

    #[derive(Clone, Default)]
    struct DummyAuth;

    impl AuthProvider for DummyAuth {
        fn add_auth_headers(&self, _headers: &mut HeaderMap) {}
    }

    #[derive(Clone)]
    struct CapturingTransport {
        last_request: Arc<Mutex<Option<Request>>>,
        response_body: Arc<Vec<u8>>,
    }

    impl CapturingTransport {
        fn new(response_body: Vec<u8>) -> Self {
            Self {
                last_request: Arc::new(Mutex::new(None)),
                response_body: Arc::new(response_body),
            }
        }
    }

    #[async_trait]
    impl HttpTransport for CapturingTransport {
        async fn execute(&self, req: Request) -> Result<Response, TransportError> {
            *self.last_request.lock().expect("lock request store") = Some(req);
            Ok(Response {
                status: StatusCode::OK,
                headers: HeaderMap::new(),
                body: self.response_body.as_ref().clone().into(),
            })
        }

        async fn stream(&self, _req: Request) -> Result<StreamResponse, TransportError> {
            Err(TransportError::Build("stream should not run".to_string()))
        }
    }

    #[test]
    fn path_is_responses_compact() {
        assert_eq!(
            CompactClient::<DummyTransport, DummyAuth>::path(),
            "responses/compact"
        );
    }

    fn provider(base_url: &str, wire_api: WireApi) -> Provider {
        Provider {
            name: "test".to_string(),
            base_url: base_url.to_string(),
            wire_api,
            query_params: None,
            headers: HeaderMap::new(),
            retry: crate::provider::RetryConfig {
                max_attempts: 1,
                base_delay: Duration::from_millis(1),
                retry_429: false,
                retry_5xx: true,
                retry_transport: true,
            },
            stream_idle_timeout: Duration::from_secs(1),
        }
    }

    #[tokio::test]
    async fn anthropic_compact_input_posts_to_messages_and_parses_summary() {
        let transport = CapturingTransport::new(
            serde_json::to_vec(&json!({
                "content": [
                    {
                        "type": "text",
                        "text": "{\"summary\":\"condensed thread\"}"
                    }
                ]
            }))
            .expect("serialize response"),
        );
        let client = CompactClient::new(
            transport.clone(),
            provider("https://api.anthropic.com/v1", WireApi::Anthropic),
            DummyAuth,
        );
        let input = CompactionInput {
            model: "claude-test",
            input: &[],
            instructions: "compact this thread",
            tools: Vec::new(),
            parallel_tool_calls: false,
            reasoning: None,
            text: None,
        };

        let output = client
            .compact_input(&input, HeaderMap::new())
            .await
            .expect("anthropic compaction should succeed");
        assert_eq!(
            output,
            vec![ResponseItem::Compaction {
                encrypted_content: "condensed thread".to_string(),
            }]
        );

        let request = transport
            .last_request
            .lock()
            .expect("lock request store")
            .clone()
            .expect("request should be captured");
        assert_eq!(request.method, Method::POST);
        assert_eq!(request.url, "https://api.anthropic.com/v1/messages");
        let body = request.body.expect("request body should be present");
        let json = body.json().expect("request body should be JSON");
        assert_eq!(json["model"], "claude-test");
        assert_eq!(json["stream"], false);
        assert!(json.get("tools").is_none());
        let system = json["system"]
            .as_str()
            .expect("system prompt should be a string");
        assert!(system.starts_with("compact this thread\n\nRespond with JSON only."));
        assert!(system.contains("\"summary\":{\"type\":\"string\"}"));
    }
}
