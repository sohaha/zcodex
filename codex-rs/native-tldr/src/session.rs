use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use std::time::SystemTime;

use crate::api::AnalysisResponse;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirtyState {
    pub dirty_files: usize,
    pub reindex_pending: bool,
    pub cache_invalidated: bool,
    pub invalidated_entries: usize,
}

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

    pub fn reindex_pending(&self) -> bool {
        self.should_invalidate_cache()
    }

    pub fn store_analysis(&mut self, key: String, response: AnalysisResponse) {
        self.cache.insert(key, response);
        self.last_query_at = Some(SystemTime::now());
    }

    pub fn mark_dirty(&mut self, path: PathBuf) -> DirtyState {
        self.dirty_files.insert(path);
        let reindex_pending = self.should_invalidate_cache();
        let invalidated_entries = if reindex_pending { self.cache.len() } else { 0 };
        let cache_invalidated = invalidated_entries > 0;
        if cache_invalidated {
            self.cache.clear();
        }
        DirtyState {
            dirty_files: self.dirty_files.len(),
            reindex_pending,
            cache_invalidated,
            invalidated_entries,
        }
    }

    pub fn should_invalidate_cache(&self) -> bool {
        self.dirty_files.len() >= self.config.dirty_file_threshold
    }

    pub fn clear_dirty_files(&mut self) -> bool {
        let had_dirty_files = !self.dirty_files.is_empty();
        self.dirty_files.clear();
        had_dirty_files
    }

    pub fn snapshot(&self) -> SessionSnapshot {
        SessionSnapshot {
            cached_entries: self.cache.len(),
            dirty_files: self.dirty_files.len(),
            dirty_file_threshold: self.config.dirty_file_threshold,
            reindex_pending: self.should_invalidate_cache(),
            last_query_at: self.last_query_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Session;
    use super::SessionConfig;
    use crate::api::AnalysisKind;
    use crate::api::AnalysisResponse;
    use pretty_assertions::assert_eq;
    use std::path::PathBuf;
    use std::time::Duration;

    #[test]
    fn mark_dirty_invalidates_cache_when_threshold_is_reached() {
        let mut session = Session::new(SessionConfig {
            idle_timeout: Duration::from_secs(60),
            dirty_file_threshold: 1,
        });
        session.store_analysis(
            "rust:main".to_string(),
            AnalysisResponse::placeholder(AnalysisKind::Ast),
        );

        let dirty_state = session.mark_dirty(PathBuf::from("src/main.rs"));

        assert_eq!(dirty_state.dirty_files, 1);
        assert_eq!(dirty_state.reindex_pending, true);
        assert_eq!(dirty_state.cache_invalidated, true);
        assert_eq!(dirty_state.invalidated_entries, 1);
        assert_eq!(session.snapshot().cached_entries, 0);
    }

    #[test]
    fn clear_dirty_files_resets_reindex_pending() {
        let mut session = Session::new(SessionConfig {
            idle_timeout: Duration::from_secs(60),
            dirty_file_threshold: 1,
        });
        let dirty_state = session.mark_dirty(PathBuf::from("src/main.rs"));
        assert_eq!(dirty_state.reindex_pending, true);

        assert_eq!(session.clear_dirty_files(), true);
        assert_eq!(session.snapshot().reindex_pending, false);
        assert_eq!(session.clear_dirty_files(), false);
    }

    #[test]
    fn pending_reindex_disables_cache_reads() {
        let mut session = Session::new(SessionConfig {
            idle_timeout: Duration::from_secs(60),
            dirty_file_threshold: 1,
        });
        session.store_analysis(
            "rust:main".to_string(),
            AnalysisResponse::placeholder(AnalysisKind::Ast),
        );
        session.mark_dirty(PathBuf::from("src/main.rs"));

        assert_eq!(session.reindex_pending(), true);
        assert_eq!(session.cached_analysis("rust:main"), None);
    }
}
