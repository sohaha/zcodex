use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use std::time::SystemTime;

use crate::api::AnalysisResponse;
use crate::daemon::StructuredFailure;
use crate::lang_support::SupportedLanguage;
use crate::semantic::SemanticReindexReport;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirtyState {
    pub dirty_files: usize,
    pub reindex_pending: bool,
    pub cache_invalidated: bool,
    pub invalidated_entries: usize,
    pub background_reindex_claimed: bool,
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
    pub background_reindex_in_progress: bool,
    pub last_query_at: Option<SystemTime>,
    pub last_reindex: Option<SemanticReindexReport>,
    pub last_reindex_attempt: Option<SemanticReindexReport>,
    pub last_warm: Option<WarmReport>,
    pub last_structured_failure: Option<StructuredFailure>,
    pub degraded_mode_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum WarmStatus {
    Busy,
    Failed,
    Loaded,
    Reindexed,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WarmReport {
    pub status: WarmStatus,
    pub languages: Vec<SupportedLanguage>,
    pub started_at: SystemTime,
    pub finished_at: SystemTime,
    pub message: String,
}

#[derive(Debug)]
pub struct Session {
    config: SessionConfig,
    cache: HashMap<String, AnalysisResponse>,
    dirty_files: BTreeSet<PathBuf>,
    dirty_languages: BTreeSet<SupportedLanguage>,
    has_unmapped_dirty_paths: bool,
    background_reindex_in_progress: bool,
    last_query_at: Option<SystemTime>,
    last_reindex: Option<SemanticReindexReport>,
    last_reindex_attempt: Option<SemanticReindexReport>,
    last_warm: Option<WarmReport>,
    last_structured_failure: Option<StructuredFailure>,
    degraded_mode_active: bool,
}

impl Session {
    pub fn new(config: SessionConfig) -> Self {
        Self {
            config,
            cache: HashMap::new(),
            dirty_files: BTreeSet::new(),
            dirty_languages: BTreeSet::new(),
            has_unmapped_dirty_paths: false,
            background_reindex_in_progress: false,
            last_query_at: None,
            last_reindex: None,
            last_reindex_attempt: None,
            last_warm: None,
            last_structured_failure: None,
            degraded_mode_active: false,
        }
    }

    pub fn cached_analysis(&self, key: &str) -> Option<&AnalysisResponse> {
        if self.reindex_pending() {
            None
        } else {
            self.cache.get(key)
        }
    }

    pub fn reindex_pending(&self) -> bool {
        !self.dirty_files.is_empty()
    }

    pub fn store_analysis(&mut self, key: String, response: AnalysisResponse) {
        self.cache.insert(key, response);
        self.last_query_at = Some(SystemTime::now());
    }

    pub fn mark_dirty(&mut self, path: PathBuf) -> DirtyState {
        if let Some(language) = SupportedLanguage::from_path(&path) {
            self.dirty_languages.insert(language);
        } else {
            self.has_unmapped_dirty_paths = true;
        }
        self.dirty_files.insert(path);
        let reindex_pending = self.reindex_pending();
        let invalidated_entries = if self.should_invalidate_cache() {
            self.cache.len()
        } else {
            0
        };
        let cache_invalidated = invalidated_entries > 0;
        if cache_invalidated {
            self.cache.clear();
        }
        DirtyState {
            dirty_files: self.dirty_files.len(),
            reindex_pending,
            cache_invalidated,
            invalidated_entries,
            background_reindex_claimed: false,
        }
    }

    pub fn should_invalidate_cache(&self) -> bool {
        self.dirty_files.len() >= self.config.dirty_file_threshold
    }

    pub fn clear_dirty_files(&mut self) -> bool {
        let had_dirty_files = !self.dirty_files.is_empty();
        self.dirty_files.clear();
        self.dirty_languages.clear();
        self.has_unmapped_dirty_paths = false;
        self.background_reindex_in_progress = false;
        had_dirty_files
    }

    pub fn snapshot(&self) -> SessionSnapshot {
        SessionSnapshot {
            cached_entries: self.cache.len(),
            dirty_files: self.dirty_files.len(),
            dirty_file_threshold: self.config.dirty_file_threshold,
            reindex_pending: self.reindex_pending(),
            background_reindex_in_progress: self.background_reindex_in_progress,
            last_query_at: self.last_query_at,
            last_reindex: self.last_reindex.clone(),
            last_reindex_attempt: self.last_reindex_attempt.clone(),
            last_warm: self.last_warm.clone(),
            last_structured_failure: self.last_structured_failure.clone(),
            degraded_mode_active: self.degraded_mode_active,
        }
    }

    pub fn record_runtime_signals(
        &mut self,
        structured_failure: Option<StructuredFailure>,
        degraded_mode_active: bool,
    ) {
        self.last_structured_failure = structured_failure;
        self.degraded_mode_active = degraded_mode_active;
    }

    pub fn complete_reindex(&mut self, report: SemanticReindexReport) {
        self.cache.clear();
        self.dirty_files.clear();
        self.dirty_languages.clear();
        self.has_unmapped_dirty_paths = false;
        self.background_reindex_in_progress = false;
        self.last_reindex = Some(report.clone());
        self.last_reindex_attempt = Some(report);
        self.last_query_at = Some(SystemTime::now());
    }

    pub fn last_reindex_report(&self) -> Option<SemanticReindexReport> {
        self.last_reindex.clone()
    }

    pub fn record_reindex_attempt(&mut self, report: SemanticReindexReport) {
        self.background_reindex_in_progress = false;
        self.last_reindex_attempt = Some(report);
    }

    pub fn last_reindex_attempt_report(&self) -> Option<SemanticReindexReport> {
        self.last_reindex_attempt.clone()
    }

    pub fn background_reindex_in_progress(&self) -> bool {
        self.background_reindex_in_progress
    }

    pub fn claim_background_reindex(&mut self) -> Option<Vec<SupportedLanguage>> {
        if self.background_reindex_in_progress {
            return None;
        }
        let languages = self.dirty_languages();
        if languages.is_empty() {
            return None;
        }
        self.background_reindex_in_progress = true;
        Some(languages)
    }

    pub fn dirty_languages(&self) -> Vec<SupportedLanguage> {
        self.dirty_languages.iter().copied().collect()
    }

    pub fn has_unmapped_dirty_paths(&self) -> bool {
        self.has_unmapped_dirty_paths
    }

    pub fn record_warm(&mut self, report: WarmReport) {
        self.last_warm = Some(report);
    }
}

#[cfg(test)]
mod tests {
    use super::Session;
    use super::SessionConfig;
    use crate::SupportedLanguage;
    use crate::api::AnalysisKind;
    use crate::api::AnalysisResponse;
    use crate::daemon::StructuredFailure;
    use crate::daemon::StructuredFailureKind;
    use crate::semantic::SemanticReindexReport;
    use pretty_assertions::assert_eq;
    use std::path::PathBuf;
    use std::time::Duration;
    use std::time::SystemTime;

    #[test]
    fn mark_dirty_invalidates_cache_when_threshold_is_reached() {
        let mut session = Session::new(SessionConfig {
            idle_timeout: Duration::from_secs(60),
            dirty_file_threshold: 1,
        });
        session.store_analysis(
            "rust:main".to_string(),
            AnalysisResponse {
                kind: AnalysisKind::Ast,
                summary: "structure summary".to_string(),
                details: None,
            },
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
            AnalysisResponse {
                kind: AnalysisKind::Ast,
                summary: "structure summary".to_string(),
                details: None,
            },
        );
        session.mark_dirty(PathBuf::from("src/main.rs"));

        assert_eq!(session.reindex_pending(), true);
        assert_eq!(session.cached_analysis("rust:main"), None);
    }

    #[test]
    fn record_reindex_attempt_tracks_failures_without_overwriting_last_completed() {
        let mut session = Session::new(SessionConfig {
            idle_timeout: Duration::from_secs(60),
            dirty_file_threshold: 1,
        });
        let completed = SemanticReindexReport::completed(
            vec![SupportedLanguage::Rust],
            1,
            1,
            SystemTime::UNIX_EPOCH,
            SystemTime::UNIX_EPOCH,
            false,
            64,
        );
        let failed =
            SemanticReindexReport::failed(vec![SupportedLanguage::Rust], "failed", false, 64);

        session.complete_reindex(completed.clone());
        session.record_reindex_attempt(failed.clone());

        let snapshot = session.snapshot();
        assert_eq!(snapshot.last_reindex, Some(completed));
        assert_eq!(snapshot.last_reindex_attempt, Some(failed));
    }

    #[test]
    fn record_runtime_signals_persists_last_structured_failure_in_snapshot() {
        let mut session = Session::new(SessionConfig {
            idle_timeout: Duration::from_secs(60),
            dirty_file_threshold: 1,
        });
        let failure = StructuredFailure {
            kind: StructuredFailureKind::DaemonUnavailable,
            reason: "daemon missing".to_string(),
            retryable: true,
            retry_hint: Some("start daemon".to_string()),
        };

        session.record_runtime_signals(Some(failure.clone()), true);

        assert_eq!(session.snapshot().last_structured_failure, Some(failure));
        assert_eq!(session.snapshot().degraded_mode_active, true);
    }
}
