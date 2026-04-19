use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;

pub const CODEX_SELF_EXE_ENV_VAR: &str = "CODEX_SELF_EXE";

const DEFAULT_LAUNCHER_DISPLAY_NAME: &str = "codex";

/// Inject the current Codex launcher path into the shell environment when it
/// is known at runtime.
pub fn inject_codex_self_exe_env(env: &mut HashMap<String, String>, codex_self_exe: Option<&Path>) {
    let Some(codex_self_exe) = codex_self_exe else {
        return;
    };

    env.insert(
        CODEX_SELF_EXE_ENV_VAR.to_string(),
        codex_self_exe.to_string_lossy().into_owned(),
    );
}

/// Returns a user-facing launcher name derived from the current executable
/// path. This keeps hints actionable after users rename the launcher while
/// avoiding absolute-path leakage in model-visible output.
pub fn launcher_display_name(codex_self_exe: Option<&Path>) -> String {
    codex_self_exe
        .and_then(display_name_from_path)
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| DEFAULT_LAUNCHER_DISPLAY_NAME.to_string())
}

pub fn current_launcher_display_name() -> String {
    launcher_display_name(std::env::current_exe().ok().as_deref())
}

pub fn env_launcher_display_name() -> String {
    launcher_display_name(codex_self_exe_from_env().as_deref())
}

pub fn format_launcher_command(codex_self_exe: Option<&Path>, args: &[&str]) -> String {
    let mut parts = Vec::with_capacity(args.len() + 1);
    parts.push(launcher_display_name(codex_self_exe));
    parts.extend(args.iter().map(std::string::ToString::to_string));
    parts.join(" ")
}

pub fn format_current_launcher_command(args: &[&str]) -> String {
    format_launcher_command(std::env::current_exe().ok().as_deref(), args)
}

pub fn format_launcher_command_from_env(args: &[&str]) -> String {
    format_launcher_command(codex_self_exe_from_env().as_deref(), args)
}

fn codex_self_exe_from_env() -> Option<PathBuf> {
    std::env::var_os(CODEX_SELF_EXE_ENV_VAR).map(PathBuf::from)
}

fn display_name_from_path(path: &Path) -> Option<&str> {
    #[cfg(windows)]
    {
        path.file_stem()
            .and_then(|value| value.to_str())
            .filter(|value| !value.is_empty())
            .or_else(|| {
                path.file_name()
                    .and_then(|value| value.to_str())
                    .filter(|value| !value.is_empty())
            })
    }
    #[cfg(not(windows))]
    {
        path.file_name()
            .and_then(|value| value.to_str())
            .filter(|value| !value.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn launcher_display_name_uses_file_name() {
        assert_eq!(
            launcher_display_name(Some(Path::new("/tmp/z"))),
            "z".to_string()
        );
    }

    #[test]
    fn launcher_display_name_falls_back_to_codex() {
        assert_eq!(launcher_display_name(None), "codex".to_string());
    }

    #[test]
    fn format_launcher_command_prefixes_display_name() {
        assert_eq!(
            format_launcher_command(Some(Path::new("/tmp/z")), &["ztok", "git", "status"]),
            "z ztok git status".to_string()
        );
    }

    #[test]
    fn inject_codex_self_exe_env_sets_launcher_path() {
        let mut env = HashMap::new();
        inject_codex_self_exe_env(&mut env, Some(Path::new("/tmp/z")));

        assert_eq!(env.get(CODEX_SELF_EXE_ENV_VAR), Some(&"/tmp/z".to_string()));
    }
}
