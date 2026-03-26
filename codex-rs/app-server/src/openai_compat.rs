use std::collections::HashMap;
use std::convert::Infallible;
use std::io::Error as IoError;
use std::io::ErrorKind;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use axum::Router;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::Response;
use axum::response::Sse;
use axum::response::sse::Event;
use axum::response::sse::KeepAlive;
use axum::routing::get;
use axum::routing::post;
use clap::Args;
use codex_app_server_protocol::ApprovalsReviewer;
use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::ConfigWarningNotification;
use codex_app_server_protocol::InitializeCapabilities;
use codex_app_server_protocol::InitializeParams;
use codex_app_server_protocol::JSONRPCErrorError;
use codex_app_server_protocol::ModelListParams;
use codex_app_server_protocol::ModelListResponse;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::SandboxMode;
use codex_app_server_protocol::ServerNotification;
use codex_app_server_protocol::ThreadItem;
use codex_app_server_protocol::ThreadReadParams;
use codex_app_server_protocol::ThreadReadResponse;
use codex_app_server_protocol::ThreadStartParams;
use codex_app_server_protocol::ThreadStartResponse;
use codex_app_server_protocol::TurnCompletedNotification;
use codex_app_server_protocol::TurnError;
use codex_app_server_protocol::TurnStartParams;
use codex_app_server_protocol::TurnStartResponse;
use codex_arg0::Arg0DispatchPaths;
use codex_cloud_requirements::cloud_requirements_loader;
use codex_core::AuthManager;
use codex_core::check_execpolicy_for_warnings;
use codex_core::config::Config;
use codex_core::config::ConfigBuilder;
use codex_core::config_loader::CloudRequirementsLoader;
use codex_core::config_loader::LoaderOverrides;
use codex_feedback::CodexFeedback;
use codex_protocol::config_types::ReasoningSummary;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ResponseItem;
use codex_protocol::openai_models::ReasoningEffort;
use codex_protocol::protocol::SandboxPolicy;
use codex_protocol::protocol::SessionSource;
use codex_utils_cli::CliConfigOverrides;
use futures::stream;
use serde_json::Value;
use serde_json::json;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use toml::Value as TomlValue;
use tracing::info;
use tracing::warn;

use crate::in_process::DEFAULT_IN_PROCESS_CHANNEL_CAPACITY;
use crate::in_process::InProcessClientHandle;
use crate::in_process::InProcessServerEvent;
use crate::in_process::InProcessStartArgs;
use crate::in_process::start;

#[derive(Debug, Clone, Args)]
pub struct OpenAiCompatServerArgs {
    /// HTTP 监听地址。默认仅监听本机回环地址。
    #[arg(long = "listen", default_value = "127.0.0.1:8080")]
    pub listen: SocketAddr,

    /// 若设置，则要求请求携带匹配的 `Authorization: Bearer <token>`。
    #[arg(long = "auth-token-env", value_name = "ENV")]
    pub auth_token_env: Option<String>,
}

#[derive(Clone)]
struct OpenAiCompatState {
    runtime: Arc<EmbeddedRuntimeConfig>,
    auth_token: Option<Arc<str>>,
}

#[derive(Clone)]
struct EmbeddedRuntimeConfig {
    arg0_paths: Arg0DispatchPaths,
    config: Arc<Config>,
    cli_kv_overrides: Vec<(String, TomlValue)>,
    loader_overrides: LoaderOverrides,
    cloud_requirements: CloudRequirementsLoader,
    config_warnings: Vec<ConfigWarningNotification>,
}

impl EmbeddedRuntimeConfig {
    fn start_args(&self) -> InProcessStartArgs {
        InProcessStartArgs {
            arg0_paths: self.arg0_paths.clone(),
            config: self.config.clone(),
            cli_overrides: self.cli_kv_overrides.clone(),
            loader_overrides: self.loader_overrides.clone(),
            cloud_requirements: self.cloud_requirements.clone(),
            feedback: CodexFeedback::new(),
            config_warnings: self.config_warnings.clone(),
            session_source: SessionSource::Cli,
            enable_codex_api_key_env: false,
            initialize: InitializeParams {
                client_info: codex_app_server_protocol::ClientInfo {
                    name: "codex-openai-compat".to_string(),
                    title: None,
                    version: env!("CARGO_PKG_VERSION").to_string(),
                },
                capabilities: Some(InitializeCapabilities {
                    experimental_api: true,
                    opt_out_notification_methods: None,
                }),
            },
            channel_capacity: DEFAULT_IN_PROCESS_CHANNEL_CAPACITY,
        }
    }
}

#[derive(Debug)]
struct RequestPlan {
    model: Option<String>,
    base_instructions: Option<String>,
    developer_instructions: Option<String>,
    input: Vec<codex_app_server_protocol::UserInput>,
    effort: Option<ReasoningEffort>,
    summary: Option<ReasoningSummary>,
}

#[derive(Debug)]
struct TurnOutcome {
    response_id: String,
    created_at: i64,
    model: String,
    items: Vec<ResponseItem>,
    final_error: Option<TurnError>,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message: message.into(),
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.into(),
        }
    }
    fn to_response(&self) -> Response {
        (
            self.status,
            axum::Json(json!({
                "error": {
                    "message": self.message,
                    "type": status_error_type(self.status),
                }
            })),
        )
            .into_response()
    }
}

fn status_error_type(status: StatusCode) -> &'static str {
    match status {
        StatusCode::BAD_REQUEST => "invalid_request_error",
        StatusCode::UNAUTHORIZED => "authentication_error",
        _ => "server_error",
    }
}

pub async fn run_openai_compat_server(
    arg0_paths: Arg0DispatchPaths,
    cli_config_overrides: CliConfigOverrides,
    loader_overrides: LoaderOverrides,
    args: OpenAiCompatServerArgs,
) -> std::io::Result<()> {
    let runtime = Arc::new(
        build_runtime_config(arg0_paths, cli_config_overrides, loader_overrides)
            .await
            .map_err(to_io_error)?,
    );
    let auth_token = read_auth_token(args.auth_token_env.as_deref()).map_err(to_io_error)?;
    let state = OpenAiCompatState {
        runtime,
        auth_token: auth_token.map(Arc::<str>::from),
    };

    let router = Router::new()
        .route("/v1/models", get(models_handler))
        .route("/v1/responses", post(responses_handler))
        .route("/v1/chat/completions", post(chat_completions_handler))
        .with_state(state);

    let listener = TcpListener::bind(args.listen).await?;
    info!(listen = %args.listen, "openai-compatible HTTP server listening");
    axum::serve(
        listener,
        router.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .map_err(|err| IoError::other(err.to_string()))
}

async fn models_handler(State(state): State<OpenAiCompatState>, headers: HeaderMap) -> Response {
    if let Err(err) = authorize(&state, &headers) {
        return err.to_response();
    }

    match list_models(&state.runtime).await {
        Ok(models) => axum::Json(json!({
            "object": "list",
            "data": models,
        }))
        .into_response(),
        Err(err) => err.to_response(),
    }
}

async fn responses_handler(
    State(state): State<OpenAiCompatState>,
    headers: HeaderMap,
    axum::Json(body): axum::Json<Value>,
) -> Response {
    if let Err(err) = authorize(&state, &headers) {
        return err.to_response();
    }

    let stream_requested = body.get("stream").and_then(Value::as_bool).unwrap_or(false);
    if stream_requested {
        return responses_streaming(state, body).await;
    }

    let plan = match plan_from_responses_request(&body) {
        Ok(plan) => plan,
        Err(err) => return err.to_response(),
    };

    match execute_turn(&state.runtime, plan, None).await {
        Ok(outcome) => axum::Json(responses_body_from_outcome(outcome)).into_response(),
        Err(err) => err.to_response(),
    }
}

async fn chat_completions_handler(
    State(state): State<OpenAiCompatState>,
    headers: HeaderMap,
    axum::Json(body): axum::Json<Value>,
) -> Response {
    if let Err(err) = authorize(&state, &headers) {
        return err.to_response();
    }

    let stream_requested = body.get("stream").and_then(Value::as_bool).unwrap_or(false);
    if stream_requested {
        return chat_streaming(state, body).await;
    }

    let plan = match plan_from_chat_request(&body) {
        Ok(plan) => plan,
        Err(err) => return err.to_response(),
    };

    match execute_turn(&state.runtime, plan, None).await {
        Ok(outcome) => axum::Json(chat_completion_body_from_outcome(outcome)).into_response(),
        Err(err) => err.to_response(),
    }
}

async fn responses_streaming(state: OpenAiCompatState, body: Value) -> Response {
    let plan = match plan_from_responses_request(&body) {
        Ok(plan) => plan,
        Err(err) => return err.to_response(),
    };

    let (tx, rx) = mpsc::channel::<Event>(32);
    tokio::spawn(async move {
        if let Err(err) = execute_turn(
            &state.runtime,
            plan,
            Some(StreamEmitter::Responses(tx.clone())),
        )
        .await
        {
            let _ = tx
                .send(sse_json_event(
                    "error",
                    &json!({
                        "type": "error",
                        "error": {
                            "message": err.message,
                            "type": status_error_type(err.status),
                        }
                    }),
                ))
                .await;
        }
        let _ = tx.send(Event::default().data("[DONE]")).await;
    });

    sse_response(rx)
}

async fn chat_streaming(state: OpenAiCompatState, body: Value) -> Response {
    let plan = match plan_from_chat_request(&body) {
        Ok(plan) => plan,
        Err(err) => return err.to_response(),
    };

    let (tx, rx) = mpsc::channel::<Event>(32);
    tokio::spawn(async move {
        if let Err(err) =
            execute_turn(&state.runtime, plan, Some(StreamEmitter::Chat(tx.clone()))).await
        {
            let _ = tx
                .send(sse_json_data(&json!({
                    "error": {
                        "message": err.message,
                        "type": status_error_type(err.status),
                    }
                })))
                .await;
        }
        let _ = tx.send(Event::default().data("[DONE]")).await;
    });

    sse_response(rx)
}

fn sse_response(rx: mpsc::Receiver<Event>) -> Response {
    let stream = stream::unfold(rx, |mut rx| async move {
        rx.recv()
            .await
            .map(|event| (Ok::<Event, Infallible>(event), rx))
    });
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

#[derive(Clone)]
enum StreamEmitter {
    Responses(mpsc::Sender<Event>),
    Chat(mpsc::Sender<Event>),
}

impl StreamEmitter {
    async fn send(&self, event: Event) {
        let sender = match self {
            Self::Responses(sender) | Self::Chat(sender) => sender,
        };
        let _ = sender.send(event).await;
    }
}

async fn execute_turn(
    runtime: &EmbeddedRuntimeConfig,
    plan: RequestPlan,
    stream_emitter: Option<StreamEmitter>,
) -> Result<TurnOutcome, ApiError> {
    let mut client = start(runtime.start_args())
        .await
        .map_err(|err| ApiError::internal(format!("failed to start embedded app-server: {err}")))?;

    let model = plan
        .model
        .clone()
        .or_else(|| runtime.config.model.clone())
        .unwrap_or_else(|| runtime.config.model_provider_id.clone());

    let thread_start: ThreadStartResponse = request_typed(
        &client,
        ClientRequest::ThreadStart {
            request_id: RequestId::Integer(1),
            params: ThreadStartParams {
                model: Some(model.clone()),
                model_provider: None,
                cwd: Some(runtime.config.cwd.to_string_lossy().to_string()),
                approval_policy: Some(runtime.config.permissions.approval_policy.value().into()),
                approvals_reviewer: approvals_reviewer_override_from_config(&runtime.config),
                sandbox: sandbox_mode_from_policy(runtime.config.permissions.sandbox_policy.get()),
                config: config_request_overrides_from_config(&runtime.config),
                base_instructions: plan.base_instructions.clone(),
                developer_instructions: plan.developer_instructions.clone(),
                ephemeral: Some(true),
                experimental_raw_events: true,
                ..ThreadStartParams::default()
            },
        },
    )
    .await?;

    let turn_start: TurnStartResponse = request_typed(
        &client,
        ClientRequest::TurnStart {
            request_id: RequestId::Integer(2),
            params: TurnStartParams {
                thread_id: thread_start.thread.id.clone(),
                input: plan.input,
                cwd: Some(runtime.config.cwd.clone()),
                approval_policy: Some(runtime.config.permissions.approval_policy.value().into()),
                approvals_reviewer: approvals_reviewer_override_from_config(&runtime.config),
                sandbox_policy: Some(
                    runtime
                        .config
                        .permissions
                        .sandbox_policy
                        .get()
                        .clone()
                        .into(),
                ),
                model: Some(model.clone()),
                service_tier: None,
                effort: plan.effort,
                summary: plan.summary,
                personality: None,
                output_schema: None,
                collaboration_mode: None,
            },
        },
    )
    .await?;

    let response_id = format!("resp_{}", turn_start.turn.id);
    let created_at = unix_timestamp_now();
    if let Some(emitter) = stream_emitter.as_ref() {
        match emitter {
            StreamEmitter::Responses(_) => {
                emitter
                    .send(sse_json_event(
                        "response.created",
                        &json!({
                            "type": "response.created",
                            "response": {
                                "id": response_id,
                                "object": "response",
                                "created_at": created_at,
                                "model": model,
                                "status": "in_progress",
                            }
                        }),
                    ))
                    .await;
            }
            StreamEmitter::Chat(_) => {
                emitter
                    .send(sse_json_data(&chat_chunk(
                        &response_id,
                        created_at,
                        &model,
                        json!({"role": "assistant"}),
                        None,
                    )))
                    .await;
            }
        }
    }

    let mut items = Vec::new();
    let mut final_error = None;
    let mut streamed_text = String::new();

    while let Some(event) = client.next_event().await {
        match event {
            InProcessServerEvent::ServerRequest(request) => {
                let _ = client.fail_server_request(
                    request.id().clone(),
                    JSONRPCErrorError {
                        code: -32000,
                        message: "openai-compatible HTTP server does not support interactive server requests; configure approval_policy=never for unattended use".to_string(),
                        data: None,
                    },
                );
            }
            InProcessServerEvent::Lagged { skipped } => {
                warn!(
                    skipped,
                    "dropping lagged in-process events in openai-compatible server"
                );
            }
            InProcessServerEvent::ServerNotification(notification) => match notification {
                ServerNotification::AgentMessageDelta(delta) => {
                    streamed_text.push_str(&delta.delta);
                    if let Some(emitter) = stream_emitter.as_ref() {
                        match emitter {
                            StreamEmitter::Responses(_) => {
                                emitter
                                    .send(sse_json_event(
                                        "response.output_text.delta",
                                        &json!({
                                            "type": "response.output_text.delta",
                                            "delta": delta.delta,
                                        }),
                                    ))
                                    .await;
                            }
                            StreamEmitter::Chat(_) => {
                                emitter
                                    .send(sse_json_data(&chat_chunk(
                                        &response_id,
                                        created_at,
                                        &model,
                                        json!({"content": delta.delta}),
                                        None,
                                    )))
                                    .await;
                            }
                        }
                    }
                }
                ServerNotification::RawResponseItemCompleted(raw) => {
                    items.push(raw.item.clone());
                    if let Some(emitter) = stream_emitter.as_ref() {
                        match emitter {
                            StreamEmitter::Responses(_) => {
                                emitter
                                    .send(sse_json_event(
                                        "response.output_item.done",
                                        &json!({
                                            "type": "response.output_item.done",
                                            "item": raw.item,
                                        }),
                                    ))
                                    .await;
                            }
                            StreamEmitter::Chat(_) => {
                                if let ResponseItem::FunctionCall {
                                    call_id,
                                    name,
                                    arguments,
                                    ..
                                } = raw.item
                                {
                                    emitter
                                        .send(sse_json_data(&chat_chunk(
                                            &response_id,
                                            created_at,
                                            &model,
                                            json!({
                                                "tool_calls": [{
                                                    "index": 0,
                                                    "id": call_id,
                                                    "type": "function",
                                                    "function": {
                                                        "name": name,
                                                        "arguments": arguments,
                                                    }
                                                }]
                                            }),
                                            None,
                                        )))
                                        .await;
                                }
                            }
                        }
                    }
                }
                ServerNotification::TurnCompleted(completed) => {
                    final_error = completed.turn.error.clone();
                    if let Some(emitter) = stream_emitter.as_ref() {
                        emit_completion_event(
                            emitter,
                            &response_id,
                            created_at,
                            &model,
                            &items,
                            &completed,
                        )
                        .await;
                    }
                    break;
                }
                ServerNotification::Error(err) => {
                    final_error = Some(err.error);
                }
                _ => {}
            },
        }
    }

    if items.is_empty()
        && let Ok(thread) = request_typed::<ThreadReadResponse>(
            &client,
            ClientRequest::ThreadRead {
                request_id: RequestId::Integer(3),
                params: ThreadReadParams {
                    thread_id: thread_start.thread.id.clone(),
                    include_turns: true,
                },
            },
        )
        .await
    {
        items.extend(thread_items_to_response_items(
            thread
                .thread
                .turns
                .into_iter()
                .find(|turn| turn.id == turn_start.turn.id)
                .map(|turn| turn.items)
                .unwrap_or_default(),
        ));
    }

    client.shutdown().await.map_err(|err| {
        ApiError::internal(format!("failed to shutdown embedded app-server: {err}"))
    })?;

    if items.is_empty() && !streamed_text.is_empty() {
        items.push(ResponseItem::Message {
            id: None,
            role: "assistant".to_string(),
            content: vec![ContentItem::OutputText {
                text: streamed_text,
            }],
            end_turn: Some(true),
            phase: None,
        });
    }

    Ok(TurnOutcome {
        response_id,
        created_at,
        model,
        items,
        final_error,
    })
}

async fn emit_completion_event(
    emitter: &StreamEmitter,
    response_id: &str,
    created_at: i64,
    model: &str,
    items: &[ResponseItem],
    completed: &TurnCompletedNotification,
) {
    match emitter {
        StreamEmitter::Responses(_) => {
            let status = if completed.turn.error.is_some() {
                "failed"
            } else {
                "completed"
            };
            emitter
                .send(sse_json_event(
                    if completed.turn.error.is_some() {
                        "response.failed"
                    } else {
                        "response.completed"
                    },
                    &json!({
                        "type": if completed.turn.error.is_some() { "response.failed" } else { "response.completed" },
                        "response": {
                            "id": response_id,
                            "object": "response",
                            "created_at": created_at,
                            "model": model,
                            "status": status,
                            "output": items,
                        }
                    }),
                ))
                .await;
        }
        StreamEmitter::Chat(_) => {
            let finish_reason = if items
                .iter()
                .any(|item| matches!(item, ResponseItem::FunctionCall { .. }))
            {
                "tool_calls"
            } else {
                "stop"
            };
            emitter
                .send(sse_json_data(&chat_chunk(
                    response_id,
                    created_at,
                    model,
                    json!({}),
                    Some(finish_reason),
                )))
                .await;
        }
    }
}

fn responses_body_from_outcome(outcome: TurnOutcome) -> Value {
    let output_text = collect_output_text(&outcome.items);
    json!({
        "id": outcome.response_id,
        "object": "response",
        "created_at": outcome.created_at,
        "status": if outcome.final_error.is_some() { "failed" } else { "completed" },
        "model": outcome.model,
        "output": outcome.items,
        "output_text": output_text,
        "error": outcome.final_error.map(|error| json!({"message": error.message, "type": "server_error"})),
        "usage": Value::Null,
    })
}

fn chat_completion_body_from_outcome(outcome: TurnOutcome) -> Value {
    let content = collect_output_text(&outcome.items);
    let tool_calls = collect_chat_tool_calls(&outcome.items);
    let finish_reason = if tool_calls.is_empty() {
        "stop"
    } else {
        "tool_calls"
    };
    json!({
        "id": outcome.response_id,
        "object": "chat.completion",
        "created": outcome.created_at,
        "model": outcome.model,
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": if content.is_empty() { Value::Null } else { Value::String(content) },
                "tool_calls": if tool_calls.is_empty() { Value::Null } else { Value::Array(tool_calls) },
            },
            "finish_reason": finish_reason,
        }],
        "usage": Value::Null,
    })
}

fn chat_chunk(
    response_id: &str,
    created_at: i64,
    model: &str,
    delta: Value,
    finish_reason: Option<&str>,
) -> Value {
    json!({
        "id": response_id,
        "object": "chat.completion.chunk",
        "created": created_at,
        "model": model,
        "choices": [{
            "index": 0,
            "delta": delta,
            "finish_reason": finish_reason,
        }]
    })
}

fn collect_output_text(items: &[ResponseItem]) -> String {
    items
        .iter()
        .filter_map(|item| match item {
            ResponseItem::Message { content, .. } => Some(
                content
                    .iter()
                    .filter_map(|content_item| match content_item {
                        ContentItem::OutputText { text } => Some(text.as_str()),
                        ContentItem::InputText { .. } | ContentItem::InputImage { .. } => None,
                    })
                    .collect::<String>(),
            ),
            _ => None,
        })
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn collect_chat_tool_calls(items: &[ResponseItem]) -> Vec<Value> {
    items
        .iter()
        .filter_map(|item| match item {
            ResponseItem::FunctionCall {
                call_id,
                name,
                arguments,
                ..
            } => Some(json!({
                "id": call_id,
                "type": "function",
                "function": {
                    "name": name,
                    "arguments": arguments,
                }
            })),
            _ => None,
        })
        .collect()
}

fn thread_items_to_response_items(items: Vec<ThreadItem>) -> Vec<ResponseItem> {
    items
        .into_iter()
        .filter_map(|item| match item {
            ThreadItem::AgentMessage { text, .. } => Some(ResponseItem::Message {
                id: None,
                role: "assistant".to_string(),
                content: vec![ContentItem::OutputText { text }],
                end_turn: Some(true),
                phase: None,
            }),
            _ => None,
        })
        .collect()
}

async fn list_models(runtime: &EmbeddedRuntimeConfig) -> Result<Vec<Value>, ApiError> {
    let client = start(runtime.start_args())
        .await
        .map_err(|err| ApiError::internal(format!("failed to start embedded app-server: {err}")))?;
    let response: ModelListResponse = request_typed(
        &client,
        ClientRequest::ModelList {
            request_id: RequestId::Integer(1),
            params: ModelListParams {
                cursor: None,
                limit: None,
                include_hidden: Some(false),
            },
        },
    )
    .await?;
    client.shutdown().await.map_err(|err| {
        ApiError::internal(format!("failed to shutdown embedded app-server: {err}"))
    })?;

    Ok(response
        .data
        .into_iter()
        .map(|model| {
            json!({
                "id": model.model,
                "object": "model",
                "created": 0,
                "owned_by": "codex",
                "display_name": model.display_name,
                "description": model.description,
            })
        })
        .collect())
}

async fn request_typed<T>(
    client: &InProcessClientHandle,
    request: ClientRequest,
) -> Result<T, ApiError>
where
    T: serde::de::DeserializeOwned,
{
    let method = request_method_name(&request).to_string();
    let response = client
        .request(request)
        .await
        .map_err(|err| ApiError::internal(format!("{method} transport error: {err}")))?;
    let result =
        response.map_err(|err| ApiError::internal(format!("{method} failed: {}", err.message)))?;
    serde_json::from_value(result)
        .map_err(|err| ApiError::internal(format!("{method} decode error: {err}")))
}

fn request_method_name(request: &ClientRequest) -> &'static str {
    match request {
        ClientRequest::Initialize { .. } => "initialize",
        ClientRequest::ThreadStart { .. } => "thread/start",
        ClientRequest::TurnStart { .. } => "turn/start",
        ClientRequest::ModelList { .. } => "model/list",
        _ => "request",
    }
}

fn authorize(state: &OpenAiCompatState, headers: &HeaderMap) -> Result<(), ApiError> {
    let Some(expected) = state.auth_token.as_deref() else {
        return Ok(());
    };
    let actual = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .ok_or_else(|| ApiError::unauthorized("missing Authorization: Bearer token"))?;
    if actual == expected {
        Ok(())
    } else {
        Err(ApiError::unauthorized("invalid bearer token"))
    }
}

fn read_auth_token(auth_token_env: Option<&str>) -> Result<Option<String>> {
    let Some(env_name) = auth_token_env else {
        return Ok(None);
    };
    let token = std::env::var(env_name).with_context(|| {
        format!("failed to read auth token from environment variable {env_name}")
    })?;
    if token.is_empty() {
        bail!("environment variable {env_name} is empty")
    }
    Ok(Some(token))
}

async fn build_runtime_config(
    arg0_paths: Arg0DispatchPaths,
    cli_config_overrides: CliConfigOverrides,
    loader_overrides: LoaderOverrides,
) -> Result<EmbeddedRuntimeConfig> {
    let cli_kv_overrides = cli_config_overrides
        .parse_overrides()
        .map_err(anyhow::Error::msg)?;
    let cloud_requirements = match ConfigBuilder::default()
        .cli_overrides(cli_kv_overrides.clone())
        .loader_overrides(loader_overrides.clone())
        .build()
        .await
    {
        Ok(config) => {
            let auth_manager = AuthManager::shared(
                config.codex_home.clone(),
                false,
                config.cli_auth_credentials_store_mode,
            );
            cloud_requirements_loader(
                auth_manager,
                config.chatgpt_base_url,
                config.codex_home.clone(),
            )
        }
        Err(err) => {
            warn!(%err, "failed to preload config for cloud requirements");
            CloudRequirementsLoader::default()
        }
    };

    let mut config_warnings = Vec::new();
    let config = match ConfigBuilder::default()
        .cli_overrides(cli_kv_overrides.clone())
        .loader_overrides(loader_overrides.clone())
        .cloud_requirements(cloud_requirements.clone())
        .build()
        .await
    {
        Ok(config) => config,
        Err(err) => {
            config_warnings.push(ConfigWarningNotification {
                summary: "Invalid configuration; using defaults.".to_string(),
                details: Some(err.to_string()),
                path: None,
                range: None,
            });
            Config::load_default_with_cli_overrides(cli_kv_overrides.clone())?
        }
    };

    if let Ok(Some(err)) = check_execpolicy_for_warnings(&config.config_layer_stack).await {
        config_warnings.push(ConfigWarningNotification {
            summary: "Error parsing rules; custom rules not applied.".to_string(),
            details: Some(err.to_string()),
            path: None,
            range: None,
        });
    }

    for warning in &config.startup_warnings {
        config_warnings.push(ConfigWarningNotification {
            summary: warning.clone(),
            details: None,
            path: None,
            range: None,
        });
    }

    Ok(EmbeddedRuntimeConfig {
        arg0_paths,
        config: Arc::new(config),
        cli_kv_overrides,
        loader_overrides,
        cloud_requirements,
        config_warnings,
    })
}

fn sandbox_mode_from_policy(policy: &SandboxPolicy) -> Option<SandboxMode> {
    match policy {
        SandboxPolicy::DangerFullAccess => Some(SandboxMode::DangerFullAccess),
        SandboxPolicy::ReadOnly { .. } => Some(SandboxMode::ReadOnly),
        SandboxPolicy::WorkspaceWrite { .. } => Some(SandboxMode::WorkspaceWrite),
        SandboxPolicy::ExternalSandbox { .. } => None,
    }
}

fn approvals_reviewer_override_from_config(config: &Config) -> Option<ApprovalsReviewer> {
    Some(config.approvals_reviewer.into())
}

fn config_request_overrides_from_config(config: &Config) -> Option<HashMap<String, Value>> {
    config
        .active_profile
        .as_ref()
        .map(|profile| HashMap::from([("profile".to_string(), Value::String(profile.clone()))]))
}

fn plan_from_responses_request(body: &Value) -> Result<RequestPlan, ApiError> {
    reject_tools(body)?;
    let input = body
        .get("input")
        .ok_or_else(|| ApiError::bad_request("responses request requires `input`"))?;
    let messages = messages_like_to_plan(input)?;
    Ok(RequestPlan {
        model: body
            .get("model")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        base_instructions: body
            .get("instructions")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .or(messages.base_instructions),
        developer_instructions: messages.developer_instructions,
        input: messages.input,
        effort: parse_reasoning_effort(body),
        summary: None,
    })
}

fn plan_from_chat_request(body: &Value) -> Result<RequestPlan, ApiError> {
    reject_tools(body)?;
    let messages = body
        .get("messages")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            ApiError::bad_request("chat completions request requires `messages` array")
        })?;
    let plan = chat_messages_to_plan(messages)?;
    Ok(RequestPlan {
        model: body
            .get("model")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        base_instructions: plan.base_instructions,
        developer_instructions: plan.developer_instructions,
        input: plan.input,
        effort: parse_reasoning_effort(body),
        summary: None,
    })
}

fn reject_tools(body: &Value) -> Result<(), ApiError> {
    if body.get("tools").is_some() {
        return Err(ApiError::bad_request(
            "client-supplied `tools` are not supported by this Codex OpenAI-compatible server yet",
        ));
    }
    if body.get("tool_choice").is_some() {
        return Err(ApiError::bad_request(
            "client-supplied `tool_choice` is not supported by this Codex OpenAI-compatible server yet",
        ));
    }
    Ok(())
}

fn parse_reasoning_effort(body: &Value) -> Option<ReasoningEffort> {
    body.get("reasoning")
        .and_then(|reasoning| reasoning.get("effort"))
        .or_else(|| body.get("reasoning_effort"))
        .and_then(Value::as_str)
        .and_then(|value| value.parse().ok())
}

struct PlannedMessages {
    base_instructions: Option<String>,
    developer_instructions: Option<String>,
    input: Vec<codex_app_server_protocol::UserInput>,
}

fn messages_like_to_plan(input: &Value) -> Result<PlannedMessages, ApiError> {
    match input {
        Value::String(text) => Ok(PlannedMessages {
            base_instructions: None,
            developer_instructions: None,
            input: vec![codex_app_server_protocol::UserInput::Text {
                text: text.clone(),
                text_elements: Vec::new(),
            }],
        }),
        Value::Object(_) => chat_messages_to_plan(&[input.clone()]),
        Value::Array(items) => chat_messages_to_plan(items),
        _ => Err(ApiError::bad_request(
            "unsupported `input` shape; expected string, object, or array",
        )),
    }
}

fn chat_messages_to_plan(messages: &[Value]) -> Result<PlannedMessages, ApiError> {
    let mut system = Vec::new();
    let mut developer = Vec::new();
    let mut history = Vec::new();
    let mut latest_user_inputs = Vec::new();

    for (index, message) in messages.iter().enumerate() {
        let role = message
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("user");
        let content = extract_message_content(message)?;
        let is_last = index + 1 == messages.len();
        match role {
            "system" => system.push(content.text_representation),
            "developer" => developer.push(content.text_representation),
            "user" if is_last => {
                latest_user_inputs = content.inputs;
                if !content.prefix_text.is_empty() {
                    history.push(format!("user: {}", content.prefix_text));
                }
            }
            _ => {
                if !content.text_representation.is_empty() {
                    history.push(format!("{role}: {}", content.text_representation));
                }
            }
        }
    }

    if latest_user_inputs.is_empty() {
        return Err(ApiError::bad_request(
            "at least one terminal user message is required",
        ));
    }

    if !history.is_empty()
        && let Some(codex_app_server_protocol::UserInput::Text { text, .. }) =
            latest_user_inputs.first_mut()
    {
        *text = format!(
            "Conversation context:\n{}\n\nCurrent user request:\n{}",
            history.join("\n\n"),
            text
        );
    }

    Ok(PlannedMessages {
        base_instructions: join_non_empty(system),
        developer_instructions: join_non_empty(developer),
        input: latest_user_inputs,
    })
}

struct ExtractedContent {
    text_representation: String,
    prefix_text: String,
    inputs: Vec<codex_app_server_protocol::UserInput>,
}

fn extract_message_content(message: &Value) -> Result<ExtractedContent, ApiError> {
    let content = message
        .get("content")
        .ok_or_else(|| ApiError::bad_request("message is missing `content`"))?;
    match content {
        Value::String(text) => Ok(ExtractedContent {
            text_representation: text.clone(),
            prefix_text: String::new(),
            inputs: vec![codex_app_server_protocol::UserInput::Text {
                text: text.clone(),
                text_elements: Vec::new(),
            }],
        }),
        Value::Array(parts) => {
            let mut text_parts = Vec::new();
            let mut inputs = Vec::new();
            let mut prefix_parts = Vec::new();
            for part in parts {
                let part_type = part.get("type").and_then(Value::as_str).unwrap_or("text");
                match part_type {
                    "text" | "input_text" => {
                        let text = part
                            .get("text")
                            .and_then(Value::as_str)
                            .ok_or_else(|| {
                                ApiError::bad_request("text content part is missing `text`")
                            })?
                            .to_string();
                        text_parts.push(text.clone());
                        inputs.push(codex_app_server_protocol::UserInput::Text {
                            text,
                            text_elements: Vec::new(),
                        });
                    }
                    "image_url" | "input_image" => {
                        let url = part
                            .get("image_url")
                            .and_then(|image_url| match image_url {
                                Value::String(url) => Some(url.as_str()),
                                Value::Object(obj) => obj.get("url").and_then(Value::as_str),
                                _ => None,
                            })
                            .ok_or_else(|| {
                                ApiError::bad_request("image content part is missing `image_url`")
                            })?
                            .to_string();
                        prefix_parts.push(format!("[image: {url}]"));
                        inputs.push(codex_app_server_protocol::UserInput::Image { url });
                    }
                    other => {
                        return Err(ApiError::bad_request(format!(
                            "unsupported message content part type `{other}`"
                        )));
                    }
                }
            }
            Ok(ExtractedContent {
                text_representation: join_display_parts(&text_parts, &prefix_parts),
                prefix_text: join_non_empty(prefix_parts).unwrap_or_default(),
                inputs,
            })
        }
        Value::Object(obj) => {
            if let Some(text) = obj.get("text").and_then(Value::as_str) {
                Ok(ExtractedContent {
                    text_representation: text.to_string(),
                    prefix_text: String::new(),
                    inputs: vec![codex_app_server_protocol::UserInput::Text {
                        text: text.to_string(),
                        text_elements: Vec::new(),
                    }],
                })
            } else {
                Err(ApiError::bad_request(
                    "unsupported message content object; expected `text`",
                ))
            }
        }
        _ => Err(ApiError::bad_request(
            "unsupported message content shape; expected string or array",
        )),
    }
}

fn join_display_parts(text_parts: &[String], prefix_parts: &[String]) -> String {
    let mut all = Vec::new();
    if !prefix_parts.is_empty() {
        all.push(prefix_parts.join("\n"));
    }
    if !text_parts.is_empty() {
        all.push(text_parts.join("\n"));
    }
    all.join("\n")
}

fn join_non_empty(parts: Vec<String>) -> Option<String> {
    let joined = parts
        .into_iter()
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    if joined.is_empty() {
        None
    } else {
        Some(joined)
    }
}

fn sse_json_event(event: &str, value: &Value) -> Event {
    Event::default()
        .event(event)
        .json_data(value)
        .expect("event json")
}

fn sse_json_data(value: &Value) -> Event {
    Event::default().json_data(value).expect("event json")
}

fn unix_timestamp_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| i64::try_from(duration.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or_default()
}

fn to_io_error(err: anyhow::Error) -> IoError {
    IoError::new(ErrorKind::InvalidData, err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use app_test_support::create_mock_responses_server_repeating_assistant;
    use app_test_support::write_mock_responses_config_toml;
    use codex_features::Feature;
    use pretty_assertions::assert_eq;
    use std::collections::BTreeMap;
    use tempfile::TempDir;

    fn test_feature_flags() -> BTreeMap<Feature, bool> {
        BTreeMap::new()
    }

    async fn test_runtime() -> EmbeddedRuntimeConfig {
        let codex_home = TempDir::new().expect("tempdir");
        let server = create_mock_responses_server_repeating_assistant("hello from codex").await;
        write_mock_responses_config_toml(
            codex_home.path(),
            &server.uri(),
            &test_feature_flags(),
            1_000_000,
            Some(false),
            "mock-provider",
            "compact",
        )
        .expect("write config");

        let config = ConfigBuilder::default()
            .codex_home(codex_home.path().to_path_buf())
            .build()
            .await
            .expect("config");

        EmbeddedRuntimeConfig {
            arg0_paths: Arg0DispatchPaths::default(),
            config: Arc::new(config),
            cli_kv_overrides: Vec::new(),
            loader_overrides: LoaderOverrides::default(),
            cloud_requirements: CloudRequirementsLoader::default(),
            config_warnings: Vec::new(),
        }
    }

    #[tokio::test]
    async fn models_endpoint_uses_model_list() {
        let runtime = test_runtime().await;
        let models = list_models(&runtime).await.expect("list models");
        assert_eq!(models[0]["object"], "model");
    }

    #[test]
    fn responses_body_exposes_output_text_from_response_items() {
        let body = responses_body_from_outcome(TurnOutcome {
            response_id: "resp_test".to_string(),
            created_at: 1,
            model: "mock-model".to_string(),
            items: vec![ResponseItem::Message {
                id: None,
                role: "assistant".to_string(),
                content: vec![ContentItem::OutputText {
                    text: "hello from codex".to_string(),
                }],
                end_turn: Some(true),
                phase: None,
            }],
            final_error: None,
        });
        assert_eq!(body["object"], "response");
        assert_eq!(body["output_text"], "hello from codex");
        assert_eq!(body["status"], "completed");
    }

    #[tokio::test]
    async fn chat_request_serializes_history_into_prompt_prefix() {
        let plan = plan_from_chat_request(&json!({
            "messages": [
                {"role": "system", "content": "follow repo rules"},
                {"role": "assistant", "content": "old answer"},
                {"role": "user", "content": "new question"}
            ]
        }))
        .expect("plan");
        assert_eq!(
            plan.base_instructions,
            Some("follow repo rules".to_string())
        );
        let codex_app_server_protocol::UserInput::Text { text, .. } = &plan.input[0] else {
            panic!("expected text input");
        };
        assert!(text.contains("assistant: old answer"));
        assert!(text.contains("Current user request:\nnew question"));
    }

    #[test]
    fn auth_rejects_missing_bearer() {
        let state = OpenAiCompatState {
            runtime: Arc::new(EmbeddedRuntimeConfig {
                arg0_paths: Arg0DispatchPaths::default(),
                config: Arc::new(
                    Config::load_default_with_cli_overrides(Vec::new()).expect("config"),
                ),
                cli_kv_overrides: Vec::new(),
                loader_overrides: LoaderOverrides::default(),
                cloud_requirements: CloudRequirementsLoader::default(),
                config_warnings: Vec::new(),
            }),
            auth_token: Some(Arc::<str>::from("secret")),
        };
        let err = authorize(&state, &HeaderMap::new()).expect_err("should reject");
        assert_eq!(err.status, StatusCode::UNAUTHORIZED);
    }
}
