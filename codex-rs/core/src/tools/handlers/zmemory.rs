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
use async_trait::async_trait;
use codex_app_server_protocol::ConfigLayerSource;
use codex_zmemory::tool_api::ZmemoryToolCallParam;
use codex_zmemory::tool_api::run_zmemory_tool_with_context;
use std::path::PathBuf;
use tracing::warn;

pub struct ZmemoryHandler;

pub(crate) const ZMEMORY_JSON_BEGIN: &str = "---BEGIN_ZMEMORY_JSON---";
pub(crate) const ZMEMORY_JSON_END: &str = "---END_ZMEMORY_JSON---";

#[async_trait]
impl ToolHandler for ZmemoryHandler {
    type Output = FunctionToolOutput;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        let ToolInvocation {
            session,
            turn,
            payload,
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
        let args: ZmemoryToolCallParam = parse_arguments(&arguments)?;
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
                let json =
                    serde_json::to_string_pretty(&result.structured_content).map_err(|err| {
                        FunctionCallError::Fatal(format!("serialize zmemory output: {err}"))
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
