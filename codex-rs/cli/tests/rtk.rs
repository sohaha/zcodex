use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use anyhow::Result;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use tempfile::TempDir;

fn codex_command(codex_home: &Path) -> Result<assert_cmd::Command> {
    let mut cmd = assert_cmd::Command::new(codex_utils_cargo_bin::cargo_bin("codex")?);
    cmd.env("CODEX_HOME", codex_home);
    Ok(cmd)
}

fn run_command(command: &mut Command) -> Result<()> {
    let status = command.status()?;
    anyhow::ensure!(status.success(), "command failed with status {status}");
    Ok(())
}

fn init_git_repo(repo: &Path) -> Result<()> {
    run_command(Command::new("git").arg("init").arg(repo))?;
    run_command(
        Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["config", "user.name", "RTK Test"]),
    )?;
    run_command(Command::new("git").arg("-C").arg(repo).args([
        "config",
        "user.email",
        "rtk@example.com",
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
fn rtk_read_limits_output() -> Result<()> {
    let codex_home = TempDir::new()?;
    let file = codex_home.path().join("sample.txt");
    std::fs::write(&file, "one\ntwo\nthree\n")?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.args([
        "rtk",
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
fn rtk_read_tail_lines() -> Result<()> {
    let codex_home = TempDir::new()?;
    let file = codex_home.path().join("sample.txt");
    std::fs::write(&file, "one\ntwo\nthree\n")?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.args([
        "rtk",
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

#[cfg(unix)]
fn make_rtk_alias(codex_home: &Path) -> Result<PathBuf> {
    let alias = codex_home.join("rtk");
    std::os::unix::fs::symlink(codex_utils_cargo_bin::cargo_bin("codex")?, &alias)?;
    Ok(alias)
}

#[cfg(windows)]
fn make_rtk_alias(codex_home: &Path) -> Result<PathBuf> {
    let alias = codex_home.join("rtk.bat");
    std::fs::write(
        &alias,
        format!(
            "@echo off\r\n\"{}\" rtk %*\r\n",
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
    cmd.args(args).assert().success().stdout(contains("rtk "));
    Ok(())
}

#[test]
fn rtk_alias_routes_to_rtk_parser() -> Result<()> {
    let codex_home = TempDir::new()?;
    let file = codex_home.path().join("alias.txt");
    std::fs::write(&file, "alpha\nbeta\n")?;
    let alias = make_rtk_alias(codex_home.path())?;

    let mut cmd = assert_cmd::Command::new(alias);
    cmd.env("CODEX_HOME", codex_home.path())
        .args(["read", file.to_string_lossy().as_ref()])
        .assert()
        .success()
        .stdout(contains("alpha").and(contains("beta")));

    Ok(())
}

#[test]
fn rtk_help_exposes_codex_curated_command_surface() -> Result<()> {
    let codex_home = TempDir::new()?;
    let cases = [
        (
            vec!["rtk", "--help"],
            vec!["gh", "env", "wget", "golangci-lint", "cargo", "summary"],
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
            ],
        ),
        (
            vec!["rtk", "--verbose", "--help"],
            vec!["高性能 CLI 代理", "golangci-lint"],
            vec!["rewrite"],
        ),
        (
            vec!["rtk", "git", "--help"],
            vec!["status", "log", "diff"],
            vec!["rewrite"],
        ),
        (
            vec!["rtk", "--verbose", "git", "--help"],
            vec!["Git 命令，紧凑输出", "status"],
            vec!["rewrite"],
        ),
        (
            vec!["rtk", "read", "--help"],
            vec!["读取文件并智能过滤", "--max-lines", "--tail-lines"],
            vec!["rewrite"],
        ),
        (
            vec!["rtk", "--verbose", "read", "--help"],
            vec!["读取文件并智能过滤", "--max-lines", "--tail-lines"],
            vec!["rewrite"],
        ),
    ];

    for (args, required, forbidden) in cases {
        assert_help_contains(codex_home.path(), &args, &required, &forbidden)?;
    }

    Ok(())
}

#[test]
fn rtk_version_still_works_with_global_flags() -> Result<()> {
    let codex_home = TempDir::new()?;

    for args in [
        vec!["rtk", "--version"],
        vec!["rtk", "--verbose", "--version"],
        vec!["rtk", "-u", "--version"],
    ] {
        assert_version_contains(codex_home.path(), &args)?;
    }

    Ok(())
}

#[test]
fn rtk_removed_meta_commands_fail_instead_of_falling_through() -> Result<()> {
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
        cmd.args(["rtk", command_name])
            .assert()
            .failure()
            .stderr(contains("unrecognized subcommand"));
    }

    Ok(())
}

#[test]
fn rtk_builtin_parse_errors_do_not_fall_back_to_external_commands() -> Result<()> {
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
        .args(["rtk", "read", "--bogus-flag"])
        .assert()
        .failure()
        .stderr(contains("unexpected argument '--bogus-flag'"))
        .stdout(contains("FALLBACK_TRIGGERED").not());

    Ok(())
}

#[test]
fn rtk_builtin_help_does_not_fall_back_to_external_commands_after_global_flags() -> Result<()> {
    let codex_home = TempDir::new()?;
    let bin_dir = codex_home.path().join("bin");
    std::fs::create_dir(&bin_dir)?;
    let _fake_read = write_fake_command(
        &bin_dir,
        "read",
        fallback_marker_script("FALLBACK_TRIGGERED"),
    )?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.env("PATH", prepend_path(&bin_dir))
        .args(["rtk", "--verbose", "read", "--help"])
        .assert()
        .success()
        .stdout(contains("读取文件并智能过滤"))
        .stdout(contains("FALLBACK_TRIGGERED").not());

    Ok(())
}

#[test]
fn rtk_builtin_parse_errors_do_not_fall_back_after_global_flags() -> Result<()> {
    let codex_home = TempDir::new()?;
    let bin_dir = codex_home.path().join("bin");
    std::fs::create_dir(&bin_dir)?;
    let _fake_read = write_fake_command(
        &bin_dir,
        "read",
        fallback_marker_script("FALLBACK_TRIGGERED"),
    )?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.env("PATH", prepend_path(&bin_dir))
        .args(["rtk", "-vv", "read", "--bogus-flag"])
        .assert()
        .failure()
        .stderr(contains("unexpected argument '--bogus-flag'"))
        .stdout(contains("FALLBACK_TRIGGERED").not());

    Ok(())
}

#[test]
fn rtk_removed_meta_commands_still_do_not_fall_through_after_global_flags() -> Result<()> {
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
        .args(["rtk", "--verbose", "rewrite"])
        .assert()
        .failure()
        .stderr(contains("unrecognized subcommand"))
        .stdout(contains("FALLBACK_TRIGGERED").not());

    Ok(())
}

#[test]
fn rtk_removed_meta_commands_still_do_not_fall_through_after_double_dash() -> Result<()> {
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
        .args(["rtk", "--verbose", "--", "rewrite"])
        .assert()
        .failure()
        .stderr(contains("unrecognized subcommand 'rewrite'"))
        .stdout(contains("FALLBACK_TRIGGERED").not());

    Ok(())
}

#[test]
fn rtk_unknown_commands_still_fall_back_after_global_flags() -> Result<()> {
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
            "rtk",
            "--skip-env",
            "-u",
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
fn rtk_unknown_commands_still_fall_back_after_double_dash() -> Result<()> {
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
        .args(["rtk", "--verbose", "--", "custom-fallback", "alpha"])
        .assert()
        .success()
        .stdout(contains("FALLBACK_OK").and(contains("alpha")));

    Ok(())
}

#[test]
fn rtk_unknown_commands_still_fall_back_after_global_flags_and_double_dash() -> Result<()> {
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
            "rtk",
            "--skip-env",
            "-vv",
            "--",
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
fn rtk_external_help_still_falls_back_after_double_dash() -> Result<()> {
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
        .args(["rtk", "--verbose", "--", "custom-fallback", "--help"])
        .assert()
        .success()
        .stdout(contains("FALLBACK_OK").and(contains("--help")));

    Ok(())
}

#[test]
fn rtk_external_help_still_falls_back_after_global_flags_and_double_dash() -> Result<()> {
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
            "rtk",
            "--skip-env",
            "-vv",
            "--",
            "custom-fallback",
            "--help",
        ])
        .assert()
        .success()
        .stdout(contains("FALLBACK_OK").and(contains("--help")));

    Ok(())
}

#[test]
fn rtk_external_version_still_falls_back_after_double_dash() -> Result<()> {
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
        .args(["rtk", "--verbose", "--", "custom-fallback", "--version"])
        .assert()
        .success()
        .stdout(contains("FALLBACK_OK").and(contains("--version")));

    Ok(())
}

#[test]
fn rtk_builtin_help_after_global_flags_and_double_dash_stays_in_parse_error_path() -> Result<()> {
    let codex_home = TempDir::new()?;
    let bin_dir = codex_home.path().join("bin");
    std::fs::create_dir(&bin_dir)?;
    let _fake_read = write_fake_command(
        &bin_dir,
        "read",
        fallback_marker_script("FALLBACK_TRIGGERED"),
    )?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.env("PATH", prepend_path(&bin_dir))
        .args(["rtk", "--skip-env", "-vv", "--", "read", "--help"])
        .assert()
        .failure()
        .stderr(contains("subcommand 'read' exists"))
        .stdout(contains("FALLBACK_TRIGGERED").not());

    Ok(())
}

#[test]
fn rtk_builtin_help_after_double_dash_stays_in_parse_error_path() -> Result<()> {
    let codex_home = TempDir::new()?;
    let bin_dir = codex_home.path().join("bin");
    std::fs::create_dir(&bin_dir)?;
    let _fake_read = write_fake_command(
        &bin_dir,
        "read",
        fallback_marker_script("FALLBACK_TRIGGERED"),
    )?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.env("PATH", prepend_path(&bin_dir))
        .args(["rtk", "--verbose", "--", "read", "--help"])
        .assert()
        .failure()
        .stderr(contains("subcommand 'read' exists"))
        .stdout(contains("FALLBACK_TRIGGERED").not());

    Ok(())
}

#[test]
fn rtk_removed_meta_after_global_flags_and_double_dash_stays_in_parse_error_path() -> Result<()> {
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
        .args(["rtk", "--skip-env", "-vv", "--", "rewrite"])
        .assert()
        .failure()
        .stderr(contains("unrecognized subcommand 'rewrite'"))
        .stdout(contains("FALLBACK_TRIGGERED").not());

    Ok(())
}

#[test]
fn rtk_removed_meta_help_after_double_dash_stays_in_parse_error_path() -> Result<()> {
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
        .args(["rtk", "--verbose", "--", "rewrite", "--help"])
        .assert()
        .failure()
        .stderr(contains("unrecognized subcommand 'rewrite'"))
        .stdout(contains("FALLBACK_TRIGGERED").not());

    Ok(())
}

#[test]
fn rtk_builtin_command_after_double_dash_stays_in_parse_error_path() -> Result<()> {
    let codex_home = TempDir::new()?;
    let bin_dir = codex_home.path().join("bin");
    std::fs::create_dir(&bin_dir)?;
    let _fake_read = write_fake_command(
        &bin_dir,
        "read",
        fallback_marker_script("FALLBACK_TRIGGERED"),
    )?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.env("PATH", prepend_path(&bin_dir))
        .args(["rtk", "--verbose", "--", "read"])
        .assert()
        .failure()
        .stderr(contains("subcommand 'read' exists"))
        .stdout(contains("FALLBACK_TRIGGERED").not());

    Ok(())
}

#[test]
fn rtk_double_dash_falls_back_to_raw_stdbuf_command() -> Result<()> {
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
        .args(["rtk", "--", "stdbuf", "-oL", "git", "status"])
        .assert()
        .success()
        .stdout(contains("STDBUF -oL git status"));

    Ok(())
}

#[test]
fn rtk_double_dash_falls_back_to_raw_env_command() -> Result<()> {
    let codex_home = TempDir::new()?;
    let bin_dir = codex_home.path().join("bin");
    std::fs::create_dir(&bin_dir)?;

    let env_script = if cfg!(windows) {
        "@echo ENV_OK %*\r\nexit /b 0\r\n"
    } else {
        "#!/bin/sh\necho ENV_OK $@\n"
    };
    let _ = write_fake_command(&bin_dir, "env", env_script)?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.env("PATH", prepend_path(&bin_dir))
        .args([
            "rtk", "--", "env", "FOO=1", "nice", "-n", "5", "git", "status",
        ])
        .assert()
        .success()
        .stdout(contains("ENV_OK FOO=1 nice -n 5 git status"));

    Ok(())
}

#[test]
fn rtk_double_dash_preserves_literal_pipe_arg() -> Result<()> {
    let codex_home = TempDir::new()?;
    let bin_dir = codex_home.path().join("bin");
    std::fs::create_dir(&bin_dir)?;

    let env_script = if cfg!(windows) {
        "@echo ENV_QUOTED %*\r\nexit /b 0\r\n"
    } else {
        "#!/bin/sh\necho ENV_QUOTED \"$@\"\n"
    };
    let _ = write_fake_command(&bin_dir, "env", env_script)?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.env("PATH", prepend_path(&bin_dir))
        .args(["rtk", "--", "env", "FOO=1", "grep", "a|b", "src/main.rs"])
        .assert()
        .success()
        .stdout(contains("ENV_QUOTED").and(contains("FOO=1 grep a|b src/main.rs")));

    Ok(())
}

#[test]
fn rtk_double_dash_preserves_git_log_format_literal() -> Result<()> {
    let codex_home = TempDir::new()?;
    let bin_dir = codex_home.path().join("bin");
    std::fs::create_dir(&bin_dir)?;

    let nice_script = if cfg!(windows) {
        "@echo NICE_QUOTED %*\r\nexit /b 0\r\n"
    } else {
        "#!/bin/sh\necho NICE_QUOTED \"$@\"\n"
    };
    let _ = write_fake_command(&bin_dir, "nice", nice_script)?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.env("PATH", prepend_path(&bin_dir))
        .args([
            "rtk",
            "--",
            "nice",
            "-n",
            "5",
            "git",
            "log",
            "--format=%h|%s",
            "-1",
        ])
        .assert()
        .success()
        .stdout(
            contains("NICE_QUOTED")
                .and(contains("git log"))
                .and(contains("--format=%h|%s"))
                .and(contains("-1")),
        );

    Ok(())
}

#[test]
fn rtk_deps_summarizes_cargo_manifest() -> Result<()> {
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
    cmd.args(["rtk", "deps", codex_home.path().to_string_lossy().as_ref()])
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
fn rtk_double_dash_tail_fallback_stays_raw() -> Result<()> {
    let codex_home = TempDir::new()?;
    let bin_dir = codex_home.path().join("bin");
    std::fs::create_dir(&bin_dir)?;
    let file = codex_home.path().join("sample.txt");
    std::fs::write(&file, "one\ntwo\nthree\n")?;

    let tail_script = if cfg!(windows) {
        "@echo TAIL_RAW %*\r\nexit /b 0\r\n"
    } else {
        "#!/bin/sh\necho TAIL_RAW $@\n"
    };
    let _ = write_fake_command(&bin_dir, "tail", tail_script)?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.env("PATH", prepend_path(&bin_dir))
        .args(["rtk", "--", "tail", "-f", file.to_string_lossy().as_ref()])
        .assert()
        .success()
        .stdout(contains("TAIL_RAW"));

    Ok(())
}

#[test]
fn rtk_double_dash_chrt_fallback_stays_raw() -> Result<()> {
    let codex_home = TempDir::new()?;
    let bin_dir = codex_home.path().join("bin");
    std::fs::create_dir(&bin_dir)?;

    let chrt_script = if cfg!(windows) {
        "@echo CHRT_RAW %*\r\nexit /b 0\r\n"
    } else {
        "#!/bin/sh\necho CHRT_RAW $@\n"
    };
    let _ = write_fake_command(&bin_dir, "chrt", chrt_script)?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.env("PATH", prepend_path(&bin_dir))
        .args(["rtk", "--", "chrt", "-m", "1", "git", "status"])
        .assert()
        .success()
        .stdout(contains("CHRT_RAW"));

    Ok(())
}

#[test]
fn rtk_git_status_defaults_to_short_output() -> Result<()> {
    let codex_home = TempDir::new()?;
    let repo = codex_home.path().join("repo");
    std::fs::create_dir(&repo)?;
    init_git_repo(&repo)?;
    std::fs::write(repo.join("new.txt"), "hello\n")?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.current_dir(&repo)
        .args(["rtk", "git", "status"])
        .assert()
        .success()
        .stdout(contains("? 未跟踪：1 个文件").and(contains("new.txt")));

    Ok(())
}

#[test]
fn rtk_git_status_with_flags_keeps_git_exit_code() -> Result<()> {
    let codex_home = TempDir::new()?;
    let workspace = codex_home.path().join("workspace");
    std::fs::create_dir(&workspace)?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.current_dir(&workspace)
        .args(["rtk", "git", "status", "--short"])
        .assert()
        .code(128)
        .stderr(contains("not a git repository"));

    Ok(())
}

#[test]
fn rtk_git_status_reports_clean_tree() -> Result<()> {
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
        .args(["rtk", "git", "status"])
        .assert()
        .success()
        .stdout(contains("* ").and(contains("干净 — 没有可提交内容")));

    Ok(())
}

#[test]
fn rtk_git_branch_show_current_passthroughs_stdout() -> Result<()> {
    let codex_home = TempDir::new()?;
    let repo = codex_home.path().join("repo");
    std::fs::create_dir(&repo)?;
    init_git_repo(&repo)?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.current_dir(&repo)
        .args(["rtk", "git", "branch", "--show-current"])
        .assert()
        .success()
        .stdout(contains("main").or(contains("master")));

    Ok(())
}

#[test]
fn rtk_git_log_preserves_first_commit_body_line() -> Result<()> {
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
        .args(["rtk", "git", "log", "-1"])
        .assert()
        .success()
        .stdout(
            contains("feat: preserve body")
                .and(contains("BREAKING CHANGE: body line stays visible")),
        );

    Ok(())
}

#[test]
fn rtk_git_log_respects_user_oneline_format() -> Result<()> {
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
        .args(["rtk", "git", "log", "--oneline", "-1"])
        .assert()
        .success()
        .stdout(contains("fix: oneline output").and(contains("---END---").not()));

    Ok(())
}

#[test]
fn rtk_grep_adds_filename_and_line_number() -> Result<()> {
    let codex_home = TempDir::new()?;
    let workspace = codex_home.path().join("search");
    std::fs::create_dir(&workspace)?;
    std::fs::write(workspace.join("sample.txt"), "alpha\nneedle here\nomega\n")?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.current_dir(&workspace)
        .args(["rtk", "grep", "needle", "."])
        .assert()
        .success()
        .stdout(contains("sample.txt").and(contains("2: needle here")));

    Ok(())
}

#[test]
fn rtk_grep_handles_recursive_flag_without_replace_mode() -> Result<()> {
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
        .args(["rtk", "grep", "needle", ".", "-r"])
        .assert()
        .success()
        .stdout(contains("sample.txt").and(contains("2: needle here")));

    Ok(())
}

#[test]
fn rtk_log_keeps_interesting_lines() -> Result<()> {
    let codex_home = TempDir::new()?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.args(["rtk", "log"])
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
fn rtk_log_falls_back_to_last_40_lines_without_matches() -> Result<()> {
    let codex_home = TempDir::new()?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.args(["rtk", "log"])
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
fn rtk_test_filters_failure_output_and_keeps_exit_code() -> Result<()> {
    let codex_home = TempDir::new()?;

    let mut cmd = codex_command(codex_home.path())?;
    let shell = shell_args(
        "echo running 2 tests; echo ok 1; echo FAILED test_x; echo test result: FAILED; exit 9",
    );
    cmd.args(["rtk", "test"])
        .args(shell)
        .assert()
        .code(9)
        .stdout(contains("FAILED test_x").and(contains("test result: FAILED")));

    Ok(())
}

#[cfg(unix)]
#[test]
fn rtk_err_keeps_one_line_of_context_on_each_side() -> Result<()> {
    let codex_home = TempDir::new()?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.args([
        "rtk",
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
fn rtk_err_preserves_non_zero_exit_code() -> Result<()> {
    let codex_home = TempDir::new()?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.args(["rtk", "err", "sh", "-c", "echo boom >&2; exit 7"])
        .assert()
        .code(7)
        .stdout(contains("boom"));

    Ok(())
}

#[cfg(unix)]
#[test]
fn rtk_summary_preserves_non_zero_exit_code() -> Result<()> {
    let codex_home = TempDir::new()?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.args(["rtk", "summary", "sh", "-c", "echo boom >&2; exit 9"])
        .assert()
        .code(9)
        .stdout(contains("❌ 命令：").and(contains("boom")));

    Ok(())
}

#[test]
fn rtk_tree_ignores_noise_dirs_by_default() -> Result<()> {
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
    cmd.args(["rtk", "tree", workspace.to_string_lossy().as_ref()])
        .assert()
        .success()
        .stdout(contains("src").and(predicates::str::contains("node_modules").not()));

    Ok(())
}

#[test]
fn rtk_tree_respects_all_flag() -> Result<()> {
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
    cmd.args(["rtk", "tree", "-a", workspace.to_string_lossy().as_ref()])
        .assert()
        .success()
        .stdout(contains("node_modules"));

    Ok(())
}

#[cfg(windows)]
#[test]
fn rtk_err_preserves_non_zero_exit_code() -> Result<()> {
    let codex_home = TempDir::new()?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.args(["rtk", "err", "cmd", "/C", "echo boom 1>&2 & exit /b 7"])
        .assert()
        .code(7)
        .stdout(contains("boom"));

    Ok(())
}

#[cfg(windows)]
#[test]
fn rtk_summary_preserves_non_zero_exit_code() -> Result<()> {
    let codex_home = TempDir::new()?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.args(["rtk", "summary", "cmd", "/C", "echo boom 1>&2 & exit /b 9"])
        .assert()
        .code(9)
        .stdout(contains("❌ 命令：").and(contains("boom")));

    Ok(())
}
