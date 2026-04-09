use crate::tools::rewrite::tldr_routing::SearchRoute;
use crate::tools::rewrite::tldr_routing::SearchSignal;

pub(crate) struct QueryCorpusCase {
    pub(crate) pattern: &'static str,
    pub(crate) route: SearchRoute,
    pub(crate) signal: SearchSignal,
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
