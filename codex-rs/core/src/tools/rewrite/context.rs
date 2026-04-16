use crate::tools::rewrite::classification::ProblemKind;
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

#[cfg(test)]
mod tests {
    use super::AutoTldrContext;
    use super::should_auto_warm_tldr;
    use crate::tools::rewrite::ProblemKind;
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
                match_mode: None,
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
}
