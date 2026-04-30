use crate::MissionError;
use crate::MissionPhase;
use crate::MissionPhaseRecord;
use crate::MissionResult;
use crate::MissionState;
use crate::MissionStateStore;
use crate::MissionStatus;
use crate::phases::MissionPhaseDefinition;
use crate::phases::phase_definition;
use std::path::Path;

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
