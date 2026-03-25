use serde::Deserialize;
use serde::Serialize;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionConfig {
    pub idle_timeout: Duration,
    pub dirty_file_threshold: usize,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            idle_timeout: Duration::from_secs(30 * 60),
            dirty_file_threshold: 20,
        }
    }
}
