use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TldrToolDescriptor {
    pub name: String,
    pub description: String,
}

impl Default for TldrToolDescriptor {
    fn default() -> Self {
        Self {
            name: "ztldr".to_string(),
            description: "Structured code context analysis for Codex".to_string(),
        }
    }
}
