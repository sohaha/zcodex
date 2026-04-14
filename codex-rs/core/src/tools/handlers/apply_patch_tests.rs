use super::*;
use codex_apply_patch::MaybeApplyPatchVerified;
use codex_exec_server::LOCAL_FS;
use codex_protocol::models::FileSystemPermissions;
use codex_protocol::models::PermissionProfile;
use codex_protocol::permissions::FileSystemSandboxPolicy;
use codex_protocol::protocol::SandboxPolicy;
use core_test_support::PathBufExt;
use core_test_support::PathExt;
use pretty_assertions::assert_eq;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

#[tokio::test]
async fn approval_keys_include_move_destination() {
    let tmp = TempDir::new().expect("tmp");
    let cwd_path = tmp.path();
    let cwd = cwd_path.abs();
    std::fs::create_dir_all(cwd_path.join("old")).expect("create old dir");
    std::fs::create_dir_all(cwd_path.join("renamed/dir")).expect("create dest dir");
    std::fs::write(cwd_path.join("old/name.txt"), "old content\n").expect("write old file");
    let patch = r#"*** Begin Patch
*** Update File: old/name.txt
*** Move to: renamed/dir/name.txt
@@
-old content
+new content
*** End Patch"#;
    let argv = vec!["apply_patch".to_string(), patch.to_string()];
    let action = match codex_apply_patch::maybe_parse_apply_patch_verified(
        &argv,
        &cwd,
        LOCAL_FS.as_ref(),
        /*sandbox*/ None,
    )
    .await
    {
        MaybeApplyPatchVerified::Body(action) => action,
        other => panic!("expected patch body, got: {other:?}"),
    };

    let keys = file_paths_for_action(&action);
    assert_eq!(keys.len(), 2);
}

#[test]
fn rewrite_apply_patch_output_handles_windows_drive_paths() {
    let rewrites = vec![("C:\\tmp\\target.txt".to_string(), "target.txt".to_string())];

    assert_eq!(
        rewrite_apply_patch_output(
            "Failed to read file to update C:\\tmp\\target.txt\n".to_string(),
            &rewrites,
        ),
        "Failed to read file to update target.txt\n"
    );
    assert_eq!(
        rewrite_apply_patch_output(
            "Failed to find expected lines in C:\\tmp\\target.txt:\nC:\\tmp\\target.txt\n"
                .to_string(),
            &rewrites,
        ),
        "Failed to find expected lines in target.txt:\nC:\\tmp\\target.txt\n"
    );
}

#[test]
fn write_permissions_for_paths_skip_dirs_already_writable_under_workspace_root() {
    let tmp = TempDir::new().expect("tmp");
    let cwd_path = tmp.path();
    let cwd = cwd_path.abs();
    let nested = cwd_path.join("nested");
    std::fs::create_dir_all(&nested).expect("create nested dir");
    let file_path = AbsolutePathBuf::try_from(nested.join("file.txt"))
        .expect("nested file path should be absolute");
    let sandbox_policy = FileSystemSandboxPolicy::from(&SandboxPolicy::WorkspaceWrite {
        writable_roots: vec![],
        read_only_access: Default::default(),
        network_access: false,
        exclude_tmpdir_env_var: true,
        exclude_slash_tmp: false,
    });

    let permissions = write_permissions_for_paths(&[file_path], &sandbox_policy, &cwd);

    assert_eq!(permissions, None);
}

#[test]
fn write_permissions_for_paths_keep_dirs_outside_workspace_root() {
    let tmp = TempDir::new().expect("tmp");
    let cwd = tmp.path().join("workspace");
    let outside = tmp.path().join("outside");
    std::fs::create_dir_all(&cwd).expect("create cwd");
    std::fs::create_dir_all(&outside).expect("create outside dir");
    let file_path = AbsolutePathBuf::try_from(outside.join("file.txt"))
        .expect("outside file path should be absolute");
    let cwd_abs = cwd.abs();
    let sandbox_policy = FileSystemSandboxPolicy::from(&SandboxPolicy::WorkspaceWrite {
        writable_roots: vec![],
        read_only_access: Default::default(),
        network_access: false,
        exclude_tmpdir_env_var: true,
        exclude_slash_tmp: true,
    });

    let permissions = write_permissions_for_paths(&[file_path], &sandbox_policy, &cwd_abs);
    let expected_outside =
        dunce::simplified(&outside.canonicalize().expect("canonicalize outside dir")).abs();

    assert_eq!(
        permissions,
        Some(PermissionProfile {
            file_system: Some(FileSystemPermissions {
                read: Some(vec![]),
                write: Some(vec![expected_outside]),
            }),
            ..Default::default()
        })
    );
}

async fn parse_action(cwd: &Path, patch: &str) -> ApplyPatchAction {
    let cwd = cwd
        .canonicalize()
        .ok()
        .and_then(|path| AbsolutePathBuf::try_from(path).ok())
        .expect("cwd");
    let argv = vec!["apply_patch".to_string(), patch.to_string()];
    let verified =
        codex_apply_patch::maybe_parse_apply_patch_verified(&argv, &cwd, LOCAL_FS.as_ref(), None)
            .await;
    match verified {
        MaybeApplyPatchVerified::Body(verified) => verified,
        other => panic!("expected patch body, got: {other:?}"),
    }
}

#[test]
fn does_not_rewrite_success_output() {
    let rewrites = vec![("/tmp/target.txt".to_string(), "target.txt".to_string())];
    let output = "Done. Updated the following files:\nM /tmp/target.txt\n".to_string();
    assert_eq!(
        rewrite_apply_patch_output(output.clone(), &rewrites),
        output
    );
}

#[tokio::test]
async fn file_paths_for_action_returns_source_and_destination_for_move() {
    let tmp = TempDir::new().expect("tmp");
    let old_path = tmp.path().join("old.txt");
    let new_path = tmp.path().join("new.txt");
    fs::write(&old_path, "old\n").expect("write old");
    let patch = format!(
        "*** Begin Patch\n*** Update File: {}\n*** Move to: {}\n@@\n-old\n+new\n*** End Patch",
        old_path.display(),
        new_path.display()
    );
    let action = parse_action(tmp.path(), &patch).await;

    let paths = file_paths_for_action(&action);
    assert_eq!(
        paths,
        vec![
            AbsolutePathBuf::try_from(old_path).expect("abs old path"),
            AbsolutePathBuf::try_from(new_path).expect("abs new path"),
        ]
    );
}
