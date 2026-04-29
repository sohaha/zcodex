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

/// 验证 mission start 和 continue 的状态机推进（不启动 agent session）。
///
/// `mission start` 默认会调用 `codex exec` 启动 agent session，
/// 但在无 API key 的测试环境中 exec 会因配置/认证失败。
/// 使用 `--skip-git-repo-check` 避免非 git 目录报错，
/// 同时设置 `OPENAI_API_KEY=fake` 让 exec 的配置加载通过，
/// exec 最终会在 LLM 调用时失败——但这发生在状态已持久化之后，
/// 所以 stdout 中已包含状态机输出。
#[tokio::test]
async fn mission_start_and_continue_persist_state() -> Result<()> {
    let codex_home = TempDir::new()?;
    let workspace = TempDir::new()?;

    // mission start 写入状态后调用 exec，exec 因无真实 API key 会失败。
    // 我们只关心状态机部分是否正确，因此检查 stdout 包含预期输出即可，
    // 不要求整个进程成功退出。
    codex_command(codex_home.path())?
        .current_dir(workspace.path())
        .env("OPENAI_API_KEY", "fake-key-for-test")
        .args(["mission", "start", "--skip-git-repo-check", "测试目标"])
        .assert()
        .stdout(
            predicate::str::contains("Mission 状态：planning")
                .and(predicate::str::contains("当前阶段：目标澄清")),
        );

    // mission continue 同理。
    codex_command(codex_home.path())?
        .current_dir(workspace.path())
        .env("OPENAI_API_KEY", "fake-key-for-test")
        .args([
            "mission",
            "continue",
            "--skip-git-repo-check",
            "--note",
            "目标已确认",
        ])
        .assert()
        .stdout(
            predicate::str::contains("Mission 状态：planning")
                .and(predicate::str::contains("当前阶段：上下文收集")),
        );

    // status 应独立于 exec，可以直接成功。
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
