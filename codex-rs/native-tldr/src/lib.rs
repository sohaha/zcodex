#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used))]

pub mod api;
pub mod daemon;
pub mod lang_support;
pub mod mcp;
pub mod semantic;
pub mod session;

use crate::api::AnalysisRequest;
use crate::api::AnalysisResponse;
use crate::daemon::DaemonConfig;
use crate::lang_support::LanguageRegistry;
use crate::mcp::TldrToolDescriptor;
use crate::semantic::SemanticConfig;
use crate::session::SessionConfig;
use anyhow::Result;
use serde::Deserialize;
use serde::Serialize;
use std::path::PathBuf;

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
}

#[derive(Debug)]
pub struct TldrEngine {
    config: TldrConfig,
    registry: LanguageRegistry,
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

    pub fn build(self) -> TldrEngine {
        TldrEngine {
            config: self.config,
            registry: LanguageRegistry,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::TldrEngine;
    use crate::api::AnalysisKind;
    use crate::api::AnalysisRequest;
    use crate::lang_support::SupportedLanguage;
    use pretty_assertions::assert_eq;
    use std::path::PathBuf;

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
}
