//! `/zmission` 斜杠命令的子命令定义与解析。

/// `/zmission` 的子命令。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Command {
    /// 启动新 Mission 规划流程。
    Start { goal: Option<String> },
    /// 显示当前 Mission 状态。
    Status,
    /// 继续推进当前 Mission 规划阶段。
    Continue { note: Option<String> },
}

pub(crate) const MODE_NAME: &str = "ZMission";
pub(crate) const COMMAND_NAME: &str = "/zmission";

impl Command {
    /// 解析 `/zmission <args>` 参数字符串。
    pub(crate) fn parse(args: &str) -> Result<Self, String> {
        let trimmed = args.trim();
        if trimmed.is_empty() {
            return Ok(Self::Status);
        }

        let mut parts = trimmed.splitn(2, char::is_whitespace);
        let head = parts.next().unwrap_or("");

        match head.to_ascii_lowercase().as_str() {
            "start" => {
                let goal = parts
                    .next()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty());
                Ok(Self::Start { goal })
            }
            "status" => Ok(Self::Status),
            "continue" => {
                let note = parts
                    .next()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty());
                Ok(Self::Continue { note })
            }
            other => Err(format!("未知的 zmission 子命令: {other}")),
        }
    }
}

/// 判断 `/zmission` 命令在 task 进行中是否可用（根据子命令）。
pub(crate) fn entry_available_during_task(args: Option<&str>) -> bool {
    // 默认（无参数 = status）在 task 中可用
    let Some(args) = args else {
        return true;
    };
    let trimmed = args.trim();
    if trimmed.is_empty() {
        return true;
    }
    // 只有 status 在 task 进行中可用
    matches!(
        trimmed
            .split_whitespace()
            .next()
            .map(|s| s.to_ascii_lowercase())
            .as_deref(),
        Some("status")
    )
}
