use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use std::time::SystemTime;

use crate::api::AnalysisResponse;

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionSnapshot {
    pub cached_entries: usize,
    pub dirty_files: usize,
    pub dirty_file_threshold: usize,
    pub reindex_pending: bool,
    pub last_query_at: Option<SystemTime>,
}

#[derive(Debug)]
pub struct Session {
    config: SessionConfig,
    cache: HashMap<String, AnalysisResponse>,
    dirty_files: BTreeSet<PathBuf>,
    last_query_at: Option<SystemTime>,
}

impl Session {
    pub fn new(config: SessionConfig) -> Self {
        Self {
            config,
            cache: HashMap::new(),
            dirty_files: BTreeSet::new(),
            last_query_at: None,
        }
    }

    pub fn cached_analysis(&self, key: &str) -> Option<&AnalysisResponse> {
        self.cache.get(key)
    }

    pub fn store_analysis(&mut self, key: String, response: AnalysisResponse) {
        self.cache.insert(key, response);
        self.last_query_at = Some(SystemTime::now());
    }

    pub fn mark_dirty(&mut self, path: PathBuf) {
        self.dirty_files.insert(path);
    }

    pub fn should_reindex(&self) -> bool {
        self.dirty_files.len() >= self.config.dirty_file_threshold
    }

    pub fn clear_dirty_files(&mut self) {
        self.dirty_files.clear();
    }

    pub fn snapshot(&self) -> SessionSnapshot {
        SessionSnapshot {
            cached_entries: self.cache.len(),
            dirty_files: self.dirty_files.len(),
            dirty_file_threshold: self.config.dirty_file_threshold,
            reindex_pending: self.should_reindex(),
            last_query_at: self.last_query_at,
        }
    }
}
