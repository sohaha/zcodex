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
fn build_json_args_serializes_depth() {
    let args = build_command_args(RtkCommandKind::Json, r#"{"path":"package.json","depth":3}"#)
        .expect("json args should parse");

    assert_eq!(
        args,
        vec![
            OsString::from("json"),
            OsString::from("package.json"),
            OsString::from("--depth"),
            OsString::from("3"),
        ]
    );
}

#[test]
fn build_deps_args_defaults_to_current_directory() {
    let args = build_command_args(RtkCommandKind::Deps, "{}").expect("deps args should parse");

    assert_eq!(args, vec![OsString::from("deps"), OsString::from(".")]);
}

#[test]
fn build_log_args_requires_path() {
    let err = build_command_args(RtkCommandKind::Log, r#"{"path":" "}"#).expect_err("blank path");

    assert_eq!(err.to_string(), "path must not be empty".to_string());
}

#[test]
fn build_ls_args_serializes_all_flag() {
    let args = build_command_args(RtkCommandKind::Ls, r#"{"path":"src","all":true}"#)
        .expect("ls args should parse");

    assert_eq!(
        args,
        vec![
            OsString::from("ls"),
            OsString::from("--all"),
            OsString::from("src"),
        ]
    );
}

#[test]
fn build_tree_args_serializes_depth() {
    let args = build_command_args(
        RtkCommandKind::Tree,
        r#"{"path":"src","max_depth":2,"all":true}"#,
    )
    .expect("tree args should parse");

    assert_eq!(
        args,
        vec![
            OsString::from("tree"),
            OsString::from("--all"),
            OsString::from("-L"),
            OsString::from("2"),
            OsString::from("src"),
        ]
    );
}

#[test]
fn build_wc_args_serializes_mode() {
    let args = build_command_args(
        RtkCommandKind::Wc,
        r#"{"path":"Cargo.toml","mode":"lines"}"#,
    )
    .expect("wc args should parse");

    assert_eq!(
        args,
        vec![
            OsString::from("wc"),
            OsString::from("-l"),
            OsString::from("Cargo.toml"),
        ]
    );
}

#[test]
fn build_wc_args_rejects_unknown_mode() {
    let err = build_command_args(
        RtkCommandKind::Wc,
        r#"{"path":"Cargo.toml","mode":"unknown"}"#,
    )
    .expect_err("unknown mode");

    assert_eq!(
        err.to_string(),
        "mode must be one of: full, lines, words, bytes, chars".to_string()
    );
}

#[test]
fn build_git_status_args_serializes_pathspec() {
    let args = build_command_args(RtkCommandKind::GitStatus, r#"{"path":"core/src"}"#)
        .expect("git status args should parse");

    assert_eq!(
        args,
        vec![
            OsString::from("git"),
            OsString::from("status"),
            OsString::from("--"),
            OsString::from("core/src"),
        ]
    );
}

#[test]
fn build_git_diff_args_serializes_cached_target_and_path() {
    let args = build_command_args(
        RtkCommandKind::GitDiff,
        r#"{"target":"HEAD~1","path":"core/src","cached":true}"#,
    )
    .expect("git diff args should parse");

    assert_eq!(
        args,
        vec![
            OsString::from("git"),
            OsString::from("diff"),
            OsString::from("--cached"),
            OsString::from("HEAD~1"),
            OsString::from("--"),
            OsString::from("core/src"),
        ]
    );
}

#[test]
fn build_git_show_args_defaults_to_head() {
    let args = build_command_args(RtkCommandKind::GitShow, "{}").expect("git show args");

    assert_eq!(
        args,
        vec![
            OsString::from("git"),
            OsString::from("show"),
            OsString::from("HEAD"),
        ]
    );
}

#[test]
fn build_git_log_args_serializes_range_and_max_count() {
    let args = build_command_args(
        RtkCommandKind::GitLog,
        r#"{"revision_range":"main..HEAD","max_count":5}"#,
    )
    .expect("git log args should parse");

    assert_eq!(
        args,
        vec![
            OsString::from("git"),
            OsString::from("log"),
            OsString::from("-n"),
            OsString::from("5"),
            OsString::from("main..HEAD"),
        ]
    );
}

#[test]
fn build_git_log_args_rejects_zero_max_count() {
    let err = build_command_args(RtkCommandKind::GitLog, r#"{"max_count":0}"#)
        .expect_err("zero max_count should be rejected");

    assert_eq!(
        err.to_string(),
        "max_count must be greater than zero".to_string()
    );
}

#[test]
fn build_git_branch_args_serializes_filters() {
    let args = build_command_args(
        RtkCommandKind::GitBranch,
        r#"{"all":true,"contains":"HEAD","merged":true}"#,
    )
    .expect("git branch args should parse");

    assert_eq!(
        args,
        vec![
            OsString::from("git"),
            OsString::from("branch"),
            OsString::from("--all"),
            OsString::from("--contains"),
            OsString::from("HEAD"),
            OsString::from("--merged"),
        ]
    );
}

#[test]
fn build_git_branch_args_rejects_conflicting_visibility_flags() {
    let err = build_command_args(RtkCommandKind::GitBranch, r#"{"all":true,"remotes":true}"#)
        .expect_err("conflicting branch visibility should be rejected");

    assert_eq!(
        err.to_string(),
        "all and remotes cannot both be true".to_string()
    );
}

#[test]
fn build_git_stash_args_defaults_to_max_count() {
    let args = build_command_args(RtkCommandKind::GitStash, "{}").expect("git stash args");

    assert_eq!(
        args,
        vec![
            OsString::from("git"),
            OsString::from("stash"),
            OsString::from("list"),
            OsString::from("-n"),
            OsString::from("10"),
        ]
    );
}

#[test]
fn build_git_worktree_args_lists_worktrees() {
    let args = build_command_args(RtkCommandKind::GitWorktree, "{}")
        .expect("git worktree args should parse");

    assert_eq!(
        args,
        vec![
            OsString::from("git"),
            OsString::from("worktree"),
            OsString::from("list"),
        ]
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
        failed_status,
        "🔍 0 for 'needle'\n"
    ));
    assert!(!rtk_command_succeeded(
        RtkCommandKind::Read,
        failed_status,
        "🔍 0 for 'needle'\n"
    ));

    assert!(rtk_command_succeeded(
        RtkCommandKind::Summary,
        failed_status,
        "✅ Command: cargo test\n"
    ));

    let successful_status = std::process::Command::new("sh")
        .arg("-c")
        .arg("exit 0")
        .status()
        .expect("status");

    assert!(!rtk_command_succeeded(
        RtkCommandKind::Summary,
        successful_status,
        "❌ Command: cargo test\n"
    ));
}
