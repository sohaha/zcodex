use async_trait::async_trait;
use codex_protocol::ThreadId;
use codex_protocol::models::ShellCommandToolCallParams;
use codex_protocol::models::ShellToolCallParams;
use codex_rtk::ShellCommandRewriteKind;
use codex_rtk::analyze_shell_command;
use std::path::PathBuf;
use std::sync::Arc;

use crate::codex::TurnContext;
use crate::exec::ExecCapturePolicy;
use crate::exec::ExecParams;
use crate::exec_env::create_env;
use crate::exec_env::explicit_snapshot_env_overrides;
use crate::exec_env::prepend_arg0_helper_dir_to_path;
use crate::exec_policy::ExecApprovalRequest;
use crate::function_tool::FunctionCallError;
use crate::is_safe_command::is_known_safe_command;
use crate::protocol::ExecCommandSource;
use crate::shell::Shell;
use crate::skills::maybe_emit_implicit_skill_invocation;
use crate::tools::context::FunctionToolOutput;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::events::ToolEmitter;
use crate::tools::events::ToolEventCtx;
use crate::tools::handlers::apply_granted_turn_permissions;
use crate::tools::handlers::apply_patch::intercept_apply_patch;
use crate::tools::handlers::implicit_granted_permissions;
use crate::tools::handlers::normalize_and_validate_additional_permissions;
use crate::tools::handlers::parse_arguments_with_base_path;
use crate::tools::handlers::resolve_workdir_base_path;
use crate::tools::orchestrator::ToolOrchestrator;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::ToolKind;
use crate::tools::runtimes::shell::ShellRequest;
use crate::tools::runtimes::shell::ShellRuntime;
use crate::tools::runtimes::shell::ShellRuntimeBackend;
use crate::tools::sandboxing::ToolCtx;
use crate::tools::spec::ShellCommandBackendConfig;
use codex_features::Feature;
use codex_protocol::models::PermissionProfile;
use std::collections::HashMap;
pub struct ShellHandler;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ShellCommandBackend {
    Classic,
    ZshFork,
}

pub struct ShellCommandHandler {
    backend: ShellCommandBackend,
}

struct RunExecLikeArgs {
    tool_name: String,
    exec_params: ExecParams,
    display_command: Option<Vec<String>>,
    env_assignments: Vec<String>,
    additional_permissions: Option<PermissionProfile>,
    prefix_rule: Option<Vec<String>>,
    interaction_input: Option<String>,
    model_output_prefix: Option<String>,
    session: Arc<crate::codex::Session>,
    turn: Arc<TurnContext>,
    tracker: crate::tools::context::SharedTurnDiffTracker,
    call_id: String,
    freeform: bool,
    shell_runtime_backend: ShellRuntimeBackend,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RoutedCommand {
    command: String,
    display_command: Option<String>,
    env_assignments: Vec<String>,
    interaction_input: Option<String>,
    model_output_prefix: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RoutedCommandParts {
    leading_env: Vec<String>,
    argv: Vec<String>,
}

impl ShellHandler {
    fn to_exec_params(
        params: &ShellToolCallParams,
        turn_context: &TurnContext,
        thread_id: ThreadId,
    ) -> ExecParams {
        let mut env = create_env(&turn_context.shell_environment_policy, Some(thread_id));
        prepend_arg0_helper_dir_to_path(
            &mut env,
            None,
            turn_context.codex_linux_sandbox_exe.as_deref(),
        );
        ExecParams {
            command: params.command.clone(),
            cwd: turn_context.resolve_path(params.workdir.clone()),
            expiration: params.timeout_ms.into(),
            capture_policy: ExecCapturePolicy::ShellTool,
            env,
            network: turn_context.network.clone(),
            sandbox_permissions: params.sandbox_permissions.unwrap_or_default(),
            windows_sandbox_level: turn_context.windows_sandbox_level,
            windows_sandbox_private_desktop: turn_context
                .config
                .permissions
                .windows_sandbox_private_desktop,
            justification: params.justification.clone(),
            arg0: None,
        }
    }
}

impl ShellCommandHandler {
    fn codex_executable_path(path_env: Option<&str>, workdir: &std::path::Path) -> Option<PathBuf> {
        Self::resolve_codex_executable_path(
            std::env::current_exe().ok().as_deref(),
            path_env,
            workdir,
        )
    }

    fn resolve_codex_executable_path(
        current_exe: Option<&std::path::Path>,
        path_env: Option<&str>,
        workdir: &std::path::Path,
    ) -> Option<PathBuf> {
        if let Some(current_exe) = current_exe
            && current_exe.is_file()
        {
            return Some(current_exe.to_path_buf());
        }

        let path_env =
            path_env.and_then(|path_env| Self::normalize_path_env_for_lookup(path_env, workdir));

        if let Some(path_env) = path_env.as_deref()
            && let Ok(codex_exe) = which::which_in("codex", Some(path_env), workdir)
        {
            return Some(codex_exe);
        }

        which::which("codex").ok()
    }

    fn normalize_path_env_for_lookup(path_env: &str, workdir: &std::path::Path) -> Option<String> {
        let normalized = std::env::split_paths(path_env)
            .map(|path| {
                if path.is_relative() {
                    workdir.join(path)
                } else {
                    path
                }
            })
            .collect::<Vec<_>>();
        std::env::join_paths(normalized)
            .ok()
            .and_then(|path| path.into_string().ok())
            .or_else(|| Some(path_env.to_string()))
    }

    fn shell_runtime_backend(&self) -> ShellRuntimeBackend {
        match self.backend {
            ShellCommandBackend::Classic => ShellRuntimeBackend::ShellCommandClassic,
            ShellCommandBackend::ZshFork => ShellRuntimeBackend::ShellCommandZshFork,
        }
    }

    fn resolve_use_login_shell(
        login: Option<bool>,
        allow_login_shell: bool,
    ) -> Result<bool, FunctionCallError> {
        if !allow_login_shell && login == Some(true) {
            return Err(FunctionCallError::RespondToModel(
                "login shell is disabled by config; omit `login` or set it to false.".to_string(),
            ));
        }

        Ok(login.unwrap_or(allow_login_shell))
    }

    fn base_command(shell: &Shell, command: &str, use_login_shell: bool) -> Vec<String> {
        shell.derive_exec_args(command, use_login_shell)
    }

    fn route_command(command: &str) -> RoutedCommand {
        let trimmed = command.trim();
        let analysis = analyze_shell_command(command);
        match analysis.kind {
            ShellCommandRewriteKind::AlreadyRtk => {
                let Some(parts) = split_routed_command(&analysis.command) else {
                    return RoutedCommand {
                        command: analysis.command,
                        display_command: Some(trimmed.to_string()),
                        env_assignments: Vec::new(),
                        interaction_input: None,
                        model_output_prefix: None,
                    };
                };
                let executed_command = if parts.leading_env.is_empty() {
                    analysis.command
                } else {
                    render_exec_argv(&parts.argv)
                };
                tracing::info!(
                    target: "codex_core::shell_rtk",
                    original = %trimmed,
                    executed = %render_exec_command(&parts),
                    "shell_command already routed via RTK"
                );
                RoutedCommand {
                    command: executed_command,
                    display_command: Some(trimmed.to_string()),
                    env_assignments: parts.leading_env,
                    interaction_input: None,
                    model_output_prefix: None,
                }
            }
            ShellCommandRewriteKind::Rewritten => {
                let Some(parts) = split_routed_command(&analysis.command) else {
                    return RoutedCommand {
                        command: analysis.command,
                        display_command: None,
                        env_assignments: Vec::new(),
                        interaction_input: None,
                        model_output_prefix: None,
                    };
                };
                let executed_command = if parts.leading_env.is_empty() {
                    analysis.command
                } else {
                    render_exec_argv(&parts.argv)
                };
                let display_command = logical_rtk_command(&parts);
                tracing::info!(
                    target: "codex_core::shell_rtk",
                    original = %trimmed,
                    executed = %render_exec_command(&parts),
                    "shell_command routed via embedded RTK"
                );
                RoutedCommand {
                    model_output_prefix: Some(format!(
                        "[shell_command routed via embedded RTK]\noriginal: {trimmed}\nrewritten: {display_command}"
                    )),
                    interaction_input: Some(trimmed.to_string()),
                    command: executed_command,
                    display_command: Some(display_command),
                    env_assignments: parts.leading_env,
                }
            }
            ShellCommandRewriteKind::Passthrough { reason, candidate } => {
                let executed_command = analysis.command.clone();
                tracing::info!(
                    target: "codex_core::shell_rtk",
                    original = %trimmed,
                    executed = %executed_command,
                    reason = %reason.as_str(),
                    candidate = candidate,
                    "shell_command kept raw"
                );
                RoutedCommand {
                    command: analysis.command,
                    display_command: None,
                    env_assignments: Vec::new(),
                    interaction_input: None,
                    model_output_prefix: Some(format!(
                        "[shell_command kept raw]\noriginal: {}\nexecuted: {}\nreason: {}",
                        trimmed,
                        executed_command,
                        reason.as_str()
                    )),
                }
            }
        }
    }

    fn to_exec_params(
        params: &ShellCommandToolCallParams,
        session: &crate::codex::Session,
        turn_context: &TurnContext,
        thread_id: ThreadId,
        allow_login_shell: bool,
    ) -> Result<ExecParams, FunctionCallError> {
        let shell = session.user_shell();
        let use_login_shell = Self::resolve_use_login_shell(params.login, allow_login_shell)?;
        let command = Self::base_command(shell.as_ref(), &params.command, use_login_shell);
        let mut env = create_env(&turn_context.shell_environment_policy, Some(thread_id));
        prepend_arg0_helper_dir_to_path(
            &mut env,
            session.services.main_execve_wrapper_exe.as_deref(),
            turn_context.codex_linux_sandbox_exe.as_deref(),
        );

        Ok(ExecParams {
            command,
            cwd: turn_context.resolve_path(params.workdir.clone()),
            expiration: params.timeout_ms.into(),
            capture_policy: ExecCapturePolicy::ShellTool,
            env,
            network: turn_context.network.clone(),
            sandbox_permissions: params.sandbox_permissions.unwrap_or_default(),
            windows_sandbox_level: turn_context.windows_sandbox_level,
            windows_sandbox_private_desktop: turn_context
                .config
                .permissions
                .windows_sandbox_private_desktop,
            justification: params.justification.clone(),
            arg0: None,
        })
    }
}

fn resolve_rtk_physical_command(command: &str, codex_exe: Option<&std::path::Path>) -> String {
    let Some(codex_exe) = codex_exe else {
        return command.to_string();
    };
    let Some(mut argv) = shlex::split(command) else {
        return command.to_string();
    };
    let Some(index) = argv.iter().position(|token| token == "rtk") else {
        return command.to_string();
    };
    argv[index] = codex_exe.to_string_lossy().into_owned();
    argv.insert(index + 1, "rtk".to_string());
    render_exec_argv(&argv)
}

fn logical_rtk_command(parts: &RoutedCommandParts) -> String {
    let mut parts = parts.clone();
    let Some(index) = parts.argv.iter().position(|token| token == "rtk") else {
        return render_display_command(&parts);
    };
    parts.argv.insert(index, "codex".to_string());
    render_display_command(&parts)
}

fn split_routed_command(command: &str) -> Option<RoutedCommandParts> {
    let tokens = shlex::split(command)?;
    let split_at = tokens
        .iter()
        .take_while(|token| looks_like_env_assignment(token))
        .count();
    let (leading_env, argv) = tokens.split_at(split_at);
    if argv.is_empty() {
        return None;
    }
    Some(RoutedCommandParts {
        leading_env: leading_env.to_vec(),
        argv: argv.to_vec(),
    })
}

fn render_exec_command(parts: &RoutedCommandParts) -> String {
    render_command(parts, render_exec_argv)
}

fn render_display_command(parts: &RoutedCommandParts) -> String {
    render_command(parts, |argv| {
        argv.iter()
            .cloned()
            .map(render_display_token)
            .collect::<Vec<_>>()
            .join(" ")
    })
}

fn render_exec_argv(argv: &[String]) -> String {
    codex_shell_command::parse_command::shlex_join(argv)
}

fn render_command(parts: &RoutedCommandParts, render_argv: impl Fn(&[String]) -> String) -> String {
    let env_prefix = if parts.leading_env.is_empty() {
        None
    } else {
        Some(parts.leading_env.join(" "))
    };
    let argv = render_argv(&parts.argv);
    match env_prefix {
        Some(env_prefix) => format!("{env_prefix} {argv}"),
        None => argv,
    }
}

fn render_display_token(token: String) -> String {
    if looks_like_env_assignment(&token) {
        return token;
    }
    codex_shell_command::parse_command::shlex_join(&[token])
}

fn looks_like_env_assignment(token: &str) -> bool {
    let Some((name, _value)) = token.split_once('=') else {
        return false;
    };
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first == '_' || first.is_ascii_alphabetic()) {
        return false;
    }
    chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn parse_env_assignment(token: &str) -> Option<(&str, &str)> {
    let (name, value) = token.split_once('=')?;
    if looks_like_env_assignment(token) {
        Some((name, value))
    } else {
        None
    }
}

fn apply_env_assignments(env: &mut HashMap<String, String>, assignments: &[String]) {
    for assignment in assignments {
        let Some((name, raw_value)) = parse_env_assignment(assignment) else {
            continue;
        };
        let value = expand_env_assignment_value(raw_value, env);
        env.insert(name.to_string(), value);
    }
}

fn expand_env_assignment_value(raw_value: &str, env: &HashMap<String, String>) -> String {
    let mut output = String::new();
    let chars = raw_value.chars().collect::<Vec<_>>();
    let mut index = 0;
    while let Some(ch) = chars.get(index).copied() {
        if ch != '$' {
            output.push(ch);
            index += 1;
            continue;
        }

        match chars.get(index + 1).copied() {
            Some('{') => {
                let mut end = index + 2;
                while chars.get(end).is_some_and(|next| *next != '}') {
                    end += 1;
                }
                if chars.get(end) == Some(&'}') {
                    let name = chars[index + 2..end].iter().collect::<String>();
                    output.push_str(env.get(&name).map_or("", String::as_str));
                    index = end + 1;
                } else {
                    output.push(ch);
                    index += 1;
                }
            }
            Some(next) if next == '_' || next.is_ascii_alphabetic() => {
                let mut end = index + 2;
                while chars
                    .get(end)
                    .is_some_and(|next| *next == '_' || next.is_ascii_alphanumeric())
                {
                    end += 1;
                }
                let name = chars[index + 1..end].iter().collect::<String>();
                output.push_str(env.get(&name).map_or("", String::as_str));
                index = end;
            }
            _ => {
                output.push(ch);
                index += 1;
            }
        }
    }
    output
}

impl From<ShellCommandBackendConfig> for ShellCommandHandler {
    fn from(config: ShellCommandBackendConfig) -> Self {
        let backend = match config {
            ShellCommandBackendConfig::Classic => ShellCommandBackend::Classic,
            ShellCommandBackendConfig::ZshFork => ShellCommandBackend::ZshFork,
        };
        Self { backend }
    }
}

#[async_trait]
impl ToolHandler for ShellHandler {
    type Output = FunctionToolOutput;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    fn matches_kind(&self, payload: &ToolPayload) -> bool {
        matches!(
            payload,
            ToolPayload::Function { .. } | ToolPayload::LocalShell { .. }
        )
    }

    async fn is_mutating(&self, invocation: &ToolInvocation) -> bool {
        match &invocation.payload {
            ToolPayload::Function { arguments } => {
                serde_json::from_str::<ShellToolCallParams>(arguments)
                    .map(|params| !is_known_safe_command(&params.command))
                    .unwrap_or(true)
            }
            ToolPayload::LocalShell { params } => !is_known_safe_command(&params.command),
            _ => true, // unknown payloads => assume mutating
        }
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

        match payload {
            ToolPayload::Function { arguments } => {
                let cwd = resolve_workdir_base_path(&arguments, turn.cwd.as_path())?;
                let params: ShellToolCallParams =
                    parse_arguments_with_base_path(&arguments, cwd.as_path())?;
                let prefix_rule = params.prefix_rule.clone();
                let exec_params =
                    Self::to_exec_params(&params, turn.as_ref(), session.conversation_id);
                Self::run_exec_like(RunExecLikeArgs {
                    tool_name: tool_name.clone(),
                    exec_params,
                    display_command: None,
                    env_assignments: Vec::new(),
                    additional_permissions: params.additional_permissions.clone(),
                    prefix_rule,
                    interaction_input: None,
                    model_output_prefix: None,
                    session,
                    turn,
                    tracker,
                    call_id,
                    freeform: false,
                    shell_runtime_backend: ShellRuntimeBackend::Generic,
                })
                .await
            }
            ToolPayload::LocalShell { params } => {
                let exec_params =
                    Self::to_exec_params(&params, turn.as_ref(), session.conversation_id);
                Self::run_exec_like(RunExecLikeArgs {
                    tool_name: tool_name.clone(),
                    exec_params,
                    display_command: None,
                    env_assignments: Vec::new(),
                    additional_permissions: None,
                    prefix_rule: None,
                    interaction_input: None,
                    model_output_prefix: None,
                    session,
                    turn,
                    tracker,
                    call_id,
                    freeform: false,
                    shell_runtime_backend: ShellRuntimeBackend::Generic,
                })
                .await
            }
            _ => Err(FunctionCallError::RespondToModel(format!(
                "unsupported payload for shell handler: {tool_name}"
            ))),
        }
    }
}

#[async_trait]
impl ToolHandler for ShellCommandHandler {
    type Output = FunctionToolOutput;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    fn matches_kind(&self, payload: &ToolPayload) -> bool {
        matches!(payload, ToolPayload::Function { .. })
    }

    async fn is_mutating(&self, invocation: &ToolInvocation) -> bool {
        let ToolPayload::Function { arguments } = &invocation.payload else {
            return true;
        };

        serde_json::from_str::<ShellCommandToolCallParams>(arguments)
            .map(|params| {
                let use_login_shell = match Self::resolve_use_login_shell(
                    params.login,
                    invocation.turn.tools_config.allow_login_shell,
                ) {
                    Ok(use_login_shell) => use_login_shell,
                    Err(_) => return true,
                };
                let shell = invocation.session.user_shell();
                let command = Self::base_command(shell.as_ref(), &params.command, use_login_shell);
                !is_known_safe_command(&command)
            })
            .unwrap_or(true)
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

        let ToolPayload::Function { arguments } = payload else {
            return Err(FunctionCallError::RespondToModel(format!(
                "unsupported payload for shell_command handler: {tool_name}"
            )));
        };

        let cwd = resolve_workdir_base_path(&arguments, turn.cwd.as_path())?;
        let params: ShellCommandToolCallParams =
            parse_arguments_with_base_path(&arguments, cwd.as_path())?;
        maybe_emit_implicit_skill_invocation(
            session.as_ref(),
            turn.as_ref(),
            &params.command,
            cwd.as_path(),
        )
        .await;
        let mut params = params;
        let mut routed_command = Self::route_command(&params.command);
        let mut routing_env = create_env(
            &turn.shell_environment_policy,
            Some(session.conversation_id),
        );
        prepend_arg0_helper_dir_to_path(
            &mut routing_env,
            session.services.main_execve_wrapper_exe.as_deref(),
            turn.codex_linux_sandbox_exe.as_deref(),
        );
        let rtk_exe = Self::codex_executable_path(
            routing_env
                .iter()
                .find(|(key, _)| key.eq_ignore_ascii_case("PATH"))
                .map(|(_, value)| value.as_str()),
            cwd.as_path(),
        );
        routed_command.command =
            resolve_rtk_physical_command(&routed_command.command, rtk_exe.as_deref());
        let display_command = routed_command
            .display_command
            .as_deref()
            .and_then(shlex::split);
        params.command = routed_command.command.clone();
        let prefix_rule = params.prefix_rule.clone();
        let exec_params = Self::to_exec_params(
            &params,
            session.as_ref(),
            turn.as_ref(),
            session.conversation_id,
            turn.tools_config.allow_login_shell,
        )?;
        ShellHandler::run_exec_like(RunExecLikeArgs {
            tool_name,
            exec_params,
            display_command,
            env_assignments: routed_command.env_assignments,
            additional_permissions: params.additional_permissions.clone(),
            prefix_rule,
            interaction_input: routed_command.interaction_input,
            model_output_prefix: routed_command.model_output_prefix,
            session,
            turn,
            tracker,
            call_id,
            freeform: true,
            shell_runtime_backend: self.shell_runtime_backend(),
        })
        .await
    }
}

impl ShellHandler {
    async fn run_exec_like(args: RunExecLikeArgs) -> Result<FunctionToolOutput, FunctionCallError> {
        let RunExecLikeArgs {
            tool_name,
            exec_params,
            display_command,
            env_assignments,
            additional_permissions,
            prefix_rule,
            interaction_input,
            model_output_prefix,
            session,
            turn,
            tracker,
            call_id,
            freeform,
            shell_runtime_backend,
        } = args;

        let mut exec_params = exec_params;
        let dependency_env = session.dependency_env().await;
        if !dependency_env.is_empty() {
            exec_params.env.extend(dependency_env.clone());
        }
        if !env_assignments.is_empty() {
            apply_env_assignments(&mut exec_params.env, &env_assignments);
        }

        let explicit_env_overrides = explicit_snapshot_env_overrides(
            &turn.shell_environment_policy.r#set,
            &dependency_env,
            &exec_params.env,
        );

        let exec_permission_approvals_enabled =
            session.features().enabled(Feature::ExecPermissionApprovals);
        let requested_additional_permissions = additional_permissions.clone();
        let effective_additional_permissions = apply_granted_turn_permissions(
            session.as_ref(),
            exec_params.sandbox_permissions,
            additional_permissions,
        )
        .await;
        let additional_permissions_allowed = exec_permission_approvals_enabled
            || (session.features().enabled(Feature::RequestPermissionsTool)
                && effective_additional_permissions.permissions_preapproved);
        let normalized_additional_permissions = implicit_granted_permissions(
            exec_params.sandbox_permissions,
            requested_additional_permissions.as_ref(),
            &effective_additional_permissions,
        )
        .map_or_else(
            || {
                normalize_and_validate_additional_permissions(
                    additional_permissions_allowed,
                    turn.approval_policy.value(),
                    effective_additional_permissions.sandbox_permissions,
                    effective_additional_permissions.additional_permissions,
                    effective_additional_permissions.permissions_preapproved,
                    &exec_params.cwd,
                )
            },
            |permissions| Ok(Some(permissions)),
        )
        .map_err(FunctionCallError::RespondToModel)?;

        // Approval policy guard for explicit escalation in non-OnRequest modes.
        // Sticky turn permissions have already been approved, so they should
        // continue through the normal exec approval flow for the command.
        if effective_additional_permissions
            .sandbox_permissions
            .requests_sandbox_override()
            && !effective_additional_permissions.permissions_preapproved
            && !matches!(
                turn.approval_policy.value(),
                codex_protocol::protocol::AskForApproval::OnRequest
            )
        {
            let approval_policy = turn.approval_policy.value();
            return Err(FunctionCallError::RespondToModel(format!(
                "approval policy is {approval_policy:?}; reject command — you should not ask for escalated permissions if the approval policy is {approval_policy:?}"
            )));
        }

        // Intercept apply_patch if present.
        if let Some(output) = intercept_apply_patch(
            &exec_params.command,
            &exec_params.cwd,
            exec_params.expiration.timeout_ms(),
            session.clone(),
            turn.clone(),
            Some(&tracker),
            &call_id,
            tool_name.as_str(),
        )
        .await?
        {
            return Ok(output);
        }

        let source = ExecCommandSource::Agent;
        let emitter = ToolEmitter::shell(
            exec_params.command.clone(),
            display_command,
            exec_params.cwd.clone(),
            source,
            freeform,
            interaction_input,
            model_output_prefix,
        );
        let event_ctx = ToolEventCtx::new(
            session.as_ref(),
            turn.as_ref(),
            &call_id,
            /*turn_diff_tracker*/ None,
        );
        emitter.begin(event_ctx).await;

        let exec_approval_requirement = session
            .services
            .exec_policy
            .create_exec_approval_requirement_for_command(ExecApprovalRequest {
                command: &exec_params.command,
                approval_policy: turn.approval_policy.value(),
                sandbox_policy: turn.sandbox_policy.get(),
                file_system_sandbox_policy: &turn.file_system_sandbox_policy,
                sandbox_permissions: if effective_additional_permissions.permissions_preapproved {
                    codex_protocol::models::SandboxPermissions::UseDefault
                } else {
                    effective_additional_permissions.sandbox_permissions
                },
                prefix_rule,
            })
            .await;

        let req = ShellRequest {
            command: exec_params.command.clone(),
            cwd: exec_params.cwd.clone(),
            timeout_ms: exec_params.expiration.timeout_ms(),
            env: exec_params.env.clone(),
            explicit_env_overrides,
            network: exec_params.network.clone(),
            sandbox_permissions: effective_additional_permissions.sandbox_permissions,
            additional_permissions: normalized_additional_permissions,
            #[cfg(unix)]
            additional_permissions_preapproved: effective_additional_permissions
                .permissions_preapproved,
            justification: exec_params.justification.clone(),
            exec_approval_requirement,
        };
        let mut orchestrator = ToolOrchestrator::new();
        let mut runtime = {
            use ShellRuntimeBackend::*;
            match shell_runtime_backend {
                Generic => ShellRuntime::new(),
                backend @ (ShellCommandClassic | ShellCommandZshFork) => {
                    ShellRuntime::for_shell_command(backend)
                }
            }
        };
        let tool_ctx = ToolCtx {
            session: session.clone(),
            turn: turn.clone(),
            call_id: call_id.clone(),
            tool_name,
        };
        let out = orchestrator
            .run(
                &mut runtime,
                &req,
                &tool_ctx,
                &turn,
                turn.approval_policy.value(),
            )
            .await
            .map(|result| result.output);
        let event_ctx = ToolEventCtx::new(
            session.as_ref(),
            turn.as_ref(),
            &call_id,
            /*turn_diff_tracker*/ None,
        );
        let content = emitter.finish(event_ctx, out).await?;
        Ok(FunctionToolOutput::from_text(content, Some(true)))
    }
}

#[cfg(test)]
#[path = "shell_tests.rs"]
mod tests;
