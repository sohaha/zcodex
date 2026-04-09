use crate::codex::TurnContext;
use crate::config::AutoTldrRoutingMode;
use crate::tools::context::ToolPayload;
use crate::tools::rewrite::AutoTldrContext;
use crate::tools::rewrite::ToolRoutingDirectives;
use crate::tools::rewrite::decision::ToolRewriteDecision;
use crate::tools::rewrite::resolve_tldr_project_root;
use crate::tools::rewrite::tldr_routing::SearchRoute;
use crate::tools::rewrite::tldr_routing::classify_search_route;
use crate::tools::rewrite::tldr_routing::context_symbol;
use crate::tools::rewrite::tldr_routing::non_code_reason;
use crate::tools::rewrite::tldr_routing::search_action;
use crate::tools::rewrite::tldr_routing::search_reason;
use crate::tools::rewrite::tldr_routing::to_tldr_language;
use crate::tools::router::ToolCall;
use codex_native_tldr::lang_support::SupportedLanguage;
use codex_native_tldr::tool_api::TldrToolCallParam;
use codex_native_tldr::tool_api::TldrToolLanguage;
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize)]
struct GrepFilesArgs {
    pattern: String,
    #[serde(default)]
    include: Option<String>,
    #[serde(default)]
    path: Option<String>,
}

pub(crate) async fn rewrite_grep_files_to_tldr(
    turn: &TurnContext,
    call: ToolCall,
    directives: ToolRoutingDirectives,
    mode: AutoTldrRoutingMode,
) -> ToolRewriteDecision {
    let ToolPayload::Function { arguments } = &call.payload else {
        return ToolRewriteDecision::Passthrough {
            call,
            reason: "unknown_passthrough",
            signal: None,
        };
    };

    let args: GrepFilesArgs = match serde_json::from_str(arguments) {
        Ok(args) => args,
        Err(_) => {
            return ToolRewriteDecision::Passthrough {
                call,
                reason: "unknown_passthrough",
                signal: None,
            };
        }
    };

    let classification = match classify_search_route(args.pattern.as_str(), &directives) {
        Ok(classification) => classification,
        Err(reason) => {
            return ToolRewriteDecision::Passthrough {
                call,
                reason,
                signal: None,
            };
        }
    };

    let search_path = turn.resolve_path(args.path.clone());
    let auto_tldr_context = turn.auto_tldr_context.read().await.clone();
    let Some(language) = infer_language(
        &search_path,
        args.include.as_deref(),
        &auto_tldr_context,
        mode,
        directives.force_tldr,
    ) else {
        return ToolRewriteDecision::Passthrough {
            call,
            reason: non_code_reason(&search_path),
            signal: Some(classification.signal),
        };
    };

    let pattern = args.pattern.trim().to_string();
    let reason = search_reason(directives.problem_kind, classification.signal);
    let action = search_action(classification.route);
    let project_root = resolve_tldr_project_root(turn.cwd.as_path(), Some(search_path.as_path()));
    let project = project_root.display().to_string();
    let symbol = matches!(classification.route, SearchRoute::ContextSymbol)
        .then(|| context_symbol(pattern.as_str()))
        .flatten();
    let query = matches!(classification.route, SearchRoute::SemanticQuery).then_some(pattern);

    let rewritten_args = TldrToolCallParam {
        action: action.clone(),
        project: Some(project),
        language: Some(language),
        symbol,
        query,
        module: None,
        path: None,
        line: None,
        paths: None,
        only_tools: None,
        run_lint: None,
        run_typecheck: None,
        max_issues: None,
        include_install_hints: None,
    };

    let arguments = match serde_json::to_string(&rewritten_args) {
        Ok(arguments) => arguments,
        Err(_) => {
            return ToolRewriteDecision::Passthrough {
                call,
                reason: "unknown_passthrough",
                signal: Some(classification.signal),
            };
        }
    };

    ToolRewriteDecision::Rewrite {
        call: ToolCall {
            tool_name: "ztldr".to_string(),
            tool_namespace: None,
            call_id: call.call_id,
            payload: ToolPayload::Function { arguments },
        },
        reason,
        action: Some(action),
        signal: Some(classification.signal),
    }
}

fn infer_language(
    search_path: &Path,
    include: Option<&str>,
    auto_tldr_context: &AutoTldrContext,
    mode: AutoTldrRoutingMode,
    force_tldr: bool,
) -> Option<TldrToolLanguage> {
    let inferred = infer_language_from_path(search_path)
        .or_else(|| include.and_then(infer_language_from_include));
    if force_tldr || mode.uses_last_tldr_context() {
        inferred.or(auto_tldr_context.last_language)
    } else {
        inferred
    }
}

fn infer_language_from_path(path: &Path) -> Option<TldrToolLanguage> {
    Some(to_tldr_language(SupportedLanguage::from_path(path)?))
}

fn infer_language_from_include(include: &str) -> Option<TldrToolLanguage> {
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
        .find_map(|(needle, language)| include.contains(needle).then_some(*language))
}

#[cfg(test)]
mod tests {
    use super::rewrite_grep_files_to_tldr;
    use crate::codex::make_session_and_context;
    use crate::config::AutoTldrRoutingMode;
    use crate::tools::context::ToolPayload;
    use crate::tools::rewrite::AutoTldrContext;
    use crate::tools::rewrite::ProblemKind;
    use crate::tools::rewrite::ToolRoutingDirectives;
    use crate::tools::rewrite::decision::ToolRewriteDecision;
    use crate::tools::rewrite::resolve_tldr_project_root;
    use crate::tools::rewrite::test_corpus::PROJECT_QUERY_CORPUS;
    use crate::tools::rewrite::test_corpus::PROJECT_REGEX_PATTERN;
    use crate::tools::rewrite::test_corpus::project_route_counts;
    use crate::tools::rewrite::test_corpus::project_signal_counts;
    use crate::tools::rewrite::test_corpus::project_structural_search_reason_counts;
    use crate::tools::rewrite::test_corpus::route_label;
    use crate::tools::rewrite::test_corpus::signal_label;
    use crate::tools::rewrite::test_corpus::structural_search_reason;
    use crate::tools::rewrite::tldr_routing::SearchSignal;
    use crate::tools::router::ToolCall;
    use codex_native_tldr::tool_api::TldrToolAction;
    use codex_native_tldr::tool_api::TldrToolCallParam;
    use codex_native_tldr::tool_api::TldrToolLanguage;
    use pretty_assertions::assert_eq;

    #[tokio::test]
    async fn routes_symbol_searches_to_tldr_context_by_default() {
        let (_, turn) = make_session_and_context().await;
        let expected_project =
            resolve_tldr_project_root(turn.cwd.as_path(), Some(turn.cwd.as_path()))
                .display()
                .to_string();
        let call = ToolCall {
            tool_name: "grep_files".to_string(),
            tool_namespace: None,
            call_id: "call-1".to_string(),
            payload: ToolPayload::Function {
                arguments: r#"{"pattern":"create_tldr_tool","include":"*.rs"}"#.to_string(),
            },
        };

        let decision = rewrite_grep_files_to_tldr(
            &turn,
            call,
            ToolRoutingDirectives::default(),
            AutoTldrRoutingMode::Safe,
        )
        .await;

        assert_eq!(decision.signal(), Some(SearchSignal::BareSymbol));
        let ToolRewriteDecision::Rewrite { call, reason, .. } = decision else {
            panic!("expected rewrite");
        };
        assert_eq!(reason, "structural_symbol_query");
        assert_eq!(call.tool_name, "ztldr");
        let ToolPayload::Function { arguments } = call.payload else {
            panic!("expected function payload");
        };
        let args: TldrToolCallParam =
            serde_json::from_str(&arguments).expect("parse rewritten tldr args");
        assert_eq!(args.action, TldrToolAction::Context);
        assert_eq!(args.language, Some(TldrToolLanguage::Rust));
        assert_eq!(args.project.as_deref(), Some(expected_project.as_str()));
        assert_eq!(args.symbol.as_deref(), Some("create_tldr_tool"));
        assert_eq!(args.query, None);
    }

    #[tokio::test]
    async fn keeps_regex_grep_queries_on_raw_handler_path() {
        let (_, turn) = make_session_and_context().await;
        let call = ToolCall {
            tool_name: "grep_files".to_string(),
            tool_namespace: None,
            call_id: "call-2".to_string(),
            payload: ToolPayload::Function {
                arguments: format!(r#"{{"pattern":"{PROJECT_REGEX_PATTERN}","include":"*.rs"}}"#),
            },
        };

        let decision = rewrite_grep_files_to_tldr(
            &turn,
            call.clone(),
            ToolRoutingDirectives::default(),
            AutoTldrRoutingMode::Safe,
        )
        .await;

        assert_eq!(decision.signal(), None);
        let ToolRewriteDecision::Passthrough {
            call: passthrough,
            reason,
            ..
        } = decision
        else {
            panic!("expected passthrough");
        };
        assert_eq!(reason, "raw_pattern_regex");
        assert_eq!(passthrough.tool_name, call.tool_name);
    }

    #[tokio::test]
    async fn current_project_grep_corpus_summary_stays_stable() {
        use std::collections::BTreeMap;

        let (_, turn) = make_session_and_context().await;
        let corpus = PROJECT_QUERY_CORPUS
            .into_iter()
            .enumerate()
            .map(|(index, case)| {
                let reason = structural_search_reason(case.signal);
                let action = Some(route_label(case.route));
                let signal = Some(signal_label(case.signal));
                (
                    format!("call-corpus-{}", index + 1),
                    format!(
                        r#"{{"pattern":"{pattern}","include":"*.rs"}}"#,
                        pattern = case.pattern
                    ),
                    reason,
                    action,
                    signal,
                )
            })
            .chain([(
                "call-corpus-regex".to_string(),
                format!(r#"{{"pattern":"{PROJECT_REGEX_PATTERN}","include":"*.rs"}}"#),
                "raw_pattern_regex",
                None,
                None,
            )])
            .collect::<Vec<_>>();

        let mut reason_counts = BTreeMap::new();
        let mut action_counts = BTreeMap::new();
        let mut signal_counts = BTreeMap::new();

        for (call_id, arguments, expected_reason, expected_action, expected_signal) in corpus {
            let decision = rewrite_grep_files_to_tldr(
                &turn,
                ToolCall {
                    tool_name: "grep_files".to_string(),
                    tool_namespace: None,
                    call_id: call_id.to_string(),
                    payload: ToolPayload::Function {
                        arguments: arguments.to_string(),
                    },
                },
                ToolRoutingDirectives::default(),
                AutoTldrRoutingMode::Safe,
            )
            .await;

            let action = match decision.action() {
                Some(action) => Some(route_label(match action {
                    TldrToolAction::Context => {
                        crate::tools::rewrite::tldr_routing::SearchRoute::ContextSymbol
                    }
                    TldrToolAction::Semantic => {
                        crate::tools::rewrite::tldr_routing::SearchRoute::SemanticQuery
                    }
                    other => panic!("unexpected action {other:?}"),
                })),
                None => None,
            };
            let signal = decision.signal().map(signal_label);

            assert_eq!(decision.reason(), expected_reason, "call: {call_id}");
            assert_eq!(action, expected_action, "call: {call_id}");
            assert_eq!(signal, expected_signal, "call: {call_id}");

            *reason_counts.entry(expected_reason).or_insert(0usize) += 1;
            *action_counts
                .entry(expected_action.unwrap_or("none"))
                .or_insert(0usize) += 1;
            *signal_counts
                .entry(expected_signal.unwrap_or("none"))
                .or_insert(0usize) += 1;
        }

        let mut expected_reason_counts = project_structural_search_reason_counts();
        expected_reason_counts.insert("raw_pattern_regex", 1usize);
        assert_eq!(reason_counts, expected_reason_counts);
        let mut expected_action_counts = project_route_counts();
        expected_action_counts.insert("none", 1usize);
        assert_eq!(action_counts, expected_action_counts);
        let mut expected_signal_counts = project_signal_counts();
        expected_signal_counts.insert("none", 1usize);
        assert_eq!(signal_counts, expected_signal_counts);
    }

    #[tokio::test]
    async fn safe_mode_does_not_reuse_last_tldr_language_without_explicit_hint() {
        let (_, turn) = make_session_and_context().await;
        *turn.auto_tldr_context.write().await = AutoTldrContext {
            last_language: Some(TldrToolLanguage::Rust),
            ..Default::default()
        };
        let call = ToolCall {
            tool_name: "grep_files".to_string(),
            tool_namespace: None,
            call_id: "call-3".to_string(),
            payload: ToolPayload::Function {
                arguments: r#"{"pattern":"ToolCallRuntime"}"#.to_string(),
            },
        };

        let decision = rewrite_grep_files_to_tldr(
            &turn,
            call.clone(),
            ToolRoutingDirectives::default(),
            AutoTldrRoutingMode::Safe,
        )
        .await;

        assert_eq!(decision.signal(), Some(SearchSignal::BareSymbol));
        let ToolRewriteDecision::Passthrough {
            call: passthrough,
            reason,
            ..
        } = decision
        else {
            panic!("expected passthrough");
        };
        assert_eq!(reason, "unknown_passthrough");
        assert_eq!(passthrough.tool_name, call.tool_name);
    }

    #[tokio::test]
    async fn aggressive_mode_reuses_last_tldr_language_for_default_context_queries() {
        let (_, turn) = make_session_and_context().await;
        let expected_project =
            resolve_tldr_project_root(turn.cwd.as_path(), Some(turn.cwd.as_path()))
                .display()
                .to_string();
        *turn.auto_tldr_context.write().await = AutoTldrContext {
            last_language: Some(TldrToolLanguage::Rust),
            ..Default::default()
        };
        let call = ToolCall {
            tool_name: "grep_files".to_string(),
            tool_namespace: None,
            call_id: "call-3".to_string(),
            payload: ToolPayload::Function {
                arguments: r#"{"pattern":"ToolCallRuntime"}"#.to_string(),
            },
        };

        let decision = rewrite_grep_files_to_tldr(
            &turn,
            call,
            ToolRoutingDirectives::default(),
            AutoTldrRoutingMode::Aggressive,
        )
        .await;

        let ToolRewriteDecision::Rewrite { call, reason, .. } = decision else {
            panic!("expected rewrite");
        };
        assert_eq!(reason, "structural_symbol_query");
        let ToolPayload::Function { arguments } = call.payload else {
            panic!("expected function payload");
        };
        let args: TldrToolCallParam =
            serde_json::from_str(&arguments).expect("parse rewritten tldr args");
        assert_eq!(args.action, TldrToolAction::Context);
        assert_eq!(args.language, Some(TldrToolLanguage::Rust));
        assert_eq!(args.project.as_deref(), Some(expected_project.as_str()));
        assert_eq!(args.symbol.as_deref(), Some("ToolCallRuntime"));
        assert_eq!(args.query, None);
    }

    #[tokio::test]
    async fn force_tldr_reuses_last_language_even_in_safe_mode() {
        let (_, turn) = make_session_and_context().await;
        let expected_project =
            resolve_tldr_project_root(turn.cwd.as_path(), Some(turn.cwd.as_path()))
                .display()
                .to_string();
        *turn.auto_tldr_context.write().await = AutoTldrContext {
            last_language: Some(TldrToolLanguage::Rust),
            ..Default::default()
        };
        let call = ToolCall {
            tool_name: "grep_files".to_string(),
            tool_namespace: None,
            call_id: "call-4".to_string(),
            payload: ToolPayload::Function {
                arguments: r#"{"pattern":"ToolCallRuntime"}"#.to_string(),
            },
        };

        let decision = rewrite_grep_files_to_tldr(
            &turn,
            call,
            ToolRoutingDirectives {
                force_tldr: true,
                ..Default::default()
            },
            AutoTldrRoutingMode::Safe,
        )
        .await;

        let ToolRewriteDecision::Rewrite { call, reason, .. } = decision else {
            panic!("expected rewrite");
        };
        assert_eq!(reason, "structural_symbol_query");
        let ToolPayload::Function { arguments } = call.payload else {
            panic!("expected function payload");
        };
        let args: TldrToolCallParam =
            serde_json::from_str(&arguments).expect("parse rewritten tldr args");
        assert_eq!(args.action, TldrToolAction::Context);
        assert_eq!(args.language, Some(TldrToolLanguage::Rust));
        assert_eq!(args.project.as_deref(), Some(expected_project.as_str()));
        assert_eq!(args.symbol.as_deref(), Some("ToolCallRuntime"));
    }

    #[tokio::test]
    async fn factual_queries_stay_on_raw_grep_path_even_when_code_like() {
        let (_, turn) = make_session_and_context().await;
        let call = ToolCall {
            tool_name: "grep_files".to_string(),
            tool_namespace: None,
            call_id: "call-5".to_string(),
            payload: ToolPayload::Function {
                arguments: r#"{"pattern":"default_timeout","include":"*.rs"}"#.to_string(),
            },
        };

        let decision = rewrite_grep_files_to_tldr(
            &turn,
            call.clone(),
            ToolRoutingDirectives {
                problem_kind: ProblemKind::Factual,
                ..Default::default()
            },
            AutoTldrRoutingMode::Safe,
        )
        .await;

        let ToolRewriteDecision::Passthrough {
            call: passthrough,
            reason,
            ..
        } = decision
        else {
            panic!("expected passthrough");
        };
        assert_eq!(reason, "factual_query");
        assert_eq!(passthrough.tool_name, call.tool_name);
    }

    #[tokio::test]
    async fn mixed_queries_still_use_tldr_first_for_symbol_mapping() {
        let (_, turn) = make_session_and_context().await;
        let expected_project =
            resolve_tldr_project_root(turn.cwd.as_path(), Some(turn.cwd.as_path()))
                .display()
                .to_string();
        let call = ToolCall {
            tool_name: "grep_files".to_string(),
            tool_namespace: None,
            call_id: "call-6".to_string(),
            payload: ToolPayload::Function {
                arguments: r#"{"pattern":"create_tldr_tool","include":"*.rs"}"#.to_string(),
            },
        };

        let decision = rewrite_grep_files_to_tldr(
            &turn,
            call,
            ToolRoutingDirectives {
                problem_kind: ProblemKind::Mixed,
                ..Default::default()
            },
            AutoTldrRoutingMode::Safe,
        )
        .await;

        let ToolRewriteDecision::Rewrite { call, reason, .. } = decision else {
            panic!("expected rewrite");
        };
        assert_eq!(reason, "mixed_symbol_query");
        let ToolPayload::Function { arguments } = call.payload else {
            panic!("expected function payload");
        };
        let args: TldrToolCallParam =
            serde_json::from_str(&arguments).expect("parse rewritten tldr args");
        assert_eq!(args.action, TldrToolAction::Context);
        assert_eq!(args.language, Some(TldrToolLanguage::Rust));
        assert_eq!(args.project.as_deref(), Some(expected_project.as_str()));
        assert_eq!(args.symbol.as_deref(), Some("create_tldr_tool"));
    }
}
