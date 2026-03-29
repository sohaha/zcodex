use super::parse_arguments;
use crate::function_tool::FunctionCallError;
use crate::tools::context::FunctionToolOutput;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::ToolKind;
use anyhow::Result;
use async_trait::async_trait;
use codex_zmemory::tool_api::ZmemoryToolCallParam;
use codex_zmemory::tool_api::run_zmemory_tool;
use std::path::PathBuf;

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

        match run_zmemory_tool(&codex_home, args) {
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
