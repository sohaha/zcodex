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
///
/// 阶段门控：每次推进时校验当前阶段的产物文件是否存在且非空，
/// 确保无法跳过阶段分析直接推进。
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
    /// 门控逻辑：
    /// 1. 校验阶段转换合法性（不跳阶段、不从终态推进）
    /// 2. **校验当前阶段的产物文件存在且非空**
    /// 3. 记录已完成阶段，推进到下一阶段
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

        // 门控：校验当前阶段的产物文件
        self.validate_phase_artifact(current_phase)?;

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

        state.status = MissionStatus::Aborted;
        state.updated_at = Some(Utc::now());
        self.store.save(&state)?;
        Ok(self.step_for_state(state))
    }

    /// 返回当前阶段产物文件的完整路径。
    ///
    /// 用于 TUI/CLI 在阶段分析前创建文件占位，或供 agent 写入产物。
    pub fn phase_artifact_path(&self, phase: MissionPhase) -> PathBuf {
        let definition = phase_definition(phase);
        self.store.plans_dir().join(definition.artifact_filename)
    }

    /// 校验指定阶段的产物文件是否存在且非空。
    fn validate_phase_artifact(&self, phase: MissionPhase) -> MissionResult<()> {
        let definition = phase_definition(phase);
        let artifact_path = self.store.plans_dir().join(definition.artifact_filename);

        if !artifact_path.exists() {
            return Err(MissionError::PhaseArtifactMissing {
                phase: phase.to_string(),
                path: artifact_path,
            });
        }

        let content = std::fs::read_to_string(&artifact_path).unwrap_or_default();
        if content.trim().is_empty() {
            return Err(MissionError::PhaseArtifactEmpty {
                phase: phase.to_string(),
                path: artifact_path,
            });
        }

        Ok(())
    }

    pub fn load_execution_plan(&self) -> MissionResult<String> {
        let plan_path = self.store.plans_dir().join("plan.md");
        if plan_path.exists() {
            return std::fs::read_to_string(&plan_path).map_err(|source| MissionError::ReadPlan {
                path: plan_path,
                source,
            });
        }
        let fallback = self.store.plans_dir().join("worker_definition.md");
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

    /// 测试辅助：为指定阶段创建非空产物文件。
    fn create_phase_artifact(planner: &MissionPlanner, phase: MissionPhase, content: &str) {
        let path = planner.phase_artifact_path(phase);
        let dir = path.parent().unwrap();
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(&path, content).unwrap();
    }

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
    fn continue_requires_artifact_to_advance() -> anyhow::Result<()> {
        let workspace = TempDir::new()?;
        let planner = MissionPlanner::for_workspace(workspace.path());
        planner.start("ship it")?;

        // 没有产物文件 → 推进应该失败
        let result = planner.continue_planning(Some("intent ok".to_string()));
        assert!(result.is_err());
        match result.unwrap_err() {
            MissionError::PhaseArtifactMissing { phase, .. } => {
                assert_eq!(phase, "intent");
            }
            other => panic!("expected PhaseArtifactMissing, got {other:?}"),
        }

        // 创建产物文件后可以推进
        create_phase_artifact(&planner, MissionPhase::Intent, "目标说明...");

        let step = planner.continue_planning(Some("intent ok".to_string()))?;
        assert_eq!(step.state.phase, Some(MissionPhase::Context));
        assert_eq!(step.state.completed_phases.len(), 1);
        Ok(())
    }

    #[test]
    fn empty_artifact_rejected() -> anyhow::Result<()> {
        let workspace = TempDir::new()?;
        let planner = MissionPlanner::for_workspace(workspace.path());
        planner.start("ship it")?;

        // 创建空产物文件 → 应该失败
        create_phase_artifact(&planner, MissionPhase::Intent, "   ");

        let result = planner.continue_planning(None);
        assert!(result.is_err());
        match result.unwrap_err() {
            MissionError::PhaseArtifactEmpty { phase, .. } => {
                assert_eq!(phase, "intent");
            }
            other => panic!("expected PhaseArtifactEmpty, got {other:?}"),
        }
        Ok(())
    }

    #[test]
    fn continue_advances_through_all_phases_with_artifacts() -> anyhow::Result<()> {
        let workspace = TempDir::new()?;
        let planner = MissionPlanner::for_workspace(workspace.path());
        planner.start("ship it")?;

        for phase in MissionPhase::ALL {
            create_phase_artifact(&planner, phase, &format!("{} 分析内容", phase.label()));
            let step = planner.continue_planning(None)?;
            if phase == MissionPhase::Verification {
                assert_eq!(step.state.status, MissionStatus::Executing);
                assert_eq!(step.state.phase, None);
                assert_eq!(step.state.completed_phases.len(), 7);
            }
        }
        Ok(())
    }

    #[test]
    fn phase_artifact_path_returns_correct_path() -> anyhow::Result<()> {
        let workspace = TempDir::new()?;
        let planner = MissionPlanner::for_workspace(workspace.path());

        let path = planner.phase_artifact_path(MissionPhase::Intent);
        assert!(path.to_string_lossy().contains("intent.md"));
        assert!(path.to_string_lossy().contains(".agents/mission"));
        Ok(())
    }

    #[test]
    fn pause_and_resume() -> anyhow::Result<()> {
        let workspace = TempDir::new()?;
        let planner = MissionPlanner::for_workspace(workspace.path());
        planner.start("ship it")?;
        create_phase_artifact(&planner, MissionPhase::Intent, "content");
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
        create_phase_artifact(&planner, MissionPhase::Intent, "intent analysis");

        let step = planner.continue_planning(Some("confirmed".to_string()))?;
        let record = &step.state.completed_phases[0];
        assert_eq!(record.phase, MissionPhase::Intent);
        assert_eq!(record.note, "confirmed");
        assert!(record.completed_at.is_some());
        assert_eq!(record.artifact_path.as_deref(), Some("intent.md"));
        Ok(())
    }
}
