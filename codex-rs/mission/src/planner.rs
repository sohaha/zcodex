use chrono::Utc;
use std::path::Path;
use std::path::PathBuf;

use crate::MissionError;
use crate::MissionPhase;
use crate::MissionPhaseRecord;
use crate::MissionResult;
use crate::MissionState;
use crate::MissionStateStore;
use crate::MissionStatus;
use crate::phases::MissionPhaseDefinition;
use crate::phases::phase_definition;
use crate::state::MISSION_STATE_VERSION;

/// Mission 规划器，负责启动和推进 7 阶段规划状态机。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissionPlanner {
    store: MissionStateStore,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissionPlanningStep {
    pub state: MissionState,
    pub definition: Option<MissionPhaseDefinition>,
}

impl MissionPlanner {
    pub fn for_workspace(workspace: impl AsRef<Path>) -> Self {
        Self {
            store: MissionStateStore::for_workspace(workspace),
        }
    }

    pub fn start(&self, goal: impl Into<String>) -> MissionResult<MissionPlanningStep> {
        let goal = goal.into();
        let goal = goal.trim();
        if goal.is_empty() {
            return Err(MissionError::EmptyGoal);
        }

        let now = Utc::now();
        let state = MissionState {
            version: MISSION_STATE_VERSION,
            id: generate_mission_id(),
            goal: goal.to_string(),
            status: MissionStatus::Planning,
            phase: Some(MissionPhase::Intent),
            completed_phases: Vec::new(),
            created_at: Some(now),
            updated_at: Some(now),
        };
        self.store.save(&state)?;
        Ok(self.step_for_state(state))
    }

    /// 推进到下一个规划阶段。
    ///
    /// 校验阶段转换的合法性：不能跳阶段，不能从终态推进。
    pub fn continue_planning(&self, note: Option<String>) -> MissionResult<MissionPlanningStep> {
        let mut state = self
            .store
            .load()?
            .ok_or_else(|| MissionError::MissingState {
                path: self.store.state_path().to_path_buf(),
            })?;

        // 终态校验
        if state.status.is_terminal() {
            return Err(MissionError::TerminalState {
                status: state.status.to_string(),
            });
        }

        if state.status != MissionStatus::Planning {
            return Ok(self.step_for_state(state));
        }

        let Some(current_phase) = state.phase else {
            // 所有阶段完成，转入执行
            state.status = MissionStatus::Executing;
            state.updated_at = Some(Utc::now());
            self.store.save(&state)?;
            return Ok(self.step_for_state(state));
        };

        // 阶段转换校验：已完成阶段数应等于当前阶段索引
        let expected_count = current_phase.index();
        if state.completed_phases.len() != expected_count {
            return Err(MissionError::InvalidPhaseTransition {
                from: format!("completed {} phases", state.completed_phases.len()),
                to: current_phase.to_string(),
            });
        }

        let record = MissionPhaseRecord::new(current_phase, normalized_note(note, current_phase))
            .with_artifact(format!("{}.md", current_phase.label()));
        state.completed_phases.push(record);
        state.phase = current_phase.next();
        if state.phase.is_none() {
            state.status = MissionStatus::Executing;
        }
        state.updated_at = Some(Utc::now());
        self.store.save(&state)?;
        Ok(self.step_for_state(state))
    }

    /// 暂停当前 Mission。
    pub fn pause(&self) -> MissionResult<MissionPlanningStep> {
        let mut state = self
            .store
            .load()?
            .ok_or_else(|| MissionError::MissingState {
                path: self.store.state_path().to_path_buf(),
            })?;

        if state.status.is_terminal() {
            return Err(MissionError::TerminalState {
                status: state.status.to_string(),
            });
        }
        state.status = MissionStatus::Paused;
        state.updated_at = Some(Utc::now());
        self.store.save(&state)?;
        Ok(self.step_for_state(state))
    }

    /// 从暂停恢复。
    pub fn resume(&self) -> MissionResult<MissionPlanningStep> {
        let mut state = self
            .store
            .load()?
            .ok_or_else(|| MissionError::MissingState {
                path: self.store.state_path().to_path_buf(),
            })?;

        if state.status != MissionStatus::Paused {
            return Ok(self.step_for_state(state));
        }
        // 恢复到暂停前的逻辑状态
        state.status = if state.phase.is_some() {
            MissionStatus::Planning
        } else {
            MissionStatus::Executing
        };
        state.updated_at = Some(Utc::now());
        self.store.save(&state)?;
        Ok(self.step_for_state(state))
    }

    /// 中止 Mission。
    pub fn abort(&self) -> MissionResult<MissionPlanningStep> {
        let mut state = self
            .store
            .load()?
            .ok_or_else(|| MissionError::MissingState {
                path: self.store.state_path().to_path_buf(),
            })?;

        if state.status.is_terminal() {
            return Err(MissionError::TerminalState {
                status: state.status.to_string(),
            });
        }
        state.status = MissionStatus::Aborted;
        state.updated_at = Some(Utc::now());
        self.store.save(&state)?;
        Ok(self.step_for_state(state))
    }

    /// 方案存储目录（委托给 store）。
    pub fn plans_dir(&self) -> PathBuf {
        self.store.plans_dir()
    }

    /// 保存规划阶段的产物到方案目录。
    pub fn save_plan_artifact(&self, phase: MissionPhase, content: &str) -> MissionResult<PathBuf> {
        let dir = self.plans_dir();
        std::fs::create_dir_all(&dir).map_err(|source| MissionError::CreatePlanDir {
            path: dir.clone(),
            source,
        })?;
        let path = dir.join(format!("{}.md", phase.label()));
        std::fs::write(&path, content).map_err(|source| MissionError::WritePlan {
            path: path.clone(),
            source,
        })?;
        Ok(path)
    }

    /// 读取指定阶段的方案产物。
    pub fn load_plan_artifact(&self, phase: MissionPhase) -> MissionResult<String> {
        let path = self.plans_dir().join(format!("{}.md", phase.label()));
        std::fs::read_to_string(&path).map_err(|source| MissionError::ReadPlan { path, source })
    }

    /// 加载执行方案（`plan.md`）。
    ///
    /// 如果 `plan.md` 不存在，尝试从 `worker_definition.md` 获取执行步骤。
    pub fn load_execution_plan(&self) -> MissionResult<String> {
        let plan_path = self.plans_dir().join("plan.md");
        if plan_path.exists() {
            return std::fs::read_to_string(&plan_path).map_err(|source| MissionError::ReadPlan {
                path: plan_path,
                source,
            });
        }
        let fallback = self.plans_dir().join("worker_definition.md");
        if fallback.exists() {
            return std::fs::read_to_string(&fallback).map_err(|source| MissionError::ReadPlan {
                path: fallback,
                source,
            });
        }
        Err(MissionError::NoPlanToExecute)
    }

    /// 返回只读引用，供外部需要 store 信息时使用。
    pub fn store(&self) -> &MissionStateStore {
        &self.store
    }

    fn step_for_state(&self, state: MissionState) -> MissionPlanningStep {
        MissionPlanningStep {
            definition: state.phase.map(phase_definition).copied(),
            state,
        }
    }
}

/// 生成简短的 Mission ID（时间戳 + 随机后缀）。
fn generate_mission_id() -> String {
    let ts = Utc::now().format("%Y%m%d%H%M");
    let rand_part: u16 = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos()
        >> 16) as u16;
    format!("{ts}-{rand_part:04x}")
}

fn normalized_note(note: Option<String>, phase: MissionPhase) -> String {
    match note.map(|value| value.trim().to_string()) {
        Some(note) if !note.is_empty() => note,
        _ => format!("{} 阶段已确认", phase.label()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use tempfile::TempDir;

    #[test]
    fn start_creates_intent_phase_state() -> anyhow::Result<()> {
        let workspace = TempDir::new()?;
        let planner = MissionPlanner::for_workspace(workspace.path());

        let step = planner.start(" ship it ")?;

        assert_eq!(step.state.goal, "ship it");
        assert_eq!(step.state.status, MissionStatus::Planning);
        assert_eq!(step.state.phase, Some(MissionPhase::Intent));
        assert!(!step.state.id.is_empty());
        assert!(step.state.created_at.is_some());
        assert_eq!(
            step.definition.map(|definition| definition.phase),
            Some(MissionPhase::Intent)
        );
        Ok(())
    }

    #[test]
    fn continue_advances_until_executing() -> anyhow::Result<()> {
        let workspace = TempDir::new()?;
        let planner = MissionPlanner::for_workspace(workspace.path());
        planner.start("ship it")?;

        let mut step = planner.continue_planning(Some("intent ok".to_string()))?;
        assert_eq!(step.state.phase, Some(MissionPhase::Context));
        assert_eq!(step.state.completed_phases.len(), 1);

        for _ in 0..6 {
            step = planner.continue_planning(None)?;
        }

        assert_eq!(step.state.status, MissionStatus::Executing);
        assert_eq!(step.state.phase, None);
        assert_eq!(step.state.completed_phases.len(), 7);
        assert!(step.state.updated_at.is_some());
        Ok(())
    }

    #[test]
    fn pause_and_resume() -> anyhow::Result<()> {
        let workspace = TempDir::new()?;
        let planner = MissionPlanner::for_workspace(workspace.path());
        planner.start("ship it")?;
        planner.continue_planning(None)?;

        let step = planner.pause()?;
        assert_eq!(step.state.status, MissionStatus::Paused);

        let step = planner.resume()?;
        assert_eq!(step.state.status, MissionStatus::Planning);
        Ok(())
    }

    #[test]
    fn abort_is_terminal() -> anyhow::Result<()> {
        let workspace = TempDir::new()?;
        let planner = MissionPlanner::for_workspace(workspace.path());
        planner.start("ship it")?;

        let step = planner.abort()?;
        assert_eq!(step.state.status, MissionStatus::Aborted);
        assert!(step.state.status.is_terminal());

        // 从终态继续推进应该失败
        let result = planner.continue_planning(None);
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn empty_goal_rejected() -> anyhow::Result<()> {
        let workspace = TempDir::new()?;
        let planner = MissionPlanner::for_workspace(workspace.path());

        let result = planner.start("  ");
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn phase_record_has_timestamp_and_artifact() -> anyhow::Result<()> {
        let workspace = TempDir::new()?;
        let planner = MissionPlanner::for_workspace(workspace.path());
        planner.start("ship it")?;

        let step = planner.continue_planning(Some("confirmed".to_string()))?;
        let record = &step.state.completed_phases[0];
        assert_eq!(record.phase, MissionPhase::Intent);
        assert_eq!(record.note, "confirmed");
        assert!(record.completed_at.is_some());
        assert_eq!(record.artifact_path.as_deref(), Some("intent.md"));
        Ok(())
    }
}
