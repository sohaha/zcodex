use codex_protocol::protocol::AgentStatus;
use codex_protocol::protocol::CodexErrorInfo;
use codex_protocol::protocol::ErrorEvent;
use codex_protocol::protocol::EventMsg;

/// Derive the next agent status from a single emitted event.
/// Returns `None` when the event does not affect status tracking.
pub(crate) fn agent_status_from_event(msg: &EventMsg) -> Option<AgentStatus> {
    match msg {
        EventMsg::TurnStarted(_) => Some(AgentStatus::Running),
        EventMsg::TurnComplete(ev) => Some(AgentStatus::Completed(ev.last_agent_message.clone())),
        EventMsg::TurnAborted(ev) => match ev.reason {
            codex_protocol::protocol::TurnAbortReason::Interrupted
            | codex_protocol::protocol::TurnAbortReason::BudgetLimited => {
                Some(AgentStatus::Interrupted)
            }
            _ => Some(AgentStatus::Errored(format!("{:?}", ev.reason))),
        },
        EventMsg::Error(ev) => Some(AgentStatus::Errored(clarify_agent_error_message(ev))),
        EventMsg::ShutdownComplete => Some(AgentStatus::Shutdown),
        _ => None,
    }
}

pub(crate) fn is_final(status: &AgentStatus) -> bool {
    !matches!(
        status,
        AgentStatus::PendingInit | AgentStatus::Running | AgentStatus::Interrupted
    )
}

fn clarify_agent_error_message(error: &ErrorEvent) -> String {
    let message = &error.message;
    let normalized = message.to_ascii_lowercase();
    let is_usage_limit = normalized.contains("usage limit");
    let is_retry_limit_429 = normalized.contains("exceeded retry limit")
        && (normalized.contains("429") || normalized.contains("too many requests"));
    let is_too_many_requests = normalized.contains("too many requests");
    let already_clarified = normalized.contains("rate limited (http 429)");
    let is_structured_rate_limit = match error.codex_error_info {
        Some(CodexErrorInfo::UsageLimitExceeded)
        | Some(CodexErrorInfo::ResponseTooManyFailedAttempts {
            http_status_code: Some(429),
        }) => true,
        Some(_) => false,
        None => is_usage_limit || is_retry_limit_429 || is_too_many_requests,
    };

    if !already_clarified && is_structured_rate_limit {
        return format!("Rate limited (HTTP 429): {message}");
    }

    message.to_string()
}
