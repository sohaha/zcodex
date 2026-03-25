use crate::session::SessionConfig;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DaemonConfig {
    pub auto_start: bool,
    pub socket_mode: String,
    pub session: SessionConfig,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            auto_start: true,
            socket_mode: "auto".to_string(),
            session: SessionConfig::default(),
        }
    }
}
