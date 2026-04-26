use clap::Parser;
use std::path::PathBuf;

#[derive(Debug, Parser)]
pub struct AppCommand {
    /// 要在 Codex Desktop 中打开的工作区路径。
    #[arg(value_name = "路径", default_value = ".")]
    pub path: PathBuf,

    /// 覆盖 macOS DMG 下载 URL（高级）。
    #[arg(long, default_value = DEFAULT_CODEX_DMG_URL)]
    pub download_url: String,
}

pub async fn run_app(cmd: AppCommand) -> anyhow::Result<()> {
    let workspace = std::fs::canonicalize(&cmd.path).unwrap_or(cmd.path);
    #[cfg(target_os = "macos")]
    {
        crate::desktop_app::run_app_open_or_install(workspace, cmd.download_url_override).await
    }
    #[cfg(target_os = "windows")]
    {
        crate::desktop_app::run_app_open_or_install(workspace, cmd.download_url_override).await
    }
}
