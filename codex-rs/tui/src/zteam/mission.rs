use super::TeamConfig;
use super::WorkerSlot;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Mission {
    pub(crate) goal: String,
    pub(crate) mode: MissionMode,
    pub(crate) phase: MissionPhase,
    pub(crate) acceptance_checks: Vec<AcceptanceCheck>,
    pub(crate) frontend_role: Option<String>,
    pub(crate) backend_role: Option<String>,
    pub(crate) frontend_assignment: Option<String>,
    pub(crate) backend_assignment: Option<String>,
    pub(crate) validation_summary: Option<String>,
    pub(crate) blocker: Option<String>,
    pub(crate) next_action: Option<String>,
    pub(crate) cycle: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MissionMode {
    Solo(WorkerSlot),
    Parallel,
    SerialHandoff,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MissionPhase {
    Bootstrapping,
    Planning,
    Executing,
    Validating,
    Blocked,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AcceptanceCheck {
    pub(crate) summary: String,
    pub(crate) status: AcceptanceStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AcceptanceStatus {
    Pending,
    Met,
    Failed,
}

impl MissionMode {
    pub(crate) fn label(&self) -> &'static str {
        match self {
            Self::Solo(WorkerSlot::Frontend) => "solo-frontend",
            Self::Solo(WorkerSlot::Backend) => "solo-backend",
            Self::Parallel => "parallel",
            Self::SerialHandoff => "serial-handoff",
            Self::Blocked => "blocked",
        }
    }
}

impl MissionPhase {
    pub(crate) fn label(&self) -> &'static str {
        match self {
            Self::Bootstrapping => "bootstrapping",
            Self::Planning => "planning",
            Self::Executing => "executing",
            Self::Validating => "validating",
            Self::Blocked => "blocked",
            Self::Completed => "completed",
        }
    }
}

impl fmt::Display for MissionMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

impl fmt::Display for MissionPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

impl AcceptanceStatus {
    pub(crate) fn label(&self) -> &'static str {
        match self {
            Self::Pending => "待验证",
            Self::Met => "已满足",
            Self::Failed => "受阻",
        }
    }
}

impl Mission {
    pub(crate) fn assignment_mut(&mut self, worker: WorkerSlot) -> &mut Option<String> {
        match worker {
            WorkerSlot::Frontend => &mut self.frontend_assignment,
            WorkerSlot::Backend => &mut self.backend_assignment,
        }
    }
}

pub(crate) fn preview(text: &str) -> String {
    const LIMIT: usize = 60;
    let trimmed = text.trim();
    if trimmed.chars().count() <= LIMIT {
        return trimmed.to_string();
    }
    let truncated: String = trimmed.chars().take(LIMIT).collect();
    format!("{truncated}...")
}

pub(crate) fn worker_list(workers: &[WorkerSlot]) -> String {
    workers
        .iter()
        .map(std::string::ToString::to_string)
        .collect::<Vec<_>>()
        .join("、")
}

pub(crate) fn worker_task_list(workers: &[WorkerSlot]) -> String {
    workers
        .iter()
        .map(std::string::ToString::to_string)
        .collect::<Vec<_>>()
        .join("、")
}

pub(crate) fn contains_any(goal: &str, keywords: &[&str]) -> bool {
    keywords.iter().any(|keyword| goal.contains(keyword))
}

pub(crate) fn default_acceptance_checks() -> Vec<AcceptanceCheck> {
    vec![
        AcceptanceCheck {
            summary: "目标已拆成可执行的协作分工".to_string(),
            status: AcceptanceStatus::Pending,
        },
        AcceptanceCheck {
            summary: "需要参与的协作者已回流阶段结果".to_string(),
            status: AcceptanceStatus::Pending,
        },
        AcceptanceCheck {
            summary: "主线程已形成验证结论或下一轮动作".to_string(),
            status: AcceptanceStatus::Pending,
        },
    ]
}

pub(crate) fn plan_mission(goal: &str, config: &TeamConfig) -> Mission {
    let goal = goal.trim().to_string();
    let mode = infer_mission_mode(&goal, config);
    let (frontend_role, backend_role, frontend_assignment, backend_assignment, next_action) =
        mission_assignments(&goal, &mode, config);
    let blocker = match mode {
        MissionMode::Blocked => Some("目标过于模糊，暂时无法规划可执行的协作路径。".to_string()),
        _ => None,
    };
    let phase = match mode {
        MissionMode::Blocked => MissionPhase::Blocked,
        _ => MissionPhase::Bootstrapping,
    };
    Mission {
        goal,
        mode,
        phase,
        acceptance_checks: default_acceptance_checks(),
        frontend_role,
        backend_role,
        frontend_assignment,
        backend_assignment,
        validation_summary: None,
        blocker,
        next_action: Some(next_action),
        cycle: 1,
    }
}

pub(crate) fn plan_manual_override_mission(
    worker: WorkerSlot,
    message: &str,
    config: &TeamConfig,
) -> Mission {
    let mode = MissionMode::Solo(worker);
    let goal = format!("手动分派：{}", preview(message));
    let (frontend_role, backend_role, frontend_assignment, backend_assignment, _) =
        mission_assignments(message, &mode, config);
    let mut mission = Mission {
        goal,
        mode,
        phase: MissionPhase::Executing,
        acceptance_checks: default_acceptance_checks(),
        frontend_role,
        backend_role,
        frontend_assignment,
        backend_assignment,
        validation_summary: Some(
            "当前由旧命令触发手动 override；本轮结果回流后需重新判断是否继续沿 mission 主路径推进。"
                .to_string(),
        ),
        blocker: None,
        next_action: Some("等待本轮手动分派结果回流，再决定是否回到 mission 主流程。".to_string()),
        cycle: 1,
    };
    mission.assignment_mut(worker).replace(preview(message));
    mission
}

pub(crate) fn plan_manual_relay_mission(
    from: WorkerSlot,
    to: WorkerSlot,
    message: &str,
    config: &TeamConfig,
) -> Mission {
    let goal = format!("手动协作同步：{}", preview(message));
    let (frontend_role, backend_role, frontend_assignment, backend_assignment, _) =
        mission_assignments(&goal, &MissionMode::Parallel, config);
    let mut mission = Mission {
        goal,
        mode: MissionMode::Parallel,
        phase: MissionPhase::Executing,
        acceptance_checks: default_acceptance_checks(),
        frontend_role,
        backend_role,
        frontend_assignment,
        backend_assignment,
        validation_summary: Some(
            "当前由旧命令触发手动 override；请在 relay 结果回流后确认是否回到 mission 主路径。"
                .to_string(),
        ),
        blocker: None,
        next_action: Some(
            "等待 relay 目标协作者回流结果，再确认是否恢复 mission 主流程。".to_string(),
        ),
        cycle: 1,
    };
    mission.assignment_mut(to).replace(preview(message));
    mission.assignment_mut(from).get_or_insert_with(|| {
        format!(
            "向 {} 转发协作事实：{}",
            to.display_name(),
            preview(message)
        )
    });
    mission
}

fn infer_mission_mode(goal: &str, config: &TeamConfig) -> MissionMode {
    let goal_lower = goal.to_ascii_lowercase();
    if goal.chars().count() <= 6
        || contains_any(
            goal,
            &[
                "看看",
                "分析一下",
                "讨论一下",
                "聊聊",
                "帮我想想",
                "怎么弄",
                "优化什么",
            ],
        )
    {
        return MissionMode::Blocked;
    }

    let default_frontend = [
        "前端",
        "页面",
        "布局",
        "交互",
        "移动端",
        "组件",
        "样式",
        "导航",
        "表单",
        "ui",
        "frontend",
        "css",
        "react",
        "vue",
        "html",
    ];
    let default_backend = [
        "后端",
        "接口",
        "服务",
        "数据库",
        "登录",
        "token",
        "schema",
        "错误码",
        "api",
        "sql",
        "backend",
        "server",
        "migration",
        "database",
    ];

    let frontend_keywords: Vec<&str> = default_frontend
        .iter()
        .copied()
        .chain(config.frontend.domain_keywords.iter().map(String::as_str))
        .collect();
    let backend_keywords: Vec<&str> = default_backend
        .iter()
        .copied()
        .chain(config.backend.domain_keywords.iter().map(String::as_str))
        .collect();

    let frontend_like = contains_any(goal, &frontend_keywords);
    let backend_like = contains_any(goal, &backend_keywords);

    if contains_any(goal, &["先", "再", "联调", "对齐", "契约", "字段"]) {
        return MissionMode::SerialHandoff;
    }
    match (frontend_like, backend_like) {
        (true, true) => MissionMode::Parallel,
        (true, false) => MissionMode::Solo(WorkerSlot::Frontend),
        (false, true) => MissionMode::Solo(WorkerSlot::Backend),
        (false, false)
            if goal_lower.contains("同时") || goal.contains("并") || goal.contains("以及") =>
        {
            MissionMode::Parallel
        }
        _ => MissionMode::Parallel,
    }
}

pub(crate) fn mission_assignments(
    goal: &str,
    mode: &MissionMode,
    config: &TeamConfig,
) -> (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    String,
) {
    let fe_name = slot_display_name(WorkerSlot::Frontend, config);
    let be_name = slot_display_name(WorkerSlot::Backend, config);
    match mode {
        MissionMode::Solo(WorkerSlot::Frontend) => (
            Some("主导 UI/交互侧推进".to_string()),
            Some("待命并准备接收后续协助".to_string()),
            Some(format!("围绕当前目标推进前端侧工作：{goal}")),
            None,
            format!("等待{fe_name}进入协作上下文，再决定是否需要服务侧协助"),
        ),
        MissionMode::Solo(WorkerSlot::Backend) => (
            Some("待命并准备接收后续协助".to_string()),
            Some("主导服务/数据侧推进".to_string()),
            None,
            Some(format!("围绕当前目标推进后端侧工作：{goal}")),
            format!("等待{be_name}进入协作上下文，再决定是否需要 UI 侧协助"),
        ),
        MissionMode::Parallel => (
            Some("负责 UI/交互侧推进".to_string()),
            Some("负责接口/数据侧推进".to_string()),
            Some(format!("拆解并推进前端侧工作：{goal}")),
            Some(format!("拆解并推进后端侧工作：{goal}")),
            "等待两个协作者都进入协作上下文，再开始首轮并行分派".to_string(),
        ),
        MissionMode::SerialHandoff => (
            Some("在契约稳定后承接 UI/交互落地".to_string()),
            Some("先稳定接口/字段/约束，再交接给前端".to_string()),
            Some(format!("等待后端先产出可消费的协作结果：{goal}")),
            Some(format!("先整理服务侧契约与约束：{goal}")),
            "先让后端明确契约，再决定前端接手时机".to_string(),
        ),
        MissionMode::Blocked => (
            None,
            None,
            None,
            None,
            "当前 mission 缺少可执行路径，等待主线程重新组织".to_string(),
        ),
    }
}

fn slot_display_name(slot: WorkerSlot, config: &TeamConfig) -> String {
    let override_val = match slot {
        WorkerSlot::Frontend => config.frontend.display_name.as_deref(),
        WorkerSlot::Backend => config.backend.display_name.as_deref(),
    };
    override_val
        .unwrap_or_else(|| slot.display_name())
        .to_string()
}
