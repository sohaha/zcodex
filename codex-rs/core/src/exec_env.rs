#[cfg(test)]
use codex_config::types::EnvironmentVariablePattern;
use codex_config::types::ShellEnvironmentPolicy;
use codex_protocol::ThreadId;
use codex_utils_cli::inject_codex_self_exe_env;
use std::collections::HashMap;
use std::path::Path;

pub use codex_config::shell_environment::CODEX_THREAD_ID_ENV_VAR;
pub use codex_utils_cli::CODEX_SELF_EXE_ENV_VAR;

/// Construct an environment map based on the rules in the specified policy. The
/// resulting map can be passed directly to `Command::envs()` after calling
/// `env_clear()` to ensure no unintended variables are leaked to the spawned
/// process.
///
/// The derivation follows the algorithm documented in the struct-level comment
/// for [`ShellEnvironmentPolicy`].
///
/// `CODEX_THREAD_ID` is injected when a thread id is provided, even when
/// `include_only` is set.
pub fn create_env(
    policy: &ShellEnvironmentPolicy,
    thread_id: Option<ThreadId>,
    codex_self_exe: Option<&Path>,
) -> HashMap<String, String> {
    let thread_id = thread_id.map(|thread_id| thread_id.to_string());
    let mut env = codex_config::shell_environment::create_env(policy, thread_id.as_deref());
    inject_codex_self_exe_env(&mut env, codex_self_exe);
    env
}

pub fn prepend_arg0_helper_dir_to_path(
    env: &mut HashMap<String, String>,
    main_execve_wrapper_exe: Option<&Path>,
    codex_linux_sandbox_exe: Option<&Path>,
) {
    let helper_dir = main_execve_wrapper_exe
        .or(codex_linux_sandbox_exe)
        .and_then(Path::parent);
    let Some(helper_dir) = helper_dir else {
        return;
    };

    let helper_dir = helper_dir.to_string_lossy();
    let path_key = if cfg!(target_os = "windows") {
        env.keys()
            .find(|key| key.eq_ignore_ascii_case("PATH"))
            .cloned()
            .unwrap_or_else(|| "PATH".to_string())
    } else {
        "PATH".to_string()
    };
    let path_separator = if cfg!(target_os = "windows") {
        ';'
    } else {
        ':'
    };

    match env.get_mut(&path_key) {
        Some(path) => {
            let already_present = path
                .split(path_separator)
                .any(|entry| entry == helper_dir.as_ref());
            if !already_present {
                *path = format!("{helper_dir}{path_separator}{path}");
            }
        }
        None => {
            env.insert(path_key, helper_dir.into_owned());
        }
    }
}

pub fn explicit_snapshot_env_overrides(
    shell_env_overrides: &HashMap<String, String>,
    dependency_env: &HashMap<String, String>,
    exec_env: &HashMap<String, String>,
) -> HashMap<String, String> {
    let mut explicit_env_overrides = shell_env_overrides.clone();
    for key in dependency_env.keys() {
        if let Some(value) = exec_env.get(key) {
            explicit_env_overrides.insert(key.clone(), value.clone());
        }
    }

    if let Some((path_key, path_value)) = exec_env
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case("PATH"))
    {
        explicit_env_overrides.insert(path_key.clone(), path_value.clone());
    }

    explicit_env_overrides
}

#[cfg(all(test, target_os = "windows"))]
fn create_env_from_vars<I>(
    vars: I,
    policy: &ShellEnvironmentPolicy,
    thread_id: Option<ThreadId>,
) -> HashMap<String, String>
where
    I: IntoIterator<Item = (String, String)>,
{
    let thread_id = thread_id.map(|thread_id| thread_id.to_string());
    codex_config::shell_environment::create_env_from_vars(vars, policy, thread_id.as_deref())
}

#[cfg(test)]
fn populate_env<I>(
    vars: I,
    policy: &ShellEnvironmentPolicy,
    thread_id: Option<ThreadId>,
) -> HashMap<String, String>
where
    I: IntoIterator<Item = (String, String)>,
{
    let thread_id = thread_id.map(|thread_id| thread_id.to_string());
    codex_config::shell_environment::populate_env(vars, policy, thread_id.as_deref())
}

#[cfg(test)]
#[path = "exec_env_tests.rs"]
mod tests;
