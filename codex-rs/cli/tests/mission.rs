use std::path::Path;

use anyhow::Result;
use predicates::prelude::PredicateBooleanExt;
use predicates::prelude::predicate;
use tempfile::TempDir;

fn codex_command(codex_home: &Path) -> Result<assert_cmd::Command> {
    let mut cmd = assert_cmd::Command::new(codex_utils_cargo_bin::cargo_bin("codex")?);
    cmd.env("CODEX_HOME", codex_home);
    Ok(cmd)
}

#[tokio::test]
async fn mission_help_renders() -> Result<()> {
    let codex_home = TempDir::new()?;
    codex_command(codex_home.path())?
        .args(["mission", "--help"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("管理 Mission 工程工作流")
                .and(predicate::str::contains("start"))
                .and(predicate::str::contains("status"))
                .and(predicate::str::contains("continue")),
        );
    Ok(())
}

#[tokio::test]
async fn mission_status_reports_empty_state() -> Result<()> {
    let codex_home = TempDir::new()?;
    let workspace = TempDir::new()?;
    codex_command(codex_home.path())?
        .current_dir(workspace.path())
        .args(["mission", "status"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Mission 状态：未启动")
                .and(predicate::str::contains(".mission/mission_state.json")),
        );
    Ok(())
}

#[tokio::test]
async fn mission_start_and_continue_persist_state() -> Result<()> {
    let codex_home = TempDir::new()?;
    let workspace = TempDir::new()?;
    codex_command(codex_home.path())?
        .current_dir(workspace.path())
        .args(["mission", "start", "测试目标"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Mission 状态：planning")
                .and(predicate::str::contains("当前阶段：目标澄清")),
        );
    codex_command(codex_home.path())?
        .current_dir(workspace.path())
        .args(["mission", "continue", "--note", "目标已确认"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Mission 状态：planning")
                .and(predicate::str::contains("当前阶段：上下文收集")),
        );
    codex_command(codex_home.path())?
        .current_dir(workspace.path())
        .args(["mission", "status"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("目标：测试目标")
                .and(predicate::str::contains("阶段：context")),
        );
    Ok(())
}
