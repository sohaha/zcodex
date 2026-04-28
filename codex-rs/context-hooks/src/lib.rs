mod event_recorder;
mod snapshot;

use std::path::Path;
use std::path::PathBuf;

pub use event_recorder::ContextHookRecord;
pub use event_recorder::EventCategory;
pub use event_recorder::record_post_tool_use_event;
pub use snapshot::build_session_snapshot;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextHooksSettings {
    pub snapshot_token_budget: usize,
    pub max_events_per_snapshot: usize,
}

impl Default for ContextHooksSettings {
    fn default() -> Self {
        Self {
            snapshot_token_budget: 2000,
            max_events_per_snapshot: 50,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ZmemoryContext {
    pub codex_home: PathBuf,
    pub cwd: PathBuf,
    pub zmemory_path: Option<PathBuf>,
    pub settings: codex_zmemory::config::ZmemorySettings,
}

impl ZmemoryContext {
    pub fn new(
        codex_home: PathBuf,
        cwd: PathBuf,
        zmemory_path: Option<PathBuf>,
        settings: codex_zmemory::config::ZmemorySettings,
    ) -> Self {
        Self {
            codex_home,
            cwd,
            zmemory_path,
            settings,
        }
    }

    fn codex_home(&self) -> &Path {
        self.codex_home.as_path()
    }

    fn cwd(&self) -> &Path {
        self.cwd.as_path()
    }
}
