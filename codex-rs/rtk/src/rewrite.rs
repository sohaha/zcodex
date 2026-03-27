use std::borrow::Cow;

const CODEX_PREFIX: &str = "codex rtk";

const DIRECT_PREFIXES: &[&str] = &[
    "aws",
    "cargo",
    "curl",
    "docker",
    "find",
    "gh",
    "git",
    "go",
    "golangci-lint",
    "grep",
    "gt",
    "kubectl",
    "lint",
    "ls",
    "mypy",
    "next",
    "npm",
    "npx",
    "pip",
    "playwright",
    "pnpm",
    "prettier",
    "prisma",
    "psql",
    "pytest",
    "ruff",
    "tree",
    "tsc",
    "vitest",
    "wc",
    "wget",
];

pub fn rewrite_shell_command(command: &str) -> Option<String> {
    let trimmed = command.trim();
    if trimmed.is_empty() || contains_shell_metacharacters(trimmed) {
        return None;
    }
    if trimmed.starts_with(CODEX_PREFIX) || trimmed == "rtk" || trimmed.starts_with("rtk ") {
        return Some(trimmed.to_string());
    }

    let args = shlex::split(trimmed)?;
    let (prefix, routed_args) = split_leading_env_prefix(&args)?;
    let [first, rest @ ..] = routed_args else {
        return None;
    };
    if first == "sudo" {
        return None;
    }

    let rewritten = match first.as_str() {
        "cat" => rewrite_cat(rest),
        "head" => rewrite_head(rest),
        "tail" => rewrite_tail(rest),
        command if DIRECT_PREFIXES.contains(&command) => {
            Some(format!("{CODEX_PREFIX} {}", join_shell_words(routed_args)))
        }
        _ => None,
    }?;

    Some(prepend_prefix(&prefix, &rewritten))
}

fn rewrite_cat(rest: &[String]) -> Option<String> {
    let rest = strip_flag_terminators(rest);
    let [path] = rest.as_slice() else {
        return None;
    };
    Some(format!("{CODEX_PREFIX} read {}", shell_escape(path)))
}

fn rewrite_head(rest: &[String]) -> Option<String> {
    let rest = strip_flag_terminators(rest);
    match rest.as_slice() {
        [path] => Some(format!(
            "{CODEX_PREFIX} read {} --max-lines 10",
            shell_escape(path)
        )),
        [count, path] => {
            if let Some(lines) = parse_numeric_short_flag(count, "-") {
                return Some(format!(
                    "{CODEX_PREFIX} read {} --max-lines {lines}",
                    shell_escape(path)
                ));
            }
            if let Some(lines) = parse_equals_flag(count, "--lines=") {
                return Some(format!(
                    "{CODEX_PREFIX} read {} --max-lines {lines}",
                    shell_escape(path)
                ));
            }
            if let Some(lines) = parse_numeric_short_flag(count, "-n") {
                return Some(format!(
                    "{CODEX_PREFIX} read {} --max-lines {lines}",
                    shell_escape(path)
                ));
            }
            None
        }
        [flag, lines, path] if *flag == "-n" => Some(format!(
            "{CODEX_PREFIX} read {} --max-lines {}",
            shell_escape(path),
            shell_escape(lines)
        )),
        [flag, lines, path] if *flag == "--lines" => Some(format!(
            "{CODEX_PREFIX} read {} --max-lines {}",
            shell_escape(path),
            shell_escape(lines)
        )),
        _ => None,
    }
}

fn rewrite_tail(rest: &[String]) -> Option<String> {
    let rest = strip_flag_terminators(rest);
    match rest.as_slice() {
        [path] => Some(format!(
            "{CODEX_PREFIX} read {} --tail-lines 10",
            shell_escape(path)
        )),
        [count, path] => {
            if let Some(lines) = parse_numeric_short_flag(count, "-") {
                return Some(format!(
                    "{CODEX_PREFIX} read {} --tail-lines {lines}",
                    shell_escape(path)
                ));
            }
            if let Some(lines) = parse_equals_flag(count, "--lines=") {
                return Some(format!(
                    "{CODEX_PREFIX} read {} --tail-lines {lines}",
                    shell_escape(path)
                ));
            }
            if let Some(lines) = parse_numeric_short_flag(count, "-n") {
                return Some(format!(
                    "{CODEX_PREFIX} read {} --tail-lines {lines}",
                    shell_escape(path)
                ));
            }
            None
        }
        [flag, lines, path] if *flag == "-n" => Some(format!(
            "{CODEX_PREFIX} read {} --tail-lines {}",
            shell_escape(path),
            shell_escape(lines)
        )),
        [flag, lines, path] if *flag == "--lines" => Some(format!(
            "{CODEX_PREFIX} read {} --tail-lines {}",
            shell_escape(path),
            shell_escape(lines)
        )),
        _ => None,
    }
}

fn split_leading_env_prefix(args: &[String]) -> Option<(Vec<String>, &[String])> {
    let mut prefix = Vec::new();
    let mut index = 0;

    while let Some(arg) = args.get(index) {
        if is_env_assignment(arg) {
            prefix.push(arg.clone());
            index += 1;
        } else {
            break;
        }
    }

    if args.get(index).is_some_and(|arg| arg == "env") {
        prefix.push("env".to_string());
        index += 1;
        while let Some(arg) = args.get(index) {
            if is_env_assignment(arg) {
                prefix.push(arg.clone());
                index += 1;
            } else {
                break;
            }
        }
    }

    args.get(index..).and_then(|rest| {
        if rest.is_empty() {
            None
        } else {
            Some((prefix, rest))
        }
    })
}

fn is_env_assignment(value: &str) -> bool {
    let Some((name, _)) = value.split_once('=') else {
        return false;
    };
    let mut chars = name.chars();
    matches!(chars.next(), Some(ch) if ch == '_' || ch.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn strip_flag_terminators(rest: &[String]) -> Vec<&str> {
    rest.iter()
        .filter_map(|value| {
            if value == "--" {
                None
            } else {
                Some(value.as_str())
            }
        })
        .collect()
}

fn prepend_prefix(prefix: &[String], rewritten: &str) -> String {
    if prefix.is_empty() {
        rewritten.to_string()
    } else {
        let escaped_prefix = join_shell_words(prefix);
        format!("{escaped_prefix} {rewritten}")
    }
}

fn join_shell_words(words: &[String]) -> String {
    words
        .iter()
        .map(|value| shell_escape(value).into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

fn parse_numeric_short_flag<'a>(value: &'a str, prefix: &str) -> Option<&'a str> {
    value
        .strip_prefix(prefix)
        .filter(|suffix| !suffix.is_empty() && suffix.chars().all(|ch| ch.is_ascii_digit()))
}

fn parse_equals_flag<'a>(value: &'a str, prefix: &str) -> Option<&'a str> {
    value
        .strip_prefix(prefix)
        .filter(|suffix| !suffix.is_empty() && suffix.chars().all(|ch| ch.is_ascii_digit()))
}

fn contains_shell_metacharacters(command: &str) -> bool {
    ['|', '&', ';', '<', '>', '\n', '\r', '`']
        .into_iter()
        .any(|char| command.contains(char))
        || command.contains("$(")
}

fn shell_escape(value: &str) -> Cow<'_, str> {
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | '.' | '_' | '-' | ':' | '='))
    {
        Cow::Borrowed(value)
    } else {
        Cow::Owned(format!("'{}'", value.replace('\'', "'\"'\"'")))
    }
}

#[cfg(test)]
mod tests {
    use super::rewrite_shell_command;

    #[test]
    fn rewrites_direct_prefix_commands() {
        assert_eq!(
            rewrite_shell_command("git status"),
            Some("codex rtk git status".to_string())
        );
        assert_eq!(
            rewrite_shell_command("cargo test -p codex-core"),
            Some("codex rtk cargo test -p codex-core".to_string())
        );
    }

    #[test]
    fn rewrites_cat_head_and_tail() {
        assert_eq!(
            rewrite_shell_command("cat src/main.rs"),
            Some("codex rtk read src/main.rs".to_string())
        );
        assert_eq!(
            rewrite_shell_command("head -20 src/main.rs"),
            Some("codex rtk read src/main.rs --max-lines 20".to_string())
        );
        assert_eq!(
            rewrite_shell_command("head src/main.rs"),
            Some("codex rtk read src/main.rs --max-lines 10".to_string())
        );
        assert_eq!(
            rewrite_shell_command("tail --lines=7 src/main.rs"),
            Some("codex rtk read src/main.rs --tail-lines 7".to_string())
        );
        assert_eq!(
            rewrite_shell_command("head -n5 -- src/main.rs"),
            Some("codex rtk read src/main.rs --max-lines 5".to_string())
        );
        assert_eq!(
            rewrite_shell_command("tail src/main.rs"),
            Some("codex rtk read src/main.rs --tail-lines 10".to_string())
        );
    }

    #[test]
    fn preserves_existing_rtk_invocations() {
        assert_eq!(
            rewrite_shell_command("codex rtk git status"),
            Some("codex rtk git status".to_string())
        );
    }

    #[test]
    fn skips_compound_or_unsafe_shell_forms() {
        assert_eq!(rewrite_shell_command("git status | head"), None);
        assert_eq!(rewrite_shell_command("sudo git status"), None);
    }

    #[test]
    fn rewrites_supported_commands_with_env_prefixes() {
        assert_eq!(
            rewrite_shell_command("FOO=1 git status"),
            Some("FOO=1 codex rtk git status".to_string())
        );
        assert_eq!(
            rewrite_shell_command("env FOO=1 BAR=2 grep TODO src"),
            Some("env FOO=1 BAR=2 codex rtk grep TODO src".to_string())
        );
    }
}
