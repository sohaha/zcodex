use crate::auth::AuthProvider;
use crate::chat_completions;
use crate::common::ResponseStream;
use crate::common::ResponsesApiRequest;
use crate::endpoint::responses::ResponsesOptions;
use crate::endpoint::session::EndpointSession;
use crate::error::ApiError;
use crate::provider::Provider;
use crate::requests::headers::build_conversation_headers;
use crate::requests::headers::insert_header;
use crate::requests::headers::subagent_header;
use crate::telemetry::SseTelemetry;
use codex_client::HttpTransport;
use codex_client::RequestTelemetry;
use http::HeaderMap;
use http::HeaderValue;
use http::Method;
use std::sync::Arc;
use tracing::instrument;

pub struct ChatCompletionsClient<T: HttpTransport, A: AuthProvider> {
    session: EndpointSession<T, A>,
    sse_telemetry: Option<Arc<dyn SseTelemetry>>,
}

impl<T: HttpTransport, A: AuthProvider> ChatCompletionsClient<T, A> {
    pub fn new(transport: T, provider: Provider, auth: A) -> Self {
        Self {
            session: EndpointSession::new(transport, provider, auth),
            sse_telemetry: None,
        }
    }

    pub fn with_telemetry(
        self,
        request: Option<Arc<dyn RequestTelemetry>>,
        sse: Option<Arc<dyn SseTelemetry>>,
    ) -> Self {
        Self {
            session: self.session.with_request_telemetry(request),
            sse_telemetry: sse,
        }
    }

    #[instrument(
        name = "chat_completions.stream_request",
        level = "info",
        skip_all,
        fields(
            transport = "chat_completions_http",
            http.method = "POST",
            api.path = "chat/completions"
        )
    )]
    pub async fn stream_request(
        &self,
        request: ResponsesApiRequest,
        options: ResponsesOptions,
    ) -> Result<ResponseStream, ApiError> {
        let ResponsesOptions {
            conversation_id,
            session_source,
            mut extra_headers,
            compression: _,
            turn_state,
        } = options;

        if let Some(ref conv_id) = conversation_id {
            insert_header(&mut extra_headers, "x-client-request-id", conv_id);
        }
        extra_headers.extend(build_conversation_headers(conversation_id));
        if let Some(subagent) = subagent_header(&session_source) {
            insert_header(&mut extra_headers, "x-openai-subagent", &subagent);
        }

        let chat_request = chat_completions::build_request(&request)?;
        let stream_response = self
            .session
            .stream_with(
                Method::POST,
                "chat/completions",
                extra_headers,
                Some(chat_request.body),
                |req| {
                    req.headers.insert(
                        http::header::ACCEPT,
                        HeaderValue::from_static("text/event-stream"),
                    );
                },
            )
            .await?;

        Ok(chat_completions::spawn_response_stream(
            stream_response,
            self.session.provider().stream_idle_timeout,
            self.sse_telemetry.clone(),
            turn_state,
            chat_request.custom_tool_names,
            chat_request.tool_search_tool_names,
            chat_request.local_shell_tool_names,
        ))
    }

    #[allow(dead_code)]
    pub async fn stream(
        &self,
        body: serde_json::Value,
        extra_headers: HeaderMap,
        turn_state: Option<Arc<std::sync::OnceLock<String>>>,
    ) -> Result<ResponseStream, ApiError> {
        let stream_response = self
            .session
            .stream_with(
                Method::POST,
                "chat/completions",
                extra_headers,
                Some(body),
                |req| {
                    req.headers.insert(
                        http::header::ACCEPT,
                        HeaderValue::from_static("text/event-stream"),
                    );
                },
            )
            .await?;

        Ok(chat_completions::spawn_response_stream(
            stream_response,
            self.session.provider().stream_idle_timeout,
            self.sse_telemetry.clone(),
            turn_state,
            Default::default(),
            Default::default(),
            Default::default(),
        ))
    }
}
