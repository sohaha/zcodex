use crate::tools::rewrite::ToolRoutingDirectives;
use crate::tools::rewrite::resolve_tldr_project_root;
use crate::tools::rewrite::tldr_routing::SearchRoute;
use crate::tools::rewrite::tldr_routing::classify_search_route;
use crate::tools::rewrite::tldr_routing::context_symbol;
use crate::tools::rewrite::tldr_routing::search_action;
use crate::tools::rewrite::tldr_routing::shell_intercept_message;
use crate::tools::rewrite::tldr_routing::shell_intercept_reason;
use crate::tools::rewrite::tldr_routing::to_tldr_language;
use codex_native_tldr::lang_support::SupportedLanguage;
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
    let query = extract_search_query(routed_command)
        .or_else(|| extract_search_query(raw_command))
        .or_else(|| extract_find_xargs_query(raw_command))?;

    let classification = classify_search_route(query.pattern.as_str(), directives).ok()?;

    let reason = shell_intercept_reason(directives.problem_kind, classification.signal)?;
    let action = search_action(classification.route);
    let project_root = resolve_tldr_project_root(cwd, Some(cwd));
    let symbol = matches!(classification.route, SearchRoute::ContextSymbol)
        .then(|| context_symbol(query.pattern.as_str()))
        .flatten();
    let query_text =
        matches!(classification.route, SearchRoute::SemanticQuery).then_some(query.pattern.clone());
    let args = TldrToolCallParam {
        action,
        project: Some(project_root.display().to_string()),
        language: infer_language(query.paths.iter().map(String::as_str)),
        symbol,
        query: query_text,
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

    Some(ShellSearchInterception {
        message: shell_intercept_message(reason, &arguments),
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

    const GLOB_LANGUAGE_HINTS: [(&str, TldrToolLanguage); 17] = [
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
        (".swift", TldrToolLanguage::Swift),
        (".lua", TldrToolLanguage::Lua),
        (".luau", TldrToolLanguage::Luau),
        (".cs", TldrToolLanguage::Csharp),
    ];

    GLOB_LANGUAGE_HINTS
        .iter()
        .find_map(|(needle, language)| token.contains(needle).then_some(*language))
}

fn infer_language_from_path(path: &Path) -> Option<TldrToolLanguage> {
    Some(to_tldr_language(SupportedLanguage::from_path(path)?))
}

#[cfg(test)]
mod tests {
    use super::maybe_intercept_shell_search;
    use crate::tools::rewrite::ProblemKind;
    use crate::tools::rewrite::ToolRoutingDirectives;
    use crate::tools::rewrite::test_corpus::PROJECT_REGEX_PATTERN;
    use crate::tools::rewrite::test_corpus::PROJECT_SHELL_CORPUS;
    use crate::tools::rewrite::test_corpus::project_route_counts;
    use crate::tools::rewrite::test_corpus::project_structural_shell_reason_counts;
    use crate::tools::rewrite::test_corpus::route_label;
    use crate::tools::rewrite::test_corpus::structural_shell_intercept_reason;
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
                .contains("structural_shell_symbol_intercept")
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
                .contains("Pass through raw grep/read for regex patterns, exact text checks, or explicit raw requests.")
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
                .contains("mixed_shell_symbol_intercept")
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
    fn explicit_raw_request_stays_on_shell_path() {
        let interception = maybe_intercept_shell_search(
            "rg create_tldr_tool src",
            "ztok grep create_tldr_tool src",
            Path::new("/workspace/codex-rs"),
            &ToolRoutingDirectives {
                disable_auto_tldr_once: true,
                ..Default::default()
            },
        );

        assert_eq!(interception.is_some(), false);
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
    fn wrapped_symbol_shell_queries_keep_wrapped_intercept_reason() {
        let interception = maybe_intercept_shell_search(
            "rg -n '`Foo.bar()`' src/main.rs",
            "ztok grep -n '`Foo.bar()`' src/main.rs",
            Path::new("/workspace/codex-rs"),
            &ToolRoutingDirectives::default(),
        )
        .expect("should intercept");

        assert!(
            interception
                .message
                .contains("structural_shell_wrapped_symbol_intercept")
        );
        assert!(interception.message.contains(r#""action":"context""#));
        assert!(interception.message.contains(r#""symbol":"Foo.bar""#));
    }

    #[test]
    fn pathlike_shell_queries_use_semantic_intercept_reason() {
        let interception = maybe_intercept_shell_search(
            "rg src/tools/spec.rs src",
            "ztok grep src/tools/spec.rs src",
            Path::new("/workspace/codex-rs"),
            &ToolRoutingDirectives::default(),
        )
        .expect("should intercept");

        assert!(
            interception
                .message
                .contains("structural_shell_pathlike_intercept")
        );
        assert!(interception.message.contains(r#""action":"semantic""#));
        assert!(
            interception
                .message
                .contains(r#""query":"src/tools/spec.rs""#)
        );
    }

    #[test]
    fn current_project_shell_corpus_summary_stays_stable() {
        use std::collections::BTreeMap;

        let corpus: Vec<(String, Option<(&str, &str)>)> = PROJECT_SHELL_CORPUS
            .into_iter()
            .map(|case| {
                (
                    case.command.to_string(),
                    case.route.zip(case.signal).map(|(route, signal)| {
                        (
                            structural_shell_intercept_reason(signal),
                            route_label(route),
                        )
                    }),
                )
            })
            .collect::<Vec<_>>();

        let mut reason_counts = BTreeMap::new();
        let mut action_counts = BTreeMap::new();
        let mut passthrough_count = 0usize;

        for (command, expected) in corpus {
            let interception = maybe_intercept_shell_search(
                &command,
                &command.replace("rg", "ztok grep"),
                Path::new("/workspace/codex-rs"),
                &ToolRoutingDirectives::default(),
            );

            match (interception, expected) {
                (Some(interception), Some((expected_reason, expected_action))) => {
                    assert!(
                        interception.message.contains(expected_reason),
                        "command: {command}"
                    );
                    assert!(
                        interception
                            .message
                            .contains(&format!(r#""action":"{expected_action}""#)),
                        "command: {command}"
                    );
                    *reason_counts.entry(expected_reason).or_insert(0usize) += 1;
                    *action_counts.entry(expected_action).or_insert(0usize) += 1;
                }
                (None, None) => passthrough_count += 1,
                (Some(_), None) => panic!("command {command} should have stayed on raw shell path"),
                (None, Some((expected_reason, expected_action))) => panic!(
                    "command {command} should have intercepted with {expected_reason} and {expected_action}"
                ),
            }
        }

        assert_eq!(reason_counts, project_structural_shell_reason_counts());
        assert_eq!(action_counts, project_route_counts());
        assert_eq!(passthrough_count, 1);
    }

    #[test]
    fn regex_queries_stay_on_shell_path() {
        let interception = maybe_intercept_shell_search(
            &format!("rg '{PROJECT_REGEX_PATTERN}' src"),
            &format!("ztok grep '{PROJECT_REGEX_PATTERN}' src"),
            Path::new("/workspace/codex-rs"),
            &ToolRoutingDirectives::default(),
        );

        assert_eq!(interception.is_some(), false);
    }
}
