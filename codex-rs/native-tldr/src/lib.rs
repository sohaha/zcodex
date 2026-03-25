#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used))]

pub mod api;
pub mod config;
pub mod daemon;
pub mod lang_support;
pub mod lifecycle;
pub mod mcp;
pub mod semantic;
pub mod session;

use crate::api::AnalysisRequest;
use crate::api::AnalysisResponse;
pub use crate::config::load_tldr_config;
use crate::daemon::DaemonConfig;
use crate::daemon::TldrDaemonConfigSummary;
use crate::lang_support::LanguageRegistry;
use crate::lang_support::SupportedLanguage;
use crate::mcp::TldrToolDescriptor;
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

    pub fn semantic_search(
        &self,
        request: SemanticSearchRequest,
    ) -> Result<SemanticSearchResponse> {
        let indexer = self.semantic_indexer();
        if !indexer.is_enabled() {
            return indexer.search(&self.config.project_root, request);
        }

        let language = request.language;
        let cached = self
            .semantic_indexes
            .read()
            .expect("semantic index cache lock should not be poisoned")
            .get(&language)
            .cloned();
        let index = if let Some(index) = cached {
            index
        } else {
            let index = indexer.build_index(&self.config.project_root, language)?;
            self.semantic_indexes
                .write()
                .expect("semantic index cache lock should not be poisoned")
                .insert(language, index.clone());
            index
        };

        Ok(indexer.search_index(&index, request.query))
    }

    pub fn semantic_reindex(&self) -> Result<SemanticReindexReport> {
        let (indexes, report) = self
            .semantic_indexer()
            .reindex_all(&self.config.project_root)?;
        let mut cache = self
            .semantic_indexes
            .write()
            .expect("semantic index cache lock should not be poisoned");
        cache.clear();
        for index in indexes {
            cache.insert(index.language, index);
        }
        Ok(report)
    }

    pub fn analyze(&self, request: AnalysisRequest) -> Result<AnalysisResponse> {
        Ok(AnalysisResponse::placeholder(request.kind))
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
    use std::path::PathBuf;
    use tempfile::tempdir;

    #[test]
    fn engine_builder_uses_expected_defaults() {
        let project_root = PathBuf::from("/tmp/project");
        let engine = TldrEngine::builder(project_root.clone()).build();

        assert_eq!(engine.config().project_root, project_root);
        assert_eq!(engine.config().daemon.auto_start, true);
        assert_eq!(engine.config().semantic.enabled, false);
        assert_eq!(
            engine.registry().supported_languages(),
            vec![
                SupportedLanguage::Go,
                SupportedLanguage::JavaScript,
                SupportedLanguage::Php,
                SupportedLanguage::Python,
                SupportedLanguage::Rust,
                SupportedLanguage::TypeScript,
                SupportedLanguage::Zig,
            ],
        );
    }

    #[test]
    fn analyze_returns_placeholder_summary() {
        let engine = TldrEngine::builder(PathBuf::from("/tmp/project")).build();
        let response = engine
            .analyze(AnalysisRequest {
                kind: AnalysisKind::Ast,
                symbol: Some("main".to_string()),
            })
            .expect("placeholder analysis should succeed");

        assert_eq!(response.kind, AnalysisKind::Ast);
        assert_eq!(response.summary, "Ast analysis is not implemented yet");
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
    fn semantic_search_reuses_cached_index_until_reindex() {
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
        let cached = engine
            .semantic_search(SemanticSearchRequest {
                language: SupportedLanguage::Rust,
                query: "login".to_string(),
            })
            .expect("cached search should succeed");
        assert_eq!(cached.matches[0].unit.symbol.as_deref(), Some("login"));

        let report = engine.semantic_reindex().expect("reindex should succeed");
        assert!(report.is_completed());
        let refreshed = engine
            .semantic_search(SemanticSearchRequest {
                language: SupportedLanguage::Rust,
                query: "logout".to_string(),
            })
            .expect("refreshed search should succeed");
        assert_eq!(refreshed.matches[0].unit.symbol.as_deref(), Some("logout"));
    }

    #[tokio::test]
    async fn daemon_caches_analysis_results() {
        let daemon = TldrDaemon::new(PathBuf::from("/tmp/project"));
        let first = daemon
            .handle_command(TldrDaemonCommand::Analyze {
                key: "rust:main".to_string(),
                request: AnalysisRequest {
                    kind: AnalysisKind::Ast,
                    symbol: Some("main".to_string()),
                },
            })
            .await
            .expect("first analyze should succeed");
        let second = daemon
            .handle_command(TldrDaemonCommand::Analyze {
                key: "rust:main".to_string(),
                request: AnalysisRequest {
                    kind: AnalysisKind::Ast,
                    symbol: Some("main".to_string()),
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
            model: "minilm".to_string(),
            auto_reindex_threshold: 1,
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
        assert!(daemon_status.semantic_reindex_pending);
        assert!(daemon_status.config.semantic_enabled);
        assert_eq!(daemon_status.config.semantic_auto_reindex_threshold, 1);
        assert_eq!(daemon_status.config.session_dirty_file_threshold, 1);
    }
}
