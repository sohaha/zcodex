#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used))]

pub mod analysis;
pub mod api;
pub mod config;
pub mod daemon;
mod diagnostics;
mod import_analysis;
pub mod lang_support;
pub mod lifecycle;
pub mod mcp;
mod project_analysis;
mod rust_analysis;
mod search;
pub mod semantic;
mod semantic_cache;
pub mod session;
pub mod tool_api;
pub mod wire;

use crate::analysis::analyze_project;
use crate::analysis::analyze_project_with_index;
use crate::api::AnalysisKind;
use crate::api::AnalysisRequest;
use crate::api::AnalysisResponse;
use crate::api::DiagnosticsRequest;
use crate::api::DiagnosticsResponse;
use crate::api::DoctorRequest;
use crate::api::DoctorResponse;
use crate::api::ImportersRequest;
use crate::api::ImportersResponse;
use crate::api::ImportsRequest;
use crate::api::ImportsResponse;
use crate::api::SearchRequest;
use crate::api::SearchResponse;
pub use crate::config::load_tldr_config;
use crate::daemon::DaemonConfig;
use crate::daemon::TldrDaemonConfigSummary;
use crate::diagnostics::collect_diagnostics;
use crate::diagnostics::doctor_tools;
use crate::import_analysis::collect_importers;
use crate::import_analysis::collect_imports;
use crate::lang_support::LanguageRegistry;
use crate::lang_support::SupportedLanguage;
use crate::mcp::TldrToolDescriptor;
use crate::search::search_project;
use crate::semantic::SemanticConfig;
use crate::semantic::SemanticIndex;
use crate::semantic::SemanticIndexer;
use crate::semantic::SemanticReindexReport;
use crate::semantic::SemanticSearchRequest;
use crate::semantic::SemanticSearchResponse;
use crate::session::SessionConfig;
use anyhow::Result;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::PoisonError;
use std::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TldrConfig {
    pub project_root: PathBuf,
    pub session: SessionConfig,
    pub daemon: DaemonConfig,
    pub semantic: SemanticConfig,
}

impl TldrConfig {
    pub fn for_project(project_root: PathBuf) -> Self {
        Self {
            project_root,
            session: SessionConfig::default(),
            daemon: DaemonConfig::default(),
            semantic: SemanticConfig::default(),
        }
    }

    pub fn daemon_config_summary(&self) -> TldrDaemonConfigSummary {
        TldrDaemonConfigSummary {
            auto_start: self.daemon.auto_start,
            socket_mode: self.daemon.socket_mode.clone(),
            semantic_enabled: self.semantic.enabled,
            semantic_auto_reindex_threshold: self.semantic.auto_reindex_threshold,
            session_dirty_file_threshold: self.session.dirty_file_threshold,
            session_idle_timeout_secs: self.session.idle_timeout.as_secs(),
        }
    }
}

#[derive(Debug)]
pub struct TldrEngine {
    config: TldrConfig,
    registry: LanguageRegistry,
    semantic_indexes: Arc<RwLock<BTreeMap<SupportedLanguage, SemanticIndex>>>,
}

impl Clone for TldrEngine {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            registry: LanguageRegistry,
            semantic_indexes: Arc::clone(&self.semantic_indexes),
        }
    }
}

impl TldrEngine {
    pub fn builder(project_root: PathBuf) -> TldrEngineBuilder {
        TldrEngineBuilder::new(project_root)
    }

    pub fn config(&self) -> &TldrConfig {
        &self.config
    }

    pub fn registry(&self) -> &LanguageRegistry {
        &self.registry
    }

    pub fn tool_descriptor(&self) -> TldrToolDescriptor {
        TldrToolDescriptor::default()
    }

    pub fn semantic_indexer(&self) -> SemanticIndexer {
        SemanticIndexer::new(self.config.semantic.clone())
    }

    fn cached_semantic_index(&self, language: SupportedLanguage) -> Option<SemanticIndex> {
        let guard = self
            .semantic_indexes
            .read()
            .unwrap_or_else(PoisonError::into_inner);
        guard.get(&language).cloned()
    }

    fn current_cached_semantic_index(
        &self,
        language: SupportedLanguage,
        indexer: &SemanticIndexer,
    ) -> Result<Option<SemanticIndex>> {
        let Some(index) = self.cached_semantic_index(language) else {
            return Ok(None);
        };
        let current_fingerprint =
            indexer.current_source_fingerprint(&self.config.project_root, language)?;
        if index.source_fingerprint == current_fingerprint {
            return Ok(Some(index));
        }
        Ok(None)
    }

    pub(crate) fn load_or_build_semantic_index(
        &self,
        language: SupportedLanguage,
    ) -> Result<SemanticIndex> {
        let indexer = self.semantic_indexer();
        if let Some(index) = self.current_cached_semantic_index(language, &indexer)? {
            return Ok(index);
        }

        let index = indexer.load_or_build_index(&self.config.project_root, language)?;
        self.semantic_indexes
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(language, index.clone());
        Ok(index)
    }

    fn load_or_build_analysis_index(&self, language: SupportedLanguage) -> Result<SemanticIndex> {
        let indexer = self.semantic_indexer();
        if let Some(index) = self.current_cached_semantic_index(language, &indexer)? {
            return Ok(index);
        }

        let mut semantic_config = self.config.semantic.clone();
        semantic_config.embedding.enabled = false;
        semantic_config.embedding_enabled = false;
        SemanticIndexer::new(semantic_config)
            .load_or_build_index(&self.config.project_root, language)
    }

    pub fn project_languages(&self) -> Result<Vec<SupportedLanguage>> {
        self.semantic_indexer()
            .project_languages(&self.config.project_root)
    }

    pub fn warm_language_indexes(
        &self,
        languages: &[SupportedLanguage],
    ) -> Result<Vec<SupportedLanguage>> {
        let mut warmed = Vec::new();
        for language in languages {
            let index = self.load_or_build_semantic_index(*language)?;
            if index.indexed_files > 0 {
                warmed.push(index.language);
            }
        }
        Ok(warmed)
    }

    pub fn semantic_search(
        &self,
        request: SemanticSearchRequest,
    ) -> Result<SemanticSearchResponse> {
        let indexer = self.semantic_indexer();
        if !indexer.is_enabled() {
            return indexer.search(&self.config.project_root, request);
        }

        let language = request.language;
        let index = self.load_or_build_semantic_index(language)?;

        indexer.search_index(&index, request.query)
    }

    pub fn semantic_reindex(&self) -> Result<SemanticReindexReport> {
        let registry = LanguageRegistry;
        self.semantic_reindex_languages(&registry.supported_languages())
    }

    pub fn semantic_reindex_languages(
        &self,
        languages: &[SupportedLanguage],
    ) -> Result<SemanticReindexReport> {
        let (indexes, report) = self
            .semantic_indexer()
            .reindex_languages(&self.config.project_root, languages)?;
        let mut cache = self
            .semantic_indexes
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        cache.clear();
        for index in indexes {
            cache.insert(index.language, index);
        }
        Ok(report)
    }

    pub fn analyze(&self, request: AnalysisRequest) -> Result<AnalysisResponse> {
        if matches!(
            request.kind,
            AnalysisKind::Impact | AnalysisKind::Calls | AnalysisKind::Dead | AnalysisKind::Arch
        ) {
            return analyze_project(&self.config.project_root, &self.config, request);
        }
        let index = self.load_or_build_analysis_index(request.language)?;
        analyze_project_with_index(&self.config.project_root, request, index)
    }

    pub fn imports(&self, request: ImportsRequest) -> Result<ImportsResponse> {
        collect_imports(&self.config.project_root, &self.config, request)
    }

    pub fn importers(&self, request: ImportersRequest) -> Result<ImportersResponse> {
        collect_importers(&self.config.project_root, &self.config, request)
    }

    pub fn search(&self, request: SearchRequest) -> Result<SearchResponse> {
        search_project(&self.config.project_root, request)
    }

    pub fn diagnostics(&self, request: DiagnosticsRequest) -> Result<DiagnosticsResponse> {
        collect_diagnostics(&self.config.project_root, request)
    }

    pub fn doctor(&self, request: DoctorRequest) -> DoctorResponse {
        doctor_tools(request)
    }
}

#[derive(Debug)]
pub struct TldrEngineBuilder {
    config: TldrConfig,
}

impl TldrEngineBuilder {
    pub fn new(project_root: PathBuf) -> Self {
        Self {
            config: TldrConfig::for_project(project_root),
        }
    }

    pub fn with_semantic(mut self, semantic: SemanticConfig) -> Self {
        self.config.semantic = semantic;
        self
    }

    pub fn with_daemon(mut self, daemon: DaemonConfig) -> Self {
        self.config.daemon = daemon;
        self
    }

    pub fn with_session(mut self, session: SessionConfig) -> Self {
        self.config.session = session;
        self
    }

    pub fn with_config(mut self, config: TldrConfig) -> Self {
        self.config = config;
        self
    }

    pub fn build(self) -> TldrEngine {
        TldrEngine {
            config: self.config,
            registry: LanguageRegistry,
            semantic_indexes: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::TldrEngine;
    use crate::api::AnalysisKind;
    use crate::api::AnalysisRequest;
    use crate::daemon::TldrDaemon;
    use crate::daemon::TldrDaemonCommand;
    use crate::lang_support::SupportedLanguage;
    use crate::semantic::SemanticConfig;
    use crate::semantic::SemanticEmbeddingConfig;
    use crate::semantic::SemanticSearchRequest;
    use pretty_assertions::assert_eq;
    use serial_test::serial;
    use std::path::PathBuf;
    use tempfile::tempdir;

    #[test]
    fn engine_builder_uses_expected_defaults() {
        let project_root = PathBuf::from("/tmp/project");
        let engine = TldrEngine::builder(project_root.clone()).build();

        assert_eq!(engine.config().project_root, project_root);
        assert_eq!(engine.config().daemon.auto_start, true);
        assert_eq!(engine.config().semantic.enabled, true);
        assert_eq!(engine.config().semantic.embedding_enabled, true);
        assert_eq!(
            engine.registry().supported_languages(),
            vec![
                SupportedLanguage::C,
                SupportedLanguage::Cpp,
                SupportedLanguage::CSharp,
                SupportedLanguage::Elixir,
                SupportedLanguage::Go,
                SupportedLanguage::Java,
                SupportedLanguage::JavaScript,
                SupportedLanguage::Lua,
                SupportedLanguage::Luau,
                SupportedLanguage::Php,
                SupportedLanguage::Python,
                SupportedLanguage::Ruby,
                SupportedLanguage::Rust,
                SupportedLanguage::Scala,
                SupportedLanguage::Swift,
                SupportedLanguage::TypeScript,
                SupportedLanguage::Zig,
            ],
        );
    }

    #[test]
    fn analyze_returns_placeholder_summary() {
        let tempdir = tempdir().expect("tempdir should exist");
        std::fs::create_dir_all(tempdir.path().join("src")).expect("src dir should exist");
        std::fs::write(tempdir.path().join("src/lib.rs"), "fn main() {}\n")
            .expect("fixture should be written");
        let engine = TldrEngine::builder(tempdir.path().to_path_buf()).build();
        let response = engine
            .analyze(AnalysisRequest {
                kind: AnalysisKind::Ast,
                language: SupportedLanguage::Rust,
                symbol: Some("main".to_string()),
                path: None,
                paths: Vec::new(),

                line: None,
            })
            .expect("analysis should succeed");

        assert_eq!(response.kind, AnalysisKind::Ast);
        assert!(response.summary.contains("structure summary:"));
        assert!(response.summary.contains("main"));
    }

    #[test]
    fn registry_initializes_all_language_parsers() {
        let engine = TldrEngine::builder(PathBuf::from("/tmp/project")).build();

        for language in engine.registry().supported_languages() {
            let mut parser = engine
                .registry()
                .parser_for(language)
                .expect("parser should initialize");
            let tree = parser
                .parse(engine.registry().sample_for(language), None)
                .expect("sample code should parse");
            assert_eq!(tree.root_node().has_error(), false);
        }
    }

    #[test]
    fn semantic_indexer_matches_engine_config() {
        let mut builder = TldrEngine::builder(PathBuf::from("/tmp/project"));
        builder = builder.with_semantic(SemanticConfig::default().with_enabled(true));
        let engine = builder.build();

        let indexer = engine.semantic_indexer();
        assert!(indexer.is_enabled());
        assert!(indexer.describe().contains("enabled"));
    }

    #[test]
    fn warm_language_indexes_loads_detected_project_languages() {
        let tempdir = tempdir().expect("tempdir should exist");
        std::fs::create_dir_all(tempdir.path().join("src")).expect("src dir should exist");
        std::fs::write(tempdir.path().join("src/lib.rs"), "fn login() {}\n")
            .expect("fixture should be written");
        let engine = TldrEngine::builder(tempdir.path().to_path_buf())
            .with_semantic(SemanticConfig::default().with_enabled(true))
            .build();

        let languages = engine
            .project_languages()
            .expect("project languages should be detected");
        let warmed = engine
            .warm_language_indexes(&languages)
            .expect("warm should succeed");

        assert_eq!(languages, vec![SupportedLanguage::Rust]);
        assert_eq!(warmed, vec![SupportedLanguage::Rust]);
        assert_eq!(
            engine
                .semantic_indexes
                .read()
                .expect("semantic index cache lock should not be poisoned")
                .len(),
            1
        );
    }

    #[test]
    #[serial]
    fn semantic_search_refreshes_cached_index_when_sources_change() {
        let tempdir = tempdir().expect("tempdir should exist");
        std::fs::create_dir_all(tempdir.path().join("src")).expect("src dir should exist");
        std::fs::write(
            tempdir.path().join("src/lib.rs"),
            "fn login() {\n    validate(user);\n}\n",
        )
        .expect("fixture should be written");
        let engine = TldrEngine::builder(tempdir.path().to_path_buf())
            .with_semantic(SemanticConfig::default().with_enabled(true))
            .build();

        let first = engine
            .semantic_search(SemanticSearchRequest {
                language: SupportedLanguage::Rust,
                query: "login".to_string(),
            })
            .expect("first search should succeed");
        assert_eq!(first.matches[0].unit.symbol.as_deref(), Some("login"));

        std::fs::write(
            tempdir.path().join("src/lib.rs"),
            "fn logout() {\n    audit(user);\n}\n",
        )
        .expect("updated fixture should be written");
        let refreshed = engine
            .semantic_search(SemanticSearchRequest {
                language: SupportedLanguage::Rust,
                query: "logout".to_string(),
            })
            .expect("refreshed search should succeed");
        assert_eq!(refreshed.matches[0].unit.symbol.as_deref(), Some("logout"));

        let cached_languages = engine
            .semantic_indexes
            .read()
            .expect("semantic index cache lock should not be poisoned")
            .len();
        assert_eq!(cached_languages, 1);

        let report = engine.semantic_reindex().expect("reindex should succeed");
        assert!(report.is_completed());
    }

    #[tokio::test]
    async fn daemon_caches_analysis_results() {
        let tempdir = tempdir().expect("tempdir should exist");
        std::fs::create_dir_all(tempdir.path().join("src")).expect("src dir should exist");
        std::fs::write(tempdir.path().join("src/main.rs"), "fn main() {}\n")
            .expect("fixture should be written");
        let daemon = TldrDaemon::new(tempdir.path().to_path_buf());
        let first = daemon
            .handle_command(TldrDaemonCommand::Analyze {
                key: "rust:main".to_string(),
                request: AnalysisRequest {
                    kind: AnalysisKind::Ast,
                    language: SupportedLanguage::Rust,
                    symbol: Some("main".to_string()),
                    path: None,
                    paths: Vec::new(),

                    line: None,
                },
            })
            .await
            .expect("first analyze should succeed");
        let second = daemon
            .handle_command(TldrDaemonCommand::Analyze {
                key: "rust:main".to_string(),
                request: AnalysisRequest {
                    kind: AnalysisKind::Ast,
                    language: SupportedLanguage::Rust,
                    symbol: Some("main".to_string()),
                    path: None,
                    paths: Vec::new(),

                    line: None,
                },
            })
            .await
            .expect("second analyze should succeed");

        assert_eq!(first.message, "computed");
        assert_eq!(second.message, "cache hit");
    }

    #[tokio::test]
    async fn daemon_marks_dirty_files() {
        let daemon = TldrDaemon::new(PathBuf::from("/tmp/project"));
        let response = daemon
            .handle_command(TldrDaemonCommand::Notify {
                path: PathBuf::from("src/main.rs"),
            })
            .await
            .expect("notify should succeed");

        assert_eq!(response.status, "ok");
        assert_eq!(response.snapshot.expect("snapshot present").dirty_files, 1);
    }

    #[tokio::test]
    async fn daemon_status_reports_config_and_reindex_state() {
        let mut config = crate::TldrConfig::for_project(PathBuf::from("/tmp/project"));
        config.semantic = SemanticConfig {
            enabled: true,
            feature_gate: "semantic-embed".to_string(),
            model: "bge-large-en-v1.5".to_string(),
            auto_reindex_threshold: 2,
            embedding_enabled: true,
            embedding: SemanticEmbeddingConfig::default(),
            ignore: Vec::new(),
        };
        config.session = crate::session::SessionConfig {
            idle_timeout: std::time::Duration::from_secs(60),
            dirty_file_threshold: 1,
        };
        let daemon = TldrDaemon::from_config(config);

        daemon
            .handle_command(TldrDaemonCommand::Notify {
                path: PathBuf::from("src/main.rs"),
            })
            .await
            .expect("notify should succeed");
        let response = daemon
            .handle_command(TldrDaemonCommand::Status)
            .await
            .expect("status should succeed");

        let snapshot = response.snapshot.expect("snapshot should exist");
        let daemon_status = response.daemon_status.expect("daemon status should exist");
        assert!(snapshot.reindex_pending);
        assert!(!snapshot.background_reindex_in_progress);
        assert!(daemon_status.semantic_reindex_pending);
        assert!(!daemon_status.semantic_reindex_in_progress);
        assert!(daemon_status.config.semantic_enabled);
        assert_eq!(daemon_status.config.semantic_auto_reindex_threshold, 2);
        assert_eq!(daemon_status.config.session_dirty_file_threshold, 1);
        assert_eq!(daemon_status.config.session_idle_timeout_secs, 60);
    }
}
