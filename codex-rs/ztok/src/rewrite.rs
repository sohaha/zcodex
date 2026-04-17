use std::borrow::Cow;
use std::path::Path;

use crate::find_cmd;

const ZTOK_PREFIX: &str = "ztok";
const ENV_PREFIX_FLAGS: &[&str] = &["-i", "--ignore-environment"];

const DIRECT_PREFIXES: &[&str] = &[
    "aws",
    "cargo",
    "curl",
    "docker",
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
    AlreadyZtok,
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
            Self::UnsupportedCommand => "command is not in the embedded ZTOK allowlist",
            Self::UnsupportedArguments => {
                "command shape is not supported by the embedded ZTOK rewriter"
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
        ShellCommandRewriteKind::AlreadyZtok | ShellCommandRewriteKind::Rewritten => {
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
    if let Some(rest) = trimmed.strip_prefix("codex ztok ") {
        if rest.starts_with('-') {
            return ShellCommandRewriteAnalysis {
                command: format!("{ZTOK_PREFIX} {rest}"),
                kind: ShellCommandRewriteKind::Rewritten,
            };
        }
        let nested = analyze_shell_command(rest);
        return match nested.kind {
            ShellCommandRewriteKind::AlreadyZtok | ShellCommandRewriteKind::Rewritten => {
                ShellCommandRewriteAnalysis {
                    command: nested.command,
                    kind: ShellCommandRewriteKind::Rewritten,
                }
            }
            ShellCommandRewriteKind::Passthrough { reason, candidate } => {
                passthrough(trimmed, reason, candidate)
            }
        };
    }
    if trimmed == "codex ztok" {
        return ShellCommandRewriteAnalysis {
            command: ZTOK_PREFIX.to_string(),
            kind: ShellCommandRewriteKind::Rewritten,
        };
    }
    if trimmed == ZTOK_PREFIX || trimmed.starts_with("ztok ") {
        return ShellCommandRewriteAnalysis {
            command: trimmed.to_string(),
            kind: ShellCommandRewriteKind::AlreadyZtok,
        };
    }
    if contains_shell_metacharacters(trimmed) {
        return passthrough(
            trimmed,
            ShellCommandPassthroughReason::ShellMetacharacters,
            looks_like_ztok_candidate(trimmed),
        );
    }

    let Some(args) = shlex::split(trimmed) else {
        return passthrough(
            trimmed,
            ShellCommandPassthroughReason::ParseFailed,
            looks_like_ztok_candidate(trimmed),
        );
    };
    let Some(parsed) = parse_command_target(&args) else {
        return passthrough(
            trimmed,
            ShellCommandPassthroughReason::MissingCommand,
            looks_like_ztok_candidate(trimmed),
        );
    };
    if parsed.target == "sudo" {
        return passthrough(trimmed, ShellCommandPassthroughReason::Sudo, true);
    }

    let rewritten = match parsed.target.as_str() {
        "cat" => rewrite_cat(parsed.rest),
        "head" => rewrite_head(parsed.rest),
        "tail" => rewrite_tail(parsed.rest),
        "find" => rewrite_find(parsed.rest),
        "rg" => rewrite_rg(parsed.rest),
        command if DIRECT_PREFIXES.contains(&command) => Some(format!(
            "{ZTOK_PREFIX} {command}{}",
            join_rest_args(parsed.rest)
        )),
        _ => {
            return passthrough(
                trimmed,
                ShellCommandPassthroughReason::UnsupportedCommand,
                looks_like_ztok_candidate(trimmed),
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
    Some(format!("{ZTOK_PREFIX} read {}", shell_escape(path)))
}

fn rewrite_head(rest: &[String]) -> Option<String> {
    rewrite_read_window(rest, "--max-lines")
}

fn rewrite_tail(rest: &[String]) -> Option<String> {
    rewrite_read_window(rest, "--tail-lines")
}

fn rewrite_rg(rest: &[String]) -> Option<String> {
    let mut pattern = None;
    let mut path = None;
    let mut extra_args = Vec::new();
    let mut index = 0;
    let mut positional_only = false;

    while let Some(arg) = rest.get(index) {
        if arg == "--" && !positional_only {
            positional_only = true;
            index += 1;
            continue;
        }

        if !positional_only && arg != "-" && arg.starts_with('-') {
            if is_unsupported_rg_flag(arg) {
                return None;
            }
            if is_rg_flag_with_value(arg) {
                let value = rest.get(index + 1)?;
                extra_args.push(arg.clone());
                extra_args.push(value.clone());
                index += 2;
                continue;
            }
            if is_rg_flag_with_inline_value(arg) {
                extra_args.push(arg.clone());
                index += 1;
                continue;
            }
            if matches_rg_short_flag_cluster(arg) || is_supported_rg_flag(arg) {
                extra_args.push(arg.clone());
                index += 1;
                continue;
            }
            if matches!(arg.as_str(), "-g" | "--glob") {
                let value = rest.get(index + 1)?;
                extra_args.push(arg.clone());
                extra_args.push(value.clone());
                index += 2;
                continue;
            }
            if arg.starts_with("--glob=")
                || arg
                    .strip_prefix("-g")
                    .is_some_and(|value| !value.is_empty())
            {
                extra_args.push(arg.clone());
                index += 1;
                continue;
            }
            if is_rg_long_flag_with_equals_value(arg) {
                extra_args.push(arg.clone());
                index += 1;
                continue;
            }
            return None;
        }

        if pattern.is_none() {
            pattern = Some(arg.clone());
        } else if path.is_none() {
            path = Some(arg.clone());
        } else {
            return None;
        }
        index += 1;
    }

    let pattern = pattern?;
    let path = path.unwrap_or_else(|| ".".to_string());
    let mut rewritten = vec![ZTOK_PREFIX.to_string(), "grep".to_string(), pattern, path];
    rewritten.extend(extra_args);
    Some(join_shell_words(&rewritten))
}

fn rewrite_find(rest: &[String]) -> Option<String> {
    if find_cmd::has_unsupported_find_flags(rest) {
        return None;
    }

    Some(format!("{ZTOK_PREFIX} find{}", join_rest_args(rest)))
}

fn rewrite_read_window(rest: &[String], line_flag: &str) -> Option<String> {
    let rest = strip_flag_terminators(rest);
    match rest.as_slice() {
        [path] => Some(format!(
            "{ZTOK_PREFIX} read {} {line_flag} 10",
            shell_escape(path)
        )),
        [count, path] => parse_read_window_count(count).map(|lines| {
            format!(
                "{ZTOK_PREFIX} read {} {line_flag} {lines}",
                shell_escape(path)
            )
        }),
        [flag, lines, path] if matches!(*flag, "-n" | "--lines") => Some(format!(
            "{ZTOK_PREFIX} read {} {line_flag} {}",
            shell_escape(path),
            shell_escape(lines)
        )),
        _ => None,
    }
}

fn parse_read_window_count(value: &str) -> Option<&str> {
    parse_numeric_short_flag(value, "-")
        .or_else(|| parse_equals_flag(value, "--lines="))
        .or_else(|| parse_numeric_short_flag(value, "-n"))
}

fn is_supported_rg_flag(arg: &str) -> bool {
    matches!(
        arg,
        "-a" | "-F"
            | "-i"
            | "-n"
            | "-o"
            | "-P"
            | "-s"
            | "-S"
            | "-U"
            | "-u"
            | "-w"
            | "-x"
            | "-c"
            | "-l"
            | "-v"
            | "-h"
            | "-L"
            | "-E"
            | "-0"
            | "--count"
            | "--files-with-matches"
            | "--files"
            | "--invert-match"
            | "--no-filename"
            | "--follow"
            | "--binary"
            | "--no-binary"
            | "--null"
            | "--case-sensitive"
            | "--crlf"
            | "--fixed-strings"
            | "--hidden"
            | "--ignore-case"
            | "--line-number"
            | "--line-regexp"
            | "--multiline"
            | "--multiline-dotall"
            | "--only-matching"
            | "--pcre2"
            | "--smart-case"
            | "--text"
            | "--trim"
            | "--word-regexp"
            | "--vimgrep"
            | "--no-line-number"
            | "--column"
            | "--no-column"
            | "--with-filename"
            | "--heading"
            | "--no-heading"
    )
}

fn is_unsupported_rg_flag(arg: &str) -> bool {
    arg == "-r" || arg.starts_with("-r") || arg == "--replace" || arg.starts_with("--replace=")
}

/// rg flags that consume the next positional token as their value.
fn is_rg_flag_with_value(arg: &str) -> bool {
    matches!(
        arg,
        "-A" | "-B"
            | "-C"
            | "-m"
            | "--after-context"
            | "--before-context"
            | "--context"
            | "--max-count"
    ) || matches!(
        arg,
        "-j" | "--threads" | "-t" | "--type" | "-T" | "--type-not" | "--sort"
    ) || matches!(
        arg,
        "--sortr" | "--dfa-size-limit" | "--regex-size-limit" | "--max-filesize" | "--encoding"
    )
}

/// Short rg flags with an inline value (e.g. `-A3`, `-B2`, `-C5`, `-m10`).
fn is_rg_flag_with_inline_value(arg: &str) -> bool {
    let Some(rest) = arg.strip_prefix('-') else {
        return false;
    };
    if rest.is_empty() || rest.starts_with('-') {
        return false;
    }
    let first = rest.as_bytes()[0];
    if !matches!(first, b'A' | b'B' | b'C' | b'm' | b'j' | b't' | b'T') {
        return false;
    }
    // Must have at least one digit after the flag letter.
    rest[1..].bytes().all(|b| b.is_ascii_digit())
}

/// Long rg flags in `--flag=value` form (value is inline after `=`).
fn is_rg_long_flag_with_equals_value(arg: &str) -> bool {
    matches!(arg, _)
        && arg.contains('=')
        && arg.split_once('=').is_some_and(|(flag, _)| {
            matches!(
                flag,
                "--after-context"
                    | "--before-context"
                    | "--context"
                    | "--max-count"
                    | "--max-filesize"
                    | "--dfa-size-limit"
                    | "--regex-size-limit"
                    | "--threads"
                    | "--sort"
                    | "--sortr"
                    | "--encoding"
                    | "--type"
                    | "--type-not"
            )
        })
}

fn matches_rg_short_flag_cluster(arg: &str) -> bool {
    let Some(cluster) = arg.strip_prefix('-') else {
        return false;
    };
    !cluster.is_empty()
        && !cluster.starts_with('-')
        && cluster.chars().all(|ch| {
            matches!(
                ch,
                'a' | 'F' | 'i' | 'n' | 'o' | 'P' | 's' | 'S' | 'U' | 'u' | 'w' | 'x'
            )
        })
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

fn looks_like_ztok_candidate(command: &str) -> bool {
    let Some(args) = shlex::split(command) else {
        return false;
    };
    let Some(parsed) = parse_command_target(&args) else {
        return false;
    };
    is_ztok_candidate_name(parsed.target.as_str())
}

fn is_ztok_candidate_name(command: &str) -> bool {
    matches!(command, "cat" | "head" | "rg" | "tail") || DIRECT_PREFIXES.contains(&command)
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
            ("git status", Some("ztok git status")),
            (
                "cargo test -p codex-core",
                Some("ztok cargo test -p codex-core"),
            ),
            ("command git status", Some("ztok git status")),
            ("command -p git status", Some("ztok git status")),
            (
                "command -- git -C repo status",
                Some("ztok git -C repo status"),
            ),
            (
                "command -p -- git -C repo status",
                Some("ztok git -C repo status"),
            ),
            ("/usr/bin/git status", Some("ztok git status")),
            ("git -C repo status", Some("ztok git -C repo status")),
            (
                "cargo --manifest-path Cargo.toml test -p codex-core",
                Some("ztok cargo --manifest-path Cargo.toml test -p codex-core"),
            ),
            (
                "cargo +nightly test -p codex-core",
                Some("ztok cargo '+nightly' test -p codex-core"),
            ),
            (
                "git -c color.ui=always -C repo status",
                Some("ztok git -c color.ui=always -C repo status"),
            ),
            (
                "git --git-dir .git --work-tree . status",
                Some("ztok git --git-dir .git --work-tree . status"),
            ),
            ("nice -n 5 git status", Some("nice -n 5 ztok git status")),
            ("nice -5 -- git status", Some("nice -5 ztok git status")),
            ("stdbuf -oL git status", Some("stdbuf -oL ztok git status")),
            (
                "nice -n 5 stdbuf -oL git status",
                Some("nice -n 5 stdbuf -oL ztok git status"),
            ),
            (
                "command nice -n 5 git status",
                Some("nice -n 5 ztok git status"),
            ),
            (
                "command -p stdbuf -oL git status",
                Some("stdbuf -oL ztok git status"),
            ),
            (
                "command cargo +nightly test -p codex-core",
                Some("ztok cargo '+nightly' test -p codex-core"),
            ),
            (
                "/usr/bin/nice -n 5 git status",
                Some("nice -n 5 ztok git status"),
            ),
            (
                "ionice -c 3 git status",
                Some("ionice -c 3 ztok git status"),
            ),
            (
                "ionice -c2 -n7 -- git status",
                Some("ionice -c2 -n7 ztok git status"),
            ),
            (
                "command ionice -c2 git status",
                Some("ionice -c2 ztok git status"),
            ),
            (
                "nice -n 5 ionice -c2 git status",
                Some("nice -n 5 ionice -c2 ztok git status"),
            ),
            ("chrt -r 10 git status", Some("chrt -r 10 ztok git status")),
            (
                "chrt --fifo 20 -- git status",
                Some("chrt --fifo 20 ztok git status"),
            ),
            (
                "command chrt -b 0 git status",
                Some("chrt -b 0 ztok git status"),
            ),
            (
                "nice -n 5 chrt -r 10 git status",
                Some("nice -n 5 chrt -r 10 ztok git status"),
            ),
            (
                "find apps -name '*.test.ts' -type f",
                Some("ztok find apps -name '*.test.ts' -type f"),
            ),
        ]);
    }

    #[test]
    fn rewrites_cat_head_and_tail() {
        assert_rewrite_cases(&[
            ("cat src/main.rs", Some("ztok read src/main.rs")),
            (
                "head -20 src/main.rs",
                Some("ztok read src/main.rs --max-lines 20"),
            ),
            (
                "head src/main.rs",
                Some("ztok read src/main.rs --max-lines 10"),
            ),
            (
                "tail --lines=7 src/main.rs",
                Some("ztok read src/main.rs --tail-lines 7"),
            ),
            (
                "head -n5 -- src/main.rs",
                Some("ztok read src/main.rs --max-lines 5"),
            ),
            (
                "tail src/main.rs",
                Some("ztok read src/main.rs --tail-lines 10"),
            ),
        ]);
    }

    #[test]
    fn rewrites_simple_rg_commands_to_ztok_grep() {
        assert_rewrite_cases(&[
            (
                "rg -n needle src/main.rs",
                Some("ztok grep needle src/main.rs -n"),
            ),
            (
                "rg -ni \"a|b\" src/main.rs",
                Some("ztok grep 'a|b' src/main.rs -ni"),
            ),
            (
                "env FOO=1 rg -n \"render_long_help|print_help|render_help|help_flag|version_flag|try_parse_from\\(\" /workspace/codex-rs/cli/src/main.rs",
                Some(
                    "env FOO=1 ztok grep 'render_long_help|print_help|render_help|help_flag|version_flag|try_parse_from\\(' /workspace/codex-rs/cli/src/main.rs -n",
                ),
            ),
            (
                "rg -n needle --glob '*.rs'",
                Some("ztok grep needle . -n --glob '*.rs'"),
            ),
            (
                "rg needle --glob '*.txt' src",
                Some("ztok grep needle src --glob '*.txt'"),
            ),
            (
                "rg needle -- ./-dash.txt",
                Some("ztok grep needle ./-dash.txt"),
            ),
            ("rg needle -", Some("ztok grep needle -")),
            (
                "rg --glob='*.rs' needle src/main.rs",
                Some("ztok grep needle src/main.rs '--glob=*.rs'"),
            ),
            (
                "rg -n -A 3 needle src/main.rs",
                Some("ztok grep needle src/main.rs -n -A 3"),
            ),
            (
                "rg -n -B 2 needle src/main.rs",
                Some("ztok grep needle src/main.rs -n -B 2"),
            ),
            (
                "rg -n -C 5 needle src/main.rs",
                Some("ztok grep needle src/main.rs -n -C 5"),
            ),
            (
                "rg -n -A 3 -B 2 needle src/main.rs",
                Some("ztok grep needle src/main.rs -n -A 3 -B 2"),
            ),
            (
                "rg -n -B2 -A5 needle src",
                Some("ztok grep needle src -n -B2 -A5"),
            ),
            (
                "rg --after-context=3 needle src",
                Some("ztok grep needle src --after-context=3"),
            ),
            (
                "rg -c needle src/main.rs",
                Some("ztok grep needle src/main.rs -c"),
            ),
            ("rg -l needle src", Some("ztok grep needle src -l")),
        ]);
    }

    #[test]
    fn preserves_existing_ztok_invocations() {
        assert_eq!(
            rewrite_shell_command("ztok git status"),
            Some("ztok git status".to_string())
        );
        assert_eq!(
            rewrite_shell_command("codex ztok git status"),
            Some("ztok git status".to_string())
        );
        assert_eq!(
            rewrite_shell_command("codex ztok --help"),
            Some("ztok --help".to_string())
        );
        assert_eq!(
            rewrite_shell_command("codex ztok --version"),
            Some("ztok --version".to_string())
        );
        assert_eq!(
            rewrite_shell_command("codex ztok"),
            Some("ztok".to_string())
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
            ("FOO=1 git status", Some("FOO=1 ztok git status")),
            (
                "env FOO=1 BAR=2 grep TODO src",
                Some("env FOO=1 BAR=2 ztok grep TODO src"),
            ),
            (
                "env -- FOO=1 git status",
                Some("env -- FOO=1 ztok git status"),
            ),
            (
                "env -i -u HOME git -C repo status",
                Some("env -i -u HOME ztok git -C repo status"),
            ),
            (
                "env --chdir=repo nice -n 5 git status",
                Some("env --chdir=repo nice -n 5 ztok git status"),
            ),
            (
                "FOO=1 command nice -n 5 git status",
                Some("FOO=1 nice -n 5 ztok git status"),
            ),
            (
                "env -i BAR=2 command -p stdbuf -oL git status",
                Some("env -i BAR=2 stdbuf -oL ztok git status"),
            ),
            (
                "env --chdir=repo command ionice -c2 nice -n 5 git status",
                Some("env --chdir=repo ionice -c2 nice -n 5 ztok git status"),
            ),
            (
                "env FOO=1 command chrt -r 10 /usr/bin/git status",
                Some("env FOO=1 chrt -r 10 ztok git status"),
            ),
            (
                "env --chdir=repo command -p stdbuf -oL git --help",
                Some("env --chdir=repo stdbuf -oL ztok git --help"),
            ),
            (
                "codex ztok env --chdir=repo command -p stdbuf -oL git --help",
                Some("env --chdir=repo stdbuf -oL ztok git --help"),
            ),
        ]);
    }

    #[test]
    fn codex_ztok_prefix_preserves_nested_rewrite_matrix() {
        assert_rewrite_cases(&[
            (
                "codex ztok command -p stdbuf -oL git status",
                Some("stdbuf -oL ztok git status"),
            ),
            (
                "codex ztok env --chdir=repo command nice -n 5 git status",
                Some("env --chdir=repo nice -n 5 ztok git status"),
            ),
        ]);

        for (command, reason) in [
            (
                "codex ztok tail -f src/main.rs",
                ShellCommandPassthroughReason::UnsupportedArguments,
            ),
            (
                "codex ztok env -i",
                ShellCommandPassthroughReason::MissingCommand,
            ),
            (
                "codex ztok command chrt -m git status",
                ShellCommandPassthroughReason::UnsupportedCommand,
            ),
        ] {
            let analysis = analyze_shell_command(command);
            assert_eq!(analysis.command, command);
            assert_eq!(
                analysis.kind,
                ShellCommandRewriteKind::Passthrough {
                    reason,
                    candidate: matches!(
                        reason,
                        ShellCommandPassthroughReason::UnsupportedArguments
                    ),
                }
            );
        }
    }

    #[test]
    fn codex_ztok_rewrites_quoted_literals_through_prefixes() {
        assert_rewrite_cases(&[
            (
                "codex ztok env FOO=1 command grep \"a|b\" src/main.rs",
                Some("env FOO=1 ztok grep 'a|b' src/main.rs"),
            ),
            (
                "codex ztok command nice -n 5 git log --format='%h|%s' -1",
                Some("nice -n 5 ztok git log '--format=%h|%s' -1"),
            ),
        ]);
    }

    #[test]
    fn codex_ztok_prefix_keeps_real_shell_syntax_raw() {
        for command in [
            "codex ztok grep \"$(pwd)\" src/main.rs",
            "codex ztok env FOO=1 git status | head",
        ] {
            let analysis = analyze_shell_command(command);
            assert_eq!(analysis.command, command);
            assert_eq!(
                analysis.kind,
                ShellCommandRewriteKind::Passthrough {
                    reason: ShellCommandPassthroughReason::ShellMetacharacters,
                    candidate: true,
                }
            );
        }
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
                candidate: false,
            }
        );

        let analysis = analyze_shell_command("ionice -p 123 git status");
        assert_eq!(analysis.command, "ionice -p 123 git status");
        assert_eq!(
            analysis.kind,
            ShellCommandRewriteKind::Passthrough {
                reason: ShellCommandPassthroughReason::UnsupportedCommand,
                candidate: false,
            }
        );

        let analysis = analyze_shell_command("chrt -m git status");
        assert_eq!(analysis.command, "chrt -m git status");
        assert_eq!(
            analysis.kind,
            ShellCommandRewriteKind::Passthrough {
                reason: ShellCommandPassthroughReason::UnsupportedCommand,
                candidate: false,
            }
        );
    }

    #[test]
    fn reports_missing_command_after_prefixes() {
        for command in [
            "env -i",
            "env --chdir=repo --",
            "command -p",
            "command -p --",
            "stdbuf -oL",
            "codex ztok env -i",
        ] {
            let analysis = analyze_shell_command(command);
            assert_eq!(analysis.command, command);
            assert_eq!(
                analysis.kind,
                ShellCommandRewriteKind::Passthrough {
                    reason: ShellCommandPassthroughReason::MissingCommand,
                    candidate: false,
                }
            );
        }
    }

    #[test]
    fn unsupported_wrapper_flags_do_not_mark_candidates() {
        for command in [
            "env FOO=1 ionice -p 123 git status",
            "command chrt -m git status",
            "codex ztok ionice -p 123 git status",
        ] {
            let analysis = analyze_shell_command(command);
            assert_eq!(analysis.command, command);
            assert_eq!(
                analysis.kind,
                ShellCommandRewriteKind::Passthrough {
                    reason: ShellCommandPassthroughReason::UnsupportedCommand,
                    candidate: false,
                }
            );
        }
    }

    #[test]
    fn parse_failures_stay_raw_without_candidate_hints() {
        for command in [
            "git 'unterminated",
            "env FOO=1 git \"unterminated",
            "codex ztok grep 'unterminated",
        ] {
            let analysis = analyze_shell_command(command);
            assert_eq!(analysis.command, command);
            assert_eq!(
                analysis.kind,
                ShellCommandRewriteKind::Passthrough {
                    reason: ShellCommandPassthroughReason::ParseFailed,
                    candidate: false,
                }
            );
        }
    }

    #[test]
    fn preserves_quoted_literals_while_blocking_real_shell_syntax() {
        assert_rewrite_cases(&[
            (
                "grep 'a|b' src/main.rs",
                Some("ztok grep 'a|b' src/main.rs"),
            ),
            (
                "grep \"a|b\" src/main.rs",
                Some("ztok grep 'a|b' src/main.rs"),
            ),
            (
                "curl 'https://example.com?a=1&b=2'",
                Some("ztok curl 'https://example.com?a=1&b=2'"),
            ),
            (
                "git log --format='%h|%s' -1",
                Some("ztok git log '--format=%h|%s' -1"),
            ),
            (
                "grep a\\|b src/main.rs",
                Some("ztok grep 'a|b' src/main.rs"),
            ),
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
        assert_eq!(
            analyze_shell_command("codex ztok git status | head").kind,
            ShellCommandRewriteKind::Passthrough {
                reason: ShellCommandPassthroughReason::ShellMetacharacters,
                candidate: true,
            }
        );
        assert_eq!(
            analyze_shell_command("codex ztok sudo git status").kind,
            ShellCommandRewriteKind::Passthrough {
                reason: ShellCommandPassthroughReason::Sudo,
                candidate: true,
            }
        );
    }

    #[test]
    fn keeps_sudo_prefixed_candidates_raw_even_through_prefixes() {
        for command in [
            "env FOO=1 sudo git status",
            "command sudo git status",
            "nice -n 5 sudo git status",
            "command -p nice -n 5 sudo git status",
            "codex ztok env FOO=1 sudo git status",
        ] {
            let analysis = analyze_shell_command(command);
            assert_eq!(analysis.command, command);
            assert_eq!(
                analysis.kind,
                ShellCommandRewriteKind::Passthrough {
                    reason: ShellCommandPassthroughReason::Sudo,
                    candidate: true,
                }
            );
        }
    }

    #[test]
    fn leaves_unsupported_cat_head_tail_shapes_raw() {
        for command in [
            "cat src/main.rs src/lib.rs",
            "head -n 3 src/main.rs src/lib.rs",
            "tail -f src/main.rs",
            "env FOO=1 command tail -f src/main.rs",
            "find . -name '*.rs' -o -name '*.ts'",
            "find . -name '*.tmp' -exec rm {} ';'",
            "rg -r '$1' needle src/main.rs",
            "rg --replace '$1' needle src/main.rs",
            "rg needle src/main.rs nested.rs",
        ] {
            let analysis = analyze_shell_command(command);
            assert_eq!(analysis.command, command);
            assert_eq!(
                analysis.kind,
                ShellCommandRewriteKind::Passthrough {
                    reason: ShellCommandPassthroughReason::UnsupportedArguments,
                    candidate: true,
                }
            );
        }
    }

    #[test]
    fn candidate_detection_stays_false_for_non_ztok_commands() {
        for command in [
            "python -c 'print(1)'",
            "env FOO=1 python -c 'print(1)'",
            "command python -c 'print(1)'",
            "nice -n 5 python -c 'print(1)'",
            "codex ztok python -c 'print(1)'",
        ] {
            let analysis = analyze_shell_command(command);
            assert_eq!(analysis.command, command);
            assert_eq!(
                analysis.kind,
                ShellCommandRewriteKind::Passthrough {
                    reason: ShellCommandPassthroughReason::UnsupportedCommand,
                    candidate: false,
                }
            );
        }
    }
}
