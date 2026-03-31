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
struct ReadFileArgs {
    path: String,
    #[serde(default)]
    offset: Option<usize>,
    #[serde(default)]
    anchor_line: Option<usize>,
}

pub(crate) async fn rewrite_read_file_to_tldr(
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

    if directives.force_raw_read {
        return ToolRewriteDecision::Passthrough {
            call,
            reason: "force_raw_read",
        };
    }

    if matches!(directives.problem_kind, ProblemKind::Factual) && !directives.force_tldr {
        return ToolRewriteDecision::Passthrough {
            call,
            reason: "factual_query",
        };
    }

    let args: ReadFileArgs = match serde_json::from_str(arguments) {
        Ok(args) => args,
        Err(_) => {
            return ToolRewriteDecision::Passthrough {
                call,
                reason: "unknown_passthrough",
            };
        }
    };

    let trimmed_path = args.path.trim();
    if trimmed_path.is_empty() {
        return ToolRewriteDecision::Passthrough {
            call,
            reason: "unknown_passthrough",
        };
    }

    let resolved_path = turn.resolve_path(Some(trimmed_path.to_string()));
    let auto_tldr_context = turn.auto_tldr_context.read().await.clone();
    let Some(language) = infer_language(
        &resolved_path,
        &auto_tldr_context,
        mode,
        directives.force_tldr,
    ) else {
        return ToolRewriteDecision::Passthrough {
            call,
            reason: non_code_reason(&resolved_path),
        };
    };

    let reason = match directives.problem_kind {
        ProblemKind::Structural => "structural_file_extract",
        ProblemKind::Mixed => "mixed_file_extract",
        ProblemKind::Factual => "factual_file_extract",
    };
    let project = resolve_tldr_project_root(turn.cwd.as_path(), Some(resolved_path.as_path()))
        .display()
        .to_string();
    let line = args.anchor_line.or(args.offset);
    let rewritten_args = TldrToolCallParam {
        action: TldrToolAction::Extract,
        project: Some(project),
        language: Some(language),
        symbol: None,
        query: None,
        module: None,
        path: Some(resolved_path.display().to_string()),
        line,
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
            tool_name: "tldr".to_string(),
            tool_namespace: None,
            call_id: call.call_id,
            payload: ToolPayload::Function { arguments },
        },
        reason,
        action: Some(TldrToolAction::Extract),
    }
}

fn infer_language(
    resolved_path: &Path,
    auto_tldr_context: &AutoTldrContext,
    mode: AutoTldrRoutingMode,
    force_tldr: bool,
) -> Option<TldrToolLanguage> {
    let inferred = SupportedLanguage::from_path(resolved_path).map(supported_to_tool_language);
    if force_tldr || mode.uses_last_tldr_context() {
        inferred.or(auto_tldr_context.last_language)
    } else {
        inferred
    }
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

fn non_code_reason(search_path: &Path) -> &'static str {
    if search_path.extension().is_some() {
        "non_code_path"
    } else {
        "unknown_passthrough"
    }
}

#[cfg(test)]
mod tests {
    use super::rewrite_read_file_to_tldr;
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
    async fn structural_reads_route_to_tldr_extract() {
        let (_, turn) = make_session_and_context().await;
        let expected_project =
            resolve_tldr_project_root(turn.cwd.as_path(), Some(turn.cwd.as_path()))
                .display()
                .to_string();
        let call = ToolCall {
            tool_name: "read_file".to_string(),
            tool_namespace: None,
            call_id: "call-read-1".to_string(),
            payload: ToolPayload::Function {
                arguments: r#"{"path":"src/tools/spec.rs","offset":12}"#.to_string(),
            },
        };

        let decision = rewrite_read_file_to_tldr(
            &turn,
            call,
            ToolRoutingDirectives::default(),
            AutoTldrRoutingMode::Safe,
        )
        .await;

        let ToolRewriteDecision::Rewrite { call, reason, .. } = decision else {
            panic!("expected rewrite");
        };
        assert_eq!(reason, "structural_file_extract");
        let ToolPayload::Function { arguments } = call.payload else {
            panic!("expected function payload");
        };
        let args: TldrToolCallParam =
            serde_json::from_str(&arguments).expect("parse rewritten tldr args");
        assert_eq!(args.action, TldrToolAction::Extract);
        assert_eq!(args.language, Some(TldrToolLanguage::Rust));
        assert_eq!(args.project.as_deref(), Some(expected_project.as_str()));
        assert_eq!(args.line, Some(12));
        assert!(
            args.path
                .as_deref()
                .is_some_and(|path| path.ends_with("src/tools/spec.rs")),
            "unexpected path: {:?}",
            args.path
        );
    }

    #[tokio::test]
    async fn factual_reads_stay_on_raw_handler_path() {
        let (_, turn) = make_session_and_context().await;
        let call = ToolCall {
            tool_name: "read_file".to_string(),
            tool_namespace: None,
            call_id: "call-read-2".to_string(),
            payload: ToolPayload::Function {
                arguments: r#"{"path":"Cargo.toml","offset":1}"#.to_string(),
            },
        };

        let decision = rewrite_read_file_to_tldr(
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
    async fn force_tldr_can_reuse_last_language_for_extensionless_reads() {
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
            tool_name: "read_file".to_string(),
            tool_namespace: None,
            call_id: "call-read-3".to_string(),
            payload: ToolPayload::Function {
                arguments: r#"{"path":"Makefile","anchor_line":3}"#.to_string(),
            },
        };

        let decision = rewrite_read_file_to_tldr(
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
        assert_eq!(reason, "structural_file_extract");
        let ToolPayload::Function { arguments } = call.payload else {
            panic!("expected function payload");
        };
        let args: TldrToolCallParam =
            serde_json::from_str(&arguments).expect("parse rewritten tldr args");
        assert_eq!(args.action, TldrToolAction::Extract);
        assert_eq!(args.language, Some(TldrToolLanguage::Rust));
        assert_eq!(args.project.as_deref(), Some(expected_project.as_str()));
        assert_eq!(args.line, Some(3));
    }
}
