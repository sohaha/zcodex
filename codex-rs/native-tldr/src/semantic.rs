use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemanticConfig {
    pub enabled: bool,
    pub feature_gate: String,
    pub model: String,
    pub auto_reindex_threshold: usize,
}

impl Default for SemanticConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            feature_gate: "semantic-embed".to_string(),
            model: "minilm".to_string(),
            auto_reindex_threshold: 20,
        }
    }
}

impl SemanticConfig {
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }
}

/// Stub semantic indexer that uses the config for gating and thresholds.
#[derive(Debug, Clone)]
pub struct SemanticIndexer {
    config: SemanticConfig,
}

impl SemanticIndexer {
    pub fn new(config: SemanticConfig) -> Self {
        Self { config }
    }

    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    pub fn auto_reindex_threshold(&self) -> usize {
        self.config.auto_reindex_threshold
    }

    pub fn should_reindex(&self, dirty_files: usize) -> bool {
        dirty_files >= self.config.auto_reindex_threshold
    }

    pub fn describe(&self) -> String {
        format!(
            "semantic {} threshold={}, feature_gate={}",
            if self.is_enabled() {
                "enabled"
            } else {
                "disabled"
            },
            self.config.auto_reindex_threshold,
            self.config.feature_gate
        )
    }
}

#[cfg(test)]
mod tests {
    use super::SemanticConfig;
    use super::SemanticIndexer;

    #[test]
    fn semantic_indexer_defaults_disabled() {
        let config = SemanticConfig::default();
        let indexer = SemanticIndexer::new(config);

        assert!(!indexer.is_enabled());
        assert_eq!(indexer.auto_reindex_threshold(), 20);
        assert!(!indexer.should_reindex(19));
        assert!(indexer.should_reindex(20));
    }

    #[test]
    fn semantic_config_with_enabled_toggle() {
        let indexer = SemanticIndexer::new(SemanticConfig::default().with_enabled(true));
        assert!(indexer.is_enabled());
        assert!(indexer.describe().contains("enabled"));
    }
}
