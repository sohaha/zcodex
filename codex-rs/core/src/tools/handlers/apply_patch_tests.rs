use super::*;
use codex_apply_patch::MaybeApplyPatchVerified;
use pretty_assertions::assert_eq;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

#[test]
fn approval_keys_include_move_destination() {
    let tmp = TempDir::new().expect("tmp");
    let cwd = tmp.path();
    std::fs::create_dir_all(cwd.join("old")).expect("create old dir");
    std::fs::create_dir_all(cwd.join("renamed/dir")).expect("create dest dir");
    std::fs::write(cwd.join("old/name.txt"), "old content\n").expect("write old file");
    let patch = r#"*** Begin Patch
*** Update File: old/name.txt
*** Move to: renamed/dir/name.txt
@@
-old content
+new content
*** End Patch"#;
    let argv = vec!["apply_patch".to_string(), patch.to_string()];
    let action = match codex_apply_patch::maybe_parse_apply_patch_verified(&argv, cwd) {
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

fn parse_action(cwd: &Path, patch: &str) -> ApplyPatchAction {
    let argv = vec!["apply_patch".to_string(), patch.to_string()];
    match codex_apply_patch::maybe_parse_apply_patch_verified(&argv, cwd) {
        MaybeApplyPatchVerified::Body(action) => action,
        other => panic!("expected patch body, got: {other:?}"),
    }
}

#[tokio::test]
async fn run_apply_patch_in_process_preserves_relative_paths_in_output() {
    let tmp = TempDir::new().expect("tmp");
    let action = parse_action(
        tmp.path(),
        r#"*** Begin Patch
*** Add File: nested/new.txt
+hello
*** End Patch"#,
    );

    let output = run_apply_patch_in_process(&action)
        .await
        .expect("apply patch should run");

    assert_eq!(output.exit_code, 0);
    assert_eq!(
        fs::read_to_string(tmp.path().join("nested/new.txt")).expect("read created file"),
        "hello\n"
    );
    assert!(output.stdout.text.contains("A nested/new.txt"));
    assert!(
        !output
            .stdout
            .text
            .contains(&tmp.path().display().to_string())
    );
}

#[tokio::test]
async fn run_apply_patch_in_process_revalidates_file_before_applying() {
    let tmp = TempDir::new().expect("tmp");
    let target = tmp.path().join("target.txt");
    fs::write(&target, "before\n").expect("write target");
    let action = parse_action(
        tmp.path(),
        r#"*** Begin Patch
*** Update File: target.txt
@@
-before
+after
*** End Patch"#,
    );

    fs::write(&target, "drifted\n").expect("mutate after verification");

    let output = run_apply_patch_in_process(&action)
        .await
        .expect("apply patch should run");

    assert_eq!(output.exit_code, 1);
    assert!(output.aggregated_output.text.contains("target.txt"));
    assert_eq!(
        fs::read_to_string(&target).expect("read drifted file"),
        "drifted\n"
    );
}

#[tokio::test]
async fn run_apply_patch_in_process_only_rewrites_path_fields_in_errors() {
    let tmp = TempDir::new().expect("tmp");
    let target = tmp.path().join("target.txt");
    let original_line = target.display().to_string();
    fs::write(&target, format!("{original_line}\n")).expect("write target");
    let action = parse_action(
        tmp.path(),
        &format!(
            "*** Begin Patch\n*** Update File: target.txt\n@@\n-{original_line}\n+after\n*** End Patch"
        ),
    );

    fs::write(&target, "drifted\n").expect("mutate after verification");

    let output = run_apply_patch_in_process(&action)
        .await
        .expect("apply patch should run");

    assert_eq!(output.exit_code, 1);
    assert!(output.aggregated_output.text.contains(&format!(
        "Failed to find expected lines in target.txt:\n{original_line}"
    )));
}

#[tokio::test]
async fn run_apply_patch_in_process_handles_multiple_operations_for_same_path() {
    let tmp = TempDir::new().expect("tmp");
    let target = tmp.path().join("target.txt");
    fs::write(&target, "old\n").expect("write target");
    let action = parse_action(
        tmp.path(),
        r#"*** Begin Patch
*** Delete File: target.txt
*** Update File: target.txt
@@
-old
+new
*** End Patch"#,
    );

    let output = run_apply_patch_in_process(&action)
        .await
        .expect("apply patch should run");

    assert_eq!(output.exit_code, 1);
    assert!(output.aggregated_output.text.contains("target.txt"));
    assert!(!target.exists());
}
