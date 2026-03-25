use anyhow::Result;
use clap::Parser;
use codex_native_tldr::daemon::TldrDaemon;
use codex_native_tldr::load_tldr_config;
use std::path::PathBuf;

#[derive(Debug, Parser)]
struct Cli {
    #[arg(long, default_value = ".")]
    project: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let project_root = cli.project.canonicalize()?;
    let daemon = TldrDaemon::from_config(load_tldr_config(&project_root)?);
    daemon.run_until_shutdown().await
}
