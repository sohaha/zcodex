use codex_native_tldr::tool_api::TldrToolCallParam;
use codex_native_tldr::tool_api::TldrToolLanguage;
use std::path::PathBuf;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct AutoTldrContext {
    pub(crate) last_project_root: Option<PathBuf>,
    pub(crate) last_language: Option<TldrToolLanguage>,
    pub(crate) last_query: Option<String>,
    pub(crate) last_symbol: Option<String>,
    pub(crate) last_paths: Vec<PathBuf>,
}

impl AutoTldrContext {
    pub(crate) fn record_success(&mut self, args: &TldrToolCallParam) {
        self.last_project_root = args.project.as_ref().map(PathBuf::from);
        self.last_language = args.language;
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
}
