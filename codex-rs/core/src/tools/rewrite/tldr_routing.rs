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
) -> Result<SearchRoute, &'static str> {
    if let Some(reason) = passthrough_reason_for_search(directives) {
        return Err(reason);
    }
    let pattern = pattern.trim();
    if pattern.is_empty() {
        return Err("unknown_passthrough");
    }
    if looks_like_regex_pattern(pattern) {
        return Err("raw_pattern_regex");
    }
    if directives.prefer_context_search && looks_like_symbol(pattern) {
        Ok(SearchRoute::ContextSymbol)
    } else {
        Ok(SearchRoute::SemanticQuery)
    }
}

pub(crate) fn search_action(route: SearchRoute) -> TldrToolAction {
    match route {
        SearchRoute::ContextSymbol => TldrToolAction::Context,
        SearchRoute::SemanticQuery => TldrToolAction::Semantic,
    }
}

pub(crate) fn search_reason(problem_kind: ProblemKind, route: SearchRoute) -> &'static str {
    match (problem_kind, route) {
        (ProblemKind::Structural, SearchRoute::ContextSymbol) => "structural_symbol_query",
        (ProblemKind::Mixed, SearchRoute::ContextSymbol) => "mixed_symbol_query",
        (ProblemKind::Factual, SearchRoute::ContextSymbol) => "factual_symbol_query",
        (ProblemKind::Structural, SearchRoute::SemanticQuery) => "structural_code_search_query",
        (ProblemKind::Mixed, SearchRoute::SemanticQuery) => "mixed_code_search_query",
        (ProblemKind::Factual, SearchRoute::SemanticQuery) => "factual_code_search_query",
    }
}

pub(crate) fn extract_reason(problem_kind: ProblemKind) -> &'static str {
    match problem_kind {
        ProblemKind::Structural => "structural_file_extract",
        ProblemKind::Mixed => "mixed_file_extract",
        ProblemKind::Factual => "factual_file_extract",
    }
}

pub(crate) fn shell_intercept_reason(problem_kind: ProblemKind) -> Option<&'static str> {
    match problem_kind {
        ProblemKind::Structural => Some("structural_shell_search_intercept"),
        ProblemKind::Mixed => Some("mixed_shell_search_intercept"),
        ProblemKind::Factual => None,
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
    use super::SearchRoute;
    use super::classify_search_route;
    use super::extract_reason;
    use super::passthrough_reason_for_read;
    use super::search_reason;
    use super::shell_intercept_reason;
    use crate::tools::rewrite::ProblemKind;
    use crate::tools::rewrite::ToolRoutingDirectives;
    use pretty_assertions::assert_eq;

    #[test]
    fn search_route_prefers_context_for_symbol_queries() {
        let route = classify_search_route("create_tldr_tool", &ToolRoutingDirectives::default())
            .expect("route should succeed");
        assert_eq!(route, SearchRoute::ContextSymbol);
    }

    #[test]
    fn search_route_passthroughs_for_regex() {
        let reason = classify_search_route("foo.*bar", &ToolRoutingDirectives::default())
            .expect_err("regex should passthrough");
        assert_eq!(reason, "raw_pattern_regex");
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
    fn reason_templates_match_problem_kind() {
        assert_eq!(
            search_reason(ProblemKind::Mixed, SearchRoute::SemanticQuery),
            "mixed_code_search_query"
        );
        assert_eq!(
            extract_reason(ProblemKind::Structural),
            "structural_file_extract"
        );
        assert_eq!(shell_intercept_reason(ProblemKind::Factual), None);
    }
}
