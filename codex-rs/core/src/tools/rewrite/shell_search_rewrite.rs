use crate::tools::rewrite::ProblemKind;
use crate::tools::rewrite::ToolRoutingDirectives;
use crate::tools::rewrite::resolve_tldr_project_root;
use codex_native_tldr::lang_support::SupportedLanguage;
use codex_native_tldr::tool_api::TldrToolAction;
use codex_native_tldr::tool_api::TldrToolCallParam;
use codex_native_tldr::tool_api::TldrToolLanguage;
use std::path::Path;

pub(crate) struct ShellSearchInterception {
    pub(crate) message: String,
}

pub(crate) fn maybe_intercept_shell_search(
    raw_command: &str,
    routed_command: &str,
    cwd: &Path,
    directives: &ToolRoutingDirectives,
) -> Option<ShellSearchInterception> {
    if directives.disable_auto_tldr_once
        || directives.force_raw_grep
        || matches!(directives.problem_kind, ProblemKind::Factual)
    {
        return None;
    }

    let query = extract_search_query(routed_command)
        .or_else(|| extract_search_query(raw_command))
        .or_else(|| extract_find_xargs_query(raw_command))?;

    if looks_like_regex_pattern(&query.pattern) {
        return None;
    }

    let action = if directives.prefer_context_search && looks_like_symbol(&query.pattern) {
        TldrToolAction::Context
    } else {
        TldrToolAction::Semantic
    };
    let project_root = resolve_tldr_project_root(cwd, Some(cwd));
    let args = TldrToolCallParam {
        action: action.clone(),
        project: Some(project_root.display().to_string()),
        language: infer_language(query.paths.iter().map(String::as_str)),
        symbol: matches!(action, TldrToolAction::Context).then(|| query.pattern.clone()),
        query: matches!(action, TldrToolAction::Semantic).then(|| query.pattern.clone()),
        module: None,
        path: None,
        line: None,
        paths: (!query.paths.is_empty()).then_some(query.paths.clone()),
        only_tools: None,
        run_lint: None,
        run_typecheck: None,
        max_issues: None,
        include_install_hints: None,
    };
    let arguments = serde_json::to_string(&args).ok()?;
    let reason = match directives.problem_kind {
        ProblemKind::Structural => "structural_shell_search_intercept",
        ProblemKind::Mixed => "mixed_shell_search_intercept",
        ProblemKind::Factual => return None,
    };

    Some(ShellSearchInterception {
        message: format!(
            "Intercepted broad shell search ({reason}). This looks like a structural code-understanding query, so use ztldr first (context for symbols, semantic for natural-language code search) before broad grep.\nPrefer raw grep/read only for regex patterns, exact text checks, or when the user explicitly requests raw search.\nIf ztldr returns degradedMode or structuredFailure, report that explicitly instead of presenting it as normal success.\nSuggested ztldr arguments: {arguments}"
        ),
    })
}

#[derive(Debug)]
struct SearchQuery {
    pattern: String,
    paths: Vec<String>,
}

fn extract_search_query(command: &str) -> Option<SearchQuery> {
    let tokens = shlex::split(command)?;
    let tail = match tokens.as_slice() {
        [head, tail @ ..] if head == "rg" || head == "grep" => tail,
        [head, next, tail @ ..] if head == "ztok" && next == "grep" => tail,
        [head, next, third, tail @ ..] if head == "codex" && next == "ztok" && third == "grep" => {
            tail
        }
        _ => return None,
    };

    parse_search_tail(tail)
}

fn parse_search_tail(tokens: &[String]) -> Option<SearchQuery> {
    let mut pattern: Option<String> = None;
    let mut paths = Vec::new();
    let mut index = 0;

    while index < tokens.len() {
        let token = &tokens[index];
        if token == "--" {
            index += 1;
            break;
        }
        if let Some(explicit_pattern) = inline_pattern_flag(token) {
            if pattern.is_none() {
                pattern = Some(explicit_pattern.to_string());
            }
            index += 1;
            continue;
        }
        if matches!(token.as_str(), "-e" | "--regexp") {
            let explicit_pattern = tokens.get(index + 1)?;
            if pattern.is_none() {
                pattern = Some(explicit_pattern.clone());
            }
            index += 2;
            continue;
        }
        if is_unsupported_pattern_file_flag(token) {
            return None;
        }
        if consumes_next_value(token) {
            index += 2;
            continue;
        }
        if token.starts_with('-') {
            index += 1;
            continue;
        }

        if pattern.is_none() {
            pattern = Some(token.clone());
        } else {
            paths.push(token.clone());
        }
        index += 1;
    }

    while index < tokens.len() {
        let token = &tokens[index];
        if pattern.is_none() {
            pattern = Some(token.clone());
        } else {
            paths.push(token.clone());
        }
        index += 1;
    }

    pattern.map(|pattern| SearchQuery { pattern, paths })
}

fn extract_find_xargs_query(command: &str) -> Option<SearchQuery> {
    let tokens = shlex::split(command)?;
    let pipe_index = tokens.iter().position(|token| token == "|")?;
    let xargs_index = pipe_index + 1;
    if tokens.get(xargs_index)? != "xargs" {
        return None;
    }

    let tail = &tokens[xargs_index + 1..];
    let (search_cmd, search_tail) = match tail {
        [head, tail @ ..] if head == "rg" => (head.as_str(), tail),
        [head, next, tail @ ..] if head == "grep" && (next == "-R" || next == "-r") => {
            ("grep", tail)
        }
        [head, tail @ ..] if head == "grep" => ("grep", tail),
        _ => return None,
    };

    let query = parse_search_tail(search_tail)?;
    if search_cmd == "grep" && query.pattern.is_empty() {
        return None;
    }
    Some(SearchQuery {
        pattern: query.pattern,
        paths: Vec::new(),
    })
}

fn infer_language<'a>(paths: impl Iterator<Item = &'a str>) -> Option<TldrToolLanguage> {
    paths.filter_map(infer_language_from_token).next()
}

fn inline_pattern_flag(token: &str) -> Option<&str> {
    token
        .strip_prefix("--regexp=")
        .or_else(|| short_flag_value(token, 'e'))
}

fn is_unsupported_pattern_file_flag(token: &str) -> bool {
    matches!(token, "-f" | "--file")
        || token.starts_with("--file=")
        || short_flag_value(token, 'f').is_some()
}

fn consumes_next_value(token: &str) -> bool {
    if token.starts_with("--") {
        return matches!(
            token,
            "--glob"
                | "--iglob"
                | "--type"
                | "--type-not"
                | "--max-count"
                | "--max-filesize"
                | "--threads"
                | "--sort"
                | "--sortr"
                | "--context"
                | "--after-context"
                | "--before-context"
                | "--replace"
                | "--pre"
                | "--pre-glob"
        );
    }

    matches!(token, "-g" | "-t" | "-T" | "-m" | "-C" | "-A" | "-B" | "-j")
}

fn short_flag_value(token: &str, flag: char) -> Option<&str> {
    token
        .strip_prefix(&format!("-{flag}"))
        .filter(|value| !value.is_empty())
}

fn infer_language_from_token(token: &str) -> Option<TldrToolLanguage> {
    if let Some(language) = infer_language_from_path(Path::new(token)) {
        return Some(language);
    }

    const GLOB_LANGUAGE_HINTS: [(&str, TldrToolLanguage); 20] = [
        (".tsx", TldrToolLanguage::Typescript),
        (".ts", TldrToolLanguage::Typescript),
        (".jsx", TldrToolLanguage::Javascript),
        (".mjs", TldrToolLanguage::Javascript),
        (".cjs", TldrToolLanguage::Javascript),
        (".js", TldrToolLanguage::Javascript),
        (".rs", TldrToolLanguage::Rust),
        (".py", TldrToolLanguage::Python),
        (".go", TldrToolLanguage::Go),
        (".php", TldrToolLanguage::Php),
        (".zig", TldrToolLanguage::Zig),
        (".java", TldrToolLanguage::Java),
        (".rb", TldrToolLanguage::Ruby),
        (".scala", TldrToolLanguage::Scala),
        (".swift", TldrToolLanguage::Swift),
        (".lua", TldrToolLanguage::Lua),
        (".luau", TldrToolLanguage::Luau),
        (".ex", TldrToolLanguage::Elixir),
        (".exs", TldrToolLanguage::Elixir),
        (".cs", TldrToolLanguage::Csharp),
    ];

    GLOB_LANGUAGE_HINTS
        .iter()
        .find_map(|(needle, language)| token.contains(needle).then_some(*language))
}

fn infer_language_from_path(path: &Path) -> Option<TldrToolLanguage> {
    Some(supported_to_tool_language(SupportedLanguage::from_path(
        path,
    )?))
}

fn supported_to_tool_language(language: SupportedLanguage) -> TldrToolLanguage {
    match language {
        SupportedLanguage::C => TldrToolLanguage::C,
        SupportedLanguage::Cpp => TldrToolLanguage::Cpp,
        SupportedLanguage::CSharp => TldrToolLanguage::Csharp,
        SupportedLanguage::Elixir => TldrToolLanguage::Elixir,
        SupportedLanguage::Go => TldrToolLanguage::Go,
        SupportedLanguage::Java => TldrToolLanguage::Java,
        SupportedLanguage::JavaScript => TldrToolLanguage::Javascript,
        SupportedLanguage::Lua => TldrToolLanguage::Lua,
        SupportedLanguage::Luau => TldrToolLanguage::Luau,
        SupportedLanguage::Php => TldrToolLanguage::Php,
        SupportedLanguage::Python => TldrToolLanguage::Python,
        SupportedLanguage::Ruby => TldrToolLanguage::Ruby,
        SupportedLanguage::Rust => TldrToolLanguage::Rust,
        SupportedLanguage::Scala => TldrToolLanguage::Scala,
        SupportedLanguage::Swift => TldrToolLanguage::Swift,
        SupportedLanguage::TypeScript => TldrToolLanguage::Typescript,
        SupportedLanguage::Zig => TldrToolLanguage::Zig,
    }
}

fn looks_like_regex_pattern(pattern: &str) -> bool {
    pattern.chars().any(|ch| {
        matches!(
            ch,
            '\\' | '^' | '$' | '*' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '|'
        )
    })
}

fn looks_like_symbol(pattern: &str) -> bool {
    let mut chars = pattern.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | ':'))
}

#[cfg(test)]
mod tests {
    use super::maybe_intercept_shell_search;
    use crate::tools::rewrite::ProblemKind;
    use crate::tools::rewrite::ToolRoutingDirectives;
    use pretty_assertions::assert_eq;
    use std::path::Path;
    use tempfile::tempdir;

    #[test]
    fn intercepts_structural_rg_searches_with_tldr_context_suggestion() {
        let interception = maybe_intercept_shell_search(
            "rg -n create_tldr_tool src/main.rs",
            "ztok grep create_tldr_tool src/main.rs -n",
            Path::new("/workspace/codex-rs"),
            &ToolRoutingDirectives::default(),
        )
        .expect("should intercept");

        assert!(
            interception
                .message
                .contains("structural_shell_search_intercept")
        );
        assert!(interception.message.contains(r#""action":"context""#));
        assert!(
            interception
                .message
                .contains(r#""symbol":"create_tldr_tool""#)
        );
        assert!(interception.message.contains(r#""language":"rust""#));
        assert!(
            interception
                .message
                .contains("Prefer raw grep/read only for regex patterns, exact text checks, or when the user explicitly requests raw search.")
        );
        assert!(
            interception
                .message
                .contains("If ztldr returns degradedMode or structuredFailure")
        );
    }

    #[test]
    fn intercepts_mixed_find_xargs_rg_searches() {
        let interception = maybe_intercept_shell_search(
            "find . -name '*.rs' | xargs rg create_tldr_tool",
            "find . -name '*.rs' | xargs rg create_tldr_tool",
            Path::new("/workspace/codex-rs"),
            &ToolRoutingDirectives {
                problem_kind: ProblemKind::Mixed,
                ..Default::default()
            },
        )
        .expect("should intercept");

        assert!(
            interception
                .message
                .contains("mixed_shell_search_intercept")
        );
        assert!(interception.message.contains(r#""action":"context""#));
    }

    #[test]
    fn intercepts_globbed_rg_searches_without_promoting_glob_to_pattern() {
        let tempdir = tempdir().expect("tempdir should exist");
        let repo_root = tempdir.path().join("repo");
        let src_dir = repo_root.join("src");
        std::fs::create_dir_all(&src_dir).expect("repo tree should exist");
        std::fs::create_dir(repo_root.join(".git")).expect("git dir should exist");

        let interception = maybe_intercept_shell_search(
            "rg -g '*.rs' create_tldr_tool src",
            "ztok grep -g '*.rs' create_tldr_tool src",
            src_dir.as_path(),
            &ToolRoutingDirectives::default(),
        )
        .expect("should intercept");

        let project = serde_json::to_string(&repo_root.display().to_string())
            .expect("project path should serialize");
        let paths =
            serde_json::to_string(&vec!["src".to_string()]).expect("paths should serialize");

        assert!(
            interception
                .message
                .contains(r#""symbol":"create_tldr_tool""#)
        );
        assert!(
            interception
                .message
                .contains(&format!(r#""project":{project}"#))
        );
        assert!(
            interception
                .message
                .contains(&format!(r#""paths":{paths}"#))
        );
    }

    #[test]
    fn factual_queries_stay_on_shell_path() {
        let interception = maybe_intercept_shell_search(
            "rg default_timeout src",
            "ztok grep default_timeout src",
            Path::new("/workspace/codex-rs"),
            &ToolRoutingDirectives {
                problem_kind: ProblemKind::Factual,
                ..Default::default()
            },
        );

        assert_eq!(interception.is_some(), false);
    }

    #[test]
    fn regex_queries_stay_on_shell_path() {
        let interception = maybe_intercept_shell_search(
            "rg 'foo.*bar' src",
            "ztok grep 'foo.*bar' src",
            Path::new("/workspace/codex-rs"),
            &ToolRoutingDirectives::default(),
        );

        assert_eq!(interception.is_some(), false);
    }
}
