/// Update action the CLI should perform after the TUI exits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateAction {
    /// Update via `npm install -g @sohaha/zcodex@latest`.
    NpmGlobalLatest,
    /// Update via standalone Windows PowerShell installer.
    StandaloneWindows,
    /// Update via `bun install -g @sohaha/zcodex@latest`.
    BunGlobalLatest,
    /// Update via `brew upgrade codex`.
    BrewUpgrade,
}

impl UpdateAction {
    /// Returns the list of command-line arguments for invoking the update.
    pub fn command_args(self) -> (&'static str, &'static [&'static str]) {
        match self {
            UpdateAction::NpmGlobalLatest => ("npm", &["install", "-g", "@sohaha/zcodex"]),
            UpdateAction::StandaloneWindows => ("powershell", &["-NoProfile", "-Command", "& { [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12; Invoke-Expression ((Invoke-WebRequest -UseBasicParsing 'https://cnb.cool/zcodex_install.ps1').Content) }"]),
            UpdateAction::BunGlobalLatest => ("bun", &["install", "-g", "@sohaha/zcodex"]),
            UpdateAction::BrewUpgrade => ("brew", &["upgrade", "--cask", "codex"]),
        }
    }

    /// Returns string representation of the command-line arguments for invoking the update.
    pub fn command_str(self) -> String {
        let (command, args) = self.command_args();
        shlex::try_join(std::iter::once(command).chain(args.iter().copied()))
            .unwrap_or_else(|_| format!("{command} {}", args.join(" ")))
    }
}
    current_exe: &std::path::Path,
    managed_by_npm: bool,
    managed_by_bun: bool,
) -> Option<UpdateAction> {
    if managed_by_npm {
        Some(UpdateAction::NpmGlobalLatest)
    } else if managed_by_bun {
        Some(UpdateAction::BunGlobalLatest)
    } else if is_macos
        && (current_exe.starts_with("/opt/homebrew") || current_exe.starts_with("/usr/local"))
    {
        Some(UpdateAction::BrewUpgrade)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_update_action_without_env_mutation() {
        assert_eq!(
            detect_update_action(
                /*is_macos*/ false,
                std::path::Path::new("/any/path"),
                /*managed_by_npm*/ false,
                /*managed_by_bun*/ false
            ),
            None
        );
        assert_eq!(
            detect_update_action(
                /*is_macos*/ false,
                std::path::Path::new("/any/path"),
                /*managed_by_npm*/ true,
                /*managed_by_bun*/ false
            ),
            Some(UpdateAction::NpmGlobalLatest)
        );
        assert_eq!(
            detect_update_action(
                /*is_macos*/ false,
                std::path::Path::new("/any/path"),
                /*managed_by_npm*/ false,
                /*managed_by_bun*/ true
            ),
            Some(UpdateAction::BunGlobalLatest)
        );
        assert_eq!(
            detect_update_action(
                /*is_macos*/ true,
                std::path::Path::new("/opt/homebrew/bin/codex"),
                /*managed_by_npm*/ false,
                /*managed_by_bun*/ false
            ),
            Some(UpdateAction::BrewUpgrade)
        );
        assert_eq!(
            detect_update_action(
                /*is_macos*/ true,
                std::path::Path::new("/usr/local/bin/codex"),
                /*managed_by_npm*/ false,
                /*managed_by_bun*/ false
            ),
            Some(UpdateAction::BrewUpgrade)
        );
    }
}
