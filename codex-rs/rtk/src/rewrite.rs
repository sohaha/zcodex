use std::borrow::Cow;
use std::path::Path;

const RTK_PREFIX: &str = "rtk";
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
    if let Some(rest) = trimmed.strip_prefix("codex rtk ") {
        return ShellCommandRewriteAnalysis {
            command: format!("{RTK_PREFIX} {rest}"),
            kind: ShellCommandRewriteKind::Rewritten,
        };
    }
    if trimmed == "codex rtk" {
        return ShellCommandRewriteAnalysis {
            command: RTK_PREFIX.to_string(),
            kind: ShellCommandRewriteKind::Rewritten,
        };
    }
    if trimmed == RTK_PREFIX || trimmed.starts_with("rtk ") {
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
            "{RTK_PREFIX} {command}{}",
            join_rest_args(parsed.rest)
        )),
        _ => {
            return passthrough(
                trimmed,
                ShellCommandPassthroughReason::UnsupportedCommand,
                looks_like_rtk_candidate(trimmed),
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
    let (mut prefix, mut routed_args) = split_leading_env_prefix(args)?;
    loop {
        let (wrapper_prefix, rest) = split_safe_wrapper_prefix(routed_args)?;
        if wrapper_prefix.is_empty() {
            let (target, rest) = split_command_prefix(routed_args)?;
            let (wrapper_prefix, rest) = split_safe_wrapper_target(&target, rest)?;
            if wrapper_prefix.is_empty() {
                return Some(ParsedCommandTarget {
                    prefix,
                    target,
                    rest,
                });
            }
            prefix.extend(wrapper_prefix);
            routed_args = rest;
            continue;
        }
        prefix.extend(wrapper_prefix);
        routed_args = rest;
    }
}

fn rewrite_cat(rest: &[String]) -> Option<String> {
    let rest = strip_flag_terminators(rest);
    let [path] = rest.as_slice() else {
        return None;
    };
    Some(format!("{RTK_PREFIX} read {}", shell_escape(path)))
}

fn rewrite_head(rest: &[String]) -> Option<String> {
    let rest = strip_flag_terminators(rest);
    match rest.as_slice() {
        [path] => Some(format!(
            "{RTK_PREFIX} read {} --max-lines 10",
            shell_escape(path)
        )),
        [count, path] => {
            if let Some(lines) = parse_numeric_short_flag(count, "-") {
                return Some(format!(
                    "{RTK_PREFIX} read {} --max-lines {lines}",
                    shell_escape(path)
                ));
            }
            if let Some(lines) = parse_equals_flag(count, "--lines=") {
                return Some(format!(
                    "{RTK_PREFIX} read {} --max-lines {lines}",
                    shell_escape(path)
                ));
            }
            if let Some(lines) = parse_numeric_short_flag(count, "-n") {
                return Some(format!(
                    "{RTK_PREFIX} read {} --max-lines {lines}",
                    shell_escape(path)
                ));
            }
            None
        }
        [flag, lines, path] if *flag == "-n" => Some(format!(
            "{RTK_PREFIX} read {} --max-lines {}",
            shell_escape(path),
            shell_escape(lines)
        )),
        [flag, lines, path] if *flag == "--lines" => Some(format!(
            "{RTK_PREFIX} read {} --max-lines {}",
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
            "{RTK_PREFIX} read {} --tail-lines 10",
            shell_escape(path)
        )),
        [count, path] => {
            if let Some(lines) = parse_numeric_short_flag(count, "-") {
                return Some(format!(
                    "{RTK_PREFIX} read {} --tail-lines {lines}",
                    shell_escape(path)
                ));
            }
            if let Some(lines) = parse_equals_flag(count, "--lines=") {
                return Some(format!(
                    "{RTK_PREFIX} read {} --tail-lines {lines}",
                    shell_escape(path)
                ));
            }
            if let Some(lines) = parse_numeric_short_flag(count, "-n") {
                return Some(format!(
                    "{RTK_PREFIX} read {} --tail-lines {lines}",
                    shell_escape(path)
                ));
            }
            None
        }
        [flag, lines, path] if *flag == "-n" => Some(format!(
            "{RTK_PREFIX} read {} --tail-lines {}",
            shell_escape(path),
            shell_escape(lines)
        )),
        [flag, lines, path] if *flag == "--lines" => Some(format!(
            "{RTK_PREFIX} read {} --tail-lines {}",
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
                let value = args.get(index + 1)?;
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

fn split_safe_wrapper_prefix(args: &[String]) -> Option<(Vec<String>, &[String])> {
    let [first, rest @ ..] = args else {
        return None;
    };
    split_safe_wrapper_target(first, rest)
}

fn split_safe_wrapper_target<'a>(
    target: &str,
    rest: &'a [String],
) -> Option<(Vec<String>, &'a [String])> {
    match target {
        "chrt" => split_chrt_prefix(rest),
        "ionice" => split_ionice_prefix(rest),
        "nice" => split_nice_prefix(rest),
        "stdbuf" => split_stdbuf_prefix(rest),
        _ => Some((Vec::new(), rest)),
    }
}

fn split_chrt_prefix(args: &[String]) -> Option<(Vec<String>, &[String])> {
    let mut prefix = vec!["chrt".to_string()];
    let mut index = 0;
    let mut saw_policy = false;
    while let Some(arg) = args.get(index) {
        if arg == "--" {
            index += 1;
            break;
        }
        if matches!(arg.as_str(), "-a" | "--all-tasks") {
            prefix.push(arg.clone());
            index += 1;
            continue;
        }
        if matches!(
            arg.as_str(),
            "-b" | "-f" | "-i" | "-o" | "-r" | "--batch" | "--fifo" | "--idle" | "--other" | "--rr"
        ) {
            prefix.push(arg.clone());
            index += 1;
            saw_policy = true;
            continue;
        }
        break;
    }
    if !saw_policy {
        return Some((Vec::new(), args));
    }

    let priority = args.get(index)?;
    if !priority.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    prefix.push(priority.clone());
    index += 1;

    if args.get(index).is_some_and(|arg| arg == "--") {
        index += 1;
    }

    let rest = args.get(index..)?;
    if rest.is_empty() {
        return None;
    }
    Some((prefix, rest))
}

fn split_ionice_prefix(args: &[String]) -> Option<(Vec<String>, &[String])> {
    let mut prefix = vec!["ionice".to_string()];
    let mut index = 0;
    while let Some(arg) = args.get(index) {
        if arg == "--" {
            index += 1;
            break;
        }
        if matches!(arg.as_str(), "-c" | "-n" | "--class" | "--classdata") {
            let value = args.get(index + 1)?;
            if !value.chars().all(|ch| ch.is_ascii_digit()) {
                return None;
            }
            prefix.push(arg.clone());
            prefix.push(value.clone());
            index += 2;
            continue;
        }
        if matches!(arg.as_str(), "-t" | "--ignore") {
            prefix.push(arg.clone());
            index += 1;
            continue;
        }
        if let Some(value) = arg.strip_prefix("-c") {
            if value.is_empty() || !value.chars().all(|ch| ch.is_ascii_digit()) {
                return None;
            }
            prefix.push(arg.clone());
            index += 1;
            continue;
        }
        if let Some(value) = arg.strip_prefix("-n") {
            if value.is_empty() || !value.chars().all(|ch| ch.is_ascii_digit()) {
                return None;
            }
            prefix.push(arg.clone());
            index += 1;
            continue;
        }
        if let Some(value) = arg.strip_prefix("--class=") {
            if value.is_empty() || !value.chars().all(|ch| ch.is_ascii_digit()) {
                return None;
            }
            prefix.push(arg.clone());
            index += 1;
            continue;
        }
        if let Some(value) = arg.strip_prefix("--classdata=") {
            if value.is_empty() || !value.chars().all(|ch| ch.is_ascii_digit()) {
                return None;
            }
            prefix.push(arg.clone());
            index += 1;
            continue;
        }
        break;
    }
    let rest = args.get(index..)?;
    if rest.is_empty() {
        return None;
    }
    Some((prefix, rest))
}

fn split_nice_prefix(args: &[String]) -> Option<(Vec<String>, &[String])> {
    let mut prefix = vec!["nice".to_string()];
    let mut index = 0;
    while let Some(arg) = args.get(index) {
        if arg == "--" {
            index += 1;
            break;
        }
        if matches!(arg.as_str(), "-n" | "--adjustment") {
            let value = args.get(index + 1)?;
            if !is_signed_integer(value) {
                return None;
            }
            prefix.push(arg.clone());
            prefix.push(value.clone());
            index += 2;
            continue;
        }
        if let Some(value) = arg.strip_prefix("-n") {
            if value.is_empty() || !is_signed_integer(value) {
                return None;
            }
            prefix.push(arg.clone());
            index += 1;
            continue;
        }
        if matches_nice_adjustment_flag(arg) {
            prefix.push(arg.clone());
            index += 1;
            continue;
        }
        if let Some(value) = arg.strip_prefix("--adjustment=") {
            if value.is_empty() || !is_signed_integer(value) {
                return None;
            }
            prefix.push(arg.clone());
            index += 1;
            continue;
        }
        break;
    }
    let rest = args.get(index..)?;
    if rest.is_empty() {
        return None;
    }
    Some((prefix, rest))
}

fn split_stdbuf_prefix(args: &[String]) -> Option<(Vec<String>, &[String])> {
    let mut prefix = vec!["stdbuf".to_string()];
    let mut index = 0;
    let mut saw_option = false;
    while let Some(arg) = args.get(index) {
        if arg == "--" {
            index += 1;
            break;
        }
        if matches!(
            arg.as_str(),
            "-i" | "-o" | "-e" | "--input" | "--output" | "--error"
        ) {
            let value = args.get(index + 1)?;
            prefix.push(arg.clone());
            prefix.push(value.clone());
            index += 2;
            saw_option = true;
            continue;
        }
        if matches_stdbuf_attached_flag(arg) {
            prefix.push(arg.clone());
            index += 1;
            saw_option = true;
            continue;
        }
        break;
    }
    let rest = args.get(index..)?;
    if !saw_option || rest.is_empty() {
        return None;
    }
    Some((prefix, rest))
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

fn matches_nice_adjustment_flag(value: &str) -> bool {
    value
        .strip_prefix('-')
        .filter(|suffix| !suffix.is_empty())
        .is_some_and(is_signed_integer)
}

fn is_signed_integer(value: &str) -> bool {
    let Some(first) = value.chars().next() else {
        return false;
    };
    let rest = if matches!(first, '+' | '-') {
        &value[1..]
    } else {
        value
    };
    !rest.is_empty() && rest.chars().all(|ch| ch.is_ascii_digit())
}

fn matches_stdbuf_attached_flag(value: &str) -> bool {
    matches!(
        value.chars().next(),
        Some('-') if value.len() > 2 && matches!(value.as_bytes().get(1), Some(b'i' | b'o' | b'e'))
    ) || value.starts_with("--input=")
        || value.starts_with("--output=")
        || value.starts_with("--error=")
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
        if matches!(word, "chrt" | "ionice" | "nice" | "stdbuf") {
            words.next();
            return words.any(|value| {
                let name = normalize_command_name(value);
                matches!(name.as_str(), "cat" | "head" | "tail")
                    || DIRECT_PREFIXES.contains(&name.as_str())
            });
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
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum QuoteState {
        Unquoted,
        SingleQuoted,
        DoubleQuoted,
    }

    let mut chars = command.chars().peekable();
    let mut quote_state = QuoteState::Unquoted;
    let mut escaped = false;

    while let Some(ch) = chars.next() {
        match quote_state {
            QuoteState::Unquoted => {
                if escaped {
                    escaped = false;
                    continue;
                }

                match ch {
                    '\\' => escaped = true,
                    '\'' => quote_state = QuoteState::SingleQuoted,
                    '"' => quote_state = QuoteState::DoubleQuoted,
                    '|' | '&' | ';' | '<' | '>' | '\n' | '\r' | '`' => return true,
                    '$' if chars.peek().is_some_and(|next| *next == '(') => return true,
                    _ => {}
                }
            }
            QuoteState::SingleQuoted => {
                if ch == '\'' {
                    quote_state = QuoteState::Unquoted;
                }
            }
            QuoteState::DoubleQuoted => {
                if escaped {
                    escaped = false;
                    continue;
                }

                match ch {
                    '\\' => escaped = true,
                    '"' => quote_state = QuoteState::Unquoted,
                    '`' => return true,
                    '$' if chars.peek().is_some_and(|next| *next == '(') => return true,
                    _ => {}
                }
            }
        }
    }

    false
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
                "`{input}` 的重写结果不符合预期"
            );
        }
    }

    #[test]
    fn rewrites_direct_prefix_commands() {
        assert_rewrite_cases(&[
            ("git status", Some("rtk git status")),
            (
                "cargo test -p codex-core",
                Some("rtk cargo test -p codex-core"),
            ),
            ("command git status", Some("rtk git status")),
            ("command -p git status", Some("rtk git status")),
            (
                "command -- git -C repo status",
                Some("rtk git -C repo status"),
            ),
            (
                "command -p -- git -C repo status",
                Some("rtk git -C repo status"),
            ),
            ("/usr/bin/git status", Some("rtk git status")),
            ("git -C repo status", Some("rtk git -C repo status")),
            (
                "cargo --manifest-path Cargo.toml test -p codex-core",
                Some("rtk cargo --manifest-path Cargo.toml test -p codex-core"),
            ),
            (
                "cargo +nightly test -p codex-core",
                Some("rtk cargo '+nightly' test -p codex-core"),
            ),
            (
                "git -c color.ui=always -C repo status",
                Some("rtk git -c color.ui=always -C repo status"),
            ),
            (
                "git --git-dir .git --work-tree . status",
                Some("rtk git --git-dir .git --work-tree . status"),
            ),
            ("nice -n 5 git status", Some("nice -n 5 rtk git status")),
            ("nice -5 -- git status", Some("nice -5 rtk git status")),
            ("stdbuf -oL git status", Some("stdbuf -oL rtk git status")),
            (
                "nice -n 5 stdbuf -oL git status",
                Some("nice -n 5 stdbuf -oL rtk git status"),
            ),
            (
                "command nice -n 5 git status",
                Some("nice -n 5 rtk git status"),
            ),
            (
                "command -p stdbuf -oL git status",
                Some("stdbuf -oL rtk git status"),
            ),
            (
                "command cargo +nightly test -p codex-core",
                Some("rtk cargo '+nightly' test -p codex-core"),
            ),
            (
                "/usr/bin/nice -n 5 git status",
                Some("nice -n 5 rtk git status"),
            ),
            ("ionice -c 3 git status", Some("ionice -c 3 rtk git status")),
            (
                "ionice -c2 -n7 -- git status",
                Some("ionice -c2 -n7 rtk git status"),
            ),
            (
                "command ionice -c2 git status",
                Some("ionice -c2 rtk git status"),
            ),
            (
                "nice -n 5 ionice -c2 git status",
                Some("nice -n 5 ionice -c2 rtk git status"),
            ),
            ("chrt -r 10 git status", Some("chrt -r 10 rtk git status")),
            (
                "chrt --fifo 20 -- git status",
                Some("chrt --fifo 20 rtk git status"),
            ),
            (
                "command chrt -b 0 git status",
                Some("chrt -b 0 rtk git status"),
            ),
            (
                "nice -n 5 chrt -r 10 git status",
                Some("nice -n 5 chrt -r 10 rtk git status"),
            ),
        ]);
    }

    #[test]
    fn rewrites_cat_head_and_tail() {
        assert_rewrite_cases(&[
            ("cat src/main.rs", Some("rtk read src/main.rs")),
            (
                "head -20 src/main.rs",
                Some("rtk read src/main.rs --max-lines 20"),
            ),
            (
                "head src/main.rs",
                Some("rtk read src/main.rs --max-lines 10"),
            ),
            (
                "tail --lines=7 src/main.rs",
                Some("rtk read src/main.rs --tail-lines 7"),
            ),
            (
                "head -n5 -- src/main.rs",
                Some("rtk read src/main.rs --max-lines 5"),
            ),
            (
                "tail src/main.rs",
                Some("rtk read src/main.rs --tail-lines 10"),
            ),
        ]);
    }

    #[test]
    fn preserves_existing_rtk_invocations() {
        assert_eq!(
            rewrite_shell_command("rtk git status"),
            Some("rtk git status".to_string())
        );
        assert_eq!(
            rewrite_shell_command("codex rtk git status"),
            Some("rtk git status".to_string())
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
            ("FOO=1 git status", Some("FOO=1 rtk git status")),
            (
                "env FOO=1 BAR=2 grep TODO src",
                Some("env FOO=1 BAR=2 rtk grep TODO src"),
            ),
            (
                "env -- FOO=1 git status",
                Some("env -- FOO=1 rtk git status"),
            ),
            (
                "env -i -u HOME git -C repo status",
                Some("env -i -u HOME rtk git -C repo status"),
            ),
            (
                "env --chdir=repo nice -n 5 git status",
                Some("env --chdir=repo nice -n 5 rtk git status"),
            ),
            (
                "FOO=1 command nice -n 5 git status",
                Some("FOO=1 nice -n 5 rtk git status"),
            ),
            (
                "env -i BAR=2 command -p stdbuf -oL git status",
                Some("env -i BAR=2 stdbuf -oL rtk git status"),
            ),
            (
                "env --chdir=repo command ionice -c2 nice -n 5 git status",
                Some("env --chdir=repo ionice -c2 nice -n 5 rtk git status"),
            ),
            (
                "env FOO=1 command chrt -r 10 /usr/bin/git status",
                Some("env FOO=1 chrt -r 10 rtk git status"),
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

        let analysis = analyze_shell_command("nice -n git status");
        assert_eq!(analysis.command, "nice -n git status");
        assert_eq!(
            analysis.kind,
            ShellCommandRewriteKind::Passthrough {
                reason: ShellCommandPassthroughReason::MissingCommand,
                candidate: true,
            }
        );

        let analysis = analyze_shell_command("ionice -p 123 git status");
        assert_eq!(analysis.command, "ionice -p 123 git status");
        assert_eq!(
            analysis.kind,
            ShellCommandRewriteKind::Passthrough {
                reason: ShellCommandPassthroughReason::UnsupportedCommand,
                candidate: true,
            }
        );

        let analysis = analyze_shell_command("chrt -m git status");
        assert_eq!(analysis.command, "chrt -m git status");
        assert_eq!(
            analysis.kind,
            ShellCommandRewriteKind::Passthrough {
                reason: ShellCommandPassthroughReason::UnsupportedCommand,
                candidate: true,
            }
        );
    }

    #[test]
    fn preserves_quoted_literals_while_blocking_real_shell_syntax() {
        assert_rewrite_cases(&[
            ("grep 'a|b' src/main.rs", Some("rtk grep 'a|b' src/main.rs")),
            (
                "curl 'https://example.com?a=1&b=2'",
                Some("rtk curl 'https://example.com?a=1&b=2'"),
            ),
            (
                "git log --format='%h|%s' -1",
                Some("rtk git log '--format=%h|%s' -1"),
            ),
            ("grep a\\|b src/main.rs", Some("rtk grep 'a|b' src/main.rs")),
        ]);

        assert_eq!(
            analyze_shell_command("grep \"$(pwd)\" src/main.rs").kind,
            ShellCommandRewriteKind::Passthrough {
                reason: ShellCommandPassthroughReason::ShellMetacharacters,
                candidate: true,
            }
        );
        assert_eq!(
            analyze_shell_command("grep '$(pwd)' src/main.rs").kind,
            ShellCommandRewriteKind::Rewritten
        );
        assert_eq!(
            analyze_shell_command("git status >/tmp/out").kind,
            ShellCommandRewriteKind::Passthrough {
                reason: ShellCommandPassthroughReason::ShellMetacharacters,
                candidate: true,
            }
        );
    }
}
