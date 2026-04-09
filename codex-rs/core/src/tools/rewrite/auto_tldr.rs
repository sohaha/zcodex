use crate::codex::TurnContext;
use crate::config::AutoTldrRoutingMode;
use crate::tools::context::ToolPayload;
use crate::tools::rewrite::AutoTldrContext;
use crate::tools::rewrite::ProblemKind;
use crate::tools::rewrite::ToolRoutingDirectives;
use crate::tools::rewrite::decision::ToolRewriteDecision;
use crate::tools::rewrite::resolve_tldr_project_root;
use crate::tools::router::ToolCall;
use codex_native_tldr::lang_support::SupportedLanguage;
use codex_native_tldr::tool_api::TldrToolAction;
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
        };
    };

    if directives.disable_auto_tldr_once {
        return ToolRewriteDecision::Passthrough {
            call,
            reason: "explicit_raw_request",
        };
    }

    if directives.force_raw_grep {
        return ToolRewriteDecision::Passthrough {
            call,
            reason: "force_raw_grep",
        };
    }

    if matches!(directives.problem_kind, ProblemKind::Factual) && !directives.force_tldr {
        return ToolRewriteDecision::Passthrough {
            call,
            reason: "factual_query",
        };
    }

    let args: GrepFilesArgs = match serde_json::from_str(arguments) {
        Ok(args) => args,
        Err(_) => {
            return ToolRewriteDecision::Passthrough {
                call,
                reason: "unknown_passthrough",
            };
        }
    };

    let pattern = args.pattern.trim();
    if pattern.is_empty() {
        return ToolRewriteDecision::Passthrough {
            call,
            reason: "unknown_passthrough",
        };
    }

    if looks_like_regex_pattern(pattern) {
        return ToolRewriteDecision::Passthrough {
            call,
            reason: "raw_pattern_regex",
        };
    }

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
        };
    };

    let project_root = resolve_tldr_project_root(turn.cwd.as_path(), Some(search_path.as_path()));
    let project = project_root.display().to_string();

    let (action, reason, symbol, query) =
        if directives.prefer_context_search && looks_like_symbol(pattern) {
            let reason = match directives.problem_kind {
                ProblemKind::Structural => "structural_symbol_query",
                ProblemKind::Mixed => "mixed_symbol_query",
                ProblemKind::Factual => "factual_symbol_query",
            };
            (
                TldrToolAction::Context,
                reason,
                Some(pattern.to_string()),
                None,
            )
        } else {
            let reason = match directives.problem_kind {
                ProblemKind::Structural => "structural_code_search_query",
                ProblemKind::Mixed => "mixed_code_search_query",
                ProblemKind::Factual => "factual_code_search_query",
            };
            (
                TldrToolAction::Semantic,
                reason,
                None,
                Some(pattern.to_string()),
            )
        };

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
    Some(supported_to_tool_language(SupportedLanguage::from_path(
        path,
    )?))
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

fn non_code_reason(search_path: &Path) -> &'static str {
    if search_path.extension().is_some() {
        "non_code_path"
    } else {
        "unknown_passthrough"
    }
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
                arguments: r#"{"pattern":"foo.*bar","include":"*.rs"}"#.to_string(),
            },
        };

        let decision = rewrite_grep_files_to_tldr(
            &turn,
            call.clone(),
            ToolRoutingDirectives::default(),
            AutoTldrRoutingMode::Safe,
        )
        .await;

        let ToolRewriteDecision::Passthrough {
            call: passthrough,
            reason,
        } = decision
        else {
            panic!("expected passthrough");
        };
        assert_eq!(reason, "raw_pattern_regex");
        assert_eq!(passthrough.tool_name, call.tool_name);
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

        let ToolRewriteDecision::Passthrough {
            call: passthrough,
            reason,
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
