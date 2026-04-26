use clap::Parser;
use codex_app_server_client::legacy_core;
use codex_arg0::Arg0DispatchPaths;
use codex_arg0::arg0_dispatch_or_else;
use codex_config::LoaderOverrides;
use codex_tui::AppExitInfo;
use codex_tui::Cli;
use codex_tui::ExitReason;
use codex_tui::run_main;
use supports_color::Stream;

fn format_exit_messages(exit_info: AppExitInfo, color_enabled: bool) -> Vec<String> {
    let AppExitInfo {
        token_usage,
        thread_id,
        ..
    } = exit_info;

    let mut lines = Vec::new();
    if !token_usage.is_zero() {
        lines.push(codex_protocol::protocol::FinalOutput::from(token_usage).to_string());
    }

    if let Some(resume_cmd) =
        legacy_core::util::resume_command(/*thread_name*/ None, thread_id)
    {
        let command = if color_enabled {
            format!("\u{1b}[36m{resume_cmd}\u{1b}[39m")
        } else {
            resume_cmd
        };
        lines.push(format!("要继续此会话，请运行 {command}"));
    }

    lines
}

#[derive(Parser, Debug)]
struct TopCli {
    #[clap(flatten)]
    inner: Cli,
}

fn main() -> anyhow::Result<()> {
    arg0_dispatch_or_else(|arg0_paths: Arg0DispatchPaths| async move {
        let inner = TopCli::parse().inner;
        let exit_info = run_main(
            inner,
            arg0_paths,
            LoaderOverrides::default(),
            /*remote*/ None,
            /*remote_auth_token*/ None,
        )
        .await?;
        match exit_info.exit_reason {
            ExitReason::Fatal(message) => {
                eprintln!("ERROR: {message}");
                std::process::exit(1);
            }
            ExitReason::UserRequested => {}
        }

        let color_enabled = supports_color::on(Stream::Stdout).is_some();
        for line in format_exit_messages(exit_info, color_enabled) {
            println!("{line}");
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn top_cli_parses_config_flag_into_inner_cli() {
        let cli = TopCli::try_parse_from(["codex-tui", "--config", "model=\"o3\""])
            .expect("parse should succeed");

        assert_eq!(
            cli.inner.config_overrides.raw_overrides,
            vec!["model=\"o3\"".to_string()]
        );
    }
}
