use crate::tools::rewrite::classification::ProblemKind;
use crate::tools::rewrite::directives::ToolRoutingDirectives;
use codex_native_tldr::tool_api::TldrToolAction;
use codex_native_tldr::tool_api::TldrToolCallParam;
use codex_native_tldr::tool_api::TldrToolLanguage;
use std::path::PathBuf;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct AutoTldrContext {
    pub(crate) last_project_root: Option<PathBuf>,
    pub(crate) last_language: Option<TldrToolLanguage>,
    pub(crate) last_action: Option<TldrToolAction>,
    pub(crate) last_problem_kind: Option<ProblemKind>,
    pub(crate) last_degraded_mode: Option<String>,
    pub(crate) last_query: Option<String>,
    pub(crate) last_symbol: Option<String>,
    pub(crate) last_paths: Vec<PathBuf>,
    pub(crate) warmup_requested: bool,
}

impl AutoTldrContext {
    pub(crate) fn record_result(
        &mut self,
        args: &TldrToolCallParam,
        problem_kind: ProblemKind,
        degraded_mode: Option<String>,
    ) {
        self.last_project_root = args.project.as_ref().map(PathBuf::from);
        self.last_language = args.language;
        self.last_action = Some(args.action.clone());
        self.last_problem_kind = Some(problem_kind);
        self.last_degraded_mode = degraded_mode;
        self.last_query = args.query.clone();
        self.last_symbol = args.symbol.clone();

        let mut paths = Vec::new();
        if let Some(path) = args.path.as_ref() {
            paths.push(PathBuf::from(path));
        }
        if let Some(extra_paths) = args.paths.as_ref() {
            paths.extend(extra_paths.iter().map(PathBuf::from));
        }
        self.last_paths = paths;
    }

    pub(crate) fn note_warmup_requested(&mut self) {
        self.warmup_requested = true;
    }
}

pub(crate) fn build_subagent_tldr_guidance(
    directives: &ToolRoutingDirectives,
    context: &AutoTldrContext,
) -> Option<String> {
    if matches!(directives.problem_kind, ProblemKind::Factual) {
        return None;
    }

    let recommended_action = context
        .last_action
        .clone()
        .unwrap_or_else(|| recommended_action_for_problem_kind(directives.problem_kind));
    let problem_kind = problem_kind_label(directives.problem_kind);
    let action = action_label(&recommended_action);
    let project = context
        .last_project_root
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "<current project>".to_string());
    let language = context
        .last_language
        .map(language_label)
        .unwrap_or("unknown".to_string());
    let degraded_mode = context
        .last_degraded_mode
        .as_deref()
        .unwrap_or("none")
        .to_string();

    let mut lines = vec![
        "## TLDR-First Subagent Context".to_string(),
        format!(
            "- Parent task is `{problem_kind}`; start from `tldr` with `{action}` before broad grep/read unless the user explicitly asked for raw tools."
        ),
        format!("- Carry over recent project context: project={project}, language={language}."),
        format!("- Latest degraded mode: {degraded_mode}."),
    ];

    if let Some(symbol) = context.last_symbol.as_deref() {
        lines.push(format!(
            "- Reuse recent symbol hint when relevant: `{symbol}`."
        ));
    } else if let Some(query) = context.last_query.as_deref() {
        lines.push(format!(
            "- Reuse recent query hint when relevant: `{query}`."
        ));
    }

    Some(lines.join("\n"))
}

pub(crate) fn recommended_action_for_problem_kind(problem_kind: ProblemKind) -> TldrToolAction {
    match problem_kind {
        ProblemKind::Structural | ProblemKind::Mixed => TldrToolAction::Context,
        ProblemKind::Factual => TldrToolAction::Extract,
    }
}

pub(crate) fn should_auto_warm_tldr(
    problem_kind: ProblemKind,
    action: &TldrToolAction,
    context: &AutoTldrContext,
) -> bool {
    if context.warmup_requested {
        return false;
    }

    if !matches!(problem_kind, ProblemKind::Structural | ProblemKind::Mixed) {
        return false;
    }

    matches!(
        action,
        TldrToolAction::Context
            | TldrToolAction::Impact
            | TldrToolAction::Calls
            | TldrToolAction::Semantic
            | TldrToolAction::Extract
            | TldrToolAction::Search
            | TldrToolAction::Structure
            | TldrToolAction::Diagnostics
    )
}

fn action_label(action: &TldrToolAction) -> &'static str {
    match action {
        TldrToolAction::Structure => "structure",
        TldrToolAction::Search => "search",
        TldrToolAction::Extract => "extract",
        TldrToolAction::Imports => "imports",
        TldrToolAction::Importers => "importers",
        TldrToolAction::Context => "context",
        TldrToolAction::Impact => "impact",
        TldrToolAction::Calls => "calls",
        TldrToolAction::Dead => "dead",
        TldrToolAction::Arch => "arch",
        TldrToolAction::ChangeImpact => "change-impact",
        TldrToolAction::Cfg => "cfg",
        TldrToolAction::Dfg => "dfg",
        TldrToolAction::Slice => "slice",
        TldrToolAction::Semantic => "semantic",
        TldrToolAction::Diagnostics => "diagnostics",
        TldrToolAction::Doctor => "doctor",
        TldrToolAction::Ping => "ping",
        TldrToolAction::Warm => "warm",
        TldrToolAction::Snapshot => "snapshot",
        TldrToolAction::Status => "status",
        TldrToolAction::Notify => "notify",
    }
}

fn problem_kind_label(problem_kind: ProblemKind) -> &'static str {
    match problem_kind {
        ProblemKind::Structural => "structural",
        ProblemKind::Factual => "factual",
        ProblemKind::Mixed => "mixed",
    }
}

fn language_label(language: TldrToolLanguage) -> String {
    match language {
        TldrToolLanguage::C => "c",
        TldrToolLanguage::Cpp => "cpp",
        TldrToolLanguage::Csharp => "csharp",
        TldrToolLanguage::Elixir => "elixir",
        TldrToolLanguage::Go => "go",
        TldrToolLanguage::Java => "java",
        TldrToolLanguage::Javascript => "javascript",
        TldrToolLanguage::Lua => "lua",
        TldrToolLanguage::Luau => "luau",
        TldrToolLanguage::Php => "php",
        TldrToolLanguage::Python => "python",
        TldrToolLanguage::Ruby => "ruby",
        TldrToolLanguage::Rust => "rust",
        TldrToolLanguage::Scala => "scala",
        TldrToolLanguage::Swift => "swift",
        TldrToolLanguage::Typescript => "typescript",
        TldrToolLanguage::Zig => "zig",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::AutoTldrContext;
    use super::build_subagent_tldr_guidance;
    use super::recommended_action_for_problem_kind;
    use super::should_auto_warm_tldr;
    use crate::tools::rewrite::ProblemKind;
    use crate::tools::rewrite::ToolRoutingDirectives;
    use codex_native_tldr::tool_api::TldrToolAction;
    use codex_native_tldr::tool_api::TldrToolCallParam;
    use codex_native_tldr::tool_api::TldrToolLanguage;
    use pretty_assertions::assert_eq;
    use std::path::PathBuf;

    #[test]
    fn record_result_captures_action_problem_kind_and_degraded_mode() {
        let mut context = AutoTldrContext::default();
        context.record_result(
            &TldrToolCallParam {
                action: TldrToolAction::Context,
                project: Some("/tmp/project".to_string()),
                language: Some(TldrToolLanguage::Rust),
                symbol: Some("AuthService".to_string()),
                query: None,
                module: None,
                path: Some("src/lib.rs".to_string()),
                line: None,
                paths: None,
                only_tools: None,
                run_lint: None,
                run_typecheck: None,
                max_issues: None,
                include_install_hints: None,
            },
            ProblemKind::Structural,
            Some("diagnostic_only".to_string()),
        );

        assert_eq!(context.last_action, Some(TldrToolAction::Context));
        assert_eq!(context.last_problem_kind, Some(ProblemKind::Structural));
        assert_eq!(
            context.last_degraded_mode.as_deref(),
            Some("diagnostic_only")
        );
        assert_eq!(context.last_paths, vec![PathBuf::from("src/lib.rs")]);
    }

    #[test]
    fn subagent_guidance_includes_recent_context_and_degraded_mode() {
        let guidance = build_subagent_tldr_guidance(
            &ToolRoutingDirectives::default(),
            &AutoTldrContext {
                last_project_root: Some(PathBuf::from("/tmp/project")),
                last_language: Some(TldrToolLanguage::Rust),
                last_action: Some(TldrToolAction::Context),
                last_problem_kind: Some(ProblemKind::Structural),
                last_degraded_mode: Some("diagnostic_only".to_string()),
                last_query: None,
                last_symbol: Some("create_tldr_tool".to_string()),
                last_paths: Vec::new(),
                warmup_requested: false,
            },
        )
        .expect("structural guidance should exist");

        assert!(guidance.contains("## TLDR-First Subagent Context"));
        assert!(guidance.contains("`tldr` with `context`"));
        assert!(guidance.contains("diagnostic_only"));
        assert!(guidance.contains("create_tldr_tool"));
    }

    #[test]
    fn factual_subagents_do_not_receive_tldr_first_guidance() {
        let guidance = build_subagent_tldr_guidance(
            &ToolRoutingDirectives {
                problem_kind: ProblemKind::Factual,
                ..Default::default()
            },
            &AutoTldrContext::default(),
        );

        assert_eq!(guidance, None);
    }

    #[test]
    fn first_structural_queries_trigger_warm_once() {
        let mut context = AutoTldrContext::default();
        assert_eq!(
            should_auto_warm_tldr(ProblemKind::Structural, &TldrToolAction::Context, &context),
            true
        );
        context.note_warmup_requested();
        assert_eq!(
            should_auto_warm_tldr(ProblemKind::Structural, &TldrToolAction::Context, &context),
            false
        );
        assert_eq!(
            should_auto_warm_tldr(ProblemKind::Factual, &TldrToolAction::Context, &context),
            false
        );
    }

    #[test]
    fn mixed_problem_kind_recommends_context_action() {
        assert_eq!(
            recommended_action_for_problem_kind(ProblemKind::Mixed),
            TldrToolAction::Context
        );
    }
}
