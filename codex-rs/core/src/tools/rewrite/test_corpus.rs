use crate::tools::rewrite::tldr_routing::SearchRoute;
use crate::tools::rewrite::tldr_routing::SearchSignal;

pub(crate) struct QueryCorpusCase {
    pub(crate) pattern: &'static str,
    pub(crate) route: SearchRoute,
    pub(crate) signal: SearchSignal,
}

pub(crate) fn route_label(route: SearchRoute) -> &'static str {
    match route {
        SearchRoute::ContextSymbol => "context",
        SearchRoute::SemanticQuery => "semantic",
    }
}

pub(crate) fn signal_label(signal: SearchSignal) -> &'static str {
    match signal {
        SearchSignal::BareSymbol => "bare_symbol",
        SearchSignal::WrappedSymbol => "wrapped_symbol",
        SearchSignal::MemberSymbol => "member_symbol",
        SearchSignal::NaturalLanguage => "natural_language",
        SearchSignal::PathLike => "path_like",
        SearchSignal::GenericSemantic => "generic_semantic",
    }
}

pub(crate) fn structural_search_reason(signal: SearchSignal) -> &'static str {
    match signal {
        SearchSignal::BareSymbol => "structural_symbol_query",
        SearchSignal::WrappedSymbol => "structural_wrapped_symbol_query",
        SearchSignal::MemberSymbol => "structural_member_symbol_query",
        SearchSignal::NaturalLanguage => "structural_natural_language_search_query",
        SearchSignal::PathLike => "structural_pathlike_search_query",
        SearchSignal::GenericSemantic => "structural_code_search_query",
    }
}

pub(crate) fn structural_shell_intercept_reason(signal: SearchSignal) -> &'static str {
    match signal {
        SearchSignal::BareSymbol => "structural_shell_symbol_intercept",
        SearchSignal::WrappedSymbol => "structural_shell_wrapped_symbol_intercept",
        SearchSignal::MemberSymbol => "structural_shell_member_symbol_intercept",
        SearchSignal::NaturalLanguage => "structural_shell_natural_language_intercept",
        SearchSignal::PathLike => "structural_shell_pathlike_intercept",
        SearchSignal::GenericSemantic => "structural_shell_search_intercept",
    }
}

pub(crate) const PROJECT_QUERY_CORPUS: [QueryCorpusCase; 5] = [
    QueryCorpusCase {
        pattern: "rewrite_tool_call",
        route: SearchRoute::ContextSymbol,
        signal: SearchSignal::BareSymbol,
    },
    QueryCorpusCase {
        pattern: "`emit_tool_route_metric()`",
        route: SearchRoute::ContextSymbol,
        signal: SearchSignal::WrappedSymbol,
    },
    QueryCorpusCase {
        pattern: "decision.signal",
        route: SearchRoute::ContextSymbol,
        signal: SearchSignal::MemberSymbol,
    },
    QueryCorpusCase {
        pattern: "where is TurnContext defined",
        route: SearchRoute::SemanticQuery,
        signal: SearchSignal::NaturalLanguage,
    },
    QueryCorpusCase {
        pattern: "core/src/tools/rewrite/engine.rs",
        route: SearchRoute::SemanticQuery,
        signal: SearchSignal::PathLike,
    },
];

pub(crate) const PROJECT_REGEX_PATTERN: &str = "foo.*bar";
