pub(crate) mod debug_sandbox;
mod exit_status;
pub(crate) mod login;
pub mod zoffsec_cmd;
pub mod zoffsec_config;

use clap::Parser;
use codex_utils_cli::CliConfigOverrides;

pub use debug_sandbox::run_command_under_landlock;
pub use debug_sandbox::run_command_under_seatbelt;
pub use debug_sandbox::run_command_under_windows;
pub use login::read_api_key_from_stdin;
pub use login::run_login_status;
pub use login::run_login_with_api_key;
pub use login::run_login_with_chatgpt;
pub use login::run_login_with_device_code;
pub use login::run_login_with_device_code_fallback_to_browser;
pub use login::run_logout;

#[derive(Debug, Parser)]
pub struct SeatbeltCommand {
    /// 便捷别名：低摩擦沙箱自动执行（禁用网络、可写入当前工作目录和 TMPDIR 的沙箱）。
    #[arg(long = "full-auto", default_value_t = false)]
    pub full_auto: bool,

    /// 命令运行期间，通过 macOS 的 `log stream` 命令捕获沙箱拒绝记录，并在退出后打印。
    #[arg(long = "log-denials", default_value_t = false)]
    pub log_denials: bool,

    #[clap(skip)]
    pub config_overrides: CliConfigOverrides,

    /// 要在 Seatbelt 沙箱下运行的完整命令参数。
    #[arg(trailing_var_arg = true)]
    pub command: Vec<String>,
}

#[derive(Debug, Parser)]
pub struct LandlockCommand {
    /// 便捷别名：低摩擦沙箱自动执行（禁用网络、可写入当前工作目录和 TMPDIR 的沙箱）。
    #[arg(long = "full-auto", default_value_t = false)]
    pub full_auto: bool,

    #[clap(skip)]
    pub config_overrides: CliConfigOverrides,

    /// 要在 Linux 沙箱下运行的完整命令参数。
    #[arg(trailing_var_arg = true)]
    pub command: Vec<String>,
}

#[derive(Debug, Parser)]
pub struct WindowsCommand {
    /// 便捷别名：低摩擦沙箱自动执行（禁用网络、可写入当前工作目录和 TMPDIR 的沙箱）。
    #[arg(long = "full-auto", default_value_t = false)]
    pub full_auto: bool,

    #[clap(skip)]
    pub config_overrides: CliConfigOverrides,

    /// 要在 Windows 受限令牌沙箱下运行的完整命令参数。
    #[arg(trailing_var_arg = true)]
    pub command: Vec<String>,
}
