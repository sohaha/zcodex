use crate::MissionError;
use crate::MissionPhase;
use crate::MissionPhaseRecord;
use crate::MissionResult;
use crate::MissionState;
use crate::MissionStateStore;
use crate::MissionStatus;
use crate::phases::MissionPhaseDefinition;
use crate::phases::phase_definition;
use crate::state::AGENTS_MISSION_DIR_NAME;
use std::path::Path;
use std::path::PathBuf;

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

        let state = MissionState {
            goal: goal.to_string(),
            status: MissionStatus::Planning,
            phase: Some(MissionPhase::Intent),
            completed_phases: Vec::new(),
        };
        self.store.save(&state)?;
        Ok(self.step_for_state(state))
    }

    pub fn continue_planning(&self, note: Option<String>) -> MissionResult<MissionPlanningStep> {
        let mut state = self
            .store
            .load()?
            .ok_or_else(|| MissionError::MissingState {
                path: self.store.state_path().to_path_buf(),
            })?;
        if state.status != MissionStatus::Planning {
            return Ok(self.step_for_state(state));
        }

        let Some(current_phase) = state.phase else {
            state.status = MissionStatus::Executing;
            self.store.save(&state)?;
            return Ok(self.step_for_state(state));
        };

        state.completed_phases.push(MissionPhaseRecord {
            phase: current_phase,
            note: normalized_note(note, current_phase),
        });
        state.phase = current_phase.next();
        if state.phase.is_none() {
            state.status = MissionStatus::Executing;
        }
        self.store.save(&state)?;
        Ok(self.step_for_state(state))
    }

    /// 方案存储目录（`<workspace>/.agents/mission/`）。
    pub fn plans_dir(&self) -> PathBuf {
        // state_path = <workspace>/.mission/mission_state.json
        // 方案目录 = <workspace>/.agents/mission/
        let workspace = self
            .store
            .state_path()
            .parent()
            .and_then(|p| p.parent())
            .unwrap_or(self.store.state_path());
        workspace.join(AGENTS_MISSION_DIR_NAME)
    }

    /// 保存规划阶段的产物到方案目录。
    ///
    /// 文件名为 `{phase}.md`，例如 `intent.md`、`plan.md`。
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
        // 优先加载 plan.md
        let plan_path = self.plans_dir().join("plan.md");
        if plan_path.exists() {
            return std::fs::read_to_string(&plan_path).map_err(|source| MissionError::ReadPlan {
                path: plan_path,
                source,
            });
        }
        // 回退到 worker_definition.md
        let fallback = self.plans_dir().join("worker_definition.md");
        if fallback.exists() {
            return std::fs::read_to_string(&fallback).map_err(|source| MissionError::ReadPlan {
                path: fallback,
                source,
            });
        }
        Err(MissionError::NoPlanToExecute)
    }

    fn step_for_state(&self, state: MissionState) -> MissionPlanningStep {
        MissionPlanningStep {
            definition: state.phase.map(phase_definition).copied(),
            state,
        }
    }
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
        for _ in 0..6 {
            step = planner.continue_planning(None)?;
        }

        assert_eq!(step.state.status, MissionStatus::Executing);
        assert_eq!(step.state.phase, None);
        assert_eq!(step.state.completed_phases.len(), 7);
        Ok(())
    }
}
