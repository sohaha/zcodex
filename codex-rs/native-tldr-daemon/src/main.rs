use anyhow::Result;
use clap::Parser;
use codex_native_tldr::daemon::TldrDaemon;
use std::path::PathBuf;

#[derive(Debug, Parser)]
struct Cli {
    #[arg(long, default_value = ".")]
    project: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let daemon = TldrDaemon::new(cli.project.canonicalize()?);
    daemon.run_until_shutdown().await
}
