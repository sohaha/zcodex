use super::parse_arguments;
use super::zmemory::ZMEMORY_JSON_BEGIN;
use super::zmemory::ZMEMORY_JSON_END;
use super::zmemory::resolve_zmemory_config_for_turn;
use crate::function_tool::FunctionCallError;
use crate::session::session::Session;
use crate::tools::context::FunctionToolOutput;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::ToolKind;
use anyhow::Result;
use codex_zmemory::tool_api::ZmemoryToolAction;
use codex_zmemory::tool_api::ZmemoryToolCallParam;
use codex_zmemory::tool_api::run_zmemory_tool_with_context;
use serde::Deserialize;
use std::sync::Arc;

pub struct CtxHandler;

impl ToolHandler for CtxHandler {
    type Output = FunctionToolOutput;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        let ToolInvocation {
            session,
            turn,
            payload,
            tool_name,
            ..
        } = invocation;
        let arguments = match payload {
            ToolPayload::Function { arguments } => arguments,
            _ => {
                return Err(FunctionCallError::RespondToModel(
                    "ctx handler received unsupported payload".to_string(),
                ));
            }
        };
        let args = parse_ctx_tool_args(tool_name.as_str(), &arguments, &session).await?;
        let codex_home = session.codex_home().await.into_path_buf();
        let zmemory_config =
            resolve_zmemory_config_for_turn(&session, &codex_home, &turn.cwd).await;
        let zmemory_path = zmemory_config.path.as_deref();

        match run_zmemory_tool_with_context(
            &codex_home,
            turn.cwd.as_path(),
            zmemory_path,
            Some(zmemory_config.to_runtime_settings()),
            args,
        ) {
            Ok(result) => {
                let json =
                    serde_json::to_string_pretty(&result.structured_content).map_err(|err| {
                        FunctionCallError::Fatal(format!("serialize ctx output: {err}"))
                    })?;
                Ok(FunctionToolOutput::from_text(
                    format!(
                        "{summary}\n{ZMEMORY_JSON_BEGIN}\n{json}\n{ZMEMORY_JSON_END}",
                        summary = result.text
                    ),
                    Some(true),
                ))
            }
            Err(err) => Ok(FunctionToolOutput::from_text(err.to_string(), Some(false))),
        }
    }
}

#[derive(Deserialize)]
struct CtxSearchArgs {
    query: String,
    session_id: Option<String>,
    limit: Option<usize>,
}

#[derive(Deserialize)]
struct CtxStatsArgs {
    session_id: Option<String>,
}

#[derive(Deserialize)]
struct CtxPurgeArgs {
    session_id: Option<String>,
}

#[derive(Deserialize)]
struct CtxExecuteArgs {
    action: String,
    uri: Option<String>,
    query: Option<String>,
    content: Option<String>,
    limit: Option<usize>,
}

async fn parse_ctx_tool_args(
    tool_name: &str,
    arguments: &str,
    session: &Arc<Session>,
) -> Result<ZmemoryToolCallParam, FunctionCallError> {
    let session_id = session.conversation_id.to_string();
    match tool_name {
        "ctx_search" => {
            let args: CtxSearchArgs = parse_arguments(arguments)?;
            let scope_uri = args
                .session_id
                .map(|id| format!("session://{id}"))
                .unwrap_or_else(|| format!("session://{session_id}"));
            Ok(ZmemoryToolCallParam {
                action: ZmemoryToolAction::Search,
                uri: Some(scope_uri),
                query: Some(args.query),
                limit: args.limit,
                ..ZmemoryToolCallParam::default()
            })
        }
        "ctx_stats" => {
            let args: CtxStatsArgs = parse_arguments(arguments)?;
            let scope_uri = args
                .session_id
                .map(|id| format!("session://{id}"))
                .unwrap_or_else(|| format!("session://{session_id}"));
            Ok(ZmemoryToolCallParam {
                action: ZmemoryToolAction::Stats,
                uri: Some(scope_uri),
                ..ZmemoryToolCallParam::default()
            })
        }
        "ctx_doctor" => Ok(ZmemoryToolCallParam {
            action: ZmemoryToolAction::Doctor,
            ..ZmemoryToolCallParam::default()
        }),
        "ctx_purge" => {
            let args: CtxPurgeArgs = parse_arguments(arguments)?;
            let scope_uri = args
                .session_id
                .map(|id| format!("session://{id}"))
                .unwrap_or_else(|| format!("session://{session_id}"));
            Ok(ZmemoryToolCallParam {
                action: ZmemoryToolAction::DeletePath,
                uri: Some(scope_uri),
                ..ZmemoryToolCallParam::default()
            })
        }
        "ctx_execute" => {
            let args: CtxExecuteArgs = parse_arguments(arguments)?;
            let action = parse_action_from_str(&args.action)?;
            Ok(ZmemoryToolCallParam {
                action,
                uri: args.uri,
                query: args.query,
                content: args.content,
                limit: args.limit,
                ..ZmemoryToolCallParam::default()
            })
        }
        _ => Err(FunctionCallError::RespondToModel(format!(
            "unknown ctx tool name `{tool_name}`"
        ))),
    }
}

fn parse_action_from_str(s: &str) -> Result<ZmemoryToolAction, FunctionCallError> {
    match s {
        "read" => Ok(ZmemoryToolAction::Read),
        "search" => Ok(ZmemoryToolAction::Search),
        "create" => Ok(ZmemoryToolAction::Create),
        "update" => Ok(ZmemoryToolAction::Update),
        "delete-path" => Ok(ZmemoryToolAction::DeletePath),
        "stats" => Ok(ZmemoryToolAction::Stats),
        "doctor" => Ok(ZmemoryToolAction::Doctor),
        _ => Err(FunctionCallError::RespondToModel(format!(
            "unknown ctx_execute action `{s}`"
        ))),
    }
}
