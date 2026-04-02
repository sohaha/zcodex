use super::*;
use codex_protocol::config_types::WindowsSandboxLevel;
use codex_protocol::permissions::FileSystemSandboxPolicy;
use codex_protocol::permissions::NetworkSandboxPolicy;
use codex_protocol::protocol::GranularApprovalConfig;
use codex_protocol::protocol::SandboxPolicy;
use codex_sandboxing::SandboxManager;
use codex_sandboxing::SandboxTransformRequest;
use codex_sandboxing::SandboxType;
use pretty_assertions::assert_eq;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;

#[test]
fn wants_no_sandbox_approval_granular_respects_sandbox_flag() {
    let runtime = ApplyPatchRuntime::new();
    assert!(runtime.wants_no_sandbox_approval(AskForApproval::OnRequest));
    assert!(
        !runtime.wants_no_sandbox_approval(AskForApproval::Granular(GranularApprovalConfig {
            sandbox_approval: false,
            rules: true,
            skill_approval: true,
            request_permissions: true,
            mcp_elicitations: true,
        }))
    );
    assert!(
        runtime.wants_no_sandbox_approval(AskForApproval::Granular(GranularApprovalConfig {
            sandbox_approval: true,
            rules: true,
            skill_approval: true,
            request_permissions: true,
            mcp_elicitations: true,
        }))
    );
}

#[test]
fn guardian_review_request_includes_patch_context() {
    let path = std::env::temp_dir().join("guardian-apply-patch-test.txt");
    let action = ApplyPatchAction::new_add_for_test(&path, "hello".to_string());
    let expected_cwd = action.cwd.clone();
    let expected_patch = action.patch.clone();
    let request = ApplyPatchRequest {
        action,
        file_paths: vec![
            AbsolutePathBuf::from_absolute_path(&path).expect("temp path should be absolute"),
        ],
        changes: HashMap::from([(
            path,
            FileChange::Add {
                content: "hello".to_string(),
            },
        )]),
        exec_approval_requirement: ExecApprovalRequirement::NeedsApproval {
            reason: None,
            proposed_execpolicy_amendment: None,
        },
        additional_permissions: None,
        permissions_preapproved: false,
        timeout_ms: None,
    };

    let guardian_request = ApplyPatchRuntime::build_guardian_review_request(&request, "call-1");

    assert_eq!(
        guardian_request,
        GuardianApprovalRequest::ApplyPatch {
            id: "call-1".to_string(),
            cwd: expected_cwd,
            files: request.file_paths,
            patch: expected_patch,
        }
    );
}

#[cfg(target_os = "linux")]
#[test]
fn build_command_spec_keeps_linux_sandbox_separator_before_apply_patch_flag() {
    let path = std::env::temp_dir().join("apply-patch-separator-test.txt");
    let action = ApplyPatchAction::new_add_for_test(&path, "hello".to_string());
    let request = ApplyPatchRequest {
        action,
        file_paths: vec![],
        changes: HashMap::new(),
        exec_approval_requirement: ExecApprovalRequirement::Skip {
            bypass_sandbox: false,
            proposed_execpolicy_amendment: None,
        },
        additional_permissions: None,
        permissions_preapproved: true,
        timeout_ms: None,
    };

    let codex_self_exe = PathBuf::from("/tmp/codex");
    let spec = ApplyPatchRuntime::build_sandbox_command(&request, Some(&codex_self_exe))
        .expect("build command spec");
    let manager = SandboxManager::new();
    let sandbox_policy = SandboxPolicy::new_read_only_policy();
    let codex_linux_sandbox_exe = PathBuf::from("/tmp/codex-linux-sandbox");
    let exec_request = manager
        .transform(SandboxTransformRequest {
            command: spec,
            policy: &sandbox_policy,
            file_system_policy: &FileSystemSandboxPolicy::from(&sandbox_policy),
            network_policy: NetworkSandboxPolicy::Restricted,
            sandbox: SandboxType::LinuxSeccomp,
            enforce_managed_network: false,
            network: None,
            sandbox_policy_cwd: Path::new("/tmp"),
            codex_linux_sandbox_exe: Some(&codex_linux_sandbox_exe),
            use_legacy_landlock: false,
            windows_sandbox_level: WindowsSandboxLevel::Disabled,
            windows_sandbox_private_desktop: false,
        })
        .expect("transform");

    let separator = exec_request
        .command
        .iter()
        .position(|arg| arg == "--")
        .expect("linux sandbox separator");
    assert_eq!(
        exec_request.command[separator + 2],
        "--codex-run-as-apply-patch".to_string()
    );
}
