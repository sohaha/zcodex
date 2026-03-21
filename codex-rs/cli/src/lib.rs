pub mod debug_sandbox;
mod exit_status;
pub mod login;

use clap::Parser;
use codex_utils_cli::CliConfigOverrides;

#[derive(Debug, Parser)]
pub struct SeatbeltCommand {
    /// 便捷别名：低摩擦沙箱自动执行（禁用网络、可写入 cwd 和 TMPDIR 的沙箱）
    #[arg(long = "full-auto", default_value_t = false)]
    pub full_auto: bool,

    /// 命令运行期间，通过 `log stream` 捕获 macOS 沙箱拒绝记录，并在退出后打印
    #[arg(long = "log-denials", default_value_t = false)]
    pub log_denials: bool,

    #[clap(skip)]
    pub config_overrides: CliConfigOverrides,

    /// 要在 Seatbelt 下运行的完整命令参数。
    #[arg(trailing_var_arg = true)]
    pub command: Vec<String>,
}

#[derive(Debug, Parser)]
pub struct LandlockCommand {
    /// 便捷别名：低摩擦沙箱自动执行（禁用网络、可写入 cwd 和 TMPDIR 的沙箱）
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
    /// 便捷别名：低摩擦沙箱自动执行（禁用网络、可写入 cwd 和 TMPDIR 的沙箱）
    #[arg(long = "full-auto", default_value_t = false)]
    pub full_auto: bool,

    #[clap(skip)]
    pub config_overrides: CliConfigOverrides,

    /// 要在 Windows 受限令牌沙箱下运行的完整命令参数。
    #[arg(trailing_var_arg = true)]
    pub command: Vec<String>,
}
