use super::*;
use crate::codex::make_session_and_context;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::registry::ToolHandler;
use crate::turn_diff_tracker::TurnDiffTracker;
use pretty_assertions::assert_eq;
use std::ffi::OsString;
use std::path::Path;
use std::sync::Arc;
use tempfile::tempdir;

#[test]
fn build_read_args_serializes_optional_flags() {
    let args = build_command_args(
        RtkCommandKind::Read,
        r#"{"path":"README.md","level":"aggressive","max_lines":20,"line_numbers":true}"#,
    )
    .expect("read args should parse");

    assert_eq!(
        args,
        vec![
            OsString::from("read"),
            OsString::from("README.md"),
            OsString::from("--level"),
            OsString::from("aggressive"),
            OsString::from("--max-lines"),
            OsString::from("20"),
            OsString::from("--line-numbers"),
        ]
    );
}

#[test]
fn build_grep_args_rejects_empty_pattern() {
    let err = build_command_args(RtkCommandKind::Grep, r#"{"pattern":" ","path":"."}"#)
        .expect_err("blank pattern should be rejected");

    assert_eq!(err.to_string(), "pattern must not be empty".to_string());
}

#[test]
fn build_find_args_serializes_file_type() {
    let args = build_command_args(
        RtkCommandKind::Find,
        r#"{"pattern":"*.rs","path":"core/src","max_results":25,"file_type":"f"}"#,
    )
    .expect("find args should parse");

    assert_eq!(
        args,
        vec![
            OsString::from("find"),
            OsString::from("*.rs"),
            OsString::from("core/src"),
            OsString::from("--max"),
            OsString::from("25"),
            OsString::from("--file-type"),
            OsString::from("f"),
        ]
    );
}

#[test]
fn build_diff_args_requires_both_paths() {
    let err = build_command_args(RtkCommandKind::Diff, r#"{"left":"a","right":" "}"#)
        .expect_err("blank right path should be rejected");

    assert_eq!(
        err.to_string(),
        "left and right must not be empty".to_string()
    );
}

#[test]
fn build_summary_args_serializes_command() {
    let args = build_command_args(
        RtkCommandKind::Summary,
        r#"{"command":"cargo test -p codex-core"}"#,
    )
    .expect("summary args should parse");

    assert_eq!(
        args,
        vec![
            OsString::from("summary"),
            OsString::from("cargo test -p codex-core"),
        ]
    );
}

#[cfg(unix)]
#[test]
fn build_err_args_wraps_command_in_shell() {
    let args = build_command_args(
        RtkCommandKind::Err,
        r#"{"command":"cargo test -p codex-core"}"#,
    )
    .expect("err args should parse");

    assert_eq!(
        args,
        vec![
            OsString::from("err"),
            OsString::from("sh"),
            OsString::from("-c"),
            OsString::from("cargo test -p codex-core"),
        ]
    );
}

#[test]
fn current_executable_launcher_supports_codex_and_rtk_names() {
    let codex_path = Path::new(if cfg!(windows) {
        r"C:\bin\codex.exe"
    } else {
        "/usr/local/bin/codex"
    });
    let codex = current_executable_rtk_launcher(codex_path).expect("codex launcher");
    assert_eq!(codex.program, codex_path);
    assert_eq!(codex.prefix_args, vec![OsString::from("rtk")]);

    let rtk_path = Path::new(if cfg!(windows) {
        r"C:\bin\rtk.exe"
    } else {
        "/usr/local/bin/rtk"
    });
    let rtk = current_executable_rtk_launcher(rtk_path).expect("rtk launcher");
    assert_eq!(rtk.program, rtk_path);
    assert!(rtk.prefix_args.is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn run_rtk_command_captures_stdout() {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    let temp = tempdir().expect("tempdir");
    let script_path = temp.path().join("fake-rtk.sh");
    fs::write(
        &script_path,
        "#!/bin/sh\nprintf 'subcommand=%s path=%s' \"$1\" \"$2\"\n",
    )
    .expect("write fake rtk");
    let mut perms = fs::metadata(&script_path).expect("metadata").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script_path, perms).expect("chmod");

    let executable = RtkExecutable {
        program: script_path,
        prefix_args: vec![OsString::from("rtk")],
    };

    let output = run_rtk_command(
        RtkCommandKind::Read,
        &executable,
        &[OsString::from("read"), OsString::from("README.md")],
        temp.path(),
    )
    .await
    .expect("command should run");

    let success = output.success;
    assert_eq!(output.into_text(), "subcommand=rtk path=read".to_string());
    assert_eq!(success, Some(true));
}

#[cfg(unix)]
#[tokio::test]
async fn handler_override_runs_fake_rtk_binary() {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    let temp = tempdir().expect("tempdir");
    let script_path = temp.path().join("fake-rtk.sh");
    fs::write(
        &script_path,
        "#!/bin/sh\nprintf '%s|%s|%s' \"$1\" \"$2\" \"$3\"\n",
    )
    .expect("write fake rtk");
    let mut perms = fs::metadata(&script_path).expect("metadata").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script_path, perms).expect("chmod");

    let handler = RtkHandler::with_executable_override(
        RtkCommandKind::Diff,
        RtkExecutable {
            program: script_path,
            prefix_args: Vec::new(),
        },
    );
    let (session, turn) = make_session_and_context().await;
    let invocation = ToolInvocation {
        session: Arc::new(session),
        turn: Arc::new(turn),
        tracker: Arc::new(tokio::sync::Mutex::new(TurnDiffTracker::new())),
        tool_name: "rtk_diff".to_string(),
        tool_namespace: None,
        call_id: "call_1".to_string(),
        payload: ToolPayload::Function {
            arguments: r#"{"left":"a.txt","right":"b.txt"}"#.to_string(),
        },
    };

    let output = handler.handle(invocation).await.expect("handler output");
    let success = output.success;
    assert_eq!(output.into_text(), "diff|a.txt|b.txt".to_string());
    assert_eq!(success, Some(true));
}

#[cfg(unix)]
#[test]
fn grep_no_match_is_not_treated_as_failure() {
    let failed_status = std::process::Command::new("sh")
        .arg("-c")
        .arg("exit 1")
        .status()
        .expect("status");

    assert!(rtk_command_succeeded(
        RtkCommandKind::Grep,
        &failed_status,
        "🔍 0 for 'needle'\n"
    ));
    assert!(!rtk_command_succeeded(
        RtkCommandKind::Read,
        &failed_status,
        "🔍 0 for 'needle'\n"
    ));

    assert!(rtk_command_succeeded(
        RtkCommandKind::Summary,
        &failed_status,
        "✅ Command: cargo test\n"
    ));

    let successful_status = std::process::Command::new("sh")
        .arg("-c")
        .arg("exit 0")
        .status()
        .expect("status");

    assert!(!rtk_command_succeeded(
        RtkCommandKind::Summary,
        &successful_status,
        "❌ Command: cargo test\n"
    ));
}
