//! Standard type to use with the `--approval-mode` CLI option.

use clap::ValueEnum;

use codex_protocol::protocol::AskForApproval;

#[derive(Clone, Copy, Debug, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum ApprovalModeCliArg {
    /// 仅对“受信任”的命令（如 ls、cat、sed）免于向用户请求批准。
    /// 若模型提出的命令不在“受信任”集合中，则会升级给用户处理。
    Untrusted,

    /// 已弃用：运行所有命令时都不请求用户批准。
    /// 仅当命令执行失败时才请求批准，此时会升级给用户，以批准无沙箱执行。
    /// 交互式运行推荐使用 `on-request`，非交互式运行推荐使用 `never`。
    OnFailure,

    /// 由模型决定何时向用户请求批准。
    OnRequest,

    /// 从不请求用户批准。
    /// 执行失败会立即返回给模型。
    Never,
}

impl From<ApprovalModeCliArg> for AskForApproval {
    fn from(value: ApprovalModeCliArg) -> Self {
        match value {
            ApprovalModeCliArg::Untrusted => AskForApproval::UnlessTrusted,
            ApprovalModeCliArg::OnFailure => AskForApproval::OnFailure,
            ApprovalModeCliArg::OnRequest => AskForApproval::OnRequest,
            ApprovalModeCliArg::Never => AskForApproval::Never,
        }
    }
}
