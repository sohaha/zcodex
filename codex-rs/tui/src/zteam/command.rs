use super::TeamConfig;
use super::WorkerSlot;
use codex_protocol::protocol::InterAgentCommunication;

pub(crate) const MODE_NAME: &str = "ZTeam";
pub(crate) const COMMAND_NAME: &str = "/zteam";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Command {
    Start {
        goal: Option<String>,
    },
    Status,
    Attach,
    Dispatch {
        worker: WorkerSlot,
        message: String,
    },
    Relay {
        from: WorkerSlot,
        to: WorkerSlot,
        message: String,
    },
}

impl Command {
    pub(crate) fn parse(args: &str) -> Result<Self, String> {
        let trimmed = args.trim();
        if trimmed.is_empty() {
            return Err(usage().to_string());
        }

        let mut parts = trimmed.split_whitespace();
        let Some(head) = parts.next() else {
            return Err(usage().to_string());
        };
        match head.to_ascii_lowercase().as_str() {
            "start" => {
                let goal = trimmed
                    .split_once(char::is_whitespace)
                    .map(|(_, goal)| sanitize_mission_goal(goal.trim()))
                    .transpose()?
                    .flatten();
                Ok(Self::Start { goal })
            }
            "status" => {
                if parts.next().is_some() {
                    return Err(usage().to_string());
                }
                Ok(Self::Status)
            }
            "attach" => {
                if parts.next().is_some() {
                    return Err(usage().to_string());
                }
                Ok(Self::Attach)
            }
            "relay" => {
                let Some((_, rest)) = trimmed.split_once(char::is_whitespace) else {
                    return Err(usage().to_string());
                };
                let rest = rest.trim_start();
                let Some((from_raw, rest)) = rest.split_once(char::is_whitespace) else {
                    return Err(usage().to_string());
                };
                let rest = rest.trim_start();
                let Some((to_raw, message)) = rest.split_once(char::is_whitespace) else {
                    return Err(usage().to_string());
                };
                let Some(message) = Some(message.trim()).filter(|message| !message.is_empty())
                else {
                    return Err(usage().to_string());
                };
                let Some(from) = WorkerSlot::parse(from_raw) else {
                    return Err(usage().to_string());
                };
                let Some(to) = WorkerSlot::parse(to_raw) else {
                    return Err(usage().to_string());
                };
                Ok(Self::Relay {
                    from,
                    to,
                    message: message.to_string(),
                })
            }
            other => {
                let Some(worker) = WorkerSlot::parse(other) else {
                    return Err(usage().to_string());
                };
                let Some(message) = trimmed
                    .split_once(char::is_whitespace)
                    .map(|(_, message)| message.trim())
                    .filter(|message| !message.is_empty())
                else {
                    return Err(usage().to_string());
                };
                Ok(Self::Dispatch {
                    worker,
                    message: message.to_string(),
                })
            }
        }
    }
}

pub(crate) fn entry_available_during_task(args: Option<&str>) -> bool {
    let Some(head) = args.and_then(|args| args.split_whitespace().next()) else {
        return true;
    };
    head.eq_ignore_ascii_case("status")
}

pub(crate) fn disabled_message() -> String {
    format!("{MODE_NAME} 已在当前 TUI 配置中关闭，{COMMAND_NAME} 不再可用。")
}

pub(crate) fn disabled_hint() -> &'static str {
    "在 `config.toml` 中设置 `[tui].zteam_enabled = true` 后可再次启用。"
}

pub(crate) fn usage() -> &'static str {
    "用法：/zteam start <目标> | /zteam start | /zteam status | /zteam attach | /zteam <frontend|backend> <任务> | /zteam relay <frontend|backend> <frontend|backend> <消息>"
}

pub(crate) fn start_prompt(goal: Option<&str>, config: &TeamConfig) -> String {
    let frontend_role = slot_role_name(WorkerSlot::Frontend, config);
    let backend_role = slot_role_name(WorkerSlot::Backend, config);
    match goal {
        Some(goal) => format!(
            concat!(
                "进入 ZTeam Mission 模式。当前目标：`{goal}`。\n",
                "立即使用 `spawn_agent` 创建两个长期 worker：\n",
                "1. `task_name = \"frontend\"`，`agent_type = \"{frontend_role}\"`\n",
                "2. `task_name = \"backend\"`，`agent_type = \"{backend_role}\"`\n",
                "对两个 worker 都说明：它们是长期协作者，主线程负责围绕当前目标拆分任务；需要彼此同步时优先使用 `send_message` 或 `followup_task`；完成阶段结果后继续待命，不要自行关闭。\n",
                "创建完成后，只用一条简短中文消息汇报两个 worker 的 canonical task name，并补一句你准备如何围绕当前目标组织第一轮协作。除非我下一条消息明确要求实现，否则不要开始业务修改。"
            ),
            goal = goal,
            frontend_role = frontend_role,
            backend_role = backend_role,
        ),
        None => format!(
            concat!(
                "进入 ZTeam 本地协作模式（兼容入口）。立即使用 `spawn_agent` 创建两个长期 worker：\n",
                "1. `task_name = \"frontend\"`，`agent_type = \"{frontend_role}\"`\n",
                "2. `task_name = \"backend\"`，`agent_type = \"{backend_role}\"`\n",
                "对两个 worker 都说明：它们是长期协作者，主线程负责拆分任务；需要彼此同步时优先使用 `send_message` 或 `followup_task`；完成阶段结果后继续待命，不要自行关闭。\n",
                "创建完成后，只用一条简短中文消息汇报两个 worker 的 canonical task name。除非我下一条消息明确分派任务，否则不要开始实现业务工作。"
            ),
            frontend_role = frontend_role,
            backend_role = backend_role,
        ),
    }
}

pub(crate) fn sanitize_mission_goal(goal: &str) -> Result<Option<String>, String> {
    let sanitized = InterAgentCommunication::sanitize_visible_text(goal);
    let sanitized = sanitized.trim();
    if sanitized.is_empty() {
        if goal.trim().is_empty() {
            return Ok(None);
        }
        return Err("`/zteam start <目标>` 中包含的内容在净化内部协作消息后为空；请直接输入面向任务的目标。".to_string());
    }
    Ok(Some(sanitized.to_string()))
}

pub(super) fn slot_role_name(slot: WorkerSlot, config: &TeamConfig) -> &str {
    let override_val = match slot {
        WorkerSlot::Frontend => config.frontend.role_name.as_deref(),
        WorkerSlot::Backend => config.backend.role_name.as_deref(),
    };
    override_val.unwrap_or_else(|| slot.role_name())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_start_with_goal() {
        let cmd = Command::parse("start 修复登录页").unwrap();
        assert_eq!(
            cmd,
            Command::Start {
                goal: Some("修复登录页".to_string())
            }
        );
    }

    #[test]
    fn parse_start_without_goal() {
        let cmd = Command::parse("start").unwrap();
        assert_eq!(cmd, Command::Start { goal: None });
    }

    #[test]
    fn parse_status() {
        assert_eq!(Command::parse("status").unwrap(), Command::Status);
    }

    #[test]
    fn parse_attach() {
        assert_eq!(Command::parse("attach").unwrap(), Command::Attach);
    }

    #[test]
    fn parse_dispatch() {
        let cmd = Command::parse("frontend 修复布局").unwrap();
        assert_eq!(
            cmd,
            Command::Dispatch {
                worker: WorkerSlot::Frontend,
                message: "修复布局".to_string(),
            }
        );
    }

    #[test]
    fn parse_relay() {
        let cmd = Command::parse("relay frontend backend 这是消息").unwrap();
        assert_eq!(
            cmd,
            Command::Relay {
                from: WorkerSlot::Frontend,
                to: WorkerSlot::Backend,
                message: "这是消息".to_string(),
            }
        );
    }

    #[test]
    fn parse_empty_returns_usage_error() {
        let err = Command::parse("").unwrap_err();
        assert!(err.contains("用法"));
    }

    #[test]
    fn parse_unknown_returns_usage_error() {
        let err = Command::parse("unknown arg").unwrap_err();
        assert!(err.contains("用法"));
    }

    #[test]
    fn entry_available_for_status_and_bare() {
        assert!(entry_available_during_task(None));
        assert!(entry_available_during_task(Some("")));
        assert!(entry_available_during_task(Some("status")));
        assert!(entry_available_during_task(Some("STATUS")));
        assert!(!entry_available_during_task(Some("start")));
        assert!(!entry_available_during_task(Some("frontend 修复布局")));
    }
}
