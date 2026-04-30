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
        .args(["zmission", "--help"])
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
        .args(["zmission", "status"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Mission 状态：未启动")
                .and(predicate::str::contains(".mission/mission_state.json")),
        );
    Ok(())
}

/// 验证 mission start 写入状态（CLI 路径），然后 status 能正确读取。
///
/// `mission start <goal>` 会先在 CLI 侧写入状态、打印状态信息，
/// 然后调用 `run_phases_loop` 启动 TUI，在无 TTY 测试环境中 TUI 会失败。
/// 我们只检查 stdout 中的 CLI 侧状态输出，不要求进程成功退出。
///
/// `mission continue` 现在直接启动 TUI，无法在无 TTY 测试中验证 stdout。
/// 因此改为通过 `mission status` 验证状态推进。
#[tokio::test]
async fn mission_start_and_continue_persist_state() -> Result<()> {
    let codex_home = TempDir::new()?;
    let workspace = TempDir::new()?;

    // mission start 写入状态后调用 TUI，TUI 因无 TTY 会失败。
    // 我们只关心 CLI 侧状态输出是否正确。
    codex_command(codex_home.path())?
        .current_dir(workspace.path())
        .env("OPENAI_API_KEY", "fake-key-for-test")
        .args(["zmission", "start", "--skip-git-repo-check", "测试目标"])
        .assert()
        .stdout(
            predicate::str::contains("Mission 状态：planning")
                .and(predicate::str::contains("当前阶段：目标澄清")),
        );

    // status 应独立于 TUI，可以直接成功。
    codex_command(codex_home.path())?
        .current_dir(workspace.path())
        .args(["zmission", "status"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("目标：测试目标")
                .and(predicate::str::contains("阶段：intent")),
        );
    Ok(())
}
