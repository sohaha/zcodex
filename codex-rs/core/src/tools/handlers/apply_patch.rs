use std::path::Path;

use crate::apply_patch;
use crate::apply_patch::InternalApplyPatchInvocation;
use crate::apply_patch::convert_apply_patch_to_protocol;
use crate::codex::Session;
use crate::codex::TurnContext;
use crate::function_tool::FunctionCallError;
use crate::tools::context::ApplyPatchToolOutput;
use crate::tools::context::FunctionToolOutput;
use crate::tools::context::SharedTurnDiffTracker;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::events::ToolEmitter;
use crate::tools::events::ToolEventCtx;
use crate::tools::handlers::apply_granted_turn_permissions;
use crate::tools::handlers::parse_arguments;
use crate::tools::orchestrator::ToolOrchestrator;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::ToolKind;
use crate::tools::runtimes::apply_patch::ApplyPatchRequest;
use crate::tools::runtimes::apply_patch::ApplyPatchRuntime;
use crate::tools::sandboxing::ToolCtx;
use codex_apply_patch::ApplyPatchAction;
use codex_apply_patch::ApplyPatchFileChange;
use codex_protocol::models::FileSystemPermissions;
use codex_protocol::models::PermissionProfile;
use codex_sandboxing::policy_transforms::effective_file_system_sandbox_policy;
use codex_sandboxing::policy_transforms::merge_permission_profiles;
use codex_sandboxing::policy_transforms::normalize_additional_permissions;
use codex_tools::ApplyPatchToolArgs;
use codex_utils_absolute_path::AbsolutePathBuf;
use std::collections::BTreeSet;
use std::sync::Arc;

pub struct ApplyPatchHandler;

fn file_paths_for_action(action: &ApplyPatchAction) -> Vec<AbsolutePathBuf> {
    let mut keys = Vec::new();
    let cwd = action.cwd.as_path();

    for (path, change) in action.changes() {
        if let Some(key) = to_abs_path(cwd, path) {
            keys.push(key);
        }

        if let ApplyPatchFileChange::Update { move_path, .. } = change
            && let Some(dest) = move_path
            && let Some(key) = to_abs_path(cwd, dest)
        {
            keys.push(key);
        }
    }

    keys
}

fn to_abs_path(cwd: &Path, path: &Path) -> Option<AbsolutePathBuf> {
    AbsolutePathBuf::resolve_path_against_base(path, cwd).ok()
}

fn write_permissions_for_paths(
    file_paths: &[AbsolutePathBuf],
    file_system_sandbox_policy: &codex_protocol::permissions::FileSystemSandboxPolicy,
    cwd: &Path,
) -> Option<PermissionProfile> {
    let write_paths = file_paths
        .iter()
        .map(|path| {
            path.parent()
                .unwrap_or_else(|| path.clone())
                .into_path_buf()
        })
        .filter(|path| !file_system_sandbox_policy.can_write_path_with_cwd(path.as_path(), cwd))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(AbsolutePathBuf::from_absolute_path)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;

    let permissions = (!write_paths.is_empty()).then_some(PermissionProfile {
        file_system: Some(FileSystemPermissions {
            read: Some(vec![]),
            write: Some(write_paths),
        }),
        ..Default::default()
    })?;

    normalize_additional_permissions(permissions).ok()
}

async fn effective_patch_permissions(
    session: &Session,
    turn: &TurnContext,
    action: &ApplyPatchAction,
) -> (
    Vec<AbsolutePathBuf>,
    crate::tools::handlers::EffectiveAdditionalPermissions,
    codex_protocol::permissions::FileSystemSandboxPolicy,
) {
    let file_paths = file_paths_for_action(action);
    let granted_permissions = merge_permission_profiles(
        session.granted_session_permissions().await.as_ref(),
        session.granted_turn_permissions().await.as_ref(),
    );
    let file_system_sandbox_policy = effective_file_system_sandbox_policy(
        &turn.file_system_sandbox_policy,
        granted_permissions.as_ref(),
    );
    let effective_additional_permissions = apply_granted_turn_permissions(
        session,
        crate::sandboxing::SandboxPermissions::UseDefault,
        write_permissions_for_paths(&file_paths, &file_system_sandbox_policy, turn.cwd.as_path()),
    )
    .await;

    (
        file_paths,
        effective_additional_permissions,
        file_system_sandbox_policy,
    )
}

impl ToolHandler for ApplyPatchHandler {
    type Output = ApplyPatchToolOutput;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    fn matches_kind(&self, payload: &ToolPayload) -> bool {
        matches!(
            payload,
            ToolPayload::Function { .. } | ToolPayload::Custom { .. }
        )
    }

    async fn is_mutating(&self, _invocation: &ToolInvocation) -> bool {
        true
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        let ToolInvocation {
            session,
            turn,
            tracker,
            call_id,
            tool_name,
            payload,
            ..
        } = invocation;

        let patch_input = match payload {
            ToolPayload::Function { arguments } => {
                let args: ApplyPatchToolArgs = parse_arguments(&arguments)?;
                args.input
            }
            ToolPayload::Custom { input } => input,
            _ => {
                return Err(FunctionCallError::RespondToModel(
                    "apply_patch handler received unsupported payload".to_string(),
                ));
            }
        };

        // Re-parse and verify the patch so we can compute changes and approval.
        // Avoid building temporary ExecParams/command vectors; derive directly from inputs.
        let cwd = turn.cwd.clone();
        let command = vec!["apply_patch".to_string(), patch_input.clone()];
        match codex_apply_patch::maybe_parse_apply_patch_verified(&command, &cwd) {
            codex_apply_patch::MaybeApplyPatchVerified::Body(changes) => {
                let (file_paths, effective_additional_permissions, file_system_sandbox_policy) =
                    effective_patch_permissions(session.as_ref(), turn.as_ref(), &changes).await;
                match apply_patch::apply_patch(turn.as_ref(), &file_system_sandbox_policy, changes)
                    .await
                {
                    InternalApplyPatchInvocation::Output(item) => {
                        let content = item?;
                        Ok(ApplyPatchToolOutput::from_text(content))
                    }
                    InternalApplyPatchInvocation::DelegateToExec(apply) => {
                        let changes = convert_apply_patch_to_protocol(&apply.action);
                        let emitter =
                            ToolEmitter::apply_patch(changes.clone(), apply.auto_approved);
                        let event_ctx = ToolEventCtx::new(
                            session.as_ref(),
                            turn.as_ref(),
                            &call_id,
                            Some(&tracker),
                        );
                        emitter.begin(event_ctx).await;

                        let req = ApplyPatchRequest {
                            action: apply.action,
                            file_paths,
                            changes,
                            exec_approval_requirement: apply.exec_approval_requirement,
                            additional_permissions: effective_additional_permissions
                                .additional_permissions,
                            permissions_preapproved: effective_additional_permissions
                                .permissions_preapproved,
                            timeout_ms: None,
                        };

                        let mut orchestrator = ToolOrchestrator::new();
                        let mut runtime = ApplyPatchRuntime::new();
                        let tool_ctx = ToolCtx {
                            session: session.clone(),
                            turn: turn.clone(),
                            call_id: call_id.clone(),
                            tool_name: tool_name.to_string(),
                        };
                        let out = orchestrator
                            .run(
                                &mut runtime,
                                &req,
                                &tool_ctx,
                                turn.as_ref(),
                                turn.approval_policy.value(),
                            )
                            .await
                            .map(|result| result.output);
                        let event_ctx = ToolEventCtx::new(
                            session.as_ref(),
                            turn.as_ref(),
                            &call_id,
                            Some(&tracker),
                        );
                        let content = emitter.finish(event_ctx, out).await?;
                        Ok(ApplyPatchToolOutput::from_text(content))
                    }
                }
            }
            codex_apply_patch::MaybeApplyPatchVerified::CorrectnessError(parse_error) => {
                Err(FunctionCallError::RespondToModel(format!(
                    "apply_patch verification failed: {parse_error}"
                )))
            }
            codex_apply_patch::MaybeApplyPatchVerified::ShellParseError(error) => {
                tracing::trace!("Failed to parse apply_patch input, {error:?}");
                Err(FunctionCallError::RespondToModel(
                    "apply_patch handler received invalid patch input".to_string(),
                ))
            }
            codex_apply_patch::MaybeApplyPatchVerified::NotApplyPatch => {
                Err(FunctionCallError::RespondToModel(
                    "apply_patch handler received non-apply_patch input".to_string(),
                ))
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn intercept_apply_patch(
    command: &[String],
    cwd: &Path,
    timeout_ms: Option<u64>,
    session: Arc<Session>,
    turn: Arc<TurnContext>,
    tracker: Option<&SharedTurnDiffTracker>,
    call_id: &str,
    tool_name: &str,
) -> Result<Option<FunctionToolOutput>, FunctionCallError> {
    match codex_apply_patch::maybe_parse_apply_patch_verified(command, cwd) {
        codex_apply_patch::MaybeApplyPatchVerified::Body(changes) => {
            session
                .record_model_warning(
                    format!(
                        "apply_patch was requested via {tool_name}. Use the apply_patch tool instead of exec_command."
                    ),
                    turn.as_ref(),
                )
                .await;
            let (approval_keys, effective_additional_permissions, file_system_sandbox_policy) =
                effective_patch_permissions(session.as_ref(), turn.as_ref(), &changes).await;
            match apply_patch::apply_patch(turn.as_ref(), &file_system_sandbox_policy, changes)
                .await
            {
                InternalApplyPatchInvocation::Output(item) => {
                    let content = item?;
                    Ok(Some(FunctionToolOutput::from_text(content, Some(true))))
                }
                InternalApplyPatchInvocation::DelegateToExec(apply) => {
                    let changes = convert_apply_patch_to_protocol(&apply.action);
                    let emitter = ToolEmitter::apply_patch(changes.clone(), apply.auto_approved);
                    let event_ctx = ToolEventCtx::new(
                        session.as_ref(),
                        turn.as_ref(),
                        call_id,
                        tracker.as_ref().copied(),
                    );
                    emitter.begin(event_ctx).await;

                    let req = ApplyPatchRequest {
                        action: apply.action,
                        file_paths: approval_keys,
                        changes,
                        exec_approval_requirement: apply.exec_approval_requirement,
                        additional_permissions: effective_additional_permissions
                            .additional_permissions,
                        permissions_preapproved: effective_additional_permissions
                            .permissions_preapproved,
                        timeout_ms,
                    };

                    let mut orchestrator = ToolOrchestrator::new();
                    let mut runtime = ApplyPatchRuntime::new();
                    let tool_ctx = ToolCtx {
                        session: session.clone(),
                        turn: turn.clone(),
                        call_id: call_id.to_string(),
                        tool_name: tool_name.to_string(),
                    };
                    let out = orchestrator
                        .run(
                            &mut runtime,
                            &req,
                            &tool_ctx,
                            turn.as_ref(),
                            turn.approval_policy.value(),
                        )
                        .await
                        .map(|result| result.output);
                    let event_ctx = ToolEventCtx::new(
                        session.as_ref(),
                        turn.as_ref(),
                        call_id,
                        tracker.as_ref().copied(),
                    );
                    let content = emitter.finish(event_ctx, out).await?;
                    Ok(Some(FunctionToolOutput::from_text(content, Some(true))))
                }
            }
        }
        codex_apply_patch::MaybeApplyPatchVerified::CorrectnessError(parse_error) => {
            Err(FunctionCallError::RespondToModel(format!(
                "apply_patch verification failed: {parse_error}"
            )))
        }
        codex_apply_patch::MaybeApplyPatchVerified::ShellParseError(error) => {
            tracing::trace!("Failed to parse apply_patch input, {error:?}");
            Ok(None)
        }
        codex_apply_patch::MaybeApplyPatchVerified::NotApplyPatch => Ok(None),
    }
}

#[cfg(test)]
async fn run_apply_patch_in_process(
    action: &ApplyPatchAction,
) -> Result<codex_protocol::exec_output::ExecToolCallOutput, String> {
    let start = std::time::Instant::now();
    let (patch, path_rewrites) = absolutize_apply_patch(action);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let exit_code = match codex_apply_patch::apply_patch(&patch, &mut stdout, &mut stderr) {
        Ok(()) => 0,
        Err(_) => 1,
    };
    let stdout = rewrite_apply_patch_output(
        String::from_utf8_lossy(&stdout).into_owned(),
        &path_rewrites,
    );
    let stderr = rewrite_apply_patch_output(
        String::from_utf8_lossy(&stderr).into_owned(),
        &path_rewrites,
    );
    let aggregated = if stderr.is_empty() {
        stdout.clone()
    } else if stdout.is_empty() {
        stderr.clone()
    } else {
        format!("{stdout}{stderr}")
    };

    Ok(codex_protocol::exec_output::ExecToolCallOutput {
        exit_code,
        stdout: codex_protocol::exec_output::StreamOutput::new(stdout),
        stderr: codex_protocol::exec_output::StreamOutput::new(stderr),
        aggregated_output: codex_protocol::exec_output::StreamOutput::new(aggregated),
        duration: start.elapsed(),
        timed_out: false,
    })
}

#[cfg(test)]
fn absolutize_apply_patch(action: &ApplyPatchAction) -> (String, Vec<(String, String)>) {
    let mut rewritten_lines = Vec::new();
    let mut path_rewrites = Vec::new();

    for line in action.patch.lines() {
        if let Some(path) = line.strip_prefix("*** Add File: ") {
            let absolute = absolutize_patch_path(path, &action.cwd);
            path_rewrites.push((absolute.display().to_string(), path.to_string()));
            rewritten_lines.push(format!("*** Add File: {}", absolute.display()));
        } else if let Some(path) = line.strip_prefix("*** Delete File: ") {
            let absolute = absolutize_patch_path(path, &action.cwd);
            path_rewrites.push((absolute.display().to_string(), path.to_string()));
            rewritten_lines.push(format!("*** Delete File: {}", absolute.display()));
        } else if let Some(path) = line.strip_prefix("*** Update File: ") {
            let absolute = absolutize_patch_path(path, &action.cwd);
            path_rewrites.push((absolute.display().to_string(), path.to_string()));
            rewritten_lines.push(format!("*** Update File: {}", absolute.display()));
        } else if let Some(path) = line.strip_prefix("*** Move to: ") {
            let absolute = absolutize_patch_path(path, &action.cwd);
            path_rewrites.push((absolute.display().to_string(), path.to_string()));
            rewritten_lines.push(format!("*** Move to: {}", absolute.display()));
        } else {
            rewritten_lines.push(line.to_string());
        }
    }

    path_rewrites.sort_by(|left, right| right.0.len().cmp(&left.0.len()));
    path_rewrites.dedup();

    let mut rewritten = rewritten_lines.join("\n");
    if action.patch.ends_with('\n') {
        rewritten.push('\n');
    }
    (rewritten, path_rewrites)
}

#[cfg(test)]
fn absolutize_patch_path(path: &str, cwd: &Path) -> std::path::PathBuf {
    let path = Path::new(path);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    }
}

#[cfg(test)]
fn rewrite_apply_patch_output(output: String, path_rewrites: &[(String, String)]) -> String {
    let mut rewritten = output
        .lines()
        .map(|line| rewrite_apply_patch_output_line(line, path_rewrites))
        .collect::<Vec<_>>()
        .join("\n");
    if output.ends_with('\n') {
        rewritten.push('\n');
    }
    rewritten
}

#[cfg(test)]
fn rewrite_apply_patch_output_line(line: &str, path_rewrites: &[(String, String)]) -> String {
    [
        "A ",
        "M ",
        "D ",
        "Failed to create parent directories for ",
        "Failed to write file ",
        "Failed to delete file ",
        "Failed to remove original ",
        "Failed to read file to update ",
        "Failed to find expected lines in ",
    ]
    .into_iter()
    .find_map(|prefix| rewrite_line_path_after_prefix(line, prefix, path_rewrites))
    .or_else(|| rewrite_context_lookup_line(line, path_rewrites))
    .unwrap_or_else(|| line.to_string())
}

#[cfg(test)]
fn rewrite_line_path_after_prefix(
    line: &str,
    prefix: &str,
    path_rewrites: &[(String, String)],
) -> Option<String> {
    let remainder = line.strip_prefix(prefix)?;
    let (rewritten, suffix) = rewrite_path_with_optional_suffix(remainder, path_rewrites)?;
    Some(format!("{prefix}{rewritten}{suffix}"))
}

#[cfg(test)]
fn rewrite_context_lookup_line(line: &str, path_rewrites: &[(String, String)]) -> Option<String> {
    let (prefix, path) = line.rsplit_once(" in ")?;
    if !prefix.starts_with("Failed to find context '") {
        return None;
    }
    let rewritten = rewrite_exact_path(path, path_rewrites)?;
    Some(format!("{prefix} in {rewritten}"))
}

#[cfg(test)]
fn rewrite_exact_path<'a>(path: &'a str, path_rewrites: &'a [(String, String)]) -> Option<&'a str> {
    path_rewrites
        .iter()
        .find_map(|(absolute, original)| (path == absolute).then_some(original.as_str()))
}

#[cfg(test)]
fn rewrite_path_with_optional_suffix<'a>(
    path: &'a str,
    path_rewrites: &'a [(String, String)],
) -> Option<(&'a str, &'a str)> {
    path_rewrites.iter().find_map(|(absolute, original)| {
        let suffix = path.strip_prefix(absolute)?;
        (suffix.is_empty() || suffix.starts_with(':')).then_some((original.as_str(), suffix))
    })
}

#[cfg(test)]
#[path = "apply_patch_tests.rs"]
mod tests;
