use clap::Parser;
use codex_arg0::Arg0DispatchPaths;
use codex_arg0::arg0_dispatch_or_else;
use codex_serve::Cli;
use codex_serve::run_main;

fn main() -> anyhow::Result<()> {
    arg0_dispatch_or_else(|arg0_paths: Arg0DispatchPaths| async move {
        let cli = Cli::parse();
        run_main(cli, arg0_paths.codex_linux_sandbox_exe.clone()).await?;
        Ok(())
    })
}
