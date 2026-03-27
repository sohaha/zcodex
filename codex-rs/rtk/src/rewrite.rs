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
    let [first, rest @ ..] = args.as_slice() else {
        return None;
    };
    if first == "sudo" || first.contains('=') {
        return None;
    }

    match first.as_str() {
        "cat" => rewrite_cat(rest),
        "head" => rewrite_head(rest),
        "tail" => rewrite_tail(rest),
        command if DIRECT_PREFIXES.contains(&command) => Some(format!("{CODEX_PREFIX} {trimmed}")),
        _ => None,
    }
}

fn rewrite_cat(rest: &[String]) -> Option<String> {
    let [path] = rest else {
        return None;
    };
    Some(format!("{CODEX_PREFIX} read {}", shell_escape(path)))
}

fn rewrite_head(rest: &[String]) -> Option<String> {
    match rest {
        [path] => Some(format!("{CODEX_PREFIX} read {}", shell_escape(path))),
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
            None
        }
        [flag, lines, path] if flag == "-n" => Some(format!(
            "{CODEX_PREFIX} read {} --max-lines {}",
            shell_escape(path),
            shell_escape(lines)
        )),
        [flag, lines, path] if flag == "--lines" => Some(format!(
            "{CODEX_PREFIX} read {} --max-lines {}",
            shell_escape(path),
            shell_escape(lines)
        )),
        _ => None,
    }
}

fn rewrite_tail(rest: &[String]) -> Option<String> {
    match rest {
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
            None
        }
        [flag, lines, path] if flag == "-n" => Some(format!(
            "{CODEX_PREFIX} read {} --tail-lines {}",
            shell_escape(path),
            shell_escape(lines)
        )),
        [flag, lines, path] if flag == "--lines" => Some(format!(
            "{CODEX_PREFIX} read {} --tail-lines {}",
            shell_escape(path),
            shell_escape(lines)
        )),
        _ => None,
    }
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
            rewrite_shell_command("tail --lines=7 src/main.rs"),
            Some("codex rtk read src/main.rs --tail-lines 7".to_string())
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
        assert_eq!(rewrite_shell_command("FOO=1 git status"), None);
        assert_eq!(rewrite_shell_command("sudo git status"), None);
    }
}
