use chrono::DateTime;
use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;
use std::fmt;
use std::path::Path;
use std::path::PathBuf;

use crate::MissionResult;
use crate::error::MissionError;

pub const MISSION_DIR_NAME: &str = ".mission";
pub const AGENTS_MISSION_DIR_NAME: &str = ".agents/mission";
pub const MISSION_STATE_FILE_NAME: &str = "mission_state.json";

/// 当前 Mission 状态文件格式版本。
pub const MISSION_STATE_VERSION: u32 = 1;

/// Mission 的顶层运行状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MissionStatus {
    Planning,
    Executing,
    Validating,
    Completed,
    Blocked,
    /// 用户主动暂停。
    Paused,
    /// 用户中止，不再恢复。
    Aborted,
}

impl MissionStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Planning => "planning",
            Self::Executing => "executing",
            Self::Validating => "validating",
            Self::Completed => "completed",
            Self::Blocked => "blocked",
            Self::Paused => "paused",
            Self::Aborted => "aborted",
        }
    }

    /// 该状态是否为终态（不会自动变更）。
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Aborted)
    }

    /// 该状态下是否允许推进阶段。
    pub fn can_advance(self) -> bool {
        matches!(self, Self::Planning)
    }
}

impl fmt::Display for MissionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

impl std::str::FromStr for MissionStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "planning" => Ok(Self::Planning),
            "executing" => Ok(Self::Executing),
            "validating" => Ok(Self::Validating),
            "completed" => Ok(Self::Completed),
            "blocked" => Ok(Self::Blocked),
            "paused" => Ok(Self::Paused),
            "aborted" => Ok(Self::Aborted),
            other => Err(format!("未知 MissionStatus: {other}")),
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

    /// 阶段索引，0-based。
    pub fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|&p| p == self)
            .expect("MissionPhase 必须在 ALL 中")
    }

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

    /// 前一个阶段。
    pub fn prev(self) -> Option<Self> {
        match self {
            Self::Intent => None,
            Self::Context => Some(Self::Intent),
            Self::Constraints => Some(Self::Context),
            Self::Architecture => Some(Self::Constraints),
            Self::Plan => Some(Self::Architecture),
            Self::WorkerDefinition => Some(Self::Plan),
            Self::Verification => Some(Self::WorkerDefinition),
        }
    }
}

impl fmt::Display for MissionPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

impl std::str::FromStr for MissionPhase {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "intent" => Ok(Self::Intent),
            "context" => Ok(Self::Context),
            "constraints" => Ok(Self::Constraints),
            "architecture" => Ok(Self::Architecture),
            "plan" => Ok(Self::Plan),
            "worker_definition" => Ok(Self::WorkerDefinition),
            "verification" => Ok(Self::Verification),
            other => Err(format!("未知 MissionPhase: {other}")),
        }
    }
}

/// 持久化的 Mission 状态快照。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MissionState {
    /// 状态文件格式版本。
    pub version: u32,
    /// Mission 唯一标识。
    pub id: String,
    /// Mission 目标描述。
    pub goal: String,
    /// 当前顶层状态。
    pub status: MissionStatus,
    /// 当前阶段（Planning 时有值，Executing 后为 None）。
    pub phase: Option<MissionPhase>,
    /// 已完成阶段记录。
    pub completed_phases: Vec<MissionPhaseRecord>,
    /// 创建时间。
    pub created_at: Option<DateTime<Utc>>,
    /// 最后更新时间。
    pub updated_at: Option<DateTime<Utc>>,
}

fn default_state_version() -> u32 {
    MISSION_STATE_VERSION
}

impl Default for MissionState {
    fn default() -> Self {
        Self {
            version: default_state_version(),
            id: String::new(),
            goal: String::new(),
            status: MissionStatus::Planning,
            phase: None,
            completed_phases: Vec::new(),
            created_at: None,
            updated_at: None,
        }
    }
}

/// 单个 Mission 规划阶段的用户确认结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MissionPhaseRecord {
    /// 阶段。
    pub phase: MissionPhase,
    /// 用户确认备注。
    pub note: String,
    /// 阶段完成时间。
    #[serde(default)]
    pub completed_at: Option<DateTime<Utc>>,
    /// 阶段产物路径（相对于 workspace）。
    #[serde(default)]
    pub artifact_path: Option<String>,
}

impl MissionPhaseRecord {
    /// 便捷构造函数。
    pub fn new(phase: MissionPhase, note: impl Into<String>) -> Self {
        Self {
            phase,
            note: note.into(),
            completed_at: Some(Utc::now()),
            artifact_path: None,
        }
    }

    /// 设置产物路径。
    pub fn with_artifact(mut self, path: impl Into<String>) -> Self {
        self.artifact_path = Some(path.into());
        self
    }
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
    workspace_root: PathBuf,
    state_path: PathBuf,
}

impl MissionStateStore {
    pub fn for_workspace(workspace: impl AsRef<Path>) -> Self {
        Self {
            workspace_root: workspace.as_ref().to_path_buf(),
            state_path: workspace
                .as_ref()
                .join(MISSION_DIR_NAME)
                .join(MISSION_STATE_FILE_NAME),
        }
    }

    /// 工作区根目录。
    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    /// 状态文件路径。
    pub fn state_path(&self) -> &Path {
        &self.state_path
    }

    /// 方案存储目录。
    pub fn plans_dir(&self) -> PathBuf {
        self.workspace_root.join(AGENTS_MISSION_DIR_NAME)
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
            Ok(contents) => {
                let mut state: MissionState =
                    serde_json::from_str(&contents).map_err(|source| MissionError::ParseState {
                        path: self.state_path.clone(),
                        source,
                    })?;
                // 迁移：旧格式文件缺少新字段，补填默认值
                if state.id.is_empty() {
                    state.id = format!("migrated-{}", chrono::Utc::now().timestamp());
                }
                if state.created_at.is_none() {
                    state.created_at = Some(Utc::now());
                }
                state.updated_at = Some(Utc::now());
                Ok(Some(state))
            }
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
            r#"{"version":1,"goal":"ship it","status":"planning","phase":"intent","completed_phases":[]}"#,
        )?;
        let store = MissionStateStore::for_workspace(workspace.path());

        let report = store.status_report()?;
        match report {
            MissionStatusReport::Active { state, .. } => {
                assert_eq!(state.goal, "ship it");
                assert_eq!(state.status, MissionStatus::Planning);
                assert_eq!(state.phase, Some(MissionPhase::Intent));
                assert!(!state.id.is_empty());
                assert!(state.created_at.is_some());
            }
            _ => anyhow::bail!("expected Active, got Empty"),
        }
        Ok(())
    }

    #[test]
    fn save_creates_state_file() -> anyhow::Result<()> {
        let workspace = TempDir::new()?;
        let store = MissionStateStore::for_workspace(workspace.path());
        let state = MissionState::default();

        store.save(&state)?;

        let loaded = store.load()?.expect("should have state after save");
        assert_eq!(loaded.goal, state.goal);
        assert_eq!(loaded.version, MISSION_STATE_VERSION);
        Ok(())
    }

    #[test]
    fn phase_index_ordering() {
        assert_eq!(MissionPhase::Intent.index(), 0);
        assert_eq!(MissionPhase::Context.index(), 1);
        assert_eq!(MissionPhase::Constraints.index(), 2);
        assert_eq!(MissionPhase::Architecture.index(), 3);
        assert_eq!(MissionPhase::Plan.index(), 4);
        assert_eq!(MissionPhase::WorkerDefinition.index(), 5);
        assert_eq!(MissionPhase::Verification.index(), 6);
        assert!(MissionPhase::Verification.next().is_none());
        assert!(MissionPhase::Intent.prev().is_none());
        assert_eq!(
            MissionPhase::Verification.prev(),
            Some(MissionPhase::WorkerDefinition)
        );
    }

    #[test]
    fn status_terminal_and_advance() {
        assert!(MissionStatus::Planning.can_advance());
        assert!(!MissionStatus::Completed.can_advance());
        assert!(MissionStatus::Completed.is_terminal());
        assert!(!MissionStatus::Planning.is_terminal());
        assert!(MissionStatus::Aborted.is_terminal());
        assert!(!MissionStatus::Aborted.can_advance());
    }

    #[test]
    fn phase_record_builder() {
        let record = MissionPhaseRecord::new(MissionPhase::Intent, "confirmed")
            .with_artifact(".agents/mission/intent.md");
        assert_eq!(record.phase, MissionPhase::Intent);
        assert_eq!(record.note, "confirmed");
        assert!(record.completed_at.is_some());
        assert_eq!(
            record.artifact_path.as_deref(),
            Some(".agents/mission/intent.md")
        );
    }

    #[test]
    fn plans_dir_uses_workspace_root() -> anyhow::Result<()> {
        let workspace = TempDir::new()?;
        let store = MissionStateStore::for_workspace(workspace.path());
        assert_eq!(
            store.plans_dir(),
            workspace.path().join(AGENTS_MISSION_DIR_NAME)
        );
        Ok(())
    }
}
