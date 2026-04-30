use crate::MissionResult;
use crate::error::MissionError;
use serde::Deserialize;
use serde::Serialize;
use std::path::Path;
use std::path::PathBuf;

pub const MISSION_DIR_NAME: &str = ".mission";
pub const AGENTS_MISSION_DIR_NAME: &str = ".agents/mission";
pub const MISSION_STATE_FILE_NAME: &str = "mission_state.json";

/// Mission 的顶层运行状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MissionStatus {
    Planning,
    Executing,
    Validating,
    Completed,
    Blocked,
}

impl MissionStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Planning => "planning",
            Self::Executing => "executing",
            Self::Validating => "validating",
            Self::Completed => "completed",
            Self::Blocked => "blocked",
        }
    }
}

/// Mission 规划流程中的阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MissionPhase {
    Intent,
    Context,
    Constraints,
    Architecture,
    Plan,
    WorkerDefinition,
    Verification,
}

impl MissionPhase {
    pub const ALL: [Self; 7] = [
        Self::Intent,
        Self::Context,
        Self::Constraints,
        Self::Architecture,
        Self::Plan,
        Self::WorkerDefinition,
        Self::Verification,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Intent => "intent",
            Self::Context => "context",
            Self::Constraints => "constraints",
            Self::Architecture => "architecture",
            Self::Plan => "plan",
            Self::WorkerDefinition => "worker_definition",
            Self::Verification => "verification",
        }
    }

    pub fn next(self) -> Option<Self> {
        match self {
            Self::Intent => Some(Self::Context),
            Self::Context => Some(Self::Constraints),
            Self::Constraints => Some(Self::Architecture),
            Self::Architecture => Some(Self::Plan),
            Self::Plan => Some(Self::WorkerDefinition),
            Self::WorkerDefinition => Some(Self::Verification),
            Self::Verification => None,
        }
    }
}

/// 持久化的 Mission 状态快照。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MissionState {
    pub goal: String,
    pub status: MissionStatus,
    pub phase: Option<MissionPhase>,
    #[serde(default)]
    pub completed_phases: Vec<MissionPhaseRecord>,
}

/// 单个 Mission 规划阶段的用户确认结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MissionPhaseRecord {
    pub phase: MissionPhase,
    pub note: String,
}

/// 面向 CLI 展示的 Mission 状态摘要。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MissionStatusReport {
    Empty {
        state_path: PathBuf,
    },
    Active {
        state_path: PathBuf,
        state: MissionState,
    },
}

/// 负责定位与读取 Mission 状态文件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissionStateStore {
    state_path: PathBuf,
}

impl MissionStateStore {
    pub fn for_workspace(workspace: impl AsRef<Path>) -> Self {
        Self {
            state_path: workspace
                .as_ref()
                .join(MISSION_DIR_NAME)
                .join(MISSION_STATE_FILE_NAME),
        }
    }

    pub fn state_path(&self) -> &Path {
        &self.state_path
    }

    pub fn status_report(&self) -> MissionResult<MissionStatusReport> {
        match self.load()? {
            Some(state) => Ok(MissionStatusReport::Active {
                state_path: self.state_path.clone(),
                state,
            }),
            None => Ok(MissionStatusReport::Empty {
                state_path: self.state_path.clone(),
            }),
        }
    }

    pub fn load(&self) -> MissionResult<Option<MissionState>> {
        match std::fs::read_to_string(&self.state_path) {
            Ok(contents) => serde_json::from_str(&contents).map(Some).map_err(|source| {
                MissionError::ParseState {
                    path: self.state_path.clone(),
                    source,
                }
            }),
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(MissionError::ReadState {
                path: self.state_path.clone(),
                source,
            }),
        }
    }

    pub fn save(&self, state: &MissionState) -> MissionResult<()> {
        let Some(state_dir) = self.state_path.parent() else {
            return Err(MissionError::CreateStateDir {
                path: self.state_path.clone(),
                source: std::io::Error::other("Mission 状态文件没有父目录"),
            });
        };
        std::fs::create_dir_all(state_dir).map_err(|source| MissionError::CreateStateDir {
            path: state_dir.to_path_buf(),
            source,
        })?;
        let contents =
            serde_json::to_string_pretty(state).map_err(|source| MissionError::SerializeState {
                path: self.state_path.clone(),
                source,
            })?;
        std::fs::write(&self.state_path, contents).map_err(|source| MissionError::WriteState {
            path: self.state_path.clone(),
            source,
        })
    }

    /// 删除当前 Mission 状态文件。
    pub fn reset(&self) -> MissionResult<()> {
        if self.state_path.exists() {
            std::fs::remove_file(&self.state_path).map_err(|source| MissionError::WriteState {
                path: self.state_path.clone(),
                source,
            })?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use tempfile::TempDir;

    #[test]
    fn missing_state_reports_empty() -> anyhow::Result<()> {
        let workspace = TempDir::new()?;
        let store = MissionStateStore::for_workspace(workspace.path());

        assert_eq!(
            store.status_report()?,
            MissionStatusReport::Empty {
                state_path: workspace
                    .path()
                    .join(MISSION_DIR_NAME)
                    .join(MISSION_STATE_FILE_NAME),
            }
        );

        Ok(())
    }

    #[test]
    fn existing_state_reports_active() -> anyhow::Result<()> {
        let workspace = TempDir::new()?;
        let mission_dir = workspace.path().join(MISSION_DIR_NAME);
        std::fs::create_dir_all(&mission_dir)?;
        std::fs::write(
            mission_dir.join(MISSION_STATE_FILE_NAME),
            r#"{"goal":"ship it","status":"planning","phase":"intent"}"#,
        )?;
        let store = MissionStateStore::for_workspace(workspace.path());

        assert_eq!(
            store.status_report()?,
            MissionStatusReport::Active {
                state_path: mission_dir.join(MISSION_STATE_FILE_NAME),
                state: MissionState {
                    goal: "ship it".to_string(),
                    status: MissionStatus::Planning,
                    phase: Some(MissionPhase::Intent),
                    completed_phases: Vec::new(),
                },
            }
        );

        Ok(())
    }

    #[test]
    fn save_creates_state_file() -> anyhow::Result<()> {
        let workspace = TempDir::new()?;
        let store = MissionStateStore::for_workspace(workspace.path());
        let state = MissionState {
            goal: "ship it".to_string(),
            status: MissionStatus::Planning,
            phase: Some(MissionPhase::Intent),
            completed_phases: Vec::new(),
        };

        store.save(&state)?;

        assert_eq!(store.load()?, Some(state));
        Ok(())
    }
}
