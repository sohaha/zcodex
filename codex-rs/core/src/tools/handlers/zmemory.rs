use super::parse_arguments;
use crate::codex::Session;
use crate::config::ConfigBuilder;
use crate::config::types::ZmemoryConfig;
use crate::function_tool::FunctionCallError;
use crate::tools::context::FunctionToolOutput;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::ToolKind;
use anyhow::Result;
use codex_app_server_protocol::ConfigLayerSource;
use codex_zmemory::tool_api::ZmemoryToolAction;
use codex_zmemory::tool_api::ZmemoryToolCallParam;
use codex_zmemory::tool_api::run_zmemory_tool_with_context;
use serde::Deserialize;
use std::path::PathBuf;
use tracing::warn;

pub struct ZmemoryHandler;

pub const ZMEMORY_JSON_BEGIN: &str = "---BEGIN_ZMEMORY_JSON---";
pub const ZMEMORY_JSON_END: &str = "---END_ZMEMORY_JSON---";

impl ToolHandler for ZmemoryHandler {
    type Output = FunctionToolOutput;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    fn handle(
        &self,
        invocation: ToolInvocation,
    ) -> impl std::future::Future<Output = Result<Self::Output, FunctionCallError>> + Send {
        async move {
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
                        "zmemory handler received unsupported payload".to_string(),
                    ));
                }
            };
            let args = parse_zmemory_tool_args(&tool_name, &arguments)?;
            let codex_home = match args
                .codex_home
                .as_deref()
                .map(str::trim)
                .filter(|path| !path.is_empty())
            {
                Some(path) => crate::util::resolve_path(&turn.cwd, &PathBuf::from(path)),
                None => session.codex_home().await,
            };

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
                    let json = serde_json::to_string_pretty(&result.structured_content).map_err(
                        |err| FunctionCallError::Fatal(format!("serialize zmemory output: {err}")),
                    )?;
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
}

#[derive(Deserialize)]
struct ReadMemoryArgs {
    uri: String,
}

#[derive(Deserialize)]
struct SearchMemoryArgs {
    query: String,
    domain: Option<String>,
    limit: Option<usize>,
}

#[derive(Deserialize)]
struct CreateMemoryArgs {
    parent_uri: String,
    content: String,
    priority: i64,
    title: Option<String>,
    disclosure: Option<String>,
}

#[derive(Deserialize)]
struct UpdateMemoryArgs {
    uri: String,
    old_string: Option<String>,
    new_string: Option<String>,
    append: Option<String>,
    priority: Option<i64>,
    disclosure: Option<String>,
}

#[derive(Deserialize)]
struct DeleteMemoryArgs {
    uri: String,
}

#[derive(Deserialize)]
struct AddAliasArgs {
    new_uri: String,
    target_uri: String,
    priority: Option<i64>,
    disclosure: Option<String>,
}

#[derive(Deserialize)]
struct ManageTriggersArgs {
    uri: String,
    add: Option<Vec<String>>,
    remove: Option<Vec<String>>,
}

fn parse_zmemory_tool_args(
    tool_name: &str,
    arguments: &str,
) -> Result<ZmemoryToolCallParam, FunctionCallError> {
    match tool_name {
        "zmemory" => parse_arguments(arguments),
        "read_memory" => {
            let args: ReadMemoryArgs = parse_arguments(arguments)?;
            Ok(ZmemoryToolCallParam {
                action: ZmemoryToolAction::Read,
                uri: Some(args.uri),
                ..ZmemoryToolCallParam::default()
            })
        }
        "search_memory" => {
            let args: SearchMemoryArgs = parse_arguments(arguments)?;
            Ok(ZmemoryToolCallParam {
                action: ZmemoryToolAction::Search,
                uri: map_domain_to_scope(args.domain),
                query: Some(args.query),
                limit: args.limit,
                ..ZmemoryToolCallParam::default()
            })
        }
        "create_memory" => {
            let args: CreateMemoryArgs = parse_arguments(arguments)?;
            Ok(ZmemoryToolCallParam {
                action: ZmemoryToolAction::Create,
                parent_uri: Some(args.parent_uri),
                content: Some(args.content),
                priority: Some(args.priority),
                title: args.title,
                disclosure: args.disclosure,
                ..ZmemoryToolCallParam::default()
            })
        }
        "update_memory" => {
            let args: UpdateMemoryArgs = parse_arguments(arguments)?;
            Ok(ZmemoryToolCallParam {
                action: ZmemoryToolAction::Update,
                uri: Some(args.uri),
                old_string: args.old_string,
                new_string: args.new_string,
                append: args.append,
                priority: args.priority,
                disclosure: args.disclosure,
                ..ZmemoryToolCallParam::default()
            })
        }
        "delete_memory" => {
            let args: DeleteMemoryArgs = parse_arguments(arguments)?;
            Ok(ZmemoryToolCallParam {
                action: ZmemoryToolAction::DeletePath,
                uri: Some(args.uri),
                ..ZmemoryToolCallParam::default()
            })
        }
        "add_alias" => {
            let args: AddAliasArgs = parse_arguments(arguments)?;
            Ok(ZmemoryToolCallParam {
                action: ZmemoryToolAction::AddAlias,
                new_uri: Some(args.new_uri),
                target_uri: Some(args.target_uri),
                priority: args.priority,
                disclosure: args.disclosure,
                ..ZmemoryToolCallParam::default()
            })
        }
        "manage_triggers" => {
            let args: ManageTriggersArgs = parse_arguments(arguments)?;
            Ok(ZmemoryToolCallParam {
                action: ZmemoryToolAction::ManageTriggers,
                uri: Some(args.uri),
                add: args.add,
                remove: args.remove,
                ..ZmemoryToolCallParam::default()
            })
        }
        _ => Err(FunctionCallError::RespondToModel(format!(
            "unknown zmemory tool name `{tool_name}`"
        ))),
    }
}

fn map_domain_to_scope(domain: Option<String>) -> Option<String> {
    let domain = domain
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    if domain.contains("://") {
        return Some(domain.to_string());
    }
    Some(format!("{domain}://"))
}

async fn resolve_zmemory_config_for_turn(
    session: &Session,
    codex_home: &std::path::Path,
    turn_cwd: &std::path::Path,
) -> ZmemoryConfig {
    let session_config = session.get_config().await;
    let current_zmemory_config = session_config.zmemory.clone();
    let zmemory_origin = session_config
        .config_layer_stack
        .origins()
        .remove("zmemory.path")
        .map(|metadata| metadata.name);
    let should_reload = session_config.cwd.as_path() != turn_cwd
        && matches!(
            zmemory_origin,
            None | Some(ConfigLayerSource::Project { .. })
        );

    if !should_reload {
        return current_zmemory_config;
    }

    match ConfigBuilder::default()
        .codex_home(codex_home.to_path_buf())
        .fallback_cwd(Some(turn_cwd.to_path_buf()))
        .build()
        .await
    {
        Ok(config) => config.zmemory,
        Err(err) => {
            warn!(
                error = %err,
                cwd = %turn_cwd.display(),
                "failed to reload zmemory config for current turn cwd; using session config"
            );
            current_zmemory_config
        }
    }
}
