/*
Module: runtimes

Concrete ToolRuntime implementations for specific tools. Each runtime stays
small and focused and reuses the orchestrator for approvals + sandbox + retry.
*/
use crate::path_utils;
use crate::shell::Shell;
use crate::skills::SkillMetadata;
use crate::tools::sandboxing::ToolError;
use codex_protocol::models::PermissionProfile;
use codex_sandboxing::SandboxCommand;
use std::collections::HashMap;
use std::path::Path;

pub mod apply_patch;
pub mod shell;
pub mod unified_exec;

#[derive(Debug, Clone)]
pub(crate) struct ExecveSessionApproval {
    /// If this execve session approval is associated with a skill script, this
    /// field contains metadata about the skill.
    #[cfg_attr(not(unix), allow(dead_code))]
    pub skill: Option<SkillMetadata>,
}

/// Shared helper to construct sandbox transform inputs from a tokenized command line.
/// Validates that at least a program is present.
pub(crate) fn build_sandbox_command(
    command: &[String],
    cwd: &Path,
    env: &HashMap<String, String>,
    additional_permissions: Option<PermissionProfile>,
) -> Result<SandboxCommand, ToolError> {
    let (program, args) = command
        .split_first()
        .ok_or_else(|| ToolError::Rejected("command args are empty".to_string()))?;
    Ok(SandboxCommand {
        program: program.clone(),
        args: args.to_vec(),
        cwd: cwd.to_path_buf(),
        env: env.clone(),
        additional_permissions,
    })
}

/// POSIX-only helper: for commands produced by `Shell::derive_exec_args`
/// for Bash/Zsh/sh of the form `[shell_path, "-lc", "<script>"]`, and
/// when a snapshot is configured on the session shell, rewrite the argv
/// to a single non-login shell that sources the snapshot before running
/// the original script:
///
///   shell -lc "<script>"
///   => user_shell -c ". SNAPSHOT (best effort); exec shell -c <script>"
///
/// This wrapper script uses POSIX constructs (`if`, `.`, `exec`) so it can
/// be run by Bash/Zsh/sh. On non-matching commands, or when command cwd does
/// not match the snapshot cwd, this is a no-op.
pub(crate) fn maybe_wrap_shell_lc_with_snapshot(
    command: &[String],
    session_shell: &Shell,
    cwd: &Path,
    explicit_env_overrides: &HashMap<String, String>,
) -> Vec<String> {
    if cfg!(windows) {
        return command.to_vec();
    }

    if command.len() < 3 {
        return command.to_vec();
    }

    let flag = command[1].as_str();
    if flag != "-lc" {
        return command.to_vec();
    }

    let shell_path = session_shell.shell_path.to_string_lossy();
    let original_shell = shell_single_quote(&command[0]);
    let original_script = &command[2];
    let trailing_args = command[3..]
        .iter()
        .map(|arg| format!(" '{}'", shell_single_quote(arg)))
        .collect::<String>();
    let (override_captures, override_exports) = build_override_exports(explicit_env_overrides);
    let snapshot = session_shell
        .shell_snapshot()
        .filter(|snapshot| snapshot.path.exists())
        .filter(|snapshot| {
            if let (Ok(snapshot_cwd), Ok(command_cwd)) = (
                path_utils::normalize_for_path_comparison(snapshot.cwd.as_path()),
                path_utils::normalize_for_path_comparison(cwd),
            ) {
                snapshot_cwd == command_cwd
            } else {
                snapshot.cwd == cwd
            }
        });

    match snapshot {
        Some(snapshot) => {
            let snapshot_path = shell_single_quote(snapshot.path.to_string_lossy().as_ref());
            let original_script = shell_single_quote(original_script);
            let rewritten_script = if override_exports.is_empty() {
                format!(
                    "if . '{snapshot_path}' >/dev/null 2>&1; then :; fi\n\nexec '{original_shell}' -c '{original_script}'{trailing_args}"
                )
            } else {
                format!(
                    "{override_captures}\n\nif . '{snapshot_path}' >/dev/null 2>&1; then :; fi\n\n{override_exports}\n\nexec '{original_shell}' -c '{original_script}'{trailing_args}"
                )
            };

            vec![shell_path.to_string(), "-c".to_string(), rewritten_script]
        }
        None if override_exports.is_empty() => command.to_vec(),
        None => {
            let restored_script = format!("{override_exports}\n\n{original_script}");
            let rewritten_script = format!(
                "{override_captures}\n\nexec '{original_shell}' -lc '{}'{}",
                shell_single_quote(&restored_script),
                trailing_args
            );

            vec![shell_path.to_string(), "-c".to_string(), rewritten_script]
        }
    }
}

fn build_override_exports(explicit_env_overrides: &HashMap<String, String>) -> (String, String) {
    let mut keys = explicit_env_overrides
        .keys()
        .filter(|key| is_valid_shell_variable_name(key))
        .collect::<Vec<_>>();
    keys.sort_unstable();

    if keys.is_empty() {
        return (String::new(), String::new());
    }

    let captures = keys
        .iter()
        .enumerate()
        .map(|(idx, key)| {
            format!(
                "export __CODEX_SNAPSHOT_OVERRIDE_SET_{idx}=\"${{{key}+x}}\"\nexport __CODEX_SNAPSHOT_OVERRIDE_{idx}=\"${{{key}-}}\""
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let restores = keys
        .iter()
        .enumerate()
        .map(|(idx, key)| {
            format!(
                "if [ -n \"${{__CODEX_SNAPSHOT_OVERRIDE_SET_{idx}}}\" ]; then export {key}=\"${{__CODEX_SNAPSHOT_OVERRIDE_{idx}}}\"; else unset {key}; fi\nunset __CODEX_SNAPSHOT_OVERRIDE_SET_{idx} __CODEX_SNAPSHOT_OVERRIDE_{idx}"
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    (captures, restores)
}

fn is_valid_shell_variable_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first == '_' || first.is_ascii_alphabetic()) {
        return false;
    }
    chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

fn shell_single_quote(input: &str) -> String {
    input.replace('\'', r#"'"'"'"#)
}

#[cfg(all(test, unix))]
#[path = "mod_tests.rs"]
mod tests;
