use std::fs;
use std::path::Path;

use codex_config::ConfigLayerStack;
use codex_config::ConfigLayerStackOrdering;

use super::ConfiguredHandler;
use super::config::HookHandlerConfig;
use super::config::HooksFile;
use super::config::MatcherGroup;
use crate::events::common::matcher_pattern_for_event;
use crate::events::common::validate_matcher_pattern;

/// The script extension preferred on the current platform.
/// macOS/Linux use `.sh`; Windows uses `.ps1`.
fn platform_script_ext() -> &'static str {
    if cfg!(windows) { ".ps1" } else { ".sh" }
}

/// The script extension that is *not* native to the current platform.
fn non_platform_script_ext() -> &'static str {
    if cfg!(windows) { ".sh" } else { ".ps1" }
}

/// Whether a file extension is one of the recognized script types.
///
/// Only `.sh` and `.ps1` are eligible for platform-based script resolution.
/// Extensions like `.ash`, `.bash`, `.eps1` are intentionally excluded.
fn is_script_ext(ext: &str) -> bool {
    matches!(ext, ".sh" | ".ps1")
}

/// Resolve a hook command to a platform-appropriate script when possible.
///
/// Scans the command string for tokens that look like file paths whose extension
/// is exactly `.sh` or `.ps1`. If the extension does **not** match the current
/// platform's preferred script type (`.ps1` on macOS/Linux, `.sh` on Windows),
/// the function checks whether a same-name file with the native extension exists
/// (resolved relative to `hooks_dir`). If found, the token is replaced in the
/// returned command string.
fn resolve_platform_script(command: &str, hooks_dir: &Path) -> String {
    let wrong_ext = non_platform_script_ext();
    let right_ext = platform_script_ext();
    let mut result = command.to_string();

    for token in command.split_whitespace() {
        // Only consider tokens that look like file paths.
        if !token.contains('/') && !token.contains('\\') && !token.starts_with('.') {
            continue;
        }

        // Extract the file extension and check that it is exactly .sh or .ps1.
        let file_name = token
            .rsplit(|c| c == '/' || c == '\\')
            .next()
            .unwrap_or(token);
        let Some(ext) = file_name.rfind('.') else {
            continue;
        };
        let ext = &file_name[ext..];
        if !is_script_ext(ext) {
            continue;
        }
        // Only resolve when the extension does not match the current platform.
        if ext != wrong_ext {
            continue;
        }

        let script_path = hooks_dir.join(token);
        if !script_path.is_file() {
            // Also try absolute paths as-is.
            let abs = Path::new(token);
            if !abs.is_file() {
                continue;
            }
        }

        let base = token.strip_suffix(wrong_ext).unwrap();
        let alt_name = format!("{base}{right_ext}");
        let alt_path = hooks_dir.join(&alt_name);
        let alt_exists = if alt_path.is_file() {
            true
        } else {
            // Also check absolute.
            Path::new(&alt_name).is_file()
        };

        if alt_exists {
            result = result.replace(token, &alt_name);
        }
    }

    result
}

pub(crate) struct DiscoveryResult {
    pub handlers: Vec<ConfiguredHandler>,
    pub warnings: Vec<String>,
}

pub(crate) fn discover_handlers(config_layer_stack: Option<&ConfigLayerStack>) -> DiscoveryResult {
    let Some(config_layer_stack) = config_layer_stack else {
        return DiscoveryResult {
            handlers: Vec::new(),
            warnings: Vec::new(),
        };
    };

    let mut handlers = Vec::new();
    let mut warnings = Vec::new();
    let mut display_order = 0_i64;

    for layer in config_layer_stack.get_layers(
        ConfigLayerStackOrdering::LowestPrecedenceFirst,
        /*include_disabled*/ false,
    ) {
        let Some(folder) = layer.config_folder() else {
            continue;
        };
        let source_path = folder.join("hooks.json");
        if !source_path.as_path().is_file() {
            continue;
        }

        let contents = match fs::read_to_string(source_path.as_path()) {
            Ok(contents) => contents,
            Err(err) => {
                warnings.push(format!(
                    "failed to read hooks config {}: {err}",
                    source_path.display()
                ));
                continue;
            }
        };

        let parsed: HooksFile = match serde_json::from_str(&contents) {
            Ok(parsed) => parsed,
            Err(err) => {
                warnings.push(format!(
                    "failed to parse hooks config {}: {err}",
                    source_path.display()
                ));
                continue;
            }
        };

        let super::config::HookEvents {
            pre_tool_use,
            post_tool_use,
            session_start,
            user_prompt_submit,
            stop,
        } = parsed.hooks;

        for (event_name, groups) in [
            (
                codex_protocol::protocol::HookEventName::PreToolUse,
                pre_tool_use,
            ),
            (
                codex_protocol::protocol::HookEventName::PostToolUse,
                post_tool_use,
            ),
            (
                codex_protocol::protocol::HookEventName::SessionStart,
                session_start,
            ),
            (
                codex_protocol::protocol::HookEventName::UserPromptSubmit,
                user_prompt_submit,
            ),
            (codex_protocol::protocol::HookEventName::Stop, stop),
        ] {
            append_matcher_groups(
                &mut handlers,
                &mut warnings,
                &mut display_order,
                source_path.as_path(),
                event_name,
                groups,
            );
        }
    }

    DiscoveryResult { handlers, warnings }
}

fn append_group_handlers(
    handlers: &mut Vec<ConfiguredHandler>,
    warnings: &mut Vec<String>,
    display_order: &mut i64,
    source_path: &Path,
    event_name: codex_protocol::protocol::HookEventName,
    matcher: Option<&str>,
    group_handlers: Vec<HookHandlerConfig>,
) {
    if let Some(matcher) = matcher
        && let Err(err) = validate_matcher_pattern(matcher)
    {
        warnings.push(format!(
            "invalid matcher {matcher:?} in {}: {err}",
            source_path.display()
        ));
        return;
    }

    for handler in group_handlers {
        match handler {
            HookHandlerConfig::Command {
                command,
                timeout_sec,
                r#async,
                status_message,
            } => {
                if r#async {
                    warnings.push(format!(
                        "skipping async hook in {}: async hooks are not supported yet",
                        source_path.display()
                    ));
                    continue;
                }
                if command.trim().is_empty() {
                    warnings.push(format!(
                        "skipping empty hook command in {}",
                        source_path.display()
                    ));
                    continue;
                }
                let timeout_sec = timeout_sec.unwrap_or(600).max(1);
                let hooks_dir = source_path.parent().unwrap_or(source_path);
                let command = resolve_platform_script(&command, hooks_dir);
                handlers.push(ConfiguredHandler {
                    event_name,
                    matcher: matcher.map(ToOwned::to_owned),
                    command,
                    timeout_sec,
                    status_message,
                    source_path: source_path.to_path_buf(),
                    display_order: *display_order,
                });
                *display_order += 1;
            }
            HookHandlerConfig::Prompt {} => warnings.push(format!(
                "skipping prompt hook in {}: prompt hooks are not supported yet",
                source_path.display()
            )),
            HookHandlerConfig::Agent {} => warnings.push(format!(
                "skipping agent hook in {}: agent hooks are not supported yet",
                source_path.display()
            )),
        }
    }
}

fn append_matcher_groups(
    handlers: &mut Vec<ConfiguredHandler>,
    warnings: &mut Vec<String>,
    display_order: &mut i64,
    source_path: &Path,
    event_name: codex_protocol::protocol::HookEventName,
    groups: Vec<MatcherGroup>,
) {
    for group in groups {
        append_group_handlers(
            handlers,
            warnings,
            display_order,
            source_path,
            event_name,
            matcher_pattern_for_event(event_name, group.matcher.as_deref()),
            group.hooks,
        );
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;
    use std::path::PathBuf;

    use codex_protocol::protocol::HookEventName;
    use pretty_assertions::assert_eq;

    use super::ConfiguredHandler;
    use super::HookHandlerConfig;
    use super::append_group_handlers;
    use super::is_script_ext;
    use super::non_platform_script_ext;
    use super::platform_script_ext;
    use super::resolve_platform_script;
    use crate::events::common::matcher_pattern_for_event;

    #[test]
    fn user_prompt_submit_ignores_invalid_matcher_during_discovery() {
        let mut handlers = Vec::new();
        let mut warnings = Vec::new();
        let mut display_order = 0;

        append_group_handlers(
            &mut handlers,
            &mut warnings,
            &mut display_order,
            Path::new("/tmp/hooks.json"),
            HookEventName::UserPromptSubmit,
            matcher_pattern_for_event(HookEventName::UserPromptSubmit, Some("[")),
            vec![HookHandlerConfig::Command {
                command: "echo hello".to_string(),
                timeout_sec: None,
                r#async: false,
                status_message: None,
            }],
        );

        assert_eq!(warnings, Vec::<String>::new());
        assert_eq!(
            handlers,
            vec![ConfiguredHandler {
                event_name: HookEventName::UserPromptSubmit,
                matcher: None,
                command: "echo hello".to_string(),
                timeout_sec: 600,
                status_message: None,
                source_path: PathBuf::from("/tmp/hooks.json"),
                display_order: 0,
            }]
        );
    }

    #[test]
    fn pre_tool_use_keeps_valid_matcher_during_discovery() {
        let mut handlers = Vec::new();
        let mut warnings = Vec::new();
        let mut display_order = 0;

        append_group_handlers(
            &mut handlers,
            &mut warnings,
            &mut display_order,
            Path::new("/tmp/hooks.json"),
            HookEventName::PreToolUse,
            matcher_pattern_for_event(HookEventName::PreToolUse, Some("^Bash$")),
            vec![HookHandlerConfig::Command {
                command: "echo hello".to_string(),
                timeout_sec: None,
                r#async: false,
                status_message: None,
            }],
        );

        assert_eq!(warnings, Vec::<String>::new());
        assert_eq!(
            handlers,
            vec![ConfiguredHandler {
                event_name: HookEventName::PreToolUse,
                matcher: Some("^Bash$".to_string()),
                command: "echo hello".to_string(),
                timeout_sec: 600,
                status_message: None,
                source_path: PathBuf::from("/tmp/hooks.json"),
                display_order: 0,
            }]
        );
    }

    #[test]
    fn pre_tool_use_treats_star_matcher_as_match_all() {
        let mut handlers = Vec::new();
        let mut warnings = Vec::new();
        let mut display_order = 0;

        append_group_handlers(
            &mut handlers,
            &mut warnings,
            &mut display_order,
            Path::new("/tmp/hooks.json"),
            HookEventName::PreToolUse,
            matcher_pattern_for_event(HookEventName::PreToolUse, Some("*")),
            vec![HookHandlerConfig::Command {
                command: "echo hello".to_string(),
                timeout_sec: None,
                r#async: false,
                status_message: None,
            }],
        );

        assert_eq!(warnings, Vec::<String>::new());
        assert_eq!(handlers.len(), 1);
        assert_eq!(handlers[0].matcher.as_deref(), Some("*"));
    }

    #[test]
    fn post_tool_use_keeps_valid_matcher_during_discovery() {
        let mut handlers = Vec::new();
        let mut warnings = Vec::new();
        let mut display_order = 0;

        append_group_handlers(
            &mut handlers,
            &mut warnings,
            &mut display_order,
            Path::new("/tmp/hooks.json"),
            HookEventName::PostToolUse,
            matcher_pattern_for_event(HookEventName::PostToolUse, Some("Edit|Write")),
            vec![HookHandlerConfig::Command {
                command: "echo hello".to_string(),
                timeout_sec: None,
                r#async: false,
                status_message: None,
            }],
        );

        assert_eq!(warnings, Vec::<String>::new());
        assert_eq!(handlers.len(), 1);
        assert_eq!(handlers[0].event_name, HookEventName::PostToolUse);
        assert_eq!(handlers[0].matcher.as_deref(), Some("Edit|Write"));
    }

    // --- resolve_platform_script tests ---

    #[test]
    fn platform_script_ext_values_are_consistent() {
        // .sh and .ps1 must differ.
        assert_ne!(platform_script_ext(), non_platform_script_ext());
    }

    #[test]
    fn non_platform_ext_script_is_replaced_when_alternative_exists() {
        // On non-Windows the "wrong" extension is .ps1 and "right" is .sh.
        // We test by constructing a temp dir with both files and verifying
        // that the non-platform script in the command gets replaced.
        let dir = tempfile::tempdir().unwrap();
        let sh = dir.path().join("my-hook.sh");
        let ps1 = dir.path().join("my-hook.ps1");
        fs::write(&sh, "#!/bin/sh").unwrap();
        fs::write(&ps1, "# ps1").unwrap();

        let wrong_ext = non_platform_script_ext();
        let right_ext = platform_script_ext();
        let command = format!("./my-hook{wrong_ext}");

        let resolved = resolve_platform_script(&command, dir.path());
        assert_eq!(resolved, format!("./my-hook{right_ext}"));
    }

    #[test]
    fn platform_native_ext_script_is_not_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let sh = dir.path().join("hook.sh");
        fs::write(&sh, "#!/bin/sh").unwrap();

        let right_ext = platform_script_ext();
        let command = format!("./hook{right_ext}");

        let resolved = resolve_platform_script(&command, dir.path());
        // Should remain unchanged because the extension already matches the platform.
        assert_eq!(resolved, command);
    }

    #[test]
    fn non_platform_script_without_alternative_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        // Only the "wrong" extension file exists.
        let wrong_ext = non_platform_script_ext();
        let wrong_file = dir.path().join(format!("solo{wrong_ext}"));
        fs::write(&wrong_file, "").unwrap();

        let command = format!("./solo{wrong_ext}");
        let resolved = resolve_platform_script(&command, dir.path());
        assert_eq!(resolved, command);
    }

    #[test]
    fn command_without_path_like_token_is_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let command = "echo hello".to_string();
        let resolved = resolve_platform_script(&command, dir.path());
        assert_eq!(resolved, command);
    }

    #[test]
    fn nested_relative_path_is_resolved() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("scripts");
        fs::create_dir_all(&sub).unwrap();

        let wrong_ext = non_platform_script_ext();
        let right_ext = platform_script_ext();
        let wrong_file = sub.join(format!("deep{wrong_ext}"));
        let right_file = sub.join(format!("deep{right_ext}"));
        fs::write(&wrong_file, "").unwrap();
        fs::write(&right_file, "").unwrap();

        let command = format!("./scripts/deep{wrong_ext}");
        let resolved = resolve_platform_script(&command, dir.path());
        assert_eq!(resolved, format!("./scripts/deep{right_ext}"));
    }

    #[test]
    fn bash_prefix_command_with_script_is_resolved() {
        let dir = tempfile::tempdir().unwrap();
        let wrong_ext = non_platform_script_ext();
        let right_ext = platform_script_ext();
        let wrong_file = dir.path().join(format!("run{wrong_ext}"));
        let right_file = dir.path().join(format!("run{right_ext}"));
        fs::write(&wrong_file, "").unwrap();
        fs::write(&right_file, "").unwrap();

        let command = format!("bash ./run{wrong_ext}");
        let resolved = resolve_platform_script(&command, dir.path());
        assert_eq!(resolved, format!("bash ./run{right_ext}"));
    }

    #[test]
    fn script_not_on_disk_is_not_replaced() {
        // Even if the extension is "wrong", if the file doesn't exist we
        // must not fabricate a replacement.
        let dir = tempfile::tempdir().unwrap();
        let wrong_ext = non_platform_script_ext();
        let command = format!("./nonexistent{wrong_ext}");
        let resolved = resolve_platform_script(&command, dir.path());
        assert_eq!(resolved, command);
    }

    // --- is_script_ext tests ---

    #[test]
    fn only_sh_and_ps1_are_script_extensions() {
        assert!(is_script_ext(".sh"));
        assert!(is_script_ext(".ps1"));
        assert!(!is_script_ext(".ash"));
        assert!(!is_script_ext(".bash"));
        assert!(!is_script_ext(".eps1"));
        assert!(!is_script_ext(".txt"));
        assert!(!is_script_ext(""));
    }

    #[test]
    fn non_script_extension_is_ignored_even_if_ends_with_sh() {
        // `foo.ash` ends with `.sh` but the extension is `.ash`, not `.sh`.
        let dir = tempfile::tempdir().unwrap();
        let wrong_ext = non_platform_script_ext();
        let right_ext = platform_script_ext();
        // Create the "right" version that should NOT be picked up because
        // the token's actual extension is `.ash`, not `.sh`/`.ps1`.
        let ash_file = dir.path().join("tool.ash");
        let alt_file = dir.path().join(format!("tool{right_ext}"));
        fs::write(&ash_file, "").unwrap();
        fs::write(&alt_file, "").unwrap();

        let command = "./tool.ash".to_string();
        let resolved = resolve_platform_script(&command, dir.path());
        assert_eq!(resolved, command);
    }

    #[test]
    fn non_script_extension_is_ignored_even_if_ends_with_ps1() {
        let dir = tempfile::tempdir().unwrap();
        let right_ext = platform_script_ext();
        let eps1_file = dir.path().join("tool.eps1");
        let alt_file = dir.path().join(format!("tool{right_ext}"));
        fs::write(&eps1_file, "").unwrap();
        fs::write(&alt_file, "").unwrap();

        let command = "./tool.eps1".to_string();
        let resolved = resolve_platform_script(&command, dir.path());
        assert_eq!(resolved, command);
    }
}
