use crate::tools::rewrite::ProblemKind;
use crate::tools::rewrite::ToolRoutingDirectives;
use codex_native_tldr::lang_support::SupportedLanguage;
use codex_native_tldr::tool_api::TldrToolAction;
use codex_native_tldr::tool_api::TldrToolLanguage;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SearchRoute {
    ContextSymbol,
    SemanticQuery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SearchSignal {
    BareSymbol,
    WrappedSymbol,
    MemberSymbol,
    NaturalLanguage,
    PathLike,
    GenericSemantic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SearchClassification {
    pub(crate) route: SearchRoute,
    pub(crate) signal: SearchSignal,
}

pub(crate) fn passthrough_reason_for_search(
    directives: &ToolRoutingDirectives,
) -> Option<&'static str> {
    if directives.disable_auto_tldr_once {
        Some("explicit_raw_request")
    } else if directives.force_raw_grep {
        Some("force_raw_grep")
    } else if matches!(directives.problem_kind, ProblemKind::Factual) && !directives.force_tldr {
        Some("factual_query")
    } else {
        None
    }
}

pub(crate) fn passthrough_reason_for_read(
    directives: &ToolRoutingDirectives,
) -> Option<&'static str> {
    if directives.disable_auto_tldr_once {
        Some("explicit_raw_request")
    } else if directives.force_raw_read {
        Some("force_raw_read")
    } else if matches!(directives.problem_kind, ProblemKind::Factual) && !directives.force_tldr {
        Some("factual_query")
    } else {
        None
    }
}

pub(crate) fn classify_search_route(
    pattern: &str,
    directives: &ToolRoutingDirectives,
) -> Result<SearchClassification, &'static str> {
    if let Some(reason) = passthrough_reason_for_search(directives) {
        return Err(reason);
    }
    let pattern = pattern.trim();
    if pattern.is_empty() {
        return Err("unknown_passthrough");
    }

    let Some(signal) = classify_search_signal(pattern) else {
        return Err("raw_pattern_regex");
    };

    let route = if directives.prefer_context_search && is_context_signal(signal) {
        SearchRoute::ContextSymbol
    } else {
        SearchRoute::SemanticQuery
    };

    Ok(SearchClassification { route, signal })
}

pub(crate) fn context_symbol(pattern: &str) -> Option<String> {
    context_symbol_candidate(pattern).map(str::to_owned)
}

pub(crate) fn search_action(route: SearchRoute) -> TldrToolAction {
    match route {
        SearchRoute::ContextSymbol => TldrToolAction::Context,
        SearchRoute::SemanticQuery => TldrToolAction::Semantic,
    }
}

pub(crate) fn search_reason(problem_kind: ProblemKind, signal: SearchSignal) -> &'static str {
    match (problem_kind, signal) {
        (ProblemKind::Structural, SearchSignal::BareSymbol) => "structural_symbol_query",
        (ProblemKind::Mixed, SearchSignal::BareSymbol) => "mixed_symbol_query",
        (ProblemKind::Factual, SearchSignal::BareSymbol) => "factual_symbol_query",
        (ProblemKind::Structural, SearchSignal::WrappedSymbol) => "structural_wrapped_symbol_query",
        (ProblemKind::Mixed, SearchSignal::WrappedSymbol) => "mixed_wrapped_symbol_query",
        (ProblemKind::Factual, SearchSignal::WrappedSymbol) => "factual_wrapped_symbol_query",
        (ProblemKind::Structural, SearchSignal::MemberSymbol) => "structural_member_symbol_query",
        (ProblemKind::Mixed, SearchSignal::MemberSymbol) => "mixed_member_symbol_query",
        (ProblemKind::Factual, SearchSignal::MemberSymbol) => "factual_member_symbol_query",
        (ProblemKind::Structural, SearchSignal::NaturalLanguage) => {
            "structural_natural_language_search_query"
        }
        (ProblemKind::Mixed, SearchSignal::NaturalLanguage) => {
            "mixed_natural_language_search_query"
        }
        (ProblemKind::Factual, SearchSignal::NaturalLanguage) => {
            "factual_natural_language_search_query"
        }
        (ProblemKind::Structural, SearchSignal::PathLike) => "structural_pathlike_search_query",
        (ProblemKind::Mixed, SearchSignal::PathLike) => "mixed_pathlike_search_query",
        (ProblemKind::Factual, SearchSignal::PathLike) => "factual_pathlike_search_query",
        (ProblemKind::Structural, SearchSignal::GenericSemantic) => "structural_code_search_query",
        (ProblemKind::Mixed, SearchSignal::GenericSemantic) => "mixed_code_search_query",
        (ProblemKind::Factual, SearchSignal::GenericSemantic) => "factual_code_search_query",
    }
}

pub(crate) fn extract_reason(problem_kind: ProblemKind) -> &'static str {
    match problem_kind {
        ProblemKind::Structural => "structural_file_extract",
        ProblemKind::Mixed => "mixed_file_extract",
        ProblemKind::Factual => "factual_file_extract",
    }
}

pub(crate) fn shell_intercept_reason(
    problem_kind: ProblemKind,
    signal: SearchSignal,
) -> Option<&'static str> {
    match problem_kind {
        ProblemKind::Factual => None,
        ProblemKind::Structural => Some(match signal {
            SearchSignal::BareSymbol => "structural_shell_symbol_intercept",
            SearchSignal::WrappedSymbol => "structural_shell_wrapped_symbol_intercept",
            SearchSignal::MemberSymbol => "structural_shell_member_symbol_intercept",
            SearchSignal::NaturalLanguage => "structural_shell_natural_language_intercept",
            SearchSignal::PathLike => "structural_shell_pathlike_intercept",
            SearchSignal::GenericSemantic => "structural_shell_search_intercept",
        }),
        ProblemKind::Mixed => Some(match signal {
            SearchSignal::BareSymbol => "mixed_shell_symbol_intercept",
            SearchSignal::WrappedSymbol => "mixed_shell_wrapped_symbol_intercept",
            SearchSignal::MemberSymbol => "mixed_shell_member_symbol_intercept",
            SearchSignal::NaturalLanguage => "mixed_shell_natural_language_intercept",
            SearchSignal::PathLike => "mixed_shell_pathlike_intercept",
            SearchSignal::GenericSemantic => "mixed_shell_search_intercept",
        }),
    }
}

pub(crate) fn shell_intercept_message(reason: &str, arguments: &str) -> String {
    format!(
        "Intercepted broad shell search ({reason}). Use ztldr first (context for symbols, semantic for natural-language code search).\nPass through raw grep/read for regex patterns, exact text checks, or explicit raw requests.\nIf ztldr returns degradedMode or structuredFailure, report that explicitly.\nSuggested ztldr arguments: {arguments}"
    )
}

pub(crate) fn non_code_reason(path: &Path) -> &'static str {
    if path.extension().is_some() {
        "non_code_path"
    } else {
        "unknown_passthrough"
    }
}

pub(crate) fn to_tldr_language(language: SupportedLanguage) -> TldrToolLanguage {
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

fn classify_search_signal(pattern: &str) -> Option<SearchSignal> {
    if let Some(symbol) = context_symbol_candidate(pattern) {
        return Some(if has_symbol_wrapper(pattern) {
            SearchSignal::WrappedSymbol
        } else if symbol.contains(['.', '#']) {
            SearchSignal::MemberSymbol
        } else {
            SearchSignal::BareSymbol
        });
    }
    if looks_like_regex_pattern(pattern) {
        return None;
    }
    if looks_like_natural_language(pattern) {
        Some(SearchSignal::NaturalLanguage)
    } else if looks_like_path(pattern) {
        Some(SearchSignal::PathLike)
    } else {
        Some(SearchSignal::GenericSemantic)
    }
}

fn is_context_signal(signal: SearchSignal) -> bool {
    matches!(
        signal,
        SearchSignal::BareSymbol | SearchSignal::WrappedSymbol | SearchSignal::MemberSymbol
    )
}

fn context_symbol_candidate(pattern: &str) -> Option<&str> {
    let trimmed = pattern.trim();
    if trimmed.contains(char::is_whitespace) {
        return None;
    }
    let trimmed = trim_symbol_wrappers(trimmed)?;
    if trimmed.is_empty() || looks_like_sentence_fragment(trimmed) {
        return None;
    }
    let trimmed = trimmed.strip_suffix("()").unwrap_or(trimmed);
    looks_like_symbol(trimmed).then_some(trimmed)
}

fn has_symbol_wrapper(pattern: &str) -> bool {
    let trimmed = pattern.trim();
    trimmed.starts_with(['`', '"', '\'']) || trimmed.ends_with(['`', '"', '\''])
}

fn trim_symbol_wrappers(pattern: &str) -> Option<&str> {
    let trimmed = pattern.trim_matches(|ch| matches!(ch, '`' | '"' | '\''));
    (!trimmed.is_empty()).then_some(trimmed)
}

fn looks_like_natural_language(pattern: &str) -> bool {
    pattern.contains(char::is_whitespace)
        || pattern.ends_with('?')
        || [
            "how", "why", "what", "where", "which", "find", "show", "list",
        ]
        .iter()
        .any(|prefix| pattern.starts_with(prefix))
}

fn looks_like_path(pattern: &str) -> bool {
    pattern.contains('/') || pattern.ends_with(".rs") || pattern.ends_with(".ts")
}

fn looks_like_sentence_fragment(pattern: &str) -> bool {
    pattern.contains('/')
        || pattern.contains('=')
        || pattern.contains(',')
        || pattern.ends_with('?')
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
    chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | ':' | '.' | '#'))
}

#[cfg(test)]
mod tests {
    use super::SearchRoute;
    use super::SearchSignal;
    use super::classify_search_route;
    use super::context_symbol;
    use super::extract_reason;
    use super::passthrough_reason_for_read;
    use super::passthrough_reason_for_search;
    use super::search_reason;
    use super::shell_intercept_reason;
    use crate::tools::rewrite::ProblemKind;
    use crate::tools::rewrite::ToolRoutingDirectives;
    use crate::tools::rewrite::test_corpus::PROJECT_QUERY_CORPUS;
    use crate::tools::rewrite::test_corpus::PROJECT_REGEX_PATTERN;
    use crate::tools::rewrite::test_corpus::project_route_counts;
    use crate::tools::rewrite::test_corpus::project_signal_counts;
    use crate::tools::rewrite::test_corpus::route_label;
    use crate::tools::rewrite::test_corpus::signal_label;
    use crate::tools::rewrite::test_corpus::structural_search_reason as corpus_structural_search_reason;
    use pretty_assertions::assert_eq;

    #[test]
    fn search_route_prefers_context_for_symbol_queries() {
        let classification =
            classify_search_route("create_tldr_tool", &ToolRoutingDirectives::default())
                .expect("route should succeed");
        assert_eq!(classification.route, SearchRoute::ContextSymbol);
        assert_eq!(classification.signal, SearchSignal::BareSymbol);
    }

    #[test]
    fn search_route_accepts_wrapped_method_symbols() {
        let classification =
            classify_search_route("`Foo.bar()`", &ToolRoutingDirectives::default())
                .expect("route should succeed");
        assert_eq!(classification.route, SearchRoute::ContextSymbol);
        assert_eq!(classification.signal, SearchSignal::WrappedSymbol);
        assert_eq!(context_symbol("`Foo.bar()`"), Some("Foo.bar".to_string()));
    }

    #[test]
    fn search_route_uses_member_signal_for_dotted_symbols() {
        let classification = classify_search_route("Foo.bar", &ToolRoutingDirectives::default())
            .expect("route should succeed");
        assert_eq!(classification.route, SearchRoute::ContextSymbol);
        assert_eq!(classification.signal, SearchSignal::MemberSymbol);
        assert_eq!(
            corpus_structural_search_reason(classification.signal),
            "structural_member_symbol_query"
        );
    }

    #[test]
    fn search_route_uses_semantic_for_natural_language_questions() {
        let classification = classify_search_route(
            "where is create_tldr_tool used",
            &ToolRoutingDirectives::default(),
        )
        .expect("route should succeed");
        assert_eq!(classification.route, SearchRoute::SemanticQuery);
        assert_eq!(classification.signal, SearchSignal::NaturalLanguage);
    }

    #[test]
    fn search_route_uses_semantic_for_paths() {
        let classification =
            classify_search_route("src/tools/spec.rs", &ToolRoutingDirectives::default())
                .expect("route should succeed");
        assert_eq!(classification.route, SearchRoute::SemanticQuery);
        assert_eq!(classification.signal, SearchSignal::PathLike);
    }

    #[test]
    fn search_route_passthroughs_for_regex() {
        let reason =
            classify_search_route(PROJECT_REGEX_PATTERN, &ToolRoutingDirectives::default())
                .expect_err("regex should passthrough");
        assert_eq!(reason, "raw_pattern_regex");
    }

    #[test]
    fn search_route_real_query_matrix_stays_stable() {
        let cases = [
            (
                "create_tldr_tool",
                SearchRoute::ContextSymbol,
                SearchSignal::BareSymbol,
                "structural_symbol_query",
                Some("structural_shell_symbol_intercept"),
            ),
            (
                "`Foo.bar()`",
                SearchRoute::ContextSymbol,
                SearchSignal::WrappedSymbol,
                "structural_wrapped_symbol_query",
                Some("structural_shell_wrapped_symbol_intercept"),
            ),
            (
                "Foo.bar",
                SearchRoute::ContextSymbol,
                SearchSignal::MemberSymbol,
                "structural_member_symbol_query",
                Some("structural_shell_member_symbol_intercept"),
            ),
            (
                "where is create_tldr_tool used",
                SearchRoute::SemanticQuery,
                SearchSignal::NaturalLanguage,
                "structural_natural_language_search_query",
                Some("structural_shell_natural_language_intercept"),
            ),
            (
                "src/tools/spec.rs",
                SearchRoute::SemanticQuery,
                SearchSignal::PathLike,
                "structural_pathlike_search_query",
                Some("structural_shell_pathlike_intercept"),
            ),
            (
                "panic handler",
                SearchRoute::SemanticQuery,
                SearchSignal::NaturalLanguage,
                "structural_natural_language_search_query",
                Some("structural_shell_natural_language_intercept"),
            ),
            (
                "ToolCallRuntimeImpl",
                SearchRoute::ContextSymbol,
                SearchSignal::BareSymbol,
                "structural_symbol_query",
                Some("structural_shell_symbol_intercept"),
            ),
            (
                "error_boundary_component",
                SearchRoute::ContextSymbol,
                SearchSignal::BareSymbol,
                "structural_symbol_query",
                Some("structural_shell_symbol_intercept"),
            ),
            (
                "symbol lookup without spaces",
                SearchRoute::SemanticQuery,
                SearchSignal::NaturalLanguage,
                "structural_natural_language_search_query",
                Some("structural_shell_natural_language_intercept"),
            ),
        ];

        for (pattern, expected_route, expected_signal, expected_reason, expected_intercept) in cases
        {
            let classification = classify_search_route(pattern, &ToolRoutingDirectives::default())
                .expect("route should succeed");
            assert_eq!(classification.route, expected_route, "pattern: {pattern}");
            assert_eq!(classification.signal, expected_signal, "pattern: {pattern}");
            assert_eq!(
                corpus_structural_search_reason(classification.signal),
                expected_reason,
                "pattern: {pattern}"
            );
            assert_eq!(
                shell_intercept_reason(ProblemKind::Structural, classification.signal),
                expected_intercept,
                "pattern: {pattern}"
            );
        }
    }

    #[test]
    fn context_like_queries_can_opt_out_of_context_route_without_losing_signal() {
        let classification = classify_search_route(
            "create_tldr_tool",
            &ToolRoutingDirectives {
                prefer_context_search: false,
                ..Default::default()
            },
        )
        .expect("route should succeed");
        assert_eq!(classification.route, SearchRoute::SemanticQuery);
        assert_eq!(classification.signal, SearchSignal::BareSymbol);
    }

    #[test]
    fn factual_queries_only_classify_when_force_tldr_is_enabled() {
        let reason = classify_search_route(
            "create_tldr_tool",
            &ToolRoutingDirectives {
                problem_kind: ProblemKind::Factual,
                ..Default::default()
            },
        )
        .expect_err("factual query should passthrough by default");
        assert_eq!(reason, "factual_query");

        let classification = classify_search_route(
            "create_tldr_tool",
            &ToolRoutingDirectives {
                problem_kind: ProblemKind::Factual,
                force_tldr: true,
                ..Default::default()
            },
        )
        .expect("forced factual query should classify");
        assert_eq!(classification.route, SearchRoute::ContextSymbol);
        assert_eq!(classification.signal, SearchSignal::BareSymbol);
        assert_eq!(
            search_reason(ProblemKind::Factual, classification.signal),
            "factual_symbol_query"
        );
        assert_eq!(
            shell_intercept_reason(ProblemKind::Factual, classification.signal),
            None
        );
    }

    #[test]
    fn current_project_query_corpus_summary_stays_stable() {
        use std::collections::BTreeMap;

        let corpus = PROJECT_QUERY_CORPUS
            .into_iter()
            .map(|case| (case.pattern, Ok((case.route, case.signal))))
            .chain([
                (
                    "ToolCallSource::Direct",
                    Ok((SearchRoute::ContextSymbol, SearchSignal::BareSymbol)),
                ),
                (
                    "panic handler",
                    Ok((SearchRoute::SemanticQuery, SearchSignal::NaturalLanguage)),
                ),
                (
                    "codex-rs/otel/src/metrics/names.rs",
                    Ok((SearchRoute::SemanticQuery, SearchSignal::PathLike)),
                ),
                (PROJECT_REGEX_PATTERN, Err("raw_pattern_regex")),
            ])
            .collect::<Vec<_>>();

        let mut route_counts = BTreeMap::new();
        let mut signal_counts = BTreeMap::new();
        let mut passthrough_counts = BTreeMap::new();

        for (pattern, expected) in corpus {
            match (
                classify_search_route(pattern, &ToolRoutingDirectives::default()),
                expected,
            ) {
                (Ok(classification), Ok((expected_route, expected_signal))) => {
                    assert_eq!(classification.route, expected_route, "pattern: {pattern}");
                    assert_eq!(classification.signal, expected_signal, "pattern: {pattern}");
                    *route_counts
                        .entry(route_label(classification.route))
                        .or_insert(0usize) += 1;
                    *signal_counts
                        .entry(signal_label(classification.signal))
                        .or_insert(0usize) += 1;
                }
                (Err(reason), Err(expected_reason)) => {
                    assert_eq!(reason, expected_reason, "pattern: {pattern}");
                    *passthrough_counts.entry(reason).or_insert(0usize) += 1;
                }
                (actual, expected) => panic!(
                    "pattern {pattern} classified unexpectedly: actual={actual:?} expected={expected:?}"
                ),
            }
        }

        let mut expected_route_counts = project_route_counts();
        *expected_route_counts.entry("context").or_insert(0usize) += 1;
        *expected_route_counts.entry("semantic").or_insert(0usize) += 2;
        assert_eq!(route_counts, expected_route_counts);

        let mut expected_signal_counts = project_signal_counts();
        *expected_signal_counts
            .entry("bare_symbol")
            .or_insert(0usize) += 1;
        *expected_signal_counts
            .entry("natural_language")
            .or_insert(0usize) += 1;
        *expected_signal_counts.entry("path_like").or_insert(0usize) += 1;
        assert_eq!(signal_counts, expected_signal_counts);
        assert_eq!(
            passthrough_counts,
            BTreeMap::from([("raw_pattern_regex", 1usize)])
        );
    }

    #[test]
    fn passthrough_reason_for_read_respects_force_raw() {
        let reason = passthrough_reason_for_read(&ToolRoutingDirectives {
            force_raw_read: true,
            ..Default::default()
        });
        assert_eq!(reason, Some("force_raw_read"));
    }

    #[test]
    fn passthrough_reason_for_search_respects_explicit_raw_request() {
        let reason = passthrough_reason_for_search(&ToolRoutingDirectives {
            disable_auto_tldr_once: true,
            ..Default::default()
        });
        assert_eq!(reason, Some("explicit_raw_request"));
    }

    #[test]
    fn passthrough_reason_for_search_respects_force_raw_grep() {
        let reason = passthrough_reason_for_search(&ToolRoutingDirectives {
            force_raw_grep: true,
            ..Default::default()
        });
        assert_eq!(reason, Some("force_raw_grep"));
    }

    #[test]
    fn reason_templates_match_problem_kind() {
        assert_eq!(
            search_reason(ProblemKind::Mixed, SearchSignal::GenericSemantic),
            "mixed_code_search_query"
        );
        assert_eq!(
            extract_reason(ProblemKind::Structural),
            "structural_file_extract"
        );
        assert_eq!(
            shell_intercept_reason(ProblemKind::Factual, SearchSignal::GenericSemantic),
            None
        );
    }
}
