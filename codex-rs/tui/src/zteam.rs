pub(crate) const MODE_NAME: &str = "ZTeam";
pub(crate) const COMMAND_NAME: &str = "/zteam";

pub(crate) fn entry_message() -> String {
    format!(
        "{MODE_NAME} 入口已启用。当前阶段先冻结命名与配置开关，双 worker 编排和协作工作台会在后续 issue 中落地。"
    )
}

pub(crate) fn entry_hint() -> &'static str {
    "可用 `tui.zteam_enabled = false` 隐藏该入口。"
}

pub(crate) fn disabled_message() -> String {
    format!("{MODE_NAME} 已在当前 TUI 配置中关闭，{COMMAND_NAME} 不再可用。")
}

pub(crate) fn disabled_hint() -> &'static str {
    "在 `config.toml` 中设置 `[tui].zteam_enabled = true` 后可再次启用。"
}
