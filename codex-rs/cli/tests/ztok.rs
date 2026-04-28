use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use anyhow::Result;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use tempfile::TempDir;

fn codex_command(codex_home: &Path) -> Result<assert_cmd::Command> {
    let mut cmd = assert_cmd::Command::new(codex_utils_cargo_bin::cargo_bin("codex")?);
    cmd.env("CODEX_HOME", codex_home)
        .env_remove("CODEX_THREAD_ID")
        .env_remove("CODEX_ZTOK_SESSION_ID")
        .env_remove("CODEX_ZTOK_BEHAVIOR")
        .env_remove("CODEX_ZTOK_RUNTIME_SETTINGS")
        .env_remove("CODEX_ZTOK_NO_DEDUP");
    Ok(cmd)
}

fn write_ztok_config(codex_home: &Path, behavior: &str) -> Result<()> {
    std::fs::write(
        codex_home.join("config.toml"),
        format!("[ztok]\nbehavior = \"{behavior}\"\n"),
    )?;
    Ok(())
}

fn run_command(command: &mut Command) -> Result<()> {
    let status = command.status()?;
    anyhow::ensure!(status.success(), "command failed with status {status}");
    Ok(())
}

fn init_git_repo(repo: &Path) -> Result<()> {
    run_command(Command::new("git").arg("init").arg(repo))?;
    run_command(Command::new("git").arg("-C").arg(repo).args([
        "config",
        "user.name",
        "ZTOK Test",
    ]))?;
    run_command(Command::new("git").arg("-C").arg(repo).args([
        "config",
        "user.email",
        "ztok@example.com",
    ]))?;
    run_command(Command::new("git").arg("-C").arg(repo).args([
        "config",
        "commit.gpgsign",
        "false",
    ]))?;
    Ok(())
}

fn command_exists(program: &str) -> bool {
    Command::new(program)
        .arg("--version")
        .status()
        .is_ok_and(|status| status.success())
}

fn prepend_path(dir: &Path) -> std::ffi::OsString {
    let original_path = std::env::var_os("PATH").unwrap_or_default();
    let mut combined_path = std::ffi::OsString::new();
    combined_path.push(dir);
    combined_path.push(if cfg!(windows) { ";" } else { ":" });
    combined_path.push(original_path);
    combined_path
}

fn fallback_marker_script(stdout: &str) -> &'static str {
    if cfg!(windows) {
        match stdout {
            "FALLBACK_TRIGGERED" => "@echo FALLBACK_TRIGGERED\r\n",
            "FALLBACK_OK %*" => "@echo FALLBACK_OK %*\r\n",
            other => panic!("unexpected fallback marker script: {other}"),
        }
    } else {
        match stdout {
            "FALLBACK_TRIGGERED" => "#!/bin/sh\necho FALLBACK_TRIGGERED\n",
            "FALLBACK_OK \"$@\"" => "#!/bin/sh\necho FALLBACK_OK \"$@\"\n",
            other => panic!("unexpected fallback marker script: {other}"),
        }
    }
}

fn echo_args_script() -> &'static str {
    if cfg!(windows) {
        "@echo %*\r\n"
    } else {
        "#!/bin/sh\nprintf '%s ' \"$@\"\nprintf '\\n'\n"
    }
}

fn stdout_script(output: &str) -> String {
    if cfg!(windows) {
        let mut script = String::from("@echo off\r\n");
        for line in output.lines() {
            if line.is_empty() {
                script.push_str("echo.\r\n");
            } else {
                script.push_str(&format!("@echo {line}\r\n"));
            }
        }
        script
    } else {
        format!("#!/bin/sh\ncat <<'EOF'\n{output}\nEOF\n")
    }
}

#[cfg(unix)]
fn shell_args(script: &str) -> [&str; 3] {
    ["sh", "-c", script]
}

#[cfg(windows)]
fn shell_args(script: &str) -> [&str; 3] {
    ["cmd", "/C", script]
}

#[cfg(unix)]
fn write_fake_command(dir: &Path, name: &str, script: &str) -> Result<PathBuf> {
    use std::os::unix::fs::PermissionsExt;

    let path = dir.join(name);
    std::fs::write(&path, script)?;
    let mut permissions = std::fs::metadata(&path)?.permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&path, permissions)?;
    Ok(path)
}

#[cfg(windows)]
fn write_fake_command(dir: &Path, name: &str, script: &str) -> Result<PathBuf> {
    let path = dir.join(format!("{name}.bat"));
    std::fs::write(&path, script)?;
    Ok(path)
}

#[test]
fn ztok_read_limits_output() -> Result<()> {
    let codex_home = TempDir::new()?;
    let file = codex_home.path().join("sample.txt");
    std::fs::write(&file, "one\ntwo\nthree\n")?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.args([
        "ztok",
        "read",
        file.to_string_lossy().as_ref(),
        "--max-lines",
        "2",
    ])
    .assert()
    .success()
    .stdout(contains("one").and(contains("省略 2 行（共 3 行）")));

    Ok(())
}

#[test]
fn ztok_read_tail_lines() -> Result<()> {
    let codex_home = TempDir::new()?;
    let file = codex_home.path().join("sample.txt");
    std::fs::write(&file, "one\ntwo\nthree\n")?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.args([
        "ztok",
        "read",
        file.to_string_lossy().as_ref(),
        "--tail-lines",
        "1",
    ])
    .assert()
    .success()
    .stdout(contains("three").and(contains("one").not()));

    Ok(())
}

#[test]
fn ztok_read_accepts_multiple_files() -> Result<()> {
    let codex_home = TempDir::new()?;
    let first = codex_home.path().join("one.txt");
    let second = codex_home.path().join("two.txt");
    std::fs::write(&first, "alpha\n")?;
    std::fs::write(&second, "beta\n")?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.args([
        "ztok",
        "read",
        first.to_string_lossy().as_ref(),
        second.to_string_lossy().as_ref(),
    ])
    .assert()
    .success()
    .stdout(contains("alpha").and(contains("beta")));

    Ok(())
}

#[test]
fn ztok_read_reuses_session_cache_when_thread_id_is_present() -> Result<()> {
    let codex_home = TempDir::new()?;
    let file = codex_home.path().join("sample.txt");
    std::fs::write(&file, "same\ncontent\n")?;

    let mut first = codex_command(codex_home.path())?;
    first
        .env("CODEX_THREAD_ID", "thread-ztok-1")
        .args(["ztok", "read", file.to_string_lossy().as_ref()])
        .assert()
        .success()
        .stdout(contains("same").and(contains("[ztok dedup").not()));

    let mut second = codex_command(codex_home.path())?;
    second
        .env("CODEX_THREAD_ID", "thread-ztok-1")
        .args(["ztok", "read", file.to_string_lossy().as_ref()])
        .assert()
        .success()
        .stdout(contains("[ztok dedup").and(contains("同一会话内已输出相同内容")));

    Ok(())
}

#[test]
fn ztok_read_trace_decisions_reports_short_reference_on_stderr() -> Result<()> {
    let codex_home = TempDir::new()?;
    let file = codex_home.path().join("sample.txt");
    let raw_content = "same\ncontent\n";
    std::fs::write(&file, raw_content)?;

    let mut first = codex_command(codex_home.path())?;
    let first_assert = first
        .env("CODEX_THREAD_ID", "thread-ztok-trace-1")
        .args(["ztok", "read", file.to_string_lossy().as_ref()])
        .assert()
        .success()
        .stdout(contains("same").and(contains("[ztok dedup").not()));
    assert!(first_assert.get_output().stderr.is_empty());

    let mut second = codex_command(codex_home.path())?;
    let second_assert = second
        .env("CODEX_THREAD_ID", "thread-ztok-trace-1")
        .args([
            "ztok",
            "--trace-decisions",
            "read",
            file.to_string_lossy().as_ref(),
        ])
        .assert()
        .success()
        .stdout(contains("[ztok dedup").and(contains("同一会话内已输出相同内容")));

    let stderr = String::from_utf8_lossy(&second_assert.get_output().stderr);
    assert!(stderr.contains("\"kind\":\"compression_decision\""));
    assert!(stderr.contains("\"command\":\"ztok read\""));
    assert!(stderr.contains("\"outputKind\":\"short_reference\""));
    assert!(stderr.contains("\"decision\":\"short_reference\""));
    assert!(!stderr.contains(raw_content));

    Ok(())
}

#[test]
fn ztok_read_disables_session_cache_without_thread_id() -> Result<()> {
    let codex_home = TempDir::new()?;
    let file = codex_home.path().join("sample.txt");
    std::fs::write(&file, "same\ncontent\n")?;

    let mut first = codex_command(codex_home.path())?;
    first
        .args(["ztok", "read", file.to_string_lossy().as_ref()])
        .assert()
        .success()
        .stdout(contains("same").and(contains("[ztok dedup").not()));

    let mut second = codex_command(codex_home.path())?;
    second
        .args(["ztok", "read", file.to_string_lossy().as_ref()])
        .assert()
        .success()
        .stdout(contains("same").and(contains("[ztok dedup").not()));

    Ok(())
}

#[test]
fn ztok_read_trace_decisions_reports_full_fallback_without_session_id() -> Result<()> {
    let codex_home = TempDir::new()?;
    let file = codex_home.path().join("sample.txt");
    let raw_content = "same\ncontent\n";
    std::fs::write(&file, raw_content)?;

    let mut cmd = codex_command(codex_home.path())?;
    let assert = cmd
        .args([
            "ztok",
            "--trace-decisions",
            "read",
            file.to_string_lossy().as_ref(),
        ])
        .assert()
        .success()
        .stdout(contains("same").and(contains("[ztok dedup").not()));

    let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
    assert!(stderr.contains("\"kind\":\"compression_decision\""));
    assert!(stderr.contains("\"command\":\"ztok read\""));
    assert!(stderr.contains("\"outputKind\":\"full\""));
    assert!(stderr.contains("\"decision\":\"full_fallback\""));
    assert!(stderr.contains("\"fallback\":\"dedup_disabled_no_session_id\""));
    assert!(!stderr.contains(raw_content));

    Ok(())
}

#[test]
fn ztok_read_basic_mode_ignores_session_cache_even_with_thread_id() -> Result<()> {
    let codex_home = TempDir::new()?;
    write_ztok_config(codex_home.path(), "basic")?;
    let file = codex_home.path().join("sample.txt");
    std::fs::write(&file, "same\ncontent\n")?;

    let mut first = codex_command(codex_home.path())?;
    first
        .env("CODEX_THREAD_ID", "thread-ztok-read-basic-1")
        .args(["ztok", "read", file.to_string_lossy().as_ref()])
        .assert()
        .success()
        .stdout(contains("same").and(contains("[ztok dedup").not()));

    let mut second = codex_command(codex_home.path())?;
    second
        .env("CODEX_THREAD_ID", "thread-ztok-read-basic-1")
        .args(["ztok", "read", file.to_string_lossy().as_ref()])
        .assert()
        .success()
        .stdout(contains("same").and(contains("[ztok dedup").not()));

    Ok(())
}

#[test]
fn ztok_json_reuses_session_cache_when_thread_id_is_present() -> Result<()> {
    let codex_home = TempDir::new()?;

    let mut first = codex_command(codex_home.path())?;
    first
        .env("CODEX_THREAD_ID", "thread-ztok-json-1")
        .args(["ztok", "json", "-"])
        .write_stdin("{\"name\":\"alpha\",\"count\":2}\n")
        .assert()
        .success()
        .stdout(contains("name: \"alpha\"").and(contains("[ztok dedup").not()));

    let mut second = codex_command(codex_home.path())?;
    second
        .env("CODEX_THREAD_ID", "thread-ztok-json-1")
        .args(["ztok", "json", "-"])
        .write_stdin("{\"name\":\"alpha\",\"count\":2}\n")
        .assert()
        .success()
        .stdout(contains("[ztok dedup").and(contains("同一会话内已输出相同内容")));

    Ok(())
}

#[test]
fn ztok_json_disables_session_cache_without_thread_id() -> Result<()> {
    let codex_home = TempDir::new()?;

    let mut first = codex_command(codex_home.path())?;
    first
        .args(["ztok", "json", "-"])
        .write_stdin("{\"name\":\"alpha\",\"count\":2}\n")
        .assert()
        .success()
        .stdout(contains("name: \"alpha\"").and(contains("[ztok dedup").not()));

    let mut second = codex_command(codex_home.path())?;
    second
        .args(["ztok", "json", "-"])
        .write_stdin("{\"name\":\"alpha\",\"count\":2}\n")
        .assert()
        .success()
        .stdout(contains("name: \"alpha\"").and(contains("[ztok dedup").not()));

    Ok(())
}

#[test]
fn ztok_json_trace_decisions_reports_short_reference_on_stderr() -> Result<()> {
    let codex_home = TempDir::new()?;
    let raw_content = "{\"name\":\"alpha\",\"count\":2}\n";

    let mut first = codex_command(codex_home.path())?;
    let first_assert = first
        .env("CODEX_THREAD_ID", "thread-ztok-json-trace-1")
        .args(["ztok", "json", "-"])
        .write_stdin(raw_content)
        .assert()
        .success()
        .stdout(contains("name: \"alpha\"").and(contains("[ztok dedup").not()));
    assert!(first_assert.get_output().stderr.is_empty());

    let mut second = codex_command(codex_home.path())?;
    let second_assert = second
        .env("CODEX_THREAD_ID", "thread-ztok-json-trace-1")
        .args(["ztok", "--trace-decisions", "json", "-"])
        .write_stdin(raw_content)
        .assert()
        .success()
        .stdout(contains("[ztok dedup").and(contains("同一会话内已输出相同内容")));

    let stderr = String::from_utf8_lossy(&second_assert.get_output().stderr);
    assert!(stderr.contains("\"command\":\"ztok json\""));
    assert!(stderr.contains("\"outputKind\":\"short_reference\""));
    assert!(stderr.contains("\"decision\":\"short_reference\""));
    assert!(!stderr.contains(raw_content));

    Ok(())
}

#[test]
fn ztok_json_basic_mode_returns_raw_text() -> Result<()> {
    let codex_home = TempDir::new()?;
    write_ztok_config(codex_home.path(), "basic")?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.args(["ztok", "json", "-"])
        .write_stdin("{\"name\":\"alpha\",\"count\":2}\n")
        .assert()
        .success()
        .stdout(contains("{\"name\":\"alpha\",\"count\":2}").and(contains("name:").not()));

    Ok(())
}

#[test]
fn ztok_json_basic_mode_rejects_keys_only() -> Result<()> {
    let codex_home = TempDir::new()?;
    write_ztok_config(codex_home.path(), "basic")?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.args(["ztok", "json", "-", "--keys-only"])
        .write_stdin("{\"name\":\"alpha\",\"count\":2}\n")
        .assert()
        .failure()
        .stderr(contains("basic 模式下不受支持"));

    Ok(())
}

#[test]
fn ztok_log_reuses_session_cache_when_thread_id_is_present() -> Result<()> {
    let codex_home = TempDir::new()?;

    let mut first = codex_command(codex_home.path())?;
    first
        .env("CODEX_THREAD_ID", "thread-ztok-log-1")
        .args(["ztok", "log"])
        .write_stdin("warning: heads up\nerror: boom\n")
        .assert()
        .success()
        .stdout(contains("1 个错误（1 个唯一）").and(contains("[ztok dedup").not()));

    let mut second = codex_command(codex_home.path())?;
    second
        .env("CODEX_THREAD_ID", "thread-ztok-log-1")
        .args(["ztok", "log"])
        .write_stdin("warning: heads up\nerror: boom\n")
        .assert()
        .success()
        .stdout(contains("[ztok dedup").and(contains("同一会话内已输出相同内容")));

    Ok(())
}

#[test]
fn ztok_log_basic_mode_returns_raw_output() -> Result<()> {
    let codex_home = TempDir::new()?;
    write_ztok_config(codex_home.path(), "basic")?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.args(["ztok", "log"])
        .write_stdin("warning: heads up\nerror: boom\n")
        .assert()
        .success()
        .stdout(
            contains("warning: heads up")
                .and(contains("error: boom"))
                .and(contains("日志摘要").not()),
        );

    Ok(())
}

#[test]
fn ztok_log_trace_decisions_reports_short_reference_on_stderr() -> Result<()> {
    let codex_home = TempDir::new()?;
    let raw_content = "warning: heads up\nerror: boom\n";

    let mut first = codex_command(codex_home.path())?;
    let first_assert = first
        .env("CODEX_THREAD_ID", "thread-ztok-log-trace-1")
        .args(["ztok", "log"])
        .write_stdin(raw_content)
        .assert()
        .success()
        .stdout(contains("1 个错误（1 个唯一）").and(contains("[ztok dedup").not()));
    assert!(first_assert.get_output().stderr.is_empty());

    let mut second = codex_command(codex_home.path())?;
    let second_assert = second
        .env("CODEX_THREAD_ID", "thread-ztok-log-trace-1")
        .args(["ztok", "--trace-decisions", "log"])
        .write_stdin(raw_content)
        .assert()
        .success()
        .stdout(contains("[ztok dedup").and(contains("同一会话内已输出相同内容")));

    let stderr = String::from_utf8_lossy(&second_assert.get_output().stderr);
    assert!(stderr.contains("\"command\":\"ztok log\""));
    assert!(stderr.contains("\"outputKind\":\"short_reference\""));
    assert!(stderr.contains("\"decision\":\"short_reference\""));
    assert!(!stderr.contains(raw_content));

    Ok(())
}

#[test]
fn ztok_summary_reuses_session_cache_when_thread_id_is_present() -> Result<()> {
    let codex_home = TempDir::new()?;
    let summary_args: Vec<&str> = if cfg!(windows) {
        vec![
            "ztok", "summary", "echo", "alpha", "&", "echo", "warning:", "boom",
        ]
    } else {
        vec![
            "ztok", "summary", "echo", "alpha", ";", "echo", "warning:", "boom",
        ]
    };

    let mut first = codex_command(codex_home.path())?;
    first
        .env("CODEX_THREAD_ID", "thread-ztok-summary-1")
        .args(&summary_args)
        .assert()
        .success()
        .stdout(contains("✅ 命令：").and(contains("[ztok dedup").not()));

    let mut second = codex_command(codex_home.path())?;
    second
        .env("CODEX_THREAD_ID", "thread-ztok-summary-1")
        .args(&summary_args)
        .assert()
        .success()
        .stdout(contains("[ztok dedup").and(contains("同一会话内已输出相同内容")));

    Ok(())
}

#[test]
fn ztok_summary_trace_decisions_reports_short_reference_on_stderr() -> Result<()> {
    let codex_home = TempDir::new()?;
    let summary_args: Vec<&str> = if cfg!(windows) {
        vec![
            "ztok", "summary", "echo", "alpha", "&", "echo", "warning:", "boom",
        ]
    } else {
        vec![
            "ztok", "summary", "echo", "alpha", ";", "echo", "warning:", "boom",
        ]
    };
    let trace_args: Vec<&str> = if cfg!(windows) {
        vec![
            "ztok",
            "--trace-decisions",
            "summary",
            "echo",
            "alpha",
            "&",
            "echo",
            "warning:",
            "boom",
        ]
    } else {
        vec![
            "ztok",
            "--trace-decisions",
            "summary",
            "echo",
            "alpha",
            ";",
            "echo",
            "warning:",
            "boom",
        ]
    };

    let mut first = codex_command(codex_home.path())?;
    let first_assert = first
        .env("CODEX_THREAD_ID", "thread-ztok-summary-trace-1")
        .args(&summary_args)
        .assert()
        .success()
        .stdout(contains("✅ 命令：").and(contains("[ztok dedup").not()));
    assert!(first_assert.get_output().stderr.is_empty());

    let mut second = codex_command(codex_home.path())?;
    let second_assert = second
        .env("CODEX_THREAD_ID", "thread-ztok-summary-trace-1")
        .args(&trace_args)
        .assert()
        .success()
        .stdout(contains("[ztok dedup").and(contains("同一会话内已输出相同内容")));

    let stderr = String::from_utf8_lossy(&second_assert.get_output().stderr);
    assert!(stderr.contains("\"command\":\"ztok summary\""));
    assert!(stderr.contains("\"outputKind\":\"short_reference\""));
    assert!(stderr.contains("\"decision\":\"short_reference\""));
    assert!(!stderr.contains("echo alpha"));
    assert!(!stderr.contains("warning: boom"));

    Ok(())
}

#[test]
fn ztok_summary_basic_mode_ignores_session_cache_when_thread_id_is_present() -> Result<()> {
    let codex_home = TempDir::new()?;
    write_ztok_config(codex_home.path(), "basic")?;
    let summary_args: Vec<&str> = if cfg!(windows) {
        vec![
            "ztok", "summary", "echo", "alpha", "&", "echo", "warning:", "boom",
        ]
    } else {
        vec![
            "ztok", "summary", "echo", "alpha", ";", "echo", "warning:", "boom",
        ]
    };

    let mut first = codex_command(codex_home.path())?;
    first
        .env("CODEX_THREAD_ID", "thread-ztok-summary-basic-1")
        .args(&summary_args)
        .assert()
        .success()
        .stdout(contains("✅ 命令：").and(contains("[ztok dedup").not()));

    let mut second = codex_command(codex_home.path())?;
    second
        .env("CODEX_THREAD_ID", "thread-ztok-summary-basic-1")
        .args(&summary_args)
        .assert()
        .success()
        .stdout(contains("✅ 命令：").and(contains("[ztok dedup").not()));

    Ok(())
}

#[test]
fn ztok_curl_reuses_session_cache_when_thread_id_is_present() -> Result<()> {
    let codex_home = TempDir::new()?;
    let bin_dir = create_fake_bin_dir(&codex_home)?;
    let curl_output = r#"{"name":"a very long user name here","count":42,"items":[1,2,3],"description":"a very long description that takes up many characters in the original JSON payload","status":"active","url":"https://example.com/api/v1/users/123"}"#;
    let _fake_curl = write_fake_command(&bin_dir, "curl", &stdout_script(curl_output))?;

    let mut first = codex_command(codex_home.path())?;
    first
        .env("PATH", prepend_path(&bin_dir))
        .env("CODEX_THREAD_ID", "thread-ztok-curl-1")
        .args(["ztok", "curl", "https://api.example.com/users"])
        .assert()
        .success()
        .stdout(
            contains("name")
                .and(contains("string"))
                .and(contains("[ztok dedup").not()),
        );

    let mut second = codex_command(codex_home.path())?;
    second
        .env("PATH", prepend_path(&bin_dir))
        .env("CODEX_THREAD_ID", "thread-ztok-curl-1")
        .args(["ztok", "curl", "https://api.example.com/users"])
        .assert()
        .success()
        .stdout(contains("[ztok dedup").and(contains("同一会话内已输出相同内容")));

    Ok(())
}

#[test]
fn ztok_curl_trace_decisions_redacts_source_and_reports_short_reference_on_stderr() -> Result<()> {
    let codex_home = TempDir::new()?;
    let bin_dir = create_fake_bin_dir(&codex_home)?;
    let raw_output = r#"{"name":"a very long user name here","count":42,"items":[1,2,3],"description":"a very long description that takes up many characters in the original JSON payload","status":"active","url":"https://example.com/api/v1/users/123"}"#;
    let _fake_curl = write_fake_command(&bin_dir, "curl", &stdout_script(raw_output))?;

    let mut first = codex_command(codex_home.path())?;
    let first_assert = first
        .env("PATH", prepend_path(&bin_dir))
        .env("CODEX_THREAD_ID", "thread-ztok-curl-trace-1")
        .args(["ztok", "curl", "https://api.example.com/users?token=secret"])
        .assert()
        .success()
        .stdout(
            contains("name")
                .and(contains("string"))
                .and(contains("[ztok dedup").not()),
        );
    assert!(first_assert.get_output().stderr.is_empty());

    let mut second = codex_command(codex_home.path())?;
    let second_assert = second
        .env("PATH", prepend_path(&bin_dir))
        .env("CODEX_THREAD_ID", "thread-ztok-curl-trace-1")
        .args([
            "ztok",
            "--trace-decisions",
            "curl",
            "https://user:pass@api.example.com/users?token=secret",
        ])
        .assert()
        .success()
        .stdout(contains("[ztok dedup").and(contains("同一会话内已输出相同内容")));

    let stderr = String::from_utf8_lossy(&second_assert.get_output().stderr);
    assert!(stderr.contains("\"command\":\"ztok curl\""));
    assert!(stderr.contains("\"source\":\"api.example.com/users\""));
    assert!(stderr.contains("\"outputKind\":\"short_reference\""));
    assert!(stderr.contains("\"decision\":\"short_reference\""));
    assert!(!stderr.contains("user:pass"));
    assert!(!stderr.contains("token=secret"));
    assert!(!stderr.contains(raw_output));

    Ok(())
}

#[test]
fn ztok_curl_keeps_internal_url_json_raw() -> Result<()> {
    let codex_home = TempDir::new()?;
    let bin_dir = create_fake_bin_dir(&codex_home)?;
    let curl_output = r#"{"r2Ready":true,"status":"ok"}"#;
    let _fake_curl = write_fake_command(&bin_dir, "curl", &stdout_script(curl_output))?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.env("PATH", prepend_path(&bin_dir))
        .args(["ztok", "curl", "http://localhost:3000/api/status"])
        .assert()
        .success()
        .stdout(contains(curl_output).and(contains("r2Ready:").not()));

    Ok(())
}

#[test]
fn ztok_wget_stdout_reuses_session_cache_when_thread_id_is_present() -> Result<()> {
    let codex_home = TempDir::new()?;
    let bin_dir = create_fake_bin_dir(&codex_home)?;
    let wget_output = (0..25)
        .map(|index| format!("line-{index}"))
        .collect::<Vec<_>>()
        .join("\n");
    let _fake_wget = write_fake_command(&bin_dir, "wget", &stdout_script(&wget_output))?;

    let mut first = codex_command(codex_home.path())?;
    first
        .env("PATH", prepend_path(&bin_dir))
        .env("CODEX_THREAD_ID", "thread-ztok-wget-1")
        .args(["ztok", "wget", "-O", "https://example.com/archive.txt"])
        .assert()
        .success()
        .stdout(
            contains("line-0")
                .and(contains("省略"))
                .and(contains("[ztok dedup").not()),
        );

    let mut second = codex_command(codex_home.path())?;
    second
        .env("PATH", prepend_path(&bin_dir))
        .env("CODEX_THREAD_ID", "thread-ztok-wget-1")
        .args(["ztok", "wget", "-O", "https://example.com/archive.txt"])
        .assert()
        .success()
        .stdout(contains("[ztok dedup").and(contains("同一会话内已输出相同内容")));

    Ok(())
}

#[test]
fn ztok_wget_stdout_keeps_internal_url_json_raw() -> Result<()> {
    let codex_home = TempDir::new()?;
    let bin_dir = create_fake_bin_dir(&codex_home)?;
    let raw_output = r#"{"r2Ready":true,"status":"ok"}"#;
    let _fake_wget = write_fake_command(&bin_dir, "wget", &stdout_script(raw_output))?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.env("PATH", prepend_path(&bin_dir))
        .args(["ztok", "wget", "-O", "http://localhost:3000/api/status"])
        .assert()
        .success()
        .stdout(contains(raw_output).and(contains("r2Ready:").not()));

    Ok(())
}

#[test]
fn ztok_wget_stdout_trace_decisions_redacts_source_and_reports_short_reference_on_stderr()
-> Result<()> {
    let codex_home = TempDir::new()?;
    let bin_dir = create_fake_bin_dir(&codex_home)?;
    let raw_output = (0..25)
        .map(|index| format!("line-{index}"))
        .collect::<Vec<_>>()
        .join("\n");
    let _fake_wget = write_fake_command(&bin_dir, "wget", &stdout_script(&raw_output))?;

    let mut first = codex_command(codex_home.path())?;
    let first_assert = first
        .env("PATH", prepend_path(&bin_dir))
        .env("CODEX_THREAD_ID", "thread-ztok-wget-trace-1")
        .args([
            "ztok",
            "wget",
            "-O",
            "https://example.com/archive.txt?sig=secret",
        ])
        .assert()
        .success()
        .stdout(
            contains("line-0")
                .and(contains("省略"))
                .and(contains("[ztok dedup").not()),
        );
    assert!(first_assert.get_output().stderr.is_empty());

    let mut second = codex_command(codex_home.path())?;
    let second_assert = second
        .env("PATH", prepend_path(&bin_dir))
        .env("CODEX_THREAD_ID", "thread-ztok-wget-trace-1")
        .args([
            "ztok",
            "--trace-decisions",
            "wget",
            "-O",
            "https://example.com/archive.txt?sig=secret",
        ])
        .assert()
        .success()
        .stdout(contains("[ztok dedup").and(contains("同一会话内已输出相同内容")));

    let stderr = String::from_utf8_lossy(&second_assert.get_output().stderr);
    assert!(stderr.contains("\"command\":\"ztok wget\""));
    assert!(stderr.contains("\"source\":\"example.com/archive.txt\""));
    assert!(stderr.contains("\"outputKind\":\"short_reference\""));
    assert!(stderr.contains("\"decision\":\"short_reference\""));
    assert!(!stderr.contains("sig=secret"));
    assert!(!stderr.contains("line-24"));

    Ok(())
}

#[test]
fn ztok_wget_stdout_basic_mode_ignores_session_cache_even_with_thread_id() -> Result<()> {
    let codex_home = TempDir::new()?;
    write_ztok_config(codex_home.path(), "basic")?;
    let bin_dir = create_fake_bin_dir(&codex_home)?;
    let wget_output = (0..25)
        .map(|index| format!("line-{index}"))
        .collect::<Vec<_>>()
        .join("\n");
    let _fake_wget = write_fake_command(&bin_dir, "wget", &stdout_script(&wget_output))?;

    let mut first = codex_command(codex_home.path())?;
    first
        .env("PATH", prepend_path(&bin_dir))
        .env("CODEX_THREAD_ID", "thread-ztok-wget-basic-1")
        .args(["ztok", "wget", "-O", "https://example.com/archive.txt"])
        .assert()
        .success()
        .stdout(contains("line-0").and(contains("[ztok dedup").not()));

    let mut second = codex_command(codex_home.path())?;
    second
        .env("PATH", prepend_path(&bin_dir))
        .env("CODEX_THREAD_ID", "thread-ztok-wget-basic-1")
        .args(["ztok", "wget", "-O", "https://example.com/archive.txt"])
        .assert()
        .success()
        .stdout(contains("line-0").and(contains("[ztok dedup").not()));

    Ok(())
}

#[test]
fn ztok_cache_inspect_and_clear_specific_session() -> Result<()> {
    let codex_home = TempDir::new()?;
    let file = codex_home.path().join("sample.txt");
    std::fs::write(&file, "same\ncontent\n")?;

    let mut prime = codex_command(codex_home.path())?;
    prime
        .env("CODEX_THREAD_ID", "thread-ztok-cache-1")
        .args(["ztok", "read", file.to_string_lossy().as_ref()])
        .assert()
        .success();

    let mut inspect = codex_command(codex_home.path())?;
    inspect
        .args(["ztok", "cache", "inspect", "thread-ztok-cache-1"])
        .assert()
        .success()
        .stdout(
            contains("session: thread-ztok-cache-1")
                .and(contains("status: present"))
                .and(contains("entries: 1 / 64"))
                .and(contains("schemaVersion: 1")),
        );

    let mut clear = codex_command(codex_home.path())?;
    clear
        .args(["ztok", "cache", "clear", "thread-ztok-cache-1"])
        .assert()
        .success()
        .stdout(contains("已清空 session cache: thread-ztok-cache-1"));

    let mut inspect_missing = codex_command(codex_home.path())?;
    inspect_missing
        .args(["ztok", "cache", "inspect", "thread-ztok-cache-1"])
        .assert()
        .success()
        .stdout(
            contains("session: thread-ztok-cache-1")
                .and(contains("status: absent"))
                .and(contains("entries: 0 / 64")),
        );

    Ok(())
}

#[test]
fn ztok_cache_expand_returns_original_snapshot_for_dedup_ref() -> Result<()> {
    let codex_home = TempDir::new()?;
    let file = codex_home.path().join("sample.txt");
    std::fs::write(&file, "same\ncontent\n")?;

    let mut first = codex_command(codex_home.path())?;
    first
        .env("CODEX_THREAD_ID", "thread-ztok-expand-1")
        .args(["ztok", "read", file.to_string_lossy().as_ref()])
        .assert()
        .success();

    let mut second = codex_command(codex_home.path())?;
    let second_assert = second
        .env("CODEX_THREAD_ID", "thread-ztok-expand-1")
        .args(["ztok", "read", file.to_string_lossy().as_ref()])
        .assert()
        .success()
        .stdout(contains("[ztok dedup"));
    let dedup_ref = extract_dedup_ref(&second_assert.get_output().stdout);

    let mut expand = codex_command(codex_home.path())?;
    expand
        .args([
            "ztok",
            "cache",
            "expand",
            "thread-ztok-expand-1",
            &dedup_ref,
        ])
        .assert()
        .success()
        .stdout(contains("same\ncontent\n"));

    Ok(())
}

#[test]
fn ztok_cache_expand_can_return_compressed_output() -> Result<()> {
    let codex_home = TempDir::new()?;
    let file = codex_home.path().join("sample.txt");
    std::fs::write(&file, "one\ntwo\nthree\n")?;

    let mut first = codex_command(codex_home.path())?;
    first
        .env("CODEX_THREAD_ID", "thread-ztok-expand-compressed-1")
        .args([
            "ztok",
            "read",
            file.to_string_lossy().as_ref(),
            "--max-lines",
            "2",
        ])
        .assert()
        .success();

    let mut second = codex_command(codex_home.path())?;
    let second_assert = second
        .env("CODEX_THREAD_ID", "thread-ztok-expand-compressed-1")
        .args([
            "ztok",
            "read",
            file.to_string_lossy().as_ref(),
            "--max-lines",
            "2",
        ])
        .assert()
        .success()
        .stdout(contains("[ztok dedup"));
    let dedup_ref = extract_dedup_ref(&second_assert.get_output().stdout);

    let mut expand = codex_command(codex_home.path())?;
    expand
        .args([
            "ztok",
            "cache",
            "expand",
            "thread-ztok-expand-compressed-1",
            &dedup_ref,
            "--compressed",
        ])
        .assert()
        .success()
        .stdout(contains("one").and(contains("省略 2 行（共 3 行）")));

    Ok(())
}

#[test]
fn ztok_cache_expand_reports_missing_ref() -> Result<()> {
    let codex_home = TempDir::new()?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.args([
        "ztok",
        "cache",
        "expand",
        "thread-ztok-expand-missing-1",
        "abcd1234",
    ])
    .assert()
    .failure()
    .stderr(contains("未找到匹配的 ztok dedup 引用"));

    Ok(())
}

#[test]
fn ztok_no_cache_cli_disables_dedup_but_keeps_compression() -> Result<()> {
    let codex_home = TempDir::new()?;
    let raw_content = "{\"name\":\"alpha\",\"count\":2}\n";

    let mut first = codex_command(codex_home.path())?;
    first
        .env("CODEX_THREAD_ID", "thread-ztok-no-cache-1")
        .args(["ztok", "--no-cache", "json", "-"])
        .write_stdin(raw_content)
        .assert()
        .success()
        .stdout(contains("name: \"alpha\"").and(contains("[ztok dedup").not()));

    let mut second = codex_command(codex_home.path())?;
    let second_assert = second
        .env("CODEX_THREAD_ID", "thread-ztok-no-cache-1")
        .args(["ztok", "--trace-decisions", "--no-cache", "json", "-"])
        .write_stdin(raw_content)
        .assert()
        .success()
        .stdout(contains("name: \"alpha\"").and(contains("[ztok dedup").not()));

    let stderr = String::from_utf8_lossy(&second_assert.get_output().stderr);
    assert!(stderr.contains("\"fallback\":\"dedup_disabled_by_user\""));
    assert!(!stderr.contains(raw_content));

    Ok(())
}

#[test]
fn ztok_no_cache_env_disables_dedup() -> Result<()> {
    let codex_home = TempDir::new()?;
    let file = codex_home.path().join("sample.txt");
    std::fs::write(&file, "same\ncontent\n")?;

    for _ in 0..2 {
        let mut cmd = codex_command(codex_home.path())?;
        cmd.env("CODEX_THREAD_ID", "thread-ztok-no-cache-env-1")
            .env("CODEX_ZTOK_NO_DEDUP", "1")
            .args(["ztok", "read", file.to_string_lossy().as_ref()])
            .assert()
            .success()
            .stdout(contains("same").and(contains("[ztok dedup").not()));
    }

    Ok(())
}

#[cfg(unix)]
fn make_ztok_alias(codex_home: &Path) -> Result<PathBuf> {
    let alias = codex_home.join("ztok");
    std::os::unix::fs::symlink(codex_utils_cargo_bin::cargo_bin("codex")?, &alias)?;
    Ok(alias)
}

#[cfg(windows)]
fn make_ztok_alias(codex_home: &Path) -> Result<PathBuf> {
    let alias = codex_home.join("ztok.bat");
    std::fs::write(
        &alias,
        format!(
            "@echo off\r\n\"{}\" ztok %*\r\n",
            codex_utils_cargo_bin::cargo_bin("codex")?.display()
        ),
    )?;
    Ok(alias)
}

fn assert_help_contains(
    codex_home: &Path,
    args: &[&str],
    required: &[&str],
    forbidden: &[&str],
) -> Result<()> {
    let mut cmd = codex_command(codex_home)?;
    let assert = cmd.args(args).assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);
    for pattern in required {
        assert!(
            stdout.contains(pattern),
            "stdout did not contain required pattern `{pattern}`.\nstdout:\n{stdout}"
        );
    }
    for pattern in forbidden {
        assert!(
            !stdout.contains(pattern),
            "stdout unexpectedly contained forbidden pattern `{pattern}`.\nstdout:\n{stdout}"
        );
    }
    Ok(())
}

fn assert_version_contains(codex_home: &Path, args: &[&str]) -> Result<()> {
    let mut cmd = codex_command(codex_home)?;
    cmd.args(args).assert().success().stdout(contains("ztok "));
    Ok(())
}

fn extract_dedup_ref(output: &[u8]) -> String {
    let stdout = String::from_utf8_lossy(output);
    let marker = "[ztok dedup ";
    let start = stdout
        .find(marker)
        .unwrap_or_else(|| panic!("missing dedup marker in stdout:\n{stdout}"))
        + marker.len();
    let end = stdout[start..]
        .find(']')
        .unwrap_or_else(|| panic!("unterminated dedup marker in stdout:\n{stdout}"))
        + start;
    stdout[start..end].to_string()
}

fn create_fake_bin_dir(codex_home: &TempDir) -> Result<PathBuf> {
    let bin_dir = codex_home.path().join("bin");
    std::fs::create_dir_all(&bin_dir)?;
    Ok(bin_dir)
}

fn assert_custom_fallback(codex_home: &TempDir, args: &[&str], required: &[&str]) -> Result<()> {
    let bin_dir = create_fake_bin_dir(codex_home)?;
    let _fake_external = write_fake_command(
        &bin_dir,
        "custom-fallback",
        fallback_marker_script("FALLBACK_OK \"$@\""),
    )?;

    let mut cmd = codex_command(codex_home.path())?;
    let assert = cmd.env("PATH", prepend_path(&bin_dir)).args(args).assert();
    let mut stdout_assert = assert.success().stdout(contains("FALLBACK_OK"));
    for pattern in required {
        stdout_assert = stdout_assert.stdout(contains(*pattern));
    }

    Ok(())
}

fn assert_parse_error_without_fallback(
    codex_home: &TempDir,
    command_name: &str,
    args: &[&str],
    stderr_pattern: &str,
) -> Result<()> {
    let bin_dir = create_fake_bin_dir(codex_home)?;
    let _fake_command = write_fake_command(
        &bin_dir,
        command_name,
        fallback_marker_script("FALLBACK_TRIGGERED"),
    )?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.env("PATH", prepend_path(&bin_dir))
        .args(args)
        .assert()
        .failure()
        .stderr(contains(stderr_pattern))
        .stdout(contains("FALLBACK_TRIGGERED").not());

    Ok(())
}

fn assert_raw_external_command(
    codex_home: &TempDir,
    command_name: &str,
    script: &str,
    args: &[&str],
    required_stdout: &[&str],
) -> Result<()> {
    let bin_dir = create_fake_bin_dir(codex_home)?;
    let _fake_command = write_fake_command(&bin_dir, command_name, script)?;

    let mut cmd = codex_command(codex_home.path())?;
    let assert = cmd.env("PATH", prepend_path(&bin_dir)).args(args).assert();
    let mut stdout_assert = assert.success();
    for pattern in required_stdout {
        stdout_assert = stdout_assert.stdout(contains(*pattern));
    }

    Ok(())
}

fn assert_success_without_fallback(
    codex_home: &TempDir,
    command_name: &str,
    args: &[&str],
    required_stdout: &[&str],
) -> Result<()> {
    let bin_dir = create_fake_bin_dir(codex_home)?;
    let _fake_command = write_fake_command(
        &bin_dir,
        command_name,
        fallback_marker_script("FALLBACK_TRIGGERED"),
    )?;

    let mut cmd = codex_command(codex_home.path())?;
    let assert = cmd.env("PATH", prepend_path(&bin_dir)).args(args).assert();
    let mut stdout_assert = assert
        .success()
        .stdout(contains("FALLBACK_TRIGGERED").not());
    for pattern in required_stdout {
        stdout_assert = stdout_assert.stdout(contains(*pattern));
    }

    Ok(())
}

#[test]
fn ztok_alias_routes_to_ztok_parser() -> Result<()> {
    let codex_home = TempDir::new()?;
    let file = codex_home.path().join("alias.txt");
    std::fs::write(&file, "alpha\nbeta\n")?;
    let alias = make_ztok_alias(codex_home.path())?;

    let mut cmd = assert_cmd::Command::new(alias);
    cmd.env("CODEX_HOME", codex_home.path())
        .args(["read", file.to_string_lossy().as_ref()])
        .assert()
        .success()
        .stdout(contains("alpha").and(contains("beta")));

    Ok(())
}

#[test]
fn ztok_help_exposes_codex_curated_command_surface() -> Result<()> {
    let codex_home = TempDir::new()?;
    let cases = [
        (
            vec!["ztok", "--help"],
            vec![
                "gh",
                "env",
                "wget",
                "golangci-lint",
                "cargo",
                "summary",
                "显示版本",
            ],
            vec![
                "  init ",
                "  gain ",
                "discover",
                "learn",
                "config",
                "proxy",
                "hook-audit",
                "cc-economics",
                "rewrite",
                "verify",
                "Print version",
            ],
        ),
        (
            vec!["ztok", "--verbose", "--help"],
            vec!["高性能 CLI 代理", "golangci-lint"],
            vec!["rewrite"],
        ),
        (
            vec!["ztok", "git", "--help"],
            vec!["status", "log", "diff"],
            vec!["rewrite"],
        ),
        (
            vec!["ztok", "--verbose", "git", "--help"],
            vec!["Git 命令，紧凑输出", "status"],
            vec!["rewrite"],
        ),
        (
            vec!["ztok", "read", "--help"],
            vec!["读取文件并智能过滤", "--max-lines", "--tail-lines"],
            vec!["rewrite"],
        ),
        (
            vec!["ztok", "--verbose", "read", "--help"],
            vec!["读取文件并智能过滤", "--max-lines", "--tail-lines"],
            vec!["rewrite"],
        ),
        (
            vec!["ztok", "json", "--help"],
            vec!["仅显示键和类型，不显示值", "--keys-only"],
            vec!["rewrite"],
        ),
        (
            vec!["ztok", "cache", "--help"],
            vec![
                "inspect",
                "expand",
                "clear",
                "管理指定 session 的 ztok cache",
            ],
            vec!["rewrite"],
        ),
        (
            vec!["ztok", "shell", "--help"],
            vec!["通用 shell 命令入口", "--filter", "raw", "err", "test"],
            vec!["rewrite"],
        ),
        (
            vec!["ztok", "vitest", "--help"],
            vec!["Vitest 命令紧凑输出", "Vitest 参数"],
            vec!["run", "rewrite"],
        ),
    ];

    for (args, required, forbidden) in cases {
        assert_help_contains(codex_home.path(), &args, &required, &forbidden)?;
    }

    Ok(())
}

#[test]
fn ztok_version_still_works_with_global_flags() -> Result<()> {
    let codex_home = TempDir::new()?;

    for args in [
        vec!["ztok", "--version"],
        vec!["ztok", "--verbose", "--version"],
        vec!["ztok", "--ultra-compact", "--version"],
    ] {
        assert_version_contains(codex_home.path(), &args)?;
    }

    Ok(())
}

#[test]
fn ztok_find_matches_hidden_dotfiles() -> Result<()> {
    let codex_home = TempDir::new()?;
    std::fs::write(codex_home.path().join(".claude.json"), "{}")?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.args([
        "ztok",
        "find",
        ".claude.json",
        codex_home.path().to_string_lossy().as_ref(),
    ])
    .assert()
    .success()
    .stdout(contains(".claude.json"));

    Ok(())
}

#[test]
fn ztok_gh_pr_merge_passthroughs_real_output() -> Result<()> {
    let codex_home = TempDir::new()?;
    let bin_dir = create_fake_bin_dir(&codex_home)?;
    let _fake_gh = write_fake_command(&bin_dir, "gh", echo_args_script())?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.env("PATH", prepend_path(&bin_dir))
        .args(["ztok", "gh", "pr", "merge", "123", "--squash"])
        .assert()
        .success()
        .stdout(
            contains("pr")
                .and(contains("merge"))
                .and(contains("123"))
                .and(contains("--squash")),
        );

    Ok(())
}

#[test]
fn ztok_pnpm_build_keeps_global_filters_in_passthrough() -> Result<()> {
    let codex_home = TempDir::new()?;
    let bin_dir = create_fake_bin_dir(&codex_home)?;
    let _fake_pnpm = write_fake_command(&bin_dir, "pnpm", echo_args_script())?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.env("PATH", prepend_path(&bin_dir))
        .args(["ztok", "pnpm", "--filter", "@web", "build", "--prod"])
        .assert()
        .success()
        .stdout(
            contains("build")
                .and(contains("--filter=@web"))
                .and(contains("--prod")),
        );

    Ok(())
}

#[test]
fn ztok_removed_meta_commands_fail_instead_of_falling_through() -> Result<()> {
    let codex_home = TempDir::new()?;

    for command_name in [
        "gain",
        "discover",
        "learn",
        "init",
        "config",
        "proxy",
        "hook-audit",
        "cc-economics",
        "verify",
        "rewrite",
    ] {
        let mut cmd = codex_command(codex_home.path())?;
        cmd.args(["ztok", command_name])
            .assert()
            .failure()
            .stderr(contains("unrecognized subcommand"));
    }

    Ok(())
}

#[test]
fn ztok_builtin_parse_errors_do_not_fall_back_to_external_commands() -> Result<()> {
    let codex_home = TempDir::new()?;
    let bin_dir = codex_home.path().join("bin");
    std::fs::create_dir(&bin_dir)?;
    let _fake_read = write_fake_command(
        &bin_dir,
        "read",
        fallback_marker_script("FALLBACK_TRIGGERED"),
    )?;
    let combined_path = prepend_path(&bin_dir);

    let mut cmd = codex_command(codex_home.path())?;
    cmd.env("PATH", combined_path)
        .args(["ztok", "read", "--bogus-flag"])
        .assert()
        .failure()
        .stderr(contains("unexpected argument '--bogus-flag'"))
        .stdout(contains("FALLBACK_TRIGGERED").not());

    Ok(())
}

#[test]
fn ztok_builtin_commands_stay_internal_after_global_flags_matrix() -> Result<()> {
    let codex_home = TempDir::new()?;

    assert_success_without_fallback(
        &codex_home,
        "read",
        &["ztok", "--verbose", "read", "--help"],
        &["读取文件并智能过滤"],
    )?;

    assert_parse_error_without_fallback(
        &codex_home,
        "read",
        &["ztok", "-vv", "read", "--bogus-flag"],
        "unexpected argument '--bogus-flag'",
    )?;

    Ok(())
}

#[test]
fn ztok_removed_meta_commands_stay_in_parse_error_path_after_global_flags_matrix() -> Result<()> {
    let codex_home = TempDir::new()?;
    let cases: &[(&[&str], &str)] =
        &[(&["ztok", "--verbose", "rewrite"], "unrecognized subcommand")];

    for (args, stderr_pattern) in cases {
        assert_parse_error_without_fallback(&codex_home, "rewrite", args, stderr_pattern)?;
    }

    Ok(())
}

#[test]
fn ztok_removed_meta_commands_still_do_not_fall_through_after_double_dash() -> Result<()> {
    let codex_home = TempDir::new()?;
    let bin_dir = codex_home.path().join("bin");
    std::fs::create_dir(&bin_dir)?;
    let _fake_rewrite = write_fake_command(
        &bin_dir,
        "rewrite",
        fallback_marker_script("FALLBACK_TRIGGERED"),
    )?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.env("PATH", prepend_path(&bin_dir))
        .args(["ztok", "--verbose", "--", "rewrite"])
        .assert()
        .failure()
        .stderr(contains("unrecognized subcommand 'rewrite'"))
        .stdout(contains("FALLBACK_TRIGGERED").not());

    Ok(())
}

#[test]
fn ztok_unknown_commands_still_fall_back_after_global_flags() -> Result<()> {
    let codex_home = TempDir::new()?;
    let bin_dir = codex_home.path().join("bin");
    std::fs::create_dir(&bin_dir)?;
    let _fake_external = write_fake_command(
        &bin_dir,
        "custom-fallback",
        fallback_marker_script("FALLBACK_OK \"$@\""),
    )?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.env("PATH", prepend_path(&bin_dir))
        .args([
            "ztok",
            "--skip-env",
            "--ultra-compact",
            "custom-fallback",
            "alpha",
            "beta",
        ])
        .assert()
        .success()
        .stdout(
            contains("FALLBACK_OK")
                .and(contains("alpha"))
                .and(contains("beta")),
        );

    Ok(())
}

#[test]
fn ztok_external_commands_still_fall_back_after_double_dash_matrix() -> Result<()> {
    let codex_home = TempDir::new()?;
    let cases: &[(&[&str], &[&str])] = &[
        (
            &["ztok", "--verbose", "--", "custom-fallback", "alpha"],
            &["alpha"],
        ),
        (
            &[
                "ztok",
                "--skip-env",
                "-vv",
                "--",
                "custom-fallback",
                "alpha",
                "beta",
            ],
            &["alpha", "beta"],
        ),
        (
            &["ztok", "--verbose", "--", "custom-fallback", "--help"],
            &["--help"],
        ),
        (
            &[
                "ztok",
                "--skip-env",
                "-vv",
                "--",
                "custom-fallback",
                "--help",
            ],
            &["--help"],
        ),
        (
            &["ztok", "--verbose", "--", "custom-fallback", "--version"],
            &["--version"],
        ),
        (
            &[
                "ztok",
                "--skip-env",
                "-vv",
                "--",
                "custom-fallback",
                "--version",
            ],
            &["--version"],
        ),
    ];

    for (args, required) in cases {
        assert_custom_fallback(&codex_home, args, required)?;
    }

    Ok(())
}

#[test]
fn ztok_builtin_commands_stay_in_parse_error_path_after_double_dash_matrix() -> Result<()> {
    let codex_home = TempDir::new()?;
    let cases: &[&[&str]] = &[
        &["ztok", "--verbose", "--", "read", "--help"],
        &["ztok", "--verbose", "--", "read"],
        &["ztok", "--skip-env", "-vv", "--", "read", "--help"],
        &["ztok", "--skip-env", "-vv", "--", "read", "--version"],
    ];

    for args in cases {
        assert_parse_error_without_fallback(&codex_home, "read", args, "subcommand 'read' exists")?;
    }

    Ok(())
}

#[test]
fn ztok_removed_meta_commands_stay_in_parse_error_path_after_double_dash_matrix() -> Result<()> {
    let codex_home = TempDir::new()?;
    let cases: &[&[&str]] = &[
        &["ztok", "--verbose", "--", "rewrite"],
        &["ztok", "--verbose", "--", "rewrite", "--help"],
        &["ztok", "--skip-env", "-vv", "--", "rewrite"],
        &["ztok", "--skip-env", "-vv", "--", "rewrite", "--help"],
        &["ztok", "--skip-env", "-vv", "--", "rewrite", "--version"],
    ];

    for args in cases {
        assert_parse_error_without_fallback(
            &codex_home,
            "rewrite",
            args,
            "unrecognized subcommand 'rewrite'",
        )?;
    }

    Ok(())
}

#[test]
fn ztok_double_dash_falls_back_to_raw_stdbuf_command() -> Result<()> {
    let codex_home = TempDir::new()?;
    let bin_dir = codex_home.path().join("bin");
    std::fs::create_dir(&bin_dir)?;

    let stdbuf_script = if cfg!(windows) {
        "@echo STDBUF %*\r\nexit /b 0\r\n"
    } else {
        "#!/bin/sh\necho STDBUF $@\n"
    };
    let _ = write_fake_command(&bin_dir, "stdbuf", stdbuf_script)?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.env("PATH", prepend_path(&bin_dir))
        .args(["ztok", "--", "stdbuf", "-oL", "git", "status"])
        .assert()
        .success()
        .stdout(contains("STDBUF -oL git status"));

    Ok(())
}

#[test]
fn ztok_double_dash_falls_back_to_raw_env_command() -> Result<()> {
    let codex_home = TempDir::new()?;
    let env_script = if cfg!(windows) {
        "@echo ENV_OK %*\r\nexit /b 0\r\n"
    } else {
        "#!/bin/sh\necho ENV_OK $@\n"
    };
    assert_raw_external_command(
        &codex_home,
        "env",
        env_script,
        &[
            "ztok", "--", "env", "FOO=1", "nice", "-n", "5", "git", "status",
        ],
        &["ENV_OK FOO=1 nice -n 5 git status"],
    )
}

#[test]
fn ztok_double_dash_preserves_literal_pipe_arg() -> Result<()> {
    let codex_home = TempDir::new()?;
    let env_script = if cfg!(windows) {
        "@echo ENV_QUOTED %*\r\nexit /b 0\r\n"
    } else {
        "#!/bin/sh\necho ENV_QUOTED \"$@\"\n"
    };
    assert_raw_external_command(
        &codex_home,
        "env",
        env_script,
        &["ztok", "--", "env", "FOO=1", "grep", "a|b", "src/main.rs"],
        &["ENV_QUOTED", "FOO=1 grep a|b src/main.rs"],
    )
}

#[test]
fn ztok_double_dash_preserves_git_log_format_literal() -> Result<()> {
    let codex_home = TempDir::new()?;
    let nice_script = if cfg!(windows) {
        "@echo NICE_QUOTED %*\r\nexit /b 0\r\n"
    } else {
        "#!/bin/sh\necho NICE_QUOTED \"$@\"\n"
    };
    assert_raw_external_command(
        &codex_home,
        "nice",
        nice_script,
        &[
            "ztok",
            "--",
            "nice",
            "-n",
            "5",
            "git",
            "log",
            "--format=%h|%s",
            "-1",
        ],
        &["NICE_QUOTED", "git log", "--format=%h|%s", "-1"],
    )
}

#[test]
fn ztok_deps_summarizes_cargo_manifest() -> Result<()> {
    let codex_home = TempDir::new()?;
    std::fs::write(
        codex_home.path().join("Cargo.toml"),
        r#"
[package]
name = "demo"
version = "0.1.0"

[dependencies]
anyhow = "1"
serde = "1"

[dev-dependencies]
pretty_assertions = "1"
"#,
    )?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.args(["ztok", "deps", codex_home.path().to_string_lossy().as_ref()])
        .assert()
        .success()
        .stdout(
            contains("Rust（Cargo.toml）")
                .and(contains("依赖（2）"))
                .and(contains("anyhow (1)"))
                .and(contains("serde (1)")),
        );

    Ok(())
}

#[test]
fn ztok_json_keys_only_hides_values() -> Result<()> {
    let codex_home = TempDir::new()?;
    let file = codex_home.path().join("payload.json");
    std::fs::write(&file, r#"{"token":"secret-value","count":2}"#)?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.args([
        "ztok",
        "json",
        file.to_string_lossy().as_ref(),
        "--keys-only",
    ])
    .assert()
    .success()
    .stdout(contains("token: string").and(contains("secret-value").not()));

    Ok(())
}

#[test]
fn ztok_vitest_drops_reporter_value_pair() -> Result<()> {
    let codex_home = TempDir::new()?;
    let bin_dir = create_fake_bin_dir(&codex_home)?;
    let _fake_vitest = write_fake_command(&bin_dir, "vitest", echo_args_script())?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.env("PATH", prepend_path(&bin_dir))
        .args(["ztok", "vitest", "--reporter", "verbose", "sample.test.ts"])
        .assert()
        .success()
        .stdout(
            contains("--reporter=json")
                .and(contains("sample.test.ts"))
                .and(contains("verbose").not()),
        );

    Ok(())
}

#[test]
fn ztok_double_dash_tail_fallback_stays_raw() -> Result<()> {
    let codex_home = TempDir::new()?;
    let file = codex_home.path().join("sample.txt");
    std::fs::write(&file, "one\ntwo\nthree\n")?;

    let tail_script = if cfg!(windows) {
        "@echo TAIL_RAW %*\r\nexit /b 0\r\n"
    } else {
        "#!/bin/sh\necho TAIL_RAW $@\n"
    };
    assert_raw_external_command(
        &codex_home,
        "tail",
        tail_script,
        &["ztok", "--", "tail", "-f", file.to_string_lossy().as_ref()],
        &["TAIL_RAW"],
    )
}

#[test]
fn ztok_double_dash_chrt_fallback_stays_raw() -> Result<()> {
    let codex_home = TempDir::new()?;
    let chrt_script = if cfg!(windows) {
        "@echo CHRT_RAW %*\r\nexit /b 0\r\n"
    } else {
        "#!/bin/sh\necho CHRT_RAW $@\n"
    };
    assert_raw_external_command(
        &codex_home,
        "chrt",
        chrt_script,
        &["ztok", "--", "chrt", "-m", "1", "git", "status"],
        &["CHRT_RAW"],
    )
}

#[test]
fn ztok_git_status_defaults_to_short_output() -> Result<()> {
    let codex_home = TempDir::new()?;
    let repo = codex_home.path().join("repo");
    std::fs::create_dir(&repo)?;
    init_git_repo(&repo)?;
    std::fs::write(repo.join("new.txt"), "hello\n")?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.current_dir(&repo)
        .args(["ztok", "git", "status"])
        .assert()
        .success()
        .stdout(contains("? 未跟踪：1 个文件").and(contains("new.txt")));

    Ok(())
}

#[test]
fn ztok_git_status_with_flags_keeps_git_exit_code() -> Result<()> {
    let codex_home = TempDir::new()?;
    let workspace = codex_home.path().join("workspace");
    std::fs::create_dir(&workspace)?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.current_dir(&workspace)
        .args(["ztok", "git", "status", "--short"])
        .assert()
        .code(128)
        .stderr(contains("not a git repository"));

    Ok(())
}

#[test]
fn ztok_git_status_reports_clean_tree() -> Result<()> {
    let codex_home = TempDir::new()?;
    let repo = codex_home.path().join("repo");
    std::fs::create_dir(&repo)?;
    init_git_repo(&repo)?;
    std::fs::write(repo.join("tracked.txt"), "hello\n")?;
    run_command(
        Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["add", "tracked.txt"]),
    )?;
    run_command(
        Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["commit", "-m", "chore: init"]),
    )?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.current_dir(&repo)
        .args(["ztok", "git", "status"])
        .assert()
        .success()
        .stdout(contains("* ").and(contains("干净 — 没有可提交内容")));

    Ok(())
}

#[test]
fn ztok_git_branch_show_current_passthroughs_stdout() -> Result<()> {
    let codex_home = TempDir::new()?;
    let repo = codex_home.path().join("repo");
    std::fs::create_dir(&repo)?;
    init_git_repo(&repo)?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.current_dir(&repo)
        .args(["ztok", "git", "branch", "--show-current"])
        .assert()
        .success()
        .stdout(contains("main").or(contains("master")));

    Ok(())
}

#[test]
fn ztok_git_log_preserves_first_commit_body_line() -> Result<()> {
    let codex_home = TempDir::new()?;
    let repo = codex_home.path().join("repo");
    std::fs::create_dir(&repo)?;

    init_git_repo(&repo)?;
    std::fs::write(repo.join("note.txt"), "body\n")?;
    run_command(
        Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["add", "note.txt"]),
    )?;
    run_command(Command::new("git").arg("-C").arg(&repo).args([
        "commit",
        "-m",
        "feat: preserve body",
        "-m",
        "BREAKING CHANGE: body line stays visible",
    ]))?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.current_dir(&repo)
        .args(["ztok", "git", "log", "-1"])
        .assert()
        .success()
        .stdout(
            contains("feat: preserve body")
                .and(contains("BREAKING CHANGE: body line stays visible")),
        );

    Ok(())
}

#[test]
fn ztok_git_log_respects_user_oneline_format() -> Result<()> {
    let codex_home = TempDir::new()?;
    let repo = codex_home.path().join("repo");
    std::fs::create_dir(&repo)?;

    init_git_repo(&repo)?;
    std::fs::write(repo.join("note.txt"), "oneline\n")?;
    run_command(
        Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["add", "note.txt"]),
    )?;
    run_command(Command::new("git").arg("-C").arg(&repo).args([
        "commit",
        "-m",
        "fix: oneline output",
    ]))?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.current_dir(&repo)
        .args(["ztok", "git", "log", "--oneline", "-1"])
        .assert()
        .success()
        .stdout(contains("fix: oneline output").and(contains("---END---").not()));

    Ok(())
}

#[test]
fn ztok_grep_adds_filename_and_line_number() -> Result<()> {
    let codex_home = TempDir::new()?;
    let workspace = codex_home.path().join("search");
    std::fs::create_dir(&workspace)?;
    std::fs::write(workspace.join("sample.txt"), "alpha\nneedle here\nomega\n")?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.current_dir(&workspace)
        .args(["ztok", "grep", "needle", "."])
        .assert()
        .success()
        .stdout(contains("sample.txt").and(contains("2: needle here")));

    Ok(())
}

#[test]
fn ztok_grep_handles_recursive_flag_without_replace_mode() -> Result<()> {
    let codex_home = TempDir::new()?;
    let workspace = codex_home.path().join("search");
    std::fs::create_dir(&workspace)?;
    std::fs::create_dir(workspace.join("nested"))?;
    std::fs::write(
        workspace.join("nested").join("sample.txt"),
        "alpha\nneedle here\nomega\n",
    )?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.current_dir(&workspace)
        .args(["ztok", "grep", "needle", ".", "-r"])
        .assert()
        .success()
        .stdout(contains("sample.txt").and(contains("2: needle here")));

    Ok(())
}

#[test]
fn ztok_grep_accepts_leading_grep_flags_and_excludes() -> Result<()> {
    let codex_home = TempDir::new()?;
    let workspace = codex_home.path().join("search");
    std::fs::create_dir(&workspace)?;
    std::fs::create_dir(workspace.join("node_modules"))?;
    std::fs::write(workspace.join("keep.txt"), "alpha\nneedle here\nomega\n")?;
    std::fs::write(
        workspace.join("node_modules").join("ignored.txt"),
        "needle should stay excluded\n",
    )?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.current_dir(&workspace)
        .args([
            "ztok",
            "grep",
            "-RInE",
            "needle",
            ".",
            "--exclude-dir=node_modules",
        ])
        .assert()
        .success()
        .stdout(contains("keep.txt").and(contains("ignored.txt").not()));

    Ok(())
}

#[test]
fn ztok_log_keeps_interesting_lines() -> Result<()> {
    let codex_home = TempDir::new()?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.args(["ztok", "log"])
        .write_stdin("info\nwarning: heads up\nerror: boom\n")
        .assert()
        .success()
        .stdout(
            contains("1 个错误（1 个唯一）")
                .and(contains("1 个警告（1 个唯一）"))
                .and(contains("warning: heads up"))
                .and(contains("error: boom")),
        );

    Ok(())
}

#[test]
fn ztok_log_falls_back_to_last_40_lines_without_matches() -> Result<()> {
    let codex_home = TempDir::new()?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.args(["ztok", "log"])
        .write_stdin((1..=50).map(|i| format!("line-{i}\n")).collect::<String>())
        .assert()
        .success()
        .stdout(
            contains("0 个错误（0 个唯一）")
                .and(contains("0 个警告（0 个唯一）"))
                .and(contains("0 条信息")),
        );

    Ok(())
}

#[test]
fn ztok_test_filters_failure_output_and_keeps_exit_code() -> Result<()> {
    let codex_home = TempDir::new()?;

    let mut cmd = codex_command(codex_home.path())?;
    let shell = shell_args(
        "echo running 2 tests; echo ok 1; echo FAILED test_x; echo test result: FAILED; exit 9",
    );
    cmd.args(["ztok", "test"])
        .args(shell)
        .assert()
        .code(9)
        .stdout(contains("FAILED test_x").and(contains("test result: FAILED")));

    Ok(())
}

#[cfg(unix)]
#[test]
fn ztok_shell_raw_preserves_stdout_and_stderr() -> Result<()> {
    let codex_home = TempDir::new()?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.args([
        "ztok",
        "shell",
        "sh",
        "-c",
        "printf 'alpha\\n'; printf 'beta\\n' >&2",
    ])
    .assert()
    .success()
    .stdout(contains("alpha").and(contains("beta")));

    Ok(())
}

#[cfg(unix)]
#[test]
fn ztok_shell_err_filter_keeps_error_context() -> Result<()> {
    let codex_home = TempDir::new()?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.args([
        "ztok",
        "shell",
        "--filter",
        "err",
        "sh",
        "-c",
        "printf 'before\\nwarning: boom\\nafter\\n'",
    ])
    .assert()
    .success()
    .stdout(
        contains("before")
            .and(contains("warning: boom"))
            .and(contains("after")),
    );

    Ok(())
}

#[cfg(unix)]
#[test]
fn ztok_err_keeps_one_line_of_context_on_each_side() -> Result<()> {
    let codex_home = TempDir::new()?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.args([
        "ztok",
        "err",
        "sh",
        "-c",
        "printf 'before\\nwarning: boom\\nafter\\n'",
    ])
    .assert()
    .success()
    .stdout(
        contains("before")
            .and(contains("warning: boom"))
            .and(contains("after")),
    );

    Ok(())
}

#[cfg(unix)]
#[test]
fn ztok_err_preserves_non_zero_exit_code() -> Result<()> {
    let codex_home = TempDir::new()?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.args(["ztok", "err", "sh", "-c", "echo boom >&2; exit 7"])
        .assert()
        .code(7)
        .stdout(contains("boom"));

    Ok(())
}

#[cfg(unix)]
#[test]
fn ztok_summary_preserves_non_zero_exit_code() -> Result<()> {
    let codex_home = TempDir::new()?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.args(["ztok", "summary", "sh", "-c", "echo boom >&2; exit 9"])
        .assert()
        .code(9)
        .stdout(contains("❌ 命令：").and(contains("boom")));

    Ok(())
}

#[test]
fn ztok_tree_ignores_noise_dirs_by_default() -> Result<()> {
    if !command_exists("tree") {
        return Ok(());
    }

    let codex_home = TempDir::new()?;
    let workspace = codex_home.path().join("tree-workspace");
    std::fs::create_dir(&workspace)?;
    std::fs::create_dir(workspace.join("src"))?;
    std::fs::create_dir_all(workspace.join("node_modules").join("pkg"))?;
    std::fs::write(workspace.join("src").join("main.rs"), "fn main() {}\n")?;
    std::fs::write(
        workspace.join("node_modules").join("pkg").join("index.js"),
        "console.log('noise')\n",
    )?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.args(["ztok", "tree", workspace.to_string_lossy().as_ref()])
        .assert()
        .success()
        .stdout(contains("src").and(predicates::str::contains("node_modules").not()));

    Ok(())
}

#[test]
fn ztok_tree_respects_all_flag() -> Result<()> {
    if !command_exists("tree") {
        return Ok(());
    }

    let codex_home = TempDir::new()?;
    let workspace = codex_home.path().join("tree-workspace");
    std::fs::create_dir(&workspace)?;
    std::fs::create_dir_all(workspace.join("node_modules").join("pkg"))?;
    std::fs::write(
        workspace.join("node_modules").join("pkg").join("index.js"),
        "console.log('noise')\n",
    )?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.args(["ztok", "tree", "-a", workspace.to_string_lossy().as_ref()])
        .assert()
        .success()
        .stdout(contains("node_modules"));

    Ok(())
}

#[cfg(windows)]
#[test]
fn ztok_err_preserves_non_zero_exit_code() -> Result<()> {
    let codex_home = TempDir::new()?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.args(["ztok", "err", "cmd", "/C", "echo boom 1>&2 & exit /b 7"])
        .assert()
        .code(7)
        .stdout(contains("boom"));

    Ok(())
}

#[cfg(windows)]
#[test]
fn ztok_summary_preserves_non_zero_exit_code() -> Result<()> {
    let codex_home = TempDir::new()?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.args(["ztok", "summary", "cmd", "/C", "echo boom 1>&2 & exit /b 9"])
        .assert()
        .code(9)
        .stdout(contains("❌ 命令：").and(contains("boom")));

    Ok(())
}
