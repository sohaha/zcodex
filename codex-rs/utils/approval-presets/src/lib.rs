use codex_protocol::protocol::AskForApproval;
use codex_protocol::protocol::SandboxPolicy;

/// A simple preset pairing an approval policy with a sandbox policy.
#[derive(Debug, Clone)]
pub struct ApprovalPreset {
    /// Stable identifier for the preset.
    pub id: &'static str,
    /// Display label shown in UIs.
    pub label: &'static str,
    /// Short human description shown next to the label in UIs.
    pub description: &'static str,
    /// Approval policy to apply.
    pub approval: AskForApproval,
    /// Sandbox policy to apply.
    pub sandbox: SandboxPolicy,
}

/// Built-in list of approval presets that pair approval and sandbox policy.
///
/// Keep this UI-agnostic so it can be reused by both TUI and MCP server.
pub fn builtin_approval_presets() -> Vec<ApprovalPreset> {
    vec![
        ApprovalPreset {
            id: "read-only",
            label: "只读",
            description: "Codex 可以读取当前工作区中的文件。编辑文件或访问互联网前需要审批。",
            approval: AskForApproval::OnRequest,
            sandbox: SandboxPolicy::new_read_only_policy(),
        },
        ApprovalPreset {
            id: "auto",
            label: "默认",
            description: "Codex 可以读取和编辑当前工作区中的文件，也可以运行命令。访问互联网或编辑其他文件前需要审批。（与 Agent 模式相同）",
            approval: AskForApproval::OnRequest,
            sandbox: SandboxPolicy::new_workspace_write_policy(),
        },
        ApprovalPreset {
            id: "full-access",
            label: "完全访问",
            description: "Codex 可以编辑当前工作区之外的文件，并可在无需审批的情况下访问互联网。使用时请谨慎。",
            approval: AskForApproval::Never,
            sandbox: SandboxPolicy::DangerFullAccess,
        },
    ]
}
