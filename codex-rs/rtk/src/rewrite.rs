use std::borrow::Cow;
use std::path::Path;

const CODEX_PREFIX: &str = "codex rtk";
const ENV_PREFIX_FLAGS: &[&str] = &["-i", "--ignore-environment"];

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShellCommandRewriteAnalysis {
    pub command: String,
    pub kind: ShellCommandRewriteKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ShellCommandRewriteKind {
    AlreadyRtk,
    Rewritten,
    Passthrough {
        reason: ShellCommandPassthroughReason,
        candidate: bool,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShellCommandPassthroughReason {
    Empty,
    ShellMetacharacters,
    ParseFailed,
    MissingCommand,
    Sudo,
    UnsupportedCommand,
    UnsupportedArguments,
}

impl ShellCommandPassthroughReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Empty => "empty command",
            Self::ShellMetacharacters => "contains compound shell syntax",
            Self::ParseFailed => "failed to parse shell words",
            Self::MissingCommand => "missing command after prefixes",
            Self::Sudo => "sudo commands are never auto-routed",
            Self::UnsupportedCommand => "command is not in the embedded RTK allowlist",
            Self::UnsupportedArguments => {
                "command shape is not supported by the embedded RTK rewriter"
            }
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
struct ParsedCommandTarget<'a> {
    prefix: Vec<String>,
    target: String,
    rest: &'a [String],
}

pub fn rewrite_shell_command(command: &str) -> Option<String> {
    let analysis = analyze_shell_command(command);
    match analysis.kind {
        ShellCommandRewriteKind::AlreadyRtk | ShellCommandRewriteKind::Rewritten => {
            Some(analysis.command)
        }
        ShellCommandRewriteKind::Passthrough { .. } => None,
    }
}

pub fn analyze_shell_command(command: &str) -> ShellCommandRewriteAnalysis {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return passthrough(trimmed, ShellCommandPassthroughReason::Empty, false);
    }
    if trimmed.starts_with(CODEX_PREFIX) || trimmed == "rtk" || trimmed.starts_with("rtk ") {
        return ShellCommandRewriteAnalysis {
            command: trimmed.to_string(),
            kind: ShellCommandRewriteKind::AlreadyRtk,
        };
    }
    if contains_shell_metacharacters(trimmed) {
        return passthrough(
            trimmed,
            ShellCommandPassthroughReason::ShellMetacharacters,
            looks_like_rtk_candidate(trimmed),
        );
    }

    let Some(args) = shlex::split(trimmed) else {
        return passthrough(
            trimmed,
            ShellCommandPassthroughReason::ParseFailed,
            looks_like_rtk_candidate(trimmed),
        );
    };
    let Some(parsed) = parse_command_target(&args) else {
        return passthrough(
            trimmed,
            ShellCommandPassthroughReason::MissingCommand,
            looks_like_rtk_candidate(trimmed),
        );
    };
    if parsed.target == "sudo" {
        return passthrough(trimmed, ShellCommandPassthroughReason::Sudo, true);
    }

    let rewritten = match parsed.target.as_str() {
        "cat" => rewrite_cat(parsed.rest),
        "head" => rewrite_head(parsed.rest),
        "tail" => rewrite_tail(parsed.rest),
        command if DIRECT_PREFIXES.contains(&command) => Some(format!(
            "{CODEX_PREFIX} {command}{}",
            join_rest_args(parsed.rest)
        )),
        _ => {
            return passthrough(
                trimmed,
                ShellCommandPassthroughReason::UnsupportedCommand,
                false,
            );
        }
    };

    match rewritten {
        Some(rewritten) => ShellCommandRewriteAnalysis {
            command: prepend_prefix(&parsed.prefix, &rewritten),
            kind: ShellCommandRewriteKind::Rewritten,
        },
        None => passthrough(
            trimmed,
            ShellCommandPassthroughReason::UnsupportedArguments,
            true,
        ),
    }
}

fn parse_command_target(args: &[String]) -> Option<ParsedCommandTarget<'_>> {
    let (prefix, routed_args) = split_leading_env_prefix(args)?;
    let (target, rest) = split_command_prefix(routed_args)?;
    Some(ParsedCommandTarget {
        prefix,
        target,
        rest,
    })
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
            if ENV_PREFIX_FLAGS.contains(&arg.as_str()) {
                prefix.push(arg.clone());
                index += 1;
                continue;
            }
            if matches!(arg.as_str(), "-u" | "-C") {
                let Some(value) = args.get(index + 1) else {
                    return None;
                };
                prefix.push(arg.clone());
                prefix.push(value.clone());
                index += 2;
                continue;
            }
            if arg.starts_with("--unset=") || arg.starts_with("--chdir=") {
                prefix.push(arg.clone());
                index += 1;
                continue;
            }
            if arg == "--" {
                prefix.push("--".to_string());
                index += 1;
                continue;
            }
            if is_env_assignment(arg) {
                prefix.push(arg.clone());
                index += 1;
                continue;
            }
            break;
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

fn split_command_prefix(args: &[String]) -> Option<(String, &[String])> {
    let [first, rest @ ..] = args else {
        return None;
    };
    if first == "command" {
        let mut index = 0;
        while let Some(flag) = rest.get(index) {
            match flag.as_str() {
                "-p" => index += 1,
                "--" => {
                    index += 1;
                    break;
                }
                _ => break,
            }
        }
        let rest = &rest[index..];
        let [next, tail @ ..] = rest else {
            return None;
        };
        return Some((normalize_command_name(next), tail));
    }
    Some((normalize_command_name(first), rest))
}

fn normalize_command_name(command: &str) -> String {
    Path::new(command)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(command)
        .to_string()
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

fn join_rest_args(words: &[String]) -> String {
    if words.is_empty() {
        String::new()
    } else {
        format!(" {}", join_shell_words(words))
    }
}

fn passthrough(
    command: &str,
    reason: ShellCommandPassthroughReason,
    candidate: bool,
) -> ShellCommandRewriteAnalysis {
    ShellCommandRewriteAnalysis {
        command: command.to_string(),
        kind: ShellCommandRewriteKind::Passthrough { reason, candidate },
    }
}

fn looks_like_rtk_candidate(command: &str) -> bool {
    let mut words = command.split_whitespace().peekable();
    while let Some(word) = words.peek().copied() {
        if is_env_assignment(word) {
            words.next();
            continue;
        }
        if word == "env" || word == "--" {
            words.next();
            continue;
        }
        if word == "command" {
            words.next();
            continue;
        }
        let name = normalize_command_name(word);
        return matches!(name.as_str(), "cat" | "head" | "tail")
            || DIRECT_PREFIXES.contains(&name.as_str());
    }
    false
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
    use super::ShellCommandPassthroughReason;
    use super::ShellCommandRewriteKind;
    use super::analyze_shell_command;
    use super::rewrite_shell_command;

    fn assert_rewrite_cases(cases: &[(&str, Option<&str>)]) {
        for (input, expected) in cases {
            assert_eq!(
                rewrite_shell_command(input),
                expected.map(str::to_string),
                "unexpected rewrite result for `{input}`"
            );
        }
    }

    #[test]
    fn rewrites_direct_prefix_commands() {
        assert_rewrite_cases(&[
            ("git status", Some("codex rtk git status")),
            (
                "cargo test -p codex-core",
                Some("codex rtk cargo test -p codex-core"),
            ),
            ("command git status", Some("codex rtk git status")),
            ("command -p git status", Some("codex rtk git status")),
            (
                "command -- git -C repo status",
                Some("codex rtk git -C repo status"),
            ),
            (
                "command -p -- git -C repo status",
                Some("codex rtk git -C repo status"),
            ),
            ("/usr/bin/git status", Some("codex rtk git status")),
            ("git -C repo status", Some("codex rtk git -C repo status")),
            (
                "cargo --manifest-path Cargo.toml test -p codex-core",
                Some("codex rtk cargo --manifest-path Cargo.toml test -p codex-core"),
            ),
        ]);
    }

    #[test]
    fn rewrites_cat_head_and_tail() {
        assert_rewrite_cases(&[
            ("cat src/main.rs", Some("codex rtk read src/main.rs")),
            (
                "head -20 src/main.rs",
                Some("codex rtk read src/main.rs --max-lines 20"),
            ),
            (
                "head src/main.rs",
                Some("codex rtk read src/main.rs --max-lines 10"),
            ),
            (
                "tail --lines=7 src/main.rs",
                Some("codex rtk read src/main.rs --tail-lines 7"),
            ),
            (
                "head -n5 -- src/main.rs",
                Some("codex rtk read src/main.rs --max-lines 5"),
            ),
            (
                "tail src/main.rs",
                Some("codex rtk read src/main.rs --tail-lines 10"),
            ),
        ]);
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
        assert_rewrite_cases(&[
            ("FOO=1 git status", Some("FOO=1 codex rtk git status")),
            (
                "env FOO=1 BAR=2 grep TODO src",
                Some("env FOO=1 BAR=2 codex rtk grep TODO src"),
            ),
            (
                "env -- FOO=1 git status",
                Some("env -- FOO=1 codex rtk git status"),
            ),
            (
                "env -i -u HOME git -C repo status",
                Some("env -i -u HOME codex rtk git -C repo status"),
            ),
        ]);
    }

    #[test]
    fn reports_passthrough_reason_for_supported_command_shapes() {
        let analysis = analyze_shell_command("git status | head");
        assert_eq!(analysis.command, "git status | head");
        assert_eq!(
            analysis.kind,
            ShellCommandRewriteKind::Passthrough {
                reason: ShellCommandPassthroughReason::ShellMetacharacters,
                candidate: true,
            }
        );
    }
}
