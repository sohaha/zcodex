use crate::codex::Session;
use crate::codex::TurnContext;
use crate::error::CodexErr;
use crate::error::SandboxErr;
use crate::exec::ExecToolCallOutput;
use crate::function_tool::FunctionCallError;
use crate::parse_command::parse_command;
use crate::protocol::EventMsg;
use crate::protocol::ExecCommandBeginEvent;
use crate::protocol::ExecCommandEndEvent;
use crate::protocol::ExecCommandSource;
use crate::protocol::ExecCommandStatus;
use crate::protocol::FileChange;
use crate::protocol::PatchApplyBeginEvent;
use crate::protocol::PatchApplyEndEvent;
use crate::protocol::PatchApplyStatus;
use crate::protocol::TurnDiffEvent;
use crate::tools::context::SharedTurnDiffTracker;
use crate::tools::sandboxing::ToolError;
use codex_protocol::parse_command::ParsedCommand;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;

use super::format_exec_output_str;

#[derive(Clone, Copy)]
pub(crate) struct ToolEventCtx<'a> {
    pub session: &'a Session,
    pub turn: &'a TurnContext,
    pub call_id: &'a str,
    pub turn_diff_tracker: Option<&'a SharedTurnDiffTracker>,
}

impl<'a> ToolEventCtx<'a> {
    pub fn new(
        session: &'a Session,
        turn: &'a TurnContext,
        call_id: &'a str,
        turn_diff_tracker: Option<&'a SharedTurnDiffTracker>,
    ) -> Self {
        Self {
            session,
            turn,
            call_id,
            turn_diff_tracker,
        }
    }
}

pub(crate) enum ToolEventStage {
    Begin,
    Success(ExecToolCallOutput),
    Failure(ToolEventFailure),
}

pub(crate) enum ToolEventFailure {
    Output(ExecToolCallOutput),
    Message(String),
    Rejected(String),
}

pub(crate) async fn emit_exec_command_begin(
    ctx: ToolEventCtx<'_>,
    command: &[String],
    cwd: &Path,
    parsed_cmd: &[ParsedCommand],
    source: ExecCommandSource,
    interaction_input: Option<String>,
    process_id: Option<&str>,
) {
    ctx.session
        .send_event(
            ctx.turn,
            EventMsg::ExecCommandBegin(ExecCommandBeginEvent {
                call_id: ctx.call_id.to_string(),
                process_id: process_id.map(str::to_owned),
                turn_id: ctx.turn.sub_id.clone(),
                command: command.to_vec(),
                cwd: cwd.to_path_buf(),
                parsed_cmd: parsed_cmd.to_vec(),
                source,
                interaction_input,
            }),
        )
        .await;
}
// Concrete, allocation-free emitter: avoid trait objects and boxed futures.
pub(crate) enum ToolEmitter {
    Shell {
        command: Vec<String>,
        cwd: PathBuf,
        source: ExecCommandSource,
        parsed_cmd: Vec<ParsedCommand>,
        freeform: bool,
        interaction_input: Option<String>,
        model_output_prefix: Option<String>,
    },
    ApplyPatch {
        changes: HashMap<PathBuf, FileChange>,
        auto_approved: bool,
    },
    UnifiedExec {
        command: Vec<String>,
        cwd: PathBuf,
        source: ExecCommandSource,
        parsed_cmd: Vec<ParsedCommand>,
        process_id: Option<String>,
    },
}

impl ToolEmitter {
    pub fn shell(
        command: Vec<String>,
        cwd: PathBuf,
        source: ExecCommandSource,
        freeform: bool,
        interaction_input: Option<String>,
        model_output_prefix: Option<String>,
    ) -> Self {
        let parsed_cmd = parse_command(&command);
        Self::Shell {
            command,
            cwd,
            source,
            parsed_cmd,
            freeform,
            interaction_input,
            model_output_prefix,
        }
    }

    pub fn apply_patch(changes: HashMap<PathBuf, FileChange>, auto_approved: bool) -> Self {
        Self::ApplyPatch {
            changes,
            auto_approved,
        }
    }

    pub fn unified_exec(
        command: &[String],
        cwd: PathBuf,
        source: ExecCommandSource,
        process_id: Option<String>,
    ) -> Self {
        let parsed_cmd = parse_command(command);
        Self::UnifiedExec {
            command: command.to_vec(),
            cwd,
            source,
            parsed_cmd,
            process_id,
        }
    }

    pub async fn emit(&self, ctx: ToolEventCtx<'_>, stage: ToolEventStage) {
        match (self, stage) {
            (
                Self::Shell {
                    command,
                    cwd,
                    source,
                    parsed_cmd,
                    interaction_input,
                    ..
                },
                stage,
            ) => {
                emit_exec_stage(
                    ctx,
                    ExecCommandInput::new(
                        command,
                        cwd.as_path(),
                        parsed_cmd,
                        *source,
                        interaction_input.as_deref(),
                        /*process_id*/ None,
                    ),
                    stage,
                )
                .await;
            }

            (
                Self::ApplyPatch {
                    changes,
                    auto_approved,
                },
                ToolEventStage::Begin,
            ) => {
                if let Some(tracker) = ctx.turn_diff_tracker {
                    let mut guard = tracker.lock().await;
                    guard.on_patch_begin(changes);
                }
                ctx.session
                    .send_event(
                        ctx.turn,
                        EventMsg::PatchApplyBegin(PatchApplyBeginEvent {
                            call_id: ctx.call_id.to_string(),
                            turn_id: ctx.turn.sub_id.clone(),
                            auto_approved: *auto_approved,
                            changes: changes.clone(),
                        }),
                    )
                    .await;
            }
            (Self::ApplyPatch { changes, .. }, ToolEventStage::Success(output)) => {
                emit_patch_end(
                    ctx,
                    changes.clone(),
                    output.stdout.text.clone(),
                    output.stderr.text.clone(),
                    output.exit_code == 0,
                    if output.exit_code == 0 {
                        PatchApplyStatus::Completed
                    } else {
                        PatchApplyStatus::Failed
                    },
                )
                .await;
            }
            (
                Self::ApplyPatch { changes, .. },
                ToolEventStage::Failure(ToolEventFailure::Output(output)),
            ) => {
                emit_patch_end(
                    ctx,
                    changes.clone(),
                    output.stdout.text.clone(),
                    output.stderr.text.clone(),
                    output.exit_code == 0,
                    if output.exit_code == 0 {
                        PatchApplyStatus::Completed
                    } else {
                        PatchApplyStatus::Failed
                    },
                )
                .await;
            }
            (
                Self::ApplyPatch { changes, .. },
                ToolEventStage::Failure(ToolEventFailure::Message(message)),
            ) => {
                emit_patch_end(
                    ctx,
                    changes.clone(),
                    String::new(),
                    (*message).to_string(),
                    /*success*/ false,
                    PatchApplyStatus::Failed,
                )
                .await;
            }
            (
                Self::ApplyPatch { changes, .. },
                ToolEventStage::Failure(ToolEventFailure::Rejected(message)),
            ) => {
                emit_patch_end(
                    ctx,
                    changes.clone(),
                    String::new(),
                    (*message).to_string(),
                    /*success*/ false,
                    PatchApplyStatus::Declined,
                )
                .await;
            }
            (
                Self::UnifiedExec {
                    command,
                    cwd,
                    source,
                    parsed_cmd,
                    process_id,
                },
                stage,
            ) => {
                emit_exec_stage(
                    ctx,
                    ExecCommandInput::new(
                        command,
                        cwd.as_path(),
                        parsed_cmd,
                        *source,
                        /*interaction_input*/ None,
                        process_id.as_deref(),
                    ),
                    stage,
                )
                .await;
            }
        }
    }

    pub async fn begin(&self, ctx: ToolEventCtx<'_>) {
        self.emit(ctx, ToolEventStage::Begin).await;
    }

    fn format_exec_output_for_model(
        &self,
        output: &ExecToolCallOutput,
        ctx: ToolEventCtx<'_>,
    ) -> String {
        match self {
            Self::Shell {
                freeform: true,
                model_output_prefix,
                ..
            } => {
                let content = super::format_exec_output_for_model_freeform(
                    output,
                    ctx.turn.truncation_policy,
                );
                match model_output_prefix {
                    Some(prefix) => format!("{prefix}\n\n{content}"),
                    None => content,
                }
            }
            _ => super::format_exec_output_for_model_structured(output, ctx.turn.truncation_policy),
        }
    }

    pub async fn finish(
        &self,
        ctx: ToolEventCtx<'_>,
        out: Result<ExecToolCallOutput, ToolError>,
    ) -> Result<String, FunctionCallError> {
        let (event, result) = match out {
            Ok(output) => {
                let content = self.format_exec_output_for_model(&output, ctx);
                let exit_code = output.exit_code;
                let event = ToolEventStage::Success(output);
                let result = if exit_code == 0 {
                    Ok(content)
                } else {
                    Err(FunctionCallError::RespondToModel(content))
                };
                (event, result)
            }
            Err(ToolError::Codex(CodexErr::Sandbox(SandboxErr::Timeout { output })))
            | Err(ToolError::Codex(CodexErr::Sandbox(SandboxErr::Denied { output, .. }))) => {
                let response = self.format_exec_output_for_model(&output, ctx);
                let event = ToolEventStage::Failure(ToolEventFailure::Output(*output));
                let result = Err(FunctionCallError::RespondToModel(response));
                (event, result)
            }
            Err(ToolError::Codex(err)) => {
                let message = format!("execution error: {err:?}");
                let event = ToolEventStage::Failure(ToolEventFailure::Message(message.clone()));
                let result = Err(FunctionCallError::RespondToModel(message));
                (event, result)
            }
            Err(ToolError::Rejected(msg)) => {
                // Normalize common rejection messages for exec tools so tests and
                // users see a clear, consistent phrase.
                //
                // NOTE: ToolError::Rejected is currently used for both user-declined approvals
                // and some operational/runtime rejection paths (for example setup failures).
                // We intentionally map all of them through the "rejected" event path for now,
                // which means a subset of non-user failures may be reported as Declined.
                //
                // TODO: We should add a new ToolError variant for user-declined approvals.
                let normalized = if msg == "rejected by user" {
                    match self {
                        Self::Shell { .. } | Self::UnifiedExec { .. } => {
                            "exec command rejected by user".to_string()
                        }
                        Self::ApplyPatch { .. } => "patch rejected by user".to_string(),
                    }
                } else {
                    msg
                };
                let event = ToolEventStage::Failure(ToolEventFailure::Rejected(normalized.clone()));
                let result = Err(FunctionCallError::RespondToModel(normalized));
                (event, result)
            }
        };
        self.emit(ctx, event).await;
        result
    }
}

struct ExecCommandInput<'a> {
    command: &'a [String],
    cwd: &'a Path,
    parsed_cmd: &'a [ParsedCommand],
    source: ExecCommandSource,
    interaction_input: Option<&'a str>,
    process_id: Option<&'a str>,
}

impl<'a> ExecCommandInput<'a> {
    fn new(
        command: &'a [String],
        cwd: &'a Path,
        parsed_cmd: &'a [ParsedCommand],
        source: ExecCommandSource,
        interaction_input: Option<&'a str>,
        process_id: Option<&'a str>,
    ) -> Self {
        Self {
            command,
            cwd,
            parsed_cmd,
            source,
            interaction_input,
            process_id,
        }
    }
}

struct ExecCommandResult {
    stdout: String,
    stderr: String,
    aggregated_output: String,
    exit_code: i32,
    duration: Duration,
    formatted_output: String,
    status: ExecCommandStatus,
}

async fn emit_exec_stage(
    ctx: ToolEventCtx<'_>,
    exec_input: ExecCommandInput<'_>,
    stage: ToolEventStage,
) {
    match stage {
        ToolEventStage::Begin => {
            emit_exec_command_begin(
                ctx,
                exec_input.command,
                exec_input.cwd,
                exec_input.parsed_cmd,
                exec_input.source,
                exec_input.interaction_input.map(str::to_owned),
                exec_input.process_id,
            )
            .await;
        }
        ToolEventStage::Success(output)
        | ToolEventStage::Failure(ToolEventFailure::Output(output)) => {
            let exec_result = ExecCommandResult {
                stdout: output.stdout.text.clone(),
                stderr: output.stderr.text.clone(),
                aggregated_output: output.aggregated_output.text.clone(),
                exit_code: output.exit_code,
                duration: output.duration,
                formatted_output: format_exec_output_str(&output, ctx.turn.truncation_policy),
                status: if output.exit_code == 0 {
                    ExecCommandStatus::Completed
                } else {
                    ExecCommandStatus::Failed
                },
            };
            emit_exec_end(ctx, exec_input, exec_result).await;
        }
        ToolEventStage::Failure(ToolEventFailure::Message(message)) => {
            let text = message.to_string();
            let exec_result = ExecCommandResult {
                stdout: String::new(),
                stderr: text.clone(),
                aggregated_output: text.clone(),
                exit_code: -1,
                duration: Duration::ZERO,
                formatted_output: text,
                status: ExecCommandStatus::Failed,
            };
            emit_exec_end(ctx, exec_input, exec_result).await;
        }
        ToolEventStage::Failure(ToolEventFailure::Rejected(message)) => {
            let text = message.to_string();
            let exec_result = ExecCommandResult {
                stdout: String::new(),
                stderr: text.clone(),
                aggregated_output: text.clone(),
                exit_code: -1,
                duration: Duration::ZERO,
                formatted_output: text,
                status: ExecCommandStatus::Declined,
            };
            emit_exec_end(ctx, exec_input, exec_result).await;
        }
    }
}

async fn emit_exec_end(
    ctx: ToolEventCtx<'_>,
    exec_input: ExecCommandInput<'_>,
    exec_result: ExecCommandResult,
) {
    ctx.session
        .send_event(
            ctx.turn,
            EventMsg::ExecCommandEnd(ExecCommandEndEvent {
                call_id: ctx.call_id.to_string(),
                process_id: exec_input.process_id.map(str::to_owned),
                turn_id: ctx.turn.sub_id.clone(),
                command: exec_input.command.to_vec(),
                cwd: exec_input.cwd.to_path_buf(),
                parsed_cmd: exec_input.parsed_cmd.to_vec(),
                source: exec_input.source,
                interaction_input: exec_input.interaction_input.map(str::to_owned),
                stdout: exec_result.stdout,
                stderr: exec_result.stderr,
                aggregated_output: exec_result.aggregated_output,
                exit_code: exec_result.exit_code,
                duration: exec_result.duration,
                formatted_output: exec_result.formatted_output,
                status: exec_result.status,
            }),
        )
        .await;
}

async fn emit_patch_end(
    ctx: ToolEventCtx<'_>,
    changes: HashMap<PathBuf, FileChange>,
    stdout: String,
    stderr: String,
    success: bool,
    status: PatchApplyStatus,
) {
    ctx.session
        .send_event(
            ctx.turn,
            EventMsg::PatchApplyEnd(PatchApplyEndEvent {
                call_id: ctx.call_id.to_string(),
                turn_id: ctx.turn.sub_id.clone(),
                stdout,
                stderr,
                success,
                changes,
                status,
            }),
        )
        .await;

    if let Some(tracker) = ctx.turn_diff_tracker {
        let unified_diff = {
            let mut guard = tracker.lock().await;
            guard.get_unified_diff()
        };
        if let Ok(Some(unified_diff)) = unified_diff {
            ctx.session
                .send_event(ctx.turn, EventMsg::TurnDiff(TurnDiffEvent { unified_diff }))
                .await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ToolEmitter;
    use super::ToolEventCtx;
    use crate::codex::make_session_and_context_with_rx;
    use crate::exec::ExecToolCallOutput;
    use crate::exec::StreamOutput;
    use crate::protocol::EventMsg;
    use crate::protocol::ExecCommandBeginEvent;
    use crate::protocol::ExecCommandEndEvent;
    use crate::protocol::ExecCommandSource;
    use pretty_assertions::assert_eq;
    use std::time::Duration;

    async fn recv_exec_begin(
        rx: &mut async_channel::Receiver<crate::protocol::Event>,
    ) -> ExecCommandBeginEvent {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let event = rx.recv().await.expect("event");
                if let EventMsg::ExecCommandBegin(begin) = event.msg {
                    break begin;
                }
            }
        })
        .await
        .expect("timed out waiting for exec begin")
    }

    async fn recv_exec_end(
        rx: &mut async_channel::Receiver<crate::protocol::Event>,
    ) -> ExecCommandEndEvent {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let event = rx.recv().await.expect("event");
                if let EventMsg::ExecCommandEnd(end) = event.msg {
                    break end;
                }
            }
        })
        .await
        .expect("timed out waiting for exec end")
    }

    #[tokio::test]
    async fn shell_emitter_preserves_interaction_input_in_exec_events() {
        let (session, turn, mut rx) = make_session_and_context_with_rx().await;
        let emitter = ToolEmitter::shell(
            vec![
                "bash".to_string(),
                "-lc".to_string(),
                "FOO=1 codex rtk git status".to_string(),
            ],
            turn.cwd.clone(),
            ExecCommandSource::Agent,
            true,
            Some("FOO=1 git status".to_string()),
            Some(
                "[shell_command routed via embedded RTK]\noriginal: FOO=1 git status\nexecuted: FOO=1 codex rtk git status"
                    .to_string(),
            ),
        );
        let ctx = ToolEventCtx::new(session.as_ref(), turn.as_ref(), "call-1", None);

        emitter.begin(ctx).await;
        let begin = recv_exec_begin(&mut rx).await;
        assert_eq!(
            begin.interaction_input,
            Some("FOO=1 git status".to_string())
        );

        let output = ExecToolCallOutput {
            stdout: StreamOutput::new("ok".to_string()),
            aggregated_output: StreamOutput::new("ok".to_string()),
            duration: Duration::from_millis(12),
            ..ExecToolCallOutput::default()
        };
        let result = emitter.finish(ctx, Ok(output)).await.expect("shell output");
        assert!(result.contains("[shell_command routed via embedded RTK]"));
        assert!(result.contains("executed: FOO=1 codex rtk git status"));
        assert!(result.contains("ok"));

        let end = recv_exec_end(&mut rx).await;
        assert_eq!(end.interaction_input, Some("FOO=1 git status".to_string()));
        assert_eq!(end.stdout, "ok");
        assert_eq!(end.status, crate::protocol::ExecCommandStatus::Completed);
    }

    #[tokio::test]
    async fn shell_emitter_omits_prefix_for_structured_output() {
        let (session, turn, _rx) = make_session_and_context_with_rx().await;
        let emitter = ToolEmitter::shell(
            vec!["bash".to_string(), "-lc".to_string(), "git status".to_string()],
            turn.cwd.clone(),
            ExecCommandSource::Agent,
            false,
            None,
            Some("[shell_command kept raw]\nreason: contains compound shell syntax\ncommand: git status | head".to_string()),
        );
        let ctx = ToolEventCtx::new(session.as_ref(), turn.as_ref(), "call-2", None);

        let output = ExecToolCallOutput {
            stdout: StreamOutput::new("stdout".to_string()),
            aggregated_output: StreamOutput::new("stdout".to_string()),
            duration: Duration::from_millis(5),
            ..ExecToolCallOutput::default()
        };
        let result = emitter.finish(ctx, Ok(output)).await.expect("shell output");
        assert!(!result.contains("[shell_command kept raw]"));
        assert!(result.contains("stdout"));
    }
}
