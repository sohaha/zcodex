//! Mission Worker Session 管理。
//!
//! 负责创建、恢复和监控 Worker session。

use crate::MissionResult;
use crate::error::MissionError;
use serde::Deserialize;
use serde::Serialize;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::time::SystemTime;

/// Worker session 目录名称。
pub const WORKER_SESSIONS_DIR: &str = "worker_sessions";

/// Handoff 目录名称。
pub const HANDOFFS_DIR: &str = "handoffs";

/// Worker 状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerStatus {
    /// Worker 已创建但未开始。
    Pending,
    /// Worker 正在执行。
    Running,
    /// Worker 已完成。
    Completed,
    /// Worker 失败。
    Failed,
    /// Worker 被取消。
    Cancelled,
}

impl WorkerStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

/// Worker session 标识符。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerId(String);

impl WorkerId {
    /// 创建新的 Worker ID。
    pub fn new(name: String, sequence: u32) -> Self {
        Self(format!("{name}-{sequence:03}"))
    }

    /// 获取 Worker 名称。
    pub fn name(&self) -> &str {
        self.0.rsplit('-').next().unwrap_or(&self.0)
    }

    /// 获取完整 ID 字符串。
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Worker session 状态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerSession {
    /// Worker ID。
    pub id: WorkerId,
    /// Worker 类型（skill 名称）。
    pub worker_type: String,
    /// Worker 状态。
    pub status: WorkerStatus,
    /// 创建时间。
    pub created_at: SystemTime,
    /// 更新时间。
    pub updated_at: SystemTime,
    /// 输入数据。
    #[serde(default)]
    pub input: WorkerInput,
    /// 输出数据。
    #[serde(default)]
    pub output: WorkerOutput,
}

/// Worker 输入数据。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct WorkerInput {
    /// 上一轮的 Handoff 数据。
    #[serde(default)]
    pub previous_handoff: Option<serde_json::Value>,
    /// Worker 特定的输入参数。
    #[serde(default)]
    pub parameters: serde_json::Value,
}

/// Worker 输出数据。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct WorkerOutput {
    /// Worker 生成的 Handoff 数据。
    #[serde(default)]
    pub handoff: Option<serde_json::Value>,
    /// Worker 执行日志。
    #[serde(default)]
    pub logs: Vec<String>,
    /// 错误信息（如果失败）。
    #[serde(default)]
    pub error: Option<String>,
}

/// Worker session 管理器。
///
/// 负责创建、恢复和监控 Worker session。
#[derive(Debug, Clone)]
pub struct WorkerManager {
    /// Mission 目录。
    mission_dir: PathBuf,
}

impl WorkerManager {
    /// 创建新的 Worker 管理器。
    pub fn new(mission_dir: impl AsRef<Path>) -> Self {
        Self {
            mission_dir: mission_dir.as_ref().to_path_buf(),
        }
    }

    /// 获取 Worker session 目录。
    pub fn sessions_dir(&self) -> PathBuf {
        self.mission_dir.join(WORKER_SESSIONS_DIR)
    }

    /// 获取 Handoff 目录。
    pub fn handoffs_dir(&self) -> PathBuf {
        self.mission_dir.join(HANDOFFS_DIR)
    }

    /// 创建新的 Worker session。
    pub fn create_session(
        &self,
        worker_type: String,
        input: WorkerInput,
    ) -> MissionResult<WorkerSession> {
        let sessions_dir = self.sessions_dir();
        fs::create_dir_all(&sessions_dir).map_err(|source| {
            MissionError::CreateWorkerSessionDir {
                path: sessions_dir.clone(),
                source,
            }
        })?;

        // 生成 Worker ID
        let sequence = self.next_sequence(&worker_type)?;
        let id = WorkerId::new(worker_type.clone(), sequence);

        let now = SystemTime::now();
        let session = WorkerSession {
            id: id.clone(),
            worker_type: worker_type.clone(),
            status: WorkerStatus::Pending,
            created_at: now,
            updated_at: now,
            input,
            output: WorkerOutput::default(),
        };

        // 保存 session
        self.save_session(&session)?;

        Ok(session)
    }

    /// 加载 Worker session。
    pub fn load_session(&self, id: &WorkerId) -> MissionResult<WorkerSession> {
        let session_path = self.session_path(id);

        let content = fs::read_to_string(&session_path).map_err(|source| {
            MissionError::ReadWorkerSession {
                id: id.as_str().to_string(),
                source,
            }
        })?;

        serde_json::from_str(&content).map_err(|source| MissionError::ParseWorkerSession {
            id: id.as_str().to_string(),
            source,
        })
    }

    /// 更新 Worker session。
    pub fn update_session(&self, session: &WorkerSession) -> MissionResult<()> {
        let mut updated = session.clone();
        updated.updated_at = SystemTime::now();
        self.save_session(&updated)
    }

    /// 保存 Worker session。
    fn save_session(&self, session: &WorkerSession) -> MissionResult<()> {
        let session_path = self.session_path(&session.id);

        let content = serde_json::to_string_pretty(session)
            .map_err(|source| MissionError::SerializeHandoff { source })?;

        fs::write(&session_path, content).map_err(|source| MissionError::WriteWorkerSession {
            id: session.id.as_str().to_string(),
            source,
        })
    }

    /// 列出所有 Worker session。
    pub fn list_sessions(&self) -> MissionResult<Vec<WorkerSession>> {
        let sessions_dir = self.sessions_dir();

        if !sessions_dir.exists() {
            return Ok(Vec::new());
        }

        let mut sessions = Vec::new();
        let entries =
            fs::read_dir(&sessions_dir).map_err(|source| MissionError::ReadWorkerSession {
                id: "<list>".to_string(),
                source,
            })?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Some(file_stem) = path.file_stem().and_then(|s| s.to_str()) {
                    let id = WorkerId(file_stem.to_string());
                    if let Ok(session) = self.load_session(&id) {
                        sessions.push(session);
                    }
                }
            }
        }

        sessions.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        Ok(sessions)
    }

    /// 获取下一个序列号。
    fn next_sequence(&self, worker_type: &str) -> MissionResult<u32> {
        let sessions = self.list_sessions()?;
        let worker_sessions: Vec<_> = sessions
            .iter()
            .filter(|s| s.worker_type == worker_type)
            .collect();

        let max_sequence = worker_sessions
            .iter()
            .filter_map(|s| s.id.name().parse::<u32>().ok())
            .max()
            .unwrap_or(0);

        Ok(max_sequence + 1)
    }

    /// 获取 session 文件路径。
    fn session_path(&self, id: &WorkerId) -> PathBuf {
        self.sessions_dir().join(format!("{}.json", id.as_str()))
    }

    /// 保存 Handoff。
    pub fn save_handoff(
        &self,
        worker_id: &WorkerId,
        handoff: &serde_json::Value,
    ) -> MissionResult<PathBuf> {
        let handoffs_dir = self.handoffs_dir();
        fs::create_dir_all(&handoffs_dir).map_err(|source| MissionError::CreateHandoffDir {
            path: handoffs_dir.clone(),
            source,
        })?;

        let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
        let filename = format!("{}-{}.json", worker_id.as_str(), timestamp);
        let handoff_path = handoffs_dir.join(&filename);

        let content = serde_json::to_string_pretty(handoff)
            .map_err(|source| MissionError::SerializeHandoff { source })?;

        fs::write(&handoff_path, content).map_err(|source| MissionError::WriteHandoff {
            path: handoff_path.clone(),
            source,
        })?;

        Ok(handoff_path)
    }

    /// 加载最新的 Handoff。
    pub fn load_latest_handoff(&self) -> MissionResult<Option<serde_json::Value>> {
        let handoffs_dir = self.handoffs_dir();

        if !handoffs_dir.exists() {
            return Ok(None);
        }

        let mut handoffs: Vec<_> = fs::read_dir(&handoffs_dir)
            .map_err(|source| MissionError::ReadWorkerSession {
                id: "<handoffs>".to_string(),
                source,
            })?
            .flatten()
            .filter(|entry| entry.path().extension().and_then(|s| s.to_str()) == Some("json"))
            .collect();

        handoffs.sort_by_key(|entry| entry.metadata().ok().and_then(|m| m.modified().ok()));

        if let Some(latest) = handoffs.last() {
            let content = fs::read_to_string(latest.path()).map_err(|source| {
                MissionError::ReadWorkerSession {
                    id: "<handoffs>".to_string(),
                    source,
                }
            })?;

            let handoff =
                serde_json::from_str(&content).map_err(|source| MissionError::ParseHandoff {
                    path: latest.path().clone(),
                    source,
                })?;

            Ok(Some(handoff))
        } else {
            Ok(None)
        }
    }

    /// 清理已完成的 Worker session。
    pub fn cleanup_completed_sessions(&self) -> MissionResult<()> {
        let sessions = self.list_sessions()?;

        for session in sessions {
            if matches!(
                session.status,
                WorkerStatus::Completed | WorkerStatus::Failed | WorkerStatus::Cancelled
            ) {
                let session_path = self.session_path(&session.id);
                // 删除 session 文件
                let _ = fs::remove_file(session_path);
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn create_session_generates_unique_id() {
        let mission_dir = TempDir::new().unwrap();
        let manager = WorkerManager::new(mission_dir.path());

        let session1 = manager
            .create_session("test-worker".to_string(), WorkerInput::default())
            .unwrap();
        let session2 = manager
            .create_session("test-worker".to_string(), WorkerInput::default())
            .unwrap();

        assert_ne!(session1.id, session2.id);
    }

    #[test]
    fn save_and_load_session() {
        let mission_dir = TempDir::new().unwrap();
        let manager = WorkerManager::new(mission_dir.path());

        let session = manager
            .create_session("test-worker".to_string(), WorkerInput::default())
            .unwrap();

        let loaded = manager.load_session(&session.id).unwrap();
        assert_eq!(loaded.id, session.id);
        assert_eq!(loaded.worker_type, session.worker_type);
    }

    #[test]
    fn list_sessions_returns_all_sessions() {
        let mission_dir = TempDir::new().unwrap();
        let manager = WorkerManager::new(mission_dir.path());

        manager
            .create_session("worker-1".to_string(), WorkerInput::default())
            .unwrap();
        manager
            .create_session("worker-2".to_string(), WorkerInput::default())
            .unwrap();

        let sessions = manager.list_sessions().unwrap();
        assert_eq!(sessions.len(), 2);
    }

    #[test]
    fn update_session_modifies_timestamp() {
        let mission_dir = TempDir::new().unwrap();
        let manager = WorkerManager::new(mission_dir.path());

        let mut session = manager
            .create_session("test-worker".to_string(), WorkerInput::default())
            .unwrap();
        let original_updated = session.updated_at;

        // 给系统一点时间确保时间戳不同
        std::thread::sleep(std::time::Duration::from_millis(10));

        session.status = WorkerStatus::Running;
        manager.update_session(&session).unwrap();

        let loaded = manager.load_session(&session.id).unwrap();
        assert!(loaded.updated_at > original_updated);
    }

    #[test]
    fn save_and_load_handoff() {
        let mission_dir = TempDir::new().unwrap();
        let manager = WorkerManager::new(mission_dir.path());

        let handoff = serde_json::json!({
            "worker": "test-worker",
            "salientSummary": "Test completed"
        });

        let worker_id = WorkerId::new("test-worker".to_string(), 1);
        manager.save_handoff(&worker_id, &handoff).unwrap();

        let loaded = manager.load_latest_handoff().unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap()["worker"], "test-worker");
    }

    #[test]
    fn cleanup_removes_completed_sessions() {
        let mission_dir = TempDir::new().unwrap();
        let manager = WorkerManager::new(mission_dir.path());

        let mut session1 = manager
            .create_session("worker-1".to_string(), WorkerInput::default())
            .unwrap();
        session1.status = WorkerStatus::Completed;
        manager.update_session(&session1).unwrap();

        let session2 = manager
            .create_session("worker-2".to_string(), WorkerInput::default())
            .unwrap();

        manager.cleanup_completed_sessions().unwrap();

        let sessions = manager.list_sessions().unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, session2.id);
    }
}
