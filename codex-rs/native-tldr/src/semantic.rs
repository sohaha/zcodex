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
