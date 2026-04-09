use crate::tools::context::ToolPayload;
use crate::tools::rewrite::tldr_routing::SearchRoute;
use crate::tools::rewrite::tldr_routing::SearchSignal;
use crate::tools::router::ToolCall;
use std::collections::BTreeMap;

pub(crate) struct QueryCorpusCase {
    pub(crate) pattern: &'static str,
    pub(crate) route: SearchRoute,
    pub(crate) signal: SearchSignal,
}

pub(crate) struct QueryMatrixCase {
    pub(crate) pattern: &'static str,
    pub(crate) route: SearchRoute,
    pub(crate) signal: SearchSignal,
}

pub(crate) struct ShellCorpusCase {
    pub(crate) command: &'static str,
    pub(crate) route: Option<SearchRoute>,
    pub(crate) signal: Option<SearchSignal>,
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

pub(crate) fn project_route_counts() -> BTreeMap<&'static str, usize> {
    let mut counts = BTreeMap::new();
    for case in PROJECT_QUERY_CORPUS {
        *counts.entry(route_label(case.route)).or_insert(0usize) += 1;
    }
    counts
}

pub(crate) fn project_signal_counts() -> BTreeMap<&'static str, usize> {
    let mut counts = BTreeMap::new();
    for case in PROJECT_QUERY_CORPUS {
        *counts.entry(signal_label(case.signal)).or_insert(0usize) += 1;
    }
    counts
}

pub(crate) fn project_structural_search_reason_counts() -> BTreeMap<&'static str, usize> {
    let mut counts = BTreeMap::new();
    for case in PROJECT_QUERY_CORPUS {
        *counts
            .entry(structural_search_reason(case.signal))
            .or_insert(0usize) += 1;
    }
    counts
}

pub(crate) fn project_structural_shell_reason_counts() -> BTreeMap<&'static str, usize> {
    let mut counts = BTreeMap::new();
    for case in PROJECT_QUERY_CORPUS {
        *counts
            .entry(structural_shell_intercept_reason(case.signal))
            .or_insert(0usize) += 1;
    }
    counts
}

pub(crate) fn grep_payload(pattern: &str, include: Option<&str>) -> String {
    match include {
        Some(include) => format!(r#"{{"pattern":"{pattern}","include":"{include}"}}"#),
        None => format!(r#"{{"pattern":"{pattern}"}}"#),
    }
}

pub(crate) fn grep_tool_call(call_id: &str, pattern: &str, include: Option<&str>) -> ToolCall {
    grep_tool_call_from_arguments(call_id, grep_payload(pattern, include))
}

pub(crate) fn grep_tool_call_from_arguments(call_id: &str, arguments: String) -> ToolCall {
    ToolCall {
        tool_name: "grep_files".to_string(),
        tool_namespace: None,
        call_id: call_id.to_string(),
        payload: ToolPayload::Function { arguments },
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

pub(crate) const REAL_QUERY_MATRIX: [QueryMatrixCase; 9] = [
    QueryMatrixCase {
        pattern: "create_tldr_tool",
        route: SearchRoute::ContextSymbol,
        signal: SearchSignal::BareSymbol,
    },
    QueryMatrixCase {
        pattern: "`Foo.bar()`",
        route: SearchRoute::ContextSymbol,
        signal: SearchSignal::WrappedSymbol,
    },
    QueryMatrixCase {
        pattern: "Foo.bar",
        route: SearchRoute::ContextSymbol,
        signal: SearchSignal::MemberSymbol,
    },
    QueryMatrixCase {
        pattern: "where is create_tldr_tool used",
        route: SearchRoute::SemanticQuery,
        signal: SearchSignal::NaturalLanguage,
    },
    QueryMatrixCase {
        pattern: "src/tools/spec.rs",
        route: SearchRoute::SemanticQuery,
        signal: SearchSignal::PathLike,
    },
    QueryMatrixCase {
        pattern: "panic handler",
        route: SearchRoute::SemanticQuery,
        signal: SearchSignal::NaturalLanguage,
    },
    QueryMatrixCase {
        pattern: "ToolCallRuntimeImpl",
        route: SearchRoute::ContextSymbol,
        signal: SearchSignal::BareSymbol,
    },
    QueryMatrixCase {
        pattern: "error_boundary_component",
        route: SearchRoute::ContextSymbol,
        signal: SearchSignal::BareSymbol,
    },
    QueryMatrixCase {
        pattern: "symbol lookup without spaces",
        route: SearchRoute::SemanticQuery,
        signal: SearchSignal::NaturalLanguage,
    },
];

pub(crate) const PROJECT_SHELL_CORPUS: [ShellCorpusCase; 6] = [
    ShellCorpusCase {
        command: "rg rewrite_tool_call core/src/tools/rewrite/engine.rs",
        route: Some(SearchRoute::ContextSymbol),
        signal: Some(SearchSignal::BareSymbol),
    },
    ShellCorpusCase {
        command: "rg `emit_tool_route_metric()` core/src/tools/rewrite/engine.rs",
        route: Some(SearchRoute::ContextSymbol),
        signal: Some(SearchSignal::WrappedSymbol),
    },
    ShellCorpusCase {
        command: "rg decision.signal core/src/tools/rewrite",
        route: Some(SearchRoute::ContextSymbol),
        signal: Some(SearchSignal::MemberSymbol),
    },
    ShellCorpusCase {
        command: "rg 'where is TurnContext defined' core/src",
        route: Some(SearchRoute::SemanticQuery),
        signal: Some(SearchSignal::NaturalLanguage),
    },
    ShellCorpusCase {
        command: "rg core/src/tools/rewrite/engine.rs core/src",
        route: Some(SearchRoute::SemanticQuery),
        signal: Some(SearchSignal::PathLike),
    },
    ShellCorpusCase {
        command: "rg 'foo.*bar' core/src",
        route: None,
        signal: None,
    },
];

pub(crate) const PROJECT_REGEX_PATTERN: &str = "foo.*bar";
