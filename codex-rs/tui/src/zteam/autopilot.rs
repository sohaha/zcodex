use super::Mission;
use super::MissionPhase;
use super::WorkerSlot;
use super::preview;
use super::worker_list;

pub(crate) const AUTOPILOT_RESULT_MARKER: &str = "ZTEAM_AUTOPILOT";
pub(crate) const MAX_REPAIR_ATTEMPTS: u8 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AutoAction {
    BootstrapWorkers,
    PlanCycle,
    DispatchCycle,
    SummarizeResults,
    RepairWorkers,
    CompleteMission,
}

impl AutoAction {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::BootstrapWorkers => "bootstrap_workers",
            Self::PlanCycle => "plan_cycle",
            Self::DispatchCycle => "dispatch_cycle",
            Self::SummarizeResults => "summarize_results",
            Self::RepairWorkers => "repair_workers",
            Self::CompleteMission => "complete_mission",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) enum WaitingOn {
    #[default]
    Idle,
    RootTurn,
    Workers(Vec<WorkerSlot>),
    Results(Vec<WorkerSlot>),
    Repair(Vec<WorkerSlot>),
}

impl WaitingOn {
    pub(crate) fn summary(&self) -> String {
        match self {
            Self::Idle => "none".to_string(),
            Self::RootTurn => "root".to_string(),
            Self::Workers(workers) => {
                format!("workers:{}", csv_workers(workers))
            }
            Self::Results(workers) => {
                format!("results:{}", csv_workers(workers))
            }
            Self::Repair(workers) => {
                format!("repair:{}", csv_workers(workers))
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct RepairAttempts {
    pub(crate) frontend: u8,
    pub(crate) backend: u8,
}

impl RepairAttempts {
    pub(crate) fn count(&self, worker: WorkerSlot) -> u8 {
        match worker {
            WorkerSlot::Frontend => self.frontend,
            WorkerSlot::Backend => self.backend,
        }
    }

    pub(crate) fn increment(&mut self, worker: WorkerSlot) {
        match worker {
            WorkerSlot::Frontend => self.frontend = self.frontend.saturating_add(1),
            WorkerSlot::Backend => self.backend = self.backend.saturating_add(1),
        }
    }

    pub(crate) fn summary(&self) -> String {
        format!(
            "F:{frontend}/{max} B:{backend}/{max}",
            frontend = self.frontend,
            backend = self.backend,
            max = MAX_REPAIR_ATTEMPTS,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AutopilotState {
    pub(crate) current_cycle: u32,
    pub(crate) pending_auto_action: Option<AutoAction>,
    pub(crate) waiting_on: WaitingOn,
    pub(crate) last_auto_action_result: Option<String>,
    pub(crate) repair_attempts: RepairAttempts,
    pub(crate) manual_override_active: bool,
    pub(crate) queued_auto_action: Option<AutoAction>,
    pub(crate) cycle_planned: bool,
    pub(crate) cycle_dispatched: bool,
    pub(crate) attach_attempted: bool,
    pub(crate) parsed_result: Option<ParsedAutopilotResult>,
}

impl Default for AutopilotState {
    fn default() -> Self {
        Self {
            current_cycle: 1,
            pending_auto_action: None,
            waiting_on: WaitingOn::Idle,
            last_auto_action_result: None,
            repair_attempts: RepairAttempts::default(),
            manual_override_active: false,
            queued_auto_action: None,
            cycle_planned: false,
            cycle_dispatched: false,
            attach_attempted: false,
            parsed_result: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParsedAutopilotResult {
    pub(crate) action: AutoAction,
    pub(crate) status: String,
    pub(crate) cycle: u32,
    pub(crate) waiting_on: Vec<WorkerSlot>,
    pub(crate) summary: String,
}

pub(crate) fn parse_result_marker(text: &str) -> Option<ParsedAutopilotResult> {
    let line = text
        .lines()
        .rev()
        .find(|line| line.trim_start().starts_with(AUTOPILOT_RESULT_MARKER))?;
    let mut action = None;
    let mut status = None;
    let mut cycle = None;
    let mut waiting_on = Vec::new();
    let mut summary = None;
    for part in line.trim().split('|').skip(1) {
        let Some((key, value)) = part.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "action" => action = parse_action(value),
            "status" => status = Some(value.to_string()),
            "cycle" => cycle = value.parse::<u32>().ok(),
            "waiting_on" => waiting_on = parse_waiting_workers(value),
            "summary" => summary = Some(value.to_string()),
            _ => {}
        }
    }
    Some(ParsedAutopilotResult {
        action: action?,
        status: status?,
        cycle: cycle.unwrap_or(1),
        waiting_on,
        summary: summary.unwrap_or_else(|| "主线程已返回 autopilot 状态。".to_string()),
    })
}

pub(crate) fn plan_cycle_prompt(
    mission: &Mission,
    cycle: u32,
    required_workers: &[WorkerSlot],
    manual_override_active: bool,
) -> String {
    format!(
        concat!(
            "ZTeam Mission Autopilot 正在规划第 {cycle} 轮协作。\n",
            "当前目标：{goal}\n",
            "当前阶段：{phase}\n",
            "本轮需要参与的 worker：{workers}\n",
            "manual override：{manual_override}\n",
            "请只做一件事：根据现有 mission brief 判断这一轮该怎么推进，但不要开始派工、不要修改代码、不要关闭 worker。\n",
            "先用 1 到 2 句中文说明本轮策略，再在最后单独输出一行：\n",
            "{marker}|action=plan_cycle|status=planned|cycle={cycle}|waiting_on={waiting_on}|summary=<不超过40字中文，不含竖线>"
        ),
        cycle = cycle,
        goal = mission.goal,
        phase = mission_phase_label(mission.phase),
        workers = named_workers(required_workers),
        manual_override = if manual_override_active {
            "active"
        } else {
            "none"
        },
        marker = AUTOPILOT_RESULT_MARKER,
        waiting_on = csv_workers(required_workers),
    )
}

pub(crate) fn dispatch_cycle_prompt(
    mission: &Mission,
    cycle: u32,
    required_workers: &[WorkerSlot],
    manual_override_active: bool,
) -> String {
    format!(
        concat!(
            "ZTeam Mission Autopilot 正在执行第 {cycle} 轮自动派工。\n",
            "当前目标：{goal}\n",
            "当前阶段：{phase}\n",
            "本轮需要参与的 worker：{workers}\n",
            "manual override：{manual_override}\n",
            "请只做一件事：基于当前 mission brief 向需要参与的 worker 发出本轮任务，优先使用 `followup_task` 或 `send_message`；不要重建 worker，不要直接结束 mission。\n",
            "完成后先用 1 句中文说明派工已发出，再在最后单独输出一行：\n",
            "{marker}|action=dispatch_cycle|status=scheduled|cycle={cycle}|waiting_on={waiting_on}|summary=<不超过40字中文，不含竖线>"
        ),
        cycle = cycle,
        goal = mission.goal,
        phase = mission_phase_label(mission.phase),
        workers = named_workers(required_workers),
        manual_override = if manual_override_active {
            "active"
        } else {
            "none"
        },
        marker = AUTOPILOT_RESULT_MARKER,
        waiting_on = csv_workers(required_workers),
    )
}

pub(crate) fn summarize_results_prompt(
    mission: &Mission,
    cycle: u32,
    required_workers: &[WorkerSlot],
    result_summaries: &[String],
    manual_override_active: bool,
) -> String {
    let result_block = if result_summaries.is_empty() {
        "暂无结果摘要。".to_string()
    } else {
        result_summaries.join("\n")
    };
    format!(
        concat!(
            "ZTeam Mission Autopilot 需要归纳第 {cycle} 轮结果并决定下一步。\n",
            "当前目标：{goal}\n",
            "当前阶段：{phase}\n",
            "本轮参与 worker：{workers}\n",
            "manual override：{manual_override}\n",
            "最近结果摘要：\n",
            "{result_block}\n",
            "请只做归纳和判定，不要直接派工或重建 worker。\n",
            "状态必须四选一：continue / complete / repair / blocked。\n",
            "若 repair，waiting_on 填需要优先恢复或重建的 worker；若 continue，waiting_on 填下一轮预计参与的 worker；若 complete 或 blocked，waiting_on 写 none。\n",
            "先用 1 到 2 句中文说明判定理由，再在最后单独输出一行：\n",
            "{marker}|action=summarize_results|status=<continue|complete|repair|blocked>|cycle={next_cycle}|waiting_on=<frontend,backend 或 none>|summary=<不超过40字中文，不含竖线>"
        ),
        cycle = cycle,
        goal = mission.goal,
        phase = mission_phase_label(mission.phase),
        workers = named_workers(required_workers),
        manual_override = if manual_override_active {
            "active"
        } else {
            "none"
        },
        result_block = result_block,
        marker = AUTOPILOT_RESULT_MARKER,
        next_cycle = cycle.saturating_add(1),
    )
}

pub(crate) fn repair_workers_prompt(
    mission: &Mission,
    cycle: u32,
    missing_workers: &[WorkerSlot],
    repair_attempts: &RepairAttempts,
) -> String {
    format!(
        concat!(
            "ZTeam Mission Autopilot 需要修复第 {cycle} 轮缺失的 worker。\n",
            "当前目标：{goal}\n",
            "当前阶段：{phase}\n",
            "缺失 worker：{workers}\n",
            "当前 repair attempts：{attempts}\n",
            "请只做一件事：重建缺失的固定 worker。使用既有 task_name 和 agent_type，对新 worker 说明它们是长期协作者，恢复后保持待命；不要改写业务目标，不要自行结束 mission。\n",
            "完成后先用 1 句中文说明 repair 已发出，再在最后单独输出一行：\n",
            "{marker}|action=repair_workers|status=scheduled|cycle={cycle}|waiting_on={waiting_on}|summary=<不超过40字中文，不含竖线>"
        ),
        cycle = cycle,
        goal = mission.goal,
        phase = mission_phase_label(mission.phase),
        workers = named_workers(missing_workers),
        attempts = repair_attempts.summary(),
        marker = AUTOPILOT_RESULT_MARKER,
        waiting_on = csv_workers(missing_workers),
    )
}

pub(crate) fn complete_mission_prompt(mission: &Mission, cycle: u32) -> String {
    format!(
        concat!(
            "ZTeam Mission Autopilot 需要收口当前 mission。\n",
            "当前目标：{goal}\n",
            "当前阶段：{phase}\n",
            "当前 cycle：{cycle}\n",
            "请只做收口：用一小段中文总结 mission 已完成的结果和当前停留点，不要再发工具调用，也不要继续派工。\n",
            "最后单独输出一行：\n",
            "{marker}|action=complete_mission|status=done|cycle={cycle}|waiting_on=none|summary=<不超过40字中文，不含竖线>"
        ),
        cycle = cycle,
        goal = mission.goal,
        phase = mission_phase_label(mission.phase),
        marker = AUTOPILOT_RESULT_MARKER,
    )
}

fn parse_action(value: &str) -> Option<AutoAction> {
    match value {
        "bootstrap_workers" => Some(AutoAction::BootstrapWorkers),
        "plan_cycle" => Some(AutoAction::PlanCycle),
        "dispatch_cycle" => Some(AutoAction::DispatchCycle),
        "summarize_results" => Some(AutoAction::SummarizeResults),
        "repair_workers" => Some(AutoAction::RepairWorkers),
        "complete_mission" => Some(AutoAction::CompleteMission),
        _ => None,
    }
}

fn parse_waiting_workers(value: &str) -> Vec<WorkerSlot> {
    if value.eq_ignore_ascii_case("none") || value.trim().is_empty() {
        return Vec::new();
    }
    value.split(',').filter_map(WorkerSlot::parse).collect()
}

fn mission_phase_label(phase: MissionPhase) -> &'static str {
    match phase {
        MissionPhase::Idle => "idle",
        MissionPhase::Bootstrapping => "bootstrapping",
        MissionPhase::Planning => "planning",
        MissionPhase::Executing => "executing",
        MissionPhase::Validating => "validating",
        MissionPhase::Blocked => "blocked",
        MissionPhase::Completed => "completed",
    }
}

fn csv_workers(workers: &[WorkerSlot]) -> String {
    if workers.is_empty() {
        return "none".to_string();
    }
    workers
        .iter()
        .map(|worker| worker.task_name())
        .collect::<Vec<_>>()
        .join(",")
}

fn named_workers(workers: &[WorkerSlot]) -> String {
    if workers.is_empty() {
        return "none".to_string();
    }
    let display = worker_list(workers);
    let canonical = workers
        .iter()
        .map(|worker| {
            format!(
                "{}({})",
                worker.display_name(),
                worker.canonical_task_name()
            )
        })
        .collect::<Vec<_>>()
        .join("、");
    format!("{display} / {canonical}")
}

pub(crate) fn result_preview(worker: WorkerSlot, text: &str) -> String {
    format!("{worker}：{}", preview(text))
}
