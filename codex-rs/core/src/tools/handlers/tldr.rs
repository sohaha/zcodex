use super::parse_arguments;
use crate::function_tool::FunctionCallError;
use crate::tools::context::FunctionToolOutput;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::ToolKind;
use anyhow::Result;
use async_trait::async_trait;
use codex_native_tldr::daemon::daemon_lock_is_held;
use codex_native_tldr::daemon::lock_path_for_project;
use codex_native_tldr::daemon::pid_path_for_project;
use codex_native_tldr::daemon::query_daemon;
use codex_native_tldr::daemon::socket_path_for_project;
use codex_native_tldr::tool_api::TldrToolCallParam;
use codex_native_tldr::tool_api::daemon_metadata_looks_alive;
use codex_native_tldr::tool_api::run_tldr_tool_with_hooks;
use std::ffi::OsString;
use std::fs::File;
use std::fs::OpenOptions;
use std::path::Path;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use std::time::Instant;
use tokio::process::Command;
use tokio::time::sleep;

pub struct TldrHandler;

#[async_trait]
impl ToolHandler for TldrHandler {
    type Output = FunctionToolOutput;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        let ToolInvocation { turn, payload, .. } = invocation;
        let arguments = match payload {
            ToolPayload::Function { arguments } => arguments,
            _ => {
                return Err(FunctionCallError::RespondToModel(
                    "tldr handler received unsupported payload".to_string(),
                ));
            }
        };
        let mut args: TldrToolCallParam = parse_arguments(&arguments)?;
        if args.project.is_none() {
            args.project = Some(turn.cwd.display().to_string());
        }

        match run_tldr_tool_with_hooks(
            args,
            |project_root, command| Box::pin(query_daemon(project_root, command)),
            |project_root| Box::pin(ensure_daemon_running(project_root)),
        )
        .await
        {
            Ok(result) => {
                let json =
                    serde_json::to_string_pretty(&result.structured_content).map_err(|err| {
                        FunctionCallError::Fatal(format!("serialize tldr output: {err}"))
                    })?;
                Ok(FunctionToolOutput::from_text(json, Some(true)))
            }
            Err(err) => Ok(FunctionToolOutput::from_text(err.to_string(), Some(false))),
        }
    }
}

async fn ensure_daemon_running(project_root: &Path) -> Result<bool> {
    if !cfg!(unix) {
        return Ok(false);
    }

    if daemon_metadata_looks_alive(project_root) {
        return Ok(true);
    }
    if daemon_lock_is_held(project_root)? {
        return wait_for_daemon_startup(project_root).await;
    }

    let Some(launcher_lock) = try_open_launcher_lock(project_root)? else {
        return wait_for_daemon_startup(project_root).await;
    };

    if daemon_metadata_looks_alive(project_root) {
        return Ok(true);
    }
    if daemon_lock_is_held(project_root)? {
        return wait_for_daemon_startup_during_launch(project_root).await;
    }

    cleanup_stale_artifacts(project_root);

    let mut child = daemon_launcher_command(project_root)?
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    tokio::spawn(async move {
        let _ = child.wait().await;
    });

    let started = wait_for_daemon_startup_during_launch(project_root).await;
    drop(launcher_lock);
    started
}

fn daemon_launcher_command(project_root: &Path) -> Result<Command> {
    let current_exe = std::env::current_exe()?;
    let mut command = Command::new(current_exe);
    command.args(daemon_launcher_args(project_root));
    Ok(command)
}

fn daemon_launcher_args(project_root: &Path) -> [OsString; 4] {
    [
        OsString::from("tldr"),
        OsString::from("internal-daemon"),
        OsString::from("--project"),
        project_root.as_os_str().to_os_string(),
    ]
}

async fn wait_for_daemon_startup(project_root: &Path) -> Result<bool> {
    wait_for_daemon_startup_with_launcher_lock(project_root, false).await
}

async fn wait_for_daemon_startup_during_launch(project_root: &Path) -> Result<bool> {
    wait_for_daemon_startup_with_launcher_lock(project_root, true).await
}

async fn wait_for_daemon_startup_with_launcher_lock(
    project_root: &Path,
    ignore_launcher_lock: bool,
) -> Result<bool> {
    let start = Instant::now();
    let timeout = Duration::from_secs(3);
    while start.elapsed() < timeout {
        if daemon_metadata_looks_alive_with_launcher_lock(project_root, ignore_launcher_lock) {
            return Ok(true);
        }
        sleep(Duration::from_millis(50)).await;
    }
    Ok(false)
}

fn daemon_metadata_looks_alive_with_launcher_lock(
    project_root: &Path,
    ignore_launcher_lock: bool,
) -> bool {
    match codex_native_tldr::daemon::daemon_health(project_root) {
        Ok(health) => {
            if health.healthy {
                return true;
            }
            if !ignore_launcher_lock && launcher_lock_is_held(project_root).unwrap_or(false) {
                return false;
            }
            if health.should_cleanup_artifacts() {
                cleanup_stale_artifacts(project_root);
            }
            false
        }
        Err(_) => false,
    }
}

fn cleanup_stale_artifacts(project_root: &Path) {
    if launcher_lock_is_held(project_root).unwrap_or(false) {
        return;
    }

    let Ok(health) = codex_native_tldr::daemon::daemon_health(project_root) else {
        return;
    };
    if !health.should_cleanup_artifacts() {
        return;
    }
    cleanup_file_if_exists(socket_path_for_project(project_root));
    cleanup_file_if_exists(pid_path_for_project(project_root));
}

fn launcher_lock_path_for_project(project_root: &Path) -> PathBuf {
    lock_path_for_project(project_root).with_extension("launch.lock")
}

fn try_open_launcher_lock(project_root: &Path) -> Result<Option<File>> {
    let lock_path = launcher_lock_path_for_project(project_root);
    if let Some(parent) = lock_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let lock_file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)?;

    match lock_file.try_lock() {
        Ok(()) => Ok(Some(lock_file)),
        Err(std::fs::TryLockError::WouldBlock) => Ok(None),
        Err(err) => Err(err.into()),
    }
}

fn launcher_lock_is_held(project_root: &Path) -> Result<bool> {
    Ok(try_open_launcher_lock(project_root)?.is_none())
}

fn cleanup_file_if_exists(path: PathBuf) {
    if let Err(err) = std::fs::remove_file(&path)
        && err.kind() != std::io::ErrorKind::NotFound
    {
        let _ = err;
    }
}
